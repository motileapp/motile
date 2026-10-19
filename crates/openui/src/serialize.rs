//! A rendered tree back to source text (language.md §9.2): the entry first, every other named
//! component a statement of its own, children before parents, then the state declarations.

use indexmap::IndexMap;

use crate::ast::{BinaryOp, UnaryOp};
use crate::element::{ActionPlan, ActionStep, Element};
use crate::js;
use crate::library::Library;
use crate::node::{ElementNode, Node};
use crate::value::{Map, Value};

/// The source of a rendered tree, with the host's state declarations after it.
pub fn serialize(root: &Element, library: &Library, state: Option<&Map>) -> String {
    let mut writer = Writer { library, statements: IndexMap::new() };
    let entry = root.statement.clone().unwrap_or_else(|| "root".to_owned());
    let expr = writer.element(root);
    let mut lines = vec![format!("{entry} = {expr}")];
    lines.extend(
        writer.statements.into_iter().filter(|(name, _)| *name != entry).map(|(name, expr)| format!("{name} = {expr}")),
    );
    for (name, value) in state.into_iter().flatten() {
        if !value.is_null() {
            let value = Writer { library, statements: IndexMap::new() }.value(value);
            lines.push(format!("{name} = {value}"));
        }
    }
    lines.join("\n")
}

struct Writer<'a> {
    library: &'a Library,
    statements: IndexMap<String, String>,
}

