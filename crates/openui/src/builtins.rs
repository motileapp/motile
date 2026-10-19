//! The names the language keeps for itself (language.md §1.3): its keywords, its call forms, the 14
//! built-in functions and the 5 built-in actions, with the params and descriptions the prompt
//! prints them with (`parser/builtins.ts`). The evaluator implements them.

use crate::schema::{self, Field, Schema};

pub const KEYWORDS: [&str; 3] = ["true", "false", "null"];
/// `Query` and `Mutation` are statement forms, `Action` an expression form.
pub const RESERVED_CALLS: [&str; 3] = ["Query", "Mutation", "Action"];
/// `name = Query(tool, args, defaults, refreshSeconds?)`.
pub const QUERY_PARAMS: [&str; 4] = ["tool", "args", "defaults", "refresh"];
/// `name = Mutation(tool, args)`.
pub const MUTATION_PARAMS: [&str; 2] = ["tool", "args"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Count,
    First,
    Last,
    Sum,
    Avg,
    Min,
    Max,
    Sort,
    Filter,
    Round,
    Abs,
    Floor,
    Ceil,
    Each,
    Run,
    ToAssistant,
    OpenUrl,
    Set,
    Reset,
    /// `Action([...])`, several actions in order.
    Action,
}

impl Builtin {
    /// The built-in functions, in the prompt's order.
    pub const FUNCTIONS: [Builtin; 14] = [
        Builtin::Count,
        Builtin::First,
        Builtin::Last,
        Builtin::Sum,
        Builtin::Avg,
        Builtin::Min,
        Builtin::Max,
        Builtin::Sort,
        Builtin::Filter,
        Builtin::Round,
        Builtin::Abs,
        Builtin::Floor,
        Builtin::Ceil,
        Builtin::Each,
    ];

    /// The built-in actions, in the prompt's order.
    pub const ACTIONS: [Builtin; 5] =
        [Builtin::Run, Builtin::ToAssistant, Builtin::OpenUrl, Builtin::Set, Builtin::Reset];

    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::FUNCTIONS.into_iter().chain(Builtin::ACTIONS).chain([Builtin::Action]).find(|b| b.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Count => "Count",
            Builtin::First => "First",
            Builtin::Last => "Last",
            Builtin::Sum => "Sum",
            Builtin::Avg => "Avg",
            Builtin::Min => "Min",
            Builtin::Max => "Max",
            Builtin::Sort => "Sort",
            Builtin::Filter => "Filter",
            Builtin::Round => "Round",
            Builtin::Abs => "Abs",
            Builtin::Floor => "Floor",
            Builtin::Ceil => "Ceil",
            Builtin::Each => "Each",
            Builtin::Run => "Run",
            Builtin::ToAssistant => "ToAssistant",
            Builtin::OpenUrl => "OpenUrl",
            Builtin::Set => "Set",
            Builtin::Reset => "Reset",
            Builtin::Action => "Action",
        }
    }

    /// An action, used in an action position, rather than a function.
    pub fn is_action(self) -> bool {
        matches!(
            self,
            Builtin::Run | Builtin::ToAssistant | Builtin::OpenUrl | Builtin::Set | Builtin::Reset | Builtin::Action
        )
    }

    /// Takes its arguments unevaluated: `@Each`'s template, the targets of `@Run`, `@Set` and
    /// `@Reset`, and the actions of `Action`.
    pub fn is_lazy(self) -> bool {
        matches!(self, Builtin::Each | Builtin::Run | Builtin::Set | Builtin::Reset | Builtin::Action)
    }

    /// The fewest and the most arguments it reads; `@Reset` takes any number of variables.
    pub fn arity(self) -> (usize, Option<usize>) {
        let params = self.params();
        let required = params.iter().filter(|param| param.required()).count();
        if self == Builtin::Reset {
            return (required, None);
        }
        (required, Some(params.len()))
    }

    pub fn params(self) -> Vec<Field> {
        let list = || Schema::array(Schema::Any).field("array");
        let numbers = || Schema::array(Schema::Number).field("numbers");
        let number = || Schema::Number.field("number");
        let variable = || Schema::Any.named("$variable").field("variable");
        match self {
            Builtin::Count | Builtin::First | Builtin::Last => vec![list()],
            Builtin::Sum | Builtin::Avg | Builtin::Min | Builtin::Max => vec![numbers()],
            Builtin::Sort => vec![
                list(),
                Schema::String.field("field"),
                Schema::enumeration(["asc", "desc"]).field("direction").optional(),
            ],
            Builtin::Filter => vec![
                list(),
                Schema::String.field("field"),
                Schema::enumeration(["==", "!=", ">", "<", ">=", "<=", "contains"]).field("operator"),
                Schema::Any.field("value"),
            ],
            Builtin::Round => vec![number(), Schema::Number.field("decimals").optional()],
            Builtin::Abs | Builtin::Floor | Builtin::Ceil => vec![number()],
            Builtin::Each => {
                vec![list(), Schema::String.field("varName"), Schema::Any.field("template")]
            }
            Builtin::Run => vec![Schema::Any.named("Query | Mutation").field("ref")],
            Builtin::ToAssistant => vec![Schema::String.field("message"), Schema::Any.field("context").optional()],
            Builtin::OpenUrl => vec![Schema::String.field("url")],
            Builtin::Set => vec![variable(), Schema::Any.field("value")],
            Builtin::Reset => vec![variable()],
            Builtin::Action => vec![Schema::array(Schema::Action).field("steps")],
        }
    }

    pub fn returns(self) -> Option<Schema> {
        match self {
            Builtin::Count
            | Builtin::Sum
            | Builtin::Avg
            | Builtin::Min
            | Builtin::Max
            | Builtin::Round
            | Builtin::Abs
            | Builtin::Floor
            | Builtin::Ceil => Some(Schema::Number),
            Builtin::First | Builtin::Last => Some(Schema::Any),
            Builtin::Sort | Builtin::Filter => Some(Schema::array(Schema::Any)),
            _ => None,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Builtin::Count => "Returns array length",
            Builtin::First => "Returns first element of array",
            Builtin::Last => "Returns last element of array",
            Builtin::Sum => "Sum of numeric array",
            Builtin::Avg => "Average of numeric array",
            Builtin::Min => "Minimum value in array",
            Builtin::Max => "Maximum value in array",
            Builtin::Sort => "Sort array by field. Direction: \"asc\" (default) or \"desc\"",
            Builtin::Filter => "Filter array by field value",
            Builtin::Round => "Round to N decimal places (default 0)",
            Builtin::Abs => "Absolute value",
            Builtin::Floor => "Round down to nearest integer",
            Builtin::Ceil => "Round up to nearest integer",
            Builtin::Each => {
                "Evaluate template for each element. varName is the loop variable — use it ONLY inside the template \
                 expression (inline). Do NOT create a separate statement for the template."
            }
            Builtin::Run => "Execute a Mutation or re-fetch a Query (ref must be a declared Query/Mutation)",
            Builtin::ToAssistant => {
                "Send a message to the assistant (for conversational buttons like \"Tell me more\", \"Explain this\"). \
                 The optional context is any value (object, array, string, number) passed to the assistant as hidden \
                 data, e.g. @ToAssistant(\"Show details\", {orderId: 42})"
            }
            Builtin::OpenUrl => "Navigate to a URL",
            Builtin::Set => "Set a $variable to a specific value",
            Builtin::Reset => {
                "Reset $variables to their declared defaults; pass several to reset each (e.g. @Reset($title, \
                 $priority) restores $title=\"\" and $priority=\"medium\")"
            }
            Builtin::Action => "Runs several actions in order",
        }
    }

    /// `Count(array: any[]) → number`, as the prompt lists it after an `@`.
    pub fn signature(self) -> String {
        schema::signature(self.name(), &self.params(), self.returns().as_ref())
    }
}