impl Writer<'_> {
    fn value(&mut self, value: &Value) -> String {
        match value {
            Value::Null => "null".to_owned(),
            Value::Bool(value) => value.to_string(),
            Value::Number(number) if number.is_finite() => js::number_to_string(*number),
            Value::Number(_) => "null".to_owned(),
            Value::String(_) => value.to_json_string(),
            Value::Array(items) => {
                format!("[{}]", items.iter().map(|item| self.value(item)).collect::<Vec<_>>().join(", "))
            }
            Value::Object(map) => {
                let entries = map.iter().map(|(key, item)| format!("{}: {}", key_text(key), self.value(item)));
                format!("{{{}}}", entries.collect::<Vec<_>>().join(", "))
            }
            Value::Element(element) => match &element.statement {
                Some(name) => {
                    if !self.statements.contains_key(name) {
                        let expr = self.element(element);
                        self.statements.insert(name.clone(), expr);
                    }
                    name.clone()
                }
                None => self.element(element),
            },
            Value::Plan(plan) => self.plan(plan),
        }
    }

    /// Props by position, `null` for a missing one, trailing nulls of optional props trimmed.
    fn element(&mut self, element: &Element) -> String {
        let Some(def) = self.library.component(&element.component) else {
            let args: Vec<String> = element.props.values().map(|value| self.value(value)).collect();
            return format!("{}({})", element.component, args.join(", "));
        };
        let mut args: Vec<String> = def
            .props
            .iter()
            .map(|param| match element.bindings.get(&param.name) {
                Some(binding) => match &binding.write {
                    Some(write) => format!("{} = {}", binding.variable, self.node(write)),
                    None => binding.variable.clone(),
                },
                None => element.props.get(&param.name).map_or_else(|| "null".to_owned(), |value| self.value(value)),
            })
            .collect();
        while args.last().is_some_and(|arg| arg == "null") && !def.props[args.len() - 1].required() {
            args.pop();
        }
        format!("{}({})", element.component, args.join(", "))
    }

    fn plan(&mut self, plan: &ActionPlan) -> String {
        if let [step] = plan.steps.as_slice() {
            return self.step(step);
        }
        let steps: Vec<String> = plan.steps.iter().map(|step| self.step(step)).collect();
        format!("Action([{}])", steps.join(", "))
    }

    fn step(&mut self, step: &ActionStep) -> String {
        match step {
            ActionStep::Run { statement, .. } => format!("@Run({statement})"),
            ActionStep::ToAssistant { message, context } => {
                let message = Value::from(message.as_str()).to_json_string();
                match context {
                    Some(context) => format!("@ToAssistant({message}, {})", self.value(context)),
                    None => format!("@ToAssistant({message})"),
                }
            }
            ActionStep::OpenUrl { url } => format!("@OpenUrl({})", Value::from(url.as_str()).to_json_string()),
            ActionStep::Set { variable, value } => format!("@Set({variable}, {})", self.node(value)),
            ActionStep::Reset { variables } => format!("@Reset({})", variables.join(", ")),
            ActionStep::Custom { name, params } => {
                let names: Vec<&str> = match self.library.action(name) {
                    Some(action) => action.params.iter().map(|param| param.name.as_str()).collect(),
                    None => params.keys().map(String::as_str).collect(),
                };
                let mut args: Vec<String> = names
                    .iter()
                    .map(|name| params.get(*name).map_or_else(|| "null".to_owned(), |value| self.value(value)))
                    .collect();
                while args.last().is_some_and(|arg| arg == "null") {
                    args.pop();
                }
                format!("@{name}({})", args.join(", "))
            }
        }
    }

    fn node(&mut self, node: &Node) -> String {
        match node {
            Node::Lit(value) => self.value(value),
            Node::Nothing => "null".to_owned(),
            Node::Array(items) => format!("[{}]", self.nodes(items)),
            Node::Object(entries) => {
                let entries = entries.iter().map(|(key, value)| format!("{}: {}", key_text(key), self.node(value)));
                format!("{{{}}}", entries.collect::<Vec<_>>().join(", "))
            }
            Node::Element(element) => self.element_node(element),
            Node::State(name) | Node::Query(name) | Node::Mutation(name) | Node::Loop(name) => name.clone(),
            Node::Unary(op, operand) => {
                let inner = self.node(operand);
                match **operand {
                    Node::Binary(..) | Node::Ternary(..) => format!("{}({inner})", op.as_str()),
                    _ if *op == UnaryOp::Neg && inner.starts_with('-') => format!("-({inner})"),
                    _ => format!("{}{inner}", op.as_str()),
                }
            }
            Node::Binary(op, left, right) => {
                let left = self.operand(left, *op, false);
                let right = self.operand(right, *op, true);
                format!("{left} {} {right}", op.as_str())
            }
            Node::Ternary(cond, then, otherwise) => {
                let cond = match **cond {
                    Node::Ternary(..) => format!("({})", self.node(cond)),
                    _ => self.node(cond),
                };
                format!("{cond} ? {} : {}", self.node(then), self.node(otherwise))
            }
            Node::Member(object, field) => {
                let object = self.postfix(object);
                if is_name(field) || field.parse::<u32>().is_ok_and(|index| index.to_string() == *field) {
                    format!("{object}.{field}")
                } else {
                    format!("{object}[{}]", Value::from(field.as_str()).to_json_string())
                }
            }
            Node::Index(object, index) => format!("{}[{}]", self.postfix(object), self.node(index)),
            Node::Assign(target, value) => format!("{target} = {}", self.node(value)),
            Node::Builtin(builtin, args) => format!("@{}({})", builtin.name(), self.nodes(args)),
            Node::Each(array, var, template) => {
                format!(
                    "@Each({}, {}, {})",
                    self.node(array),
                    Value::from(var.as_str()).to_json_string(),
                    self.node(template)
                )
            }
            Node::Function(name, args) | Node::Custom(name, args) => {
                let mut args: Vec<String> = args
                    .iter()
                    .map(|arg| arg.as_ref().map_or_else(|| "null".to_owned(), |arg| self.node(arg)))
                    .collect();
                while args.last().is_some_and(|arg| arg == "null") {
                    args.pop();
                }
                format!("@{name}({})", args.join(", "))
            }
            Node::Steps(list) => format!("Action({})", self.node(list)),
            Node::ToAssistant(message, context) => match context {
                Some(context) => format!("@ToAssistant({}, {})", self.node(message), self.node(context)),
                None => format!("@ToAssistant({})", self.node(message)),
            },
            Node::OpenUrl(url) => format!("@OpenUrl({})", self.node(url)),
            Node::Set(variable, value) => format!("@Set({variable}, {})", self.node(value)),
        }
    }

    fn nodes(&mut self, nodes: &[Node]) -> String {
        nodes.iter().map(|node| self.node(node)).collect::<Vec<_>>().join(", ")
    }

    fn element_node(&mut self, element: &ElementNode) -> String {
        let props: Map =
            element.props.iter().filter_map(|(name, node)| Some((name.clone(), node.as_lit()?.clone()))).collect();
        let dynamic: IndexMap<&str, &Node> =
            element.props.iter().filter(|(_, node)| !node.is_lit()).map(|(name, node)| (name.as_str(), node)).collect();
        let Some(def) = self.library.component(&element.component) else {
            let args: Vec<String> = element.props.iter().map(|(_, node)| self.node(node)).collect();
            return format!("{}({})", element.component, args.join(", "));
        };
        let mut args: Vec<String> = def
            .props
            .iter()
            .map(|param| match (dynamic.get(param.name.as_str()), props.get(&param.name)) {
                (Some(node), _) => self.node(node),
                (None, Some(value)) => self.value(value),
                (None, None) => "null".to_owned(),
            })
            .collect();
        while args.last().is_some_and(|arg| arg == "null") && !def.props[args.len() - 1].required() {
            args.pop();
        }
        format!("{}({})", element.component, args.join(", "))
    }

    /// An operand of a binary operator, in parentheses where reading it back would group it
    /// otherwise: `a - (b - c)`, `(a || b) && c`.
    fn operand(&mut self, node: &Node, parent: BinaryOp, right: bool) -> String {
        let text = self.node(node);
        let wrap = match node {
            Node::Ternary(..) | Node::Assign(..) => true,
            Node::Binary(op, ..) => {
                op.precedence() < parent.precedence() || (right && op.precedence() == parent.precedence())
            }
            _ => false,
        };
        if wrap { format!("({text})") } else { text }
    }

    /// What a member access or an index follows.
    fn postfix(&mut self, node: &Node) -> String {
        let text = self.node(node);
        match node {
            Node::Unary(..) | Node::Binary(..) | Node::Ternary(..) | Node::Assign(..) => format!("({text})"),
            Node::Lit(Value::Number(number)) if *number < 0.0 || text.contains(['.', 'e']) => format!("({text})"),
            _ => text,
        }
    }
}

fn is_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(text, "true" | "false" | "null")
}

/// Object keys are written unquoted when they read back as the same key.
fn key_text(key: &str) -> String {
    if is_name(key) || key.parse::<u32>().is_ok_and(|index| index.to_string() == key) {
        return key.to_owned();
    }
    Value::from(key).to_json_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn round_trip(source: &str) -> String {
        let parsed = parser::parse(&format!("x = {source}"));
        let expr = &parsed.statement("x").unwrap().expr;
        let node = lower(expr);
        Writer { library: &Library::new(Default::default()).unwrap(), statements: IndexMap::new() }.node(&node)
    }

    fn lower(expr: &crate::ast::Expr) -> Node {
        use crate::ast::Expr;
        match expr {
            Expr::Num(value) => Node::Lit(Value::Number(*value)),
            Expr::State(name) => Node::State(name.clone()),
            Expr::Unary(op, operand) => Node::Unary(*op, Box::new(lower(operand))),
            Expr::Binary(op, left, right) => Node::Binary(*op, Box::new(lower(left)), Box::new(lower(right))),
            Expr::Ternary(a, b, c) => Node::Ternary(Box::new(lower(a)), Box::new(lower(b)), Box::new(lower(c))),
            Expr::Member(object, field) => Node::Member(Box::new(lower(object)), field.clone()),
            _ => Node::null(),
        }
    }

    #[test]
    fn keeps_the_grouping() {
        for source in [
            "$a - ($b - $c)",
            "$a - $b - $c",
            "-($a + $b)",
            "($a || $b) && $c",
            "$a * ($b + 1)",
            "($a ? 1 : 2) ? 3 : 4",
            "$a ? 1 : $b ? 2 : 3",
            "!($a == 1)",
            "$a.b.c",
            "($a + 1).b",
        ] {
            assert_eq!(round_trip(source), source);
        }
        assert_eq!(round_trip("$m.0"), "$m.0");
    }
}