/// Whether a library may not use the name for a component, a function or an action.
pub fn is_reserved(name: &str) -> bool {
    KEYWORDS.contains(&name) || RESERVED_CALLS.contains(&name) || Builtin::from_name(name).is_some()
}

/// `ActionExpression`, the legacy action objects an action slot's JSON Schema describes.
pub fn action_expression_schema() -> Schema {
    let kind = |name: &str| Schema::literal(name).field("type");
    Schema::union([
        Schema::object([kind("open_url"), Schema::String.field("url")]),
        Schema::object([kind("continue_conversation"), Schema::String.field("context").optional()]),
        Schema::object([Schema::String.field("type"), Schema::record(Schema::Any).field("params").optional()]),
    ])
}

/// The JSON shape of a built-in action a restricted slot names: `OpenUrl` or `ToAssistant`.
pub fn builtin_action_ref_schema(name: &str) -> Option<Schema> {
    let index = match name {
        "OpenUrl" => 0,
        "ToAssistant" => 1,
        _ => return None,
    };
    let Schema::Union(shapes) = action_expression_schema() else { return None };
    shapes.into_iter().nth(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_the_reference_signatures() {
        let cases = [
            (Builtin::Count, "Count(array: any[]) → number"),
            (Builtin::Sum, "Sum(numbers: number[]) → number"),
            (Builtin::Sort, "Sort(array: any[], field: string, direction?: \"asc\" | \"desc\") → any[]"),
            (
                Builtin::Filter,
                "Filter(array: any[], field: string, operator: \"==\" | \"!=\" | \">\" | \"<\" | \">=\" | \"<=\" | \
                 \"contains\", value: any) → any[]",
            ),
            (Builtin::Round, "Round(number: number, decimals?: number) → number"),
            (Builtin::Each, "Each(array: any[], varName: string, template: any)"),
            (Builtin::Run, "Run(ref: Query | Mutation)"),
            (Builtin::ToAssistant, "ToAssistant(message: string, context?: any)"),
            (Builtin::Set, "Set(variable: $variable, value: any)"),
            (Builtin::Reset, "Reset(variable: $variable)"),
        ];
        for (builtin, expected) in cases {
            assert_eq!(builtin.signature(), expected);
        }
    }

    #[test]
    fn knows_names_and_arities() {
        assert_eq!(Builtin::FUNCTIONS.len() + Builtin::ACTIONS.len(), 19);
        for builtin in Builtin::FUNCTIONS.into_iter().chain(Builtin::ACTIONS).chain([Builtin::Action]) {
            assert_eq!(Builtin::from_name(builtin.name()), Some(builtin));
        }
        assert_eq!(Builtin::Sort.arity(), (2, Some(3)));
        assert_eq!(Builtin::Filter.arity(), (4, Some(4)));
        assert_eq!(Builtin::Reset.arity(), (1, None));
        assert_eq!(Builtin::Each.arity(), (3, Some(3)));
        assert!(is_reserved("Query") && is_reserved("Each") && is_reserved("OpenUrl") && is_reserved("null"));
        assert!(!is_reserved("Upper") && !is_reserved("count"));
    }
}
