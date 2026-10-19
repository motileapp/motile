//! The system prompt that teaches a model a library: `generateSystemPrompt` of thesysdev/openui
//! (`packages/lang-core/src/parser/prompt.ts`, spec/prompt.md sections 4 and 5), section by section
//! and word for word, but for the text the reference writes for its own chat library: the
//! sections that only come with tools name `FormControl`, `SelectItem` and `TextContent` and say
//! that `$variables` can't hold lists, which open-intelligent-ui's library contradicts, so they
//! are written for it here.

use crate::builtins::Builtin;
use crate::library::Library;
use crate::value::{Map, Value};

const PREAMBLE: &str = "You are an AI assistant that responds using openui-lang, a declarative UI language. Your \
                        ENTIRE response must be valid openui-lang code — no markdown, no explanations, just \
                        openui-lang.";

const ACTION_HEADER: &str = "## Action — Button Behavior";

/// What `generateSystemPrompt` takes beside the library. Flags left unset follow the reference's
/// defaults: tool calls when there are tools, bindings with tool calls, built-in functions with
/// either.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PromptOptions {
    /// Replaces the first paragraph.
    pub preamble: Option<String>,
    /// `Query()`, `Mutation()`, `@Run` and the tool workflow.
    pub tool_calls: Option<bool>,
    /// `$variables`, `@Set`, `@Reset` and the expression rules.
    pub bindings: Option<bool>,
    /// Lists `@Count`, `@Sum`… beside the library's functions.
    pub builtin_functions: Option<bool>,
    pub tools: Vec<Tool>,
    pub examples: Vec<String>,
    /// Listed after `examples`.
    pub tool_examples: Vec<String>,
    /// Bullets at the end.
    pub additional_rules: Vec<String>,
    pub edit_mode: bool,
    pub inline_mode: bool,
}

/// A tool `Query` and `Mutation` call: its name alone, or its schemas.
#[derive(Debug, Clone, PartialEq)]
pub enum Tool {
    Name(String),
    Spec(ToolSpec),
}

/// A tool as MCP describes one.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: Option<String>,
    /// A JSON Schema object of its arguments.
    pub input_schema: Value,
    /// The JSON Schema of what it returns; the prompt shows the defaults it implies.
    pub output_schema: Option<Value>,
}

struct Flags {
    tool_calls: bool,
    bindings: bool,
}

pub fn generate_system_prompt(library: &Library, options: &PromptOptions) -> String {
    let root = library.root().unwrap_or("Root");
    let tool_calls = options.tool_calls.unwrap_or(!options.tools.is_empty());
    let bindings = options.bindings.unwrap_or(tool_calls);
    let expressions = tool_calls || bindings;
    let flags = Flags { tool_calls, bindings };
    let uses_action_expression = library.components().any(|component| {
        let signature = component.signature();
        signature.contains("ActionExpression") || restricted_slot(&signature)
    });

    let mut parts = vec![
        options.preamble.clone().unwrap_or_else(|| PREAMBLE.to_owned()),
        String::new(),
        syntax_rules(root, expressions, bindings),
        String::new(),
        component_signatures(library, &flags, uses_action_expression),
    ];

    let custom_functions: Vec<String> =
        library.functions().map(|function| function_line(&function.signature(), &function.description)).collect();
    if options.builtin_functions.unwrap_or(expressions) {
        parts.push(String::new());
        parts.push(builtin_functions_section(&custom_functions, tool_calls));
    } else if !custom_functions.is_empty() {
        parts.push(String::new());
        parts.push(format!("## Built-in Functions\n\n{}", custom_functions.join("\n")));
    }

    if tool_calls {
        parts.push(String::new());
        parts.push(QUERY_SECTION.to_owned());
        parts.push(String::new());
        parts.push(MUTATION_SECTION.to_owned());
    }

    let custom_actions: Vec<String> = library
        .actions()
        .map(|action| format!("- {}", function_line(&action.signature(), &action.description)))
        .collect();
    if uses_action_expression {
        parts.push(String::new());
        parts.push(action_section(&flags, &custom_actions));
    } else if !custom_actions.is_empty() {
        parts.push(String::new());
        parts.push(format!(
            "{ACTION_HEADER}\n\nPass one step, or Action([...]) with several steps, as a button's action.\n\nAvailable \
             steps:\n{}",
            custom_actions.join("\n")
        ));
    }

    if tool_calls && bindings {
        parts.push(String::new());
        parts.push(INTERACTIVE_FILTERS_SECTION.to_owned());
    }
    if tool_calls {
        parts.push(String::new());
        parts.push(TOOL_WORKFLOW_SECTION.to_owned());
    }
    if !options.tools.is_empty() {
        parts.push(String::new());
        parts.push(tools_section(&options.tools));
    }

    parts.push(String::new());
    parts.push(streaming_rules(root, expressions, tool_calls));

    let examples: Vec<&String> = options.examples.iter().chain(&options.tool_examples).collect();
    if !examples.is_empty() {
        parts.push(String::new());
        parts.push("## Examples".to_owned());
        parts.push(String::new());
        for example in examples {
            parts.push(example.clone());
            parts.push(String::new());
        }
    }

    if options.edit_mode {
        parts.push(String::new());
        parts.push(EDIT_MODE_SECTION.to_owned());
    }
    if options.inline_mode {
        parts.push(String::new());
        parts.push(INLINE_MODE_SECTION.to_owned());
    }

    parts.push(important_rules(root, &flags));

    if !options.additional_rules.is_empty() {
        parts.push(String::new());
        for rule in &options.additional_rules {
            parts.push(format!("- {rule}"));
        }
    }
    parts.join("\n")
}

/// A restricted action slot in a signature, e.g. `share?: @CopyToClipboard | @OpenUrl`.
fn restricted_slot(signature: &str) -> bool {
    let bytes = signature.as_bytes();
    bytes.windows(4).any(|window| {
        matches!(window[0], b':' | b'|') && window[1] == b' ' && window[2] == b'@' && window[3].is_ascii_uppercase()
    })
}

fn function_line(signature: &str, description: &str) -> String {
    if description.is_empty() {
        return format!("@{signature}");
    }
    format!("@{signature} — {description}")
}

fn syntax_rules(root: &str, expressions: bool, bindings: bool) -> String {
    let mut lines = vec![
        "## Syntax Rules".to_owned(),
        String::new(),
        "1. Each statement is on its own line: `identifier = Expression`".to_owned(),
        format!("2. `root` is the entry point — every program must define `root = {root}(...)`"),
        "3. Expressions are: strings (\"...\"), numbers, booleans (true/false), null, arrays ([...]), objects ({...}), \
         or component calls TypeName(arg1, arg2, ...)"
            .to_owned(),
        "4. Use references for readability: define `name = ...` on one line, then use `name` later".to_owned(),
        "5. EVERY variable (except root) MUST be referenced by at least one other variable. Unreferenced variables are \
         silently dropped and will NOT render. Always include defined variables in their parent's children/items \
         array."
            .to_owned(),
        "6. Arguments are POSITIONAL (order matters, not names). Write `SomeComp([children], \"row\", \"l\")` NOT \
         `SomeComp([children], direction: \"row\", gap: \"l\")` — colon syntax is NOT supported and silently breaks"
            .to_owned(),
        "7. Optional arguments can be omitted from the end".to_owned(),
    ];
    let mut rules: Vec<&str> = Vec::new();
    if bindings {
        rules.push(
            "Declare mutable state with `$varName = defaultValue`. Components marked with `$binding` can read/write \
             these. Undeclared $variables are auto-created with null default.",
        );
    }
    if expressions {
        rules.extend([
            "String concatenation: `\"text\" + $var + \"more\"`",
            "Dot member access: `query.field` reads a field; on arrays it extracts that field from every element",
            "Index access: `arr[0]`, `data[index]`",
            "Arithmetic operators: +, -, *, /, % (work on numbers; + is string concat when either side is a string)",
            "Comparison: ==, !=, >, <, >=, <=",
            "Logical: &&, ||, ! (prefix)",
            "Ternary: `condition ? valueIfTrue : valueIfFalse`",
            "Parentheses for grouping: `(a + b) * c`",
        ]);
    }
    lines.extend(rules.iter().enumerate().map(|(index, rule)| format!("{}. {rule}", index + 8)));
    lines.push("- Strings use double quotes with backslash escaping".to_owned());
    lines.join("\n")
}

fn component_signatures(library: &Library, flags: &Flags, uses_action_expression: bool) -> String {
    let mut lines = vec![
        "## Component Signatures".to_owned(),
        String::new(),
        "Arguments marked with ? are optional. Sub-components can be inline or referenced; prefer references for \
         better streaming."
            .to_owned(),
    ];
    if uses_action_expression {
        let steps: Vec<String> = builtin_steps(flags)
            .map(|step| format!("@{}", step.name()))
            .chain(library.actions().map(|action| format!("@{}", action.name)))
            .collect();
        lines.push(format!(
            "Props typed `ActionExpression` accept one @step, or Action([@steps...]) for several. See the Action \
             section for available steps ({}).",
            steps.join(", ")
        ));
        if library.components().any(|component| restricted_slot(&component.signature())) {
            lines.push("Props typed like `@OpenUrl | @ToAssistant` accept only the steps listed.".to_owned());
        }
    }
    let uses_bindings =
        flags.bindings || library.components().any(|component| component.signature().contains("$binding"));
    if uses_bindings {
        lines.push("Props marked `$binding<type>` accept a `$variable` reference for two-way binding.".to_owned());
    }
    let line = |name: &str| {
        let component = library.component(name)?;
        let signature = component.signature();
        if component.description.is_empty() {
            return Some(signature);
        }
        Some(format!("{signature} — {}", component.description))
    };
    if library.groups().is_empty() {
        lines.push(String::new());
        lines.extend(library.components().filter_map(|component| line(&component.name)));
        return lines.join("\n");
    }
    let mut grouped: Vec<&str> = Vec::new();
    for group in library.groups() {
        lines.push(String::new());
        lines.push(format!("### {}", group.name));
        for name in &group.components {
            if grouped.contains(&name.as_str()) {
                continue;
            }
            let Some(text) = line(name) else { continue };
            grouped.push(name);
            lines.push(text);
        }
        lines.extend(group.notes.iter().cloned());
    }
    let ungrouped: Vec<&str> =
        library.components().map(|component| component.name.as_str()).filter(|name| !grouped.contains(name)).collect();
    if !ungrouped.is_empty() {
        lines.push(String::new());
        lines.push("### Other".to_owned());
        lines.extend(ungrouped.into_iter().filter_map(line));
    }
    lines.join("\n")
}

fn builtin_functions_section(custom: &[String], tool_calls: bool) -> String {
    let lines: Vec<String> = Builtin::FUNCTIONS
        .iter()
        .map(|builtin| function_line(&builtin.signature(), builtin.description()))
        .chain(custom.iter().cloned())
        .collect();
    let on = if tool_calls { "Query results" } else { "data" };
    format!(
        "## Built-in Functions

Data functions prefixed with `@` to distinguish from components. These are the ONLY functions available — do NOT invent new ones.
Use @-prefixed built-in functions (@Count, @Sum, @Avg, @Min, @Max, @Round) on {on}. Do NOT hardcode computed values.

{}

Builtins compose — output of one is input to the next:
`@Count(@Filter(data.rows, \"field\", \"==\", \"val\"))` for KPIs/chart values, `@Round(@Avg(data.rows.score), 1)`, `@Each(data.rows, \"item\", Comp(item.field))` for per-item rendering.
Array pluck: `data.rows.field` extracts a field from every row → use with @Sum, @Avg, charts, tables.

IMPORTANT @Each rule: The loop variable (e.g. \"item\") is ONLY available inside the @Each template expression. Always inline the template — do NOT extract it to a separate statement.
CORRECT: `list = @Each(rows, \"t\", Comp(t.name))`
WRONG: `item = Comp(t.name)` then `list = @Each(rows, \"t\", item)`, because t is undefined in item.",
        lines.join("\n")
    )
}

const QUERY_SECTION: &str = "## Query — Live Data Fetching

Fetch data from available tools. Returns defaults instantly, swaps in real data when it arrives.

```
metrics = Query(\"tool_name\", {arg1: value, arg2: $binding}, {defaultField: 0, defaultData: []}, refreshInterval?)
```

- First arg: tool name (string)
- Second arg: arguments object (may reference $bindings — re-fetches automatically on change)
- Third arg: default data (rendered immediately before fetch resolves)
- Fourth arg (optional): refresh interval in seconds (e.g. 30 for auto-refresh every 30s)
- Use dot access on results: metrics.totalEvents, metrics.data.day (array pluck)
- Query results must use regular identifiers: `metrics = Query(...)`, NOT `$metrics = Query(...)`
- Manual refresh: `Button(\"Refresh\", Action([@Run(query1), @Run(query2)]))` — re-fetches the listed queries
- Refresh all queries: create Action with @Run for each query";

const MUTATION_SECTION: &str = "## Mutation — Write Operations

Execute state-changing tool calls (create, update, delete). Unlike Query (auto-fetches on render), Mutation fires only on button click via Action.

```
result = Mutation(\"tool_name\", {arg1: $binding, arg2: \"value\"})
```

- First arg: tool name (string)
- Second arg: arguments object (evaluated with current $binding values at click time)
- result.status: \"idle\" | \"loading\" | \"success\" | \"error\"
- result.data: tool response on success
- result.error: error message on failure
- Mutation results use regular identifiers: `result = Mutation(...)`, NOT `$result`
- Show loading state: `result.status == \"loading\" ? Text(\"Saving...\") : null`";

/// The built-in steps the prompt lists: `@Run` needs tool calls, `@Set` and `@Reset` bindings.
fn builtin_steps(flags: &Flags) -> impl Iterator<Item = Builtin> + '_ {
    Builtin::ACTIONS.into_iter().filter(|step| match step {
        Builtin::Run => flags.tool_calls,
        Builtin::Set | Builtin::Reset => flags.bindings,
        _ => true,
    })
}

fn action_section(flags: &Flags, custom: &[String]) -> String {
    let steps: Vec<String> = builtin_steps(flags)
        .map(|step| format!("- {}", function_line(&step.signature(), step.description())))
        .chain(custom.iter().cloned())
        .collect();
    let mut examples = Vec::new();
    if flags.tool_calls {
        examples.push(
            "Example — mutation + refresh + reset (PREFERRED pattern):
```
$binding = \"default\"
result = Mutation(\"tool_name\", {field: $binding})
data = Query(\"tool_name\", {}, {rows: []})
onSubmit = Action([@Run(result), @Run(data), @Reset($binding)])
```",
        );
    }
    examples.push(
        "Example — simple nav:
```
viewBtn = Button(\"View\", @OpenUrl(\"https://example.com\"))
```",
    );
    let mut rules = vec![
        "- An action can be assigned to a variable or inlined: Button(\"Go\", onSubmit) and Button(\"Go\", \
         Action([...])) both work",
    ];
    if flags.tool_calls {
        rules.push("- If a @Run(mutation) step fails, remaining steps are skipped (halt on failure)");
        rules.push("- @Run(queryRef) re-fetches the query (fire-and-forget, cannot fail)");
    }
    format!(
        "{ACTION_HEADER}

An action wires a button click to operations. Pass one @-prefixed step bare, e.g. Button(\"Ask\", @ToAssistant(\"Tell me more\")). Put several steps in Action([...]), e.g. Button(\"Read\", Action([@OpenUrl(\"https://example.com\"), @ToAssistant(\"Summarize this page\")])). Steps execute in order.
Buttons without an action automatically send their label to the assistant (equivalent to @ToAssistant(label)).

Available steps:
{}

{}

{}",
        steps.join("\n"),
        examples.join("\n\n"),
        rules.join("\n")
    )
}

const INTERACTIVE_FILTERS_SECTION: &str = "## Interactive Filters

To let the user filter data with a dropdown:
1. Declare a $variable with a default: `$dateRange = \"14\"`
2. Bind a Select to it through its value, the first argument: `Select($dateRange, [{value: \"7\", label: \"Last 7 days\"}, {value: \"14\", label: \"Last 14 days\"}], \"Date range\")`
3. Pass $dateRange in Query args: `Query(\"tool\", {dateRange: $dateRange}, {defaults})`
4. When the user changes the Select, $dateRange updates and the Query automatically re-fetches

FILTER WIRING RULE: If a $binding filter is visible in the UI, EVERY relevant Query MUST reference that $binding in its args. Never show a filter dropdown while hardcoding the query args.

Rules for $variables:
- $variables hold any literal value: a string, a number, a boolean, a list or an object
- An input is interactive when its value, the first argument, is a $variable
- Queries must use regular identifiers (NOT $variables): `metrics = Query(...)` not `$metrics = Query(...)`
- **Auto-declare**: You do NOT need to explicitly declare $variables. If you use `$foo` without declaring it, the parser auto-creates `$foo = null`. You can still declare explicitly to set a default: `$days = \"14\"`

## Forms

A Form groups inputs. Each input's value is a $variable that other statements, Query args and actions read:
```
$name = \"\"
$role = \"engineer\"
contactForm = Form([nameField, roleField, submitBtn])
nameField = Input($name, \"Name\", \"text\", \"Your name\")
roleField = Select($role, [{value: \"engineer\", label: \"Engineer\"}, {value: \"designer\", label: \"Designer\"}, {value: \"pm\", label: \"PM\"}], \"Role\")
submitBtn = Button(\"Submit\")
```

For form + mutation patterns (create, refresh, reset), see the Action section example above.

IMPORTANT: A Button that runs a Mutation is disabled until its required fields are filled: `Button(\"Create\", onSubmit, \"solid\", null, null, null, $name == \"\")`.";

const TOOL_WORKFLOW_SECTION: &str = "## Data Workflow

When tools are available, follow this workflow:
1. FIRST: Call the most relevant tool to inspect the real data shape before generating code
2. Use Query() for READ operations (data that should stay live) — NEVER hardcode tool results as literal arrays or objects
3. Use Mutation() for WRITE operations (create, update, delete) — triggered by button clicks via @Run(mutationRef)
4. Use the real data from step 1 as condensed Query defaults (3-5 rows) so the UI renders immediately
5. Use @-prefixed builtins (@Count, @Filter, @Sort, @Sum) on Query results for KPIs and aggregations — the runtime evaluates these live on every refresh
6. Hardcoded arrays are ONLY for static display data (labels, options) where no tool exists

WRONG — you called a tool and got data back, but you inlined the results:
```
openCount = 2
item1 = SomeComp(\"first item title\")
item2 = SomeComp(\"second item title\")
list = SomeList([item1, item2])
chart = SomeChart([\"A\", \"B\"], [12, 8])
```
This is static — it shows stale data and won't update. Creating item1, item2, item3... manually is ALWAYS wrong when a tool exists.

RIGHT — use Query() for live data, Mutation() for writes, @builtins to derive values:
```
data = Query(\"tool_name\", {}, {rows: []})
openCount = @Count(@Filter(data.rows, \"field\", \"==\", \"value\"))
list = @Each(data.rows, \"item\", SomeComp(item.title, item.field))
createResult = Mutation(\"create_tool\", {title: $title})
submitBtn = Button(\"Create\", Action([@Run(createResult), @Run(data), @Reset($title)]))
```
Everything derives from the Query — when data refreshes, the entire dashboard updates automatically.";

const EDIT_MODE_SECTION: &str = "## Edit Mode

The runtime merges by statement name: same name = replace, new name = append.
Output ONLY statements that changed or are new. Everything else is kept automatically.

### Delete
To remove a component, re-declare its parent without that component in the parent's children array. The removed component and any statements only it referenced are automatically garbage-collected.

### Patch size guide
- Changing a title or label: 1 statement
- Adding a component: 2-3 statements (the new component + parent update)
- Removing a component: 1 statement (re-declare parent without the removed child)
- Adding a filter + wiring to query: 3-5 statements
- Restructuring into tabs: 5-10 statements

### Rules
- Reuse existing statement names exactly — do not rename
- Do NOT re-emit unchanged statements — the runtime keeps them
- A typical edit patch is 1-10 statements, not 20+
- If the existing code already satisfies the request, output only the root statement
- NEVER output the entire program as a patch. Only output what actually changes
- If you are about to output more than 10 statements, reconsider — most edits need fewer";

const INLINE_MODE_SECTION: &str = "## Inline Mode

You are in inline mode. You can respond in two ways:

### 1. Code response (when the user wants to CREATE or CHANGE the UI)
Wrap openui-lang code in triple-backtick fences. You can include explanatory text before/after:

Here's your dashboard:

```openui-lang
root = RootComp([header, content])
header = SomeHeader(\"Title\")
content = SomeContent(\"Hello world\")
```

I created a simple layout with a header.

### 2. Text-only response (when the user asks a QUESTION)
If the user asks \"what is this?\", \"explain the chart\", \"how does this work\", etc. — respond with plain text. Do NOT output any openui-lang code. The existing dashboard stays unchanged.

### Rules
- When the user asks for changes, output ONLY the changed/new statements in a fenced block
- When the user asks a question, respond with text only — NO code. The dashboard stays unchanged.
- The parser extracts code from fences automatically. Text outside fences is shown as chat.";

fn streaming_rules(root: &str, expressions: bool, tool_calls: bool) -> String {
    let mut steps = vec![format!("1. `root = {root}(...)` — UI shell appears immediately")];
    if expressions {
        steps.push("2. $variable declarations — state ready for bindings".to_owned());
        if tool_calls {
            steps.push("3. Query statements — defaults resolve immediately so components render with data".to_owned());
        }
        let n = steps.len();
        steps.push(format!("{}. Component definitions — fill in with data already available", n + 1));
        steps.push(format!("{}. Data values — leaf content last", n + 2));
    } else {
        steps.push("2. Component definitions — fill in as they stream".to_owned());
        steps.push("3. Data values — leaf content last".to_owned());
    }
    format!(
        "## Hoisting & Streaming (CRITICAL)

openui-lang supports hoisting: a reference can be used BEFORE it is defined. The parser resolves all references after the full input is parsed.

During streaming, the output is re-parsed on every chunk. Undefined references are temporarily unresolved and appear once their definitions stream in. This creates a progressive top-down reveal — structure first, then data fills in.

**Recommended statement order for optimal streaming:**
{}

Always write the root = {root}(...) statement first so the UI shell appears immediately, even before child data has streamed in.",
        steps.join("\n")
    )
}

fn important_rules(root: &str, flags: &Flags) -> String {
    let mut lines = vec![
        format!("1. root = {root}(...) is the FIRST line (for optimal streaming)."),
        "2. Every referenced name is defined. Every defined name (other than root) is reachable from root.".to_owned(),
    ];
    if flags.tool_calls {
        lines.push("3. Every Query result is referenced by at least one component.".to_owned());
    }
    if flags.bindings {
        let number = if flags.tool_calls { 4 } else { 3 };
        lines.push(format!("{number}. Every $binding appears in at least one component or expression."));
    }
    if flags.tool_calls && flags.bindings {
        lines.push("5. Every visible filter $binding appears in at least one Query args object.".to_owned());
    }
    format!(
        "## Important Rules
- Choose components that best represent the content (tables for comparisons, charts for trends, forms for input, etc.)

## Final Verification
Before finishing, walk your output and verify:
{}",
        lines.join("\n")
    )
}

/// A JSON Schema as a signature's type; a list of types, as `["string", "null"]`, is their union.
fn type_text(schema: &Value) -> String {
    match schema.get("type") {
        Some(Value::Array(kinds)) => {
            kinds.iter().filter_map(Value::as_str).map(|kind| kind_text(schema, kind)).collect::<Vec<_>>().join(" | ")
        }
        Some(Value::String(kind)) => kind_text(schema, kind),
        _ => "any".to_owned(),
    }
}

fn kind_text(schema: &Value, kind: &str) -> String {
    match kind {
        "string" => match schema.get("enum").and_then(Value::as_array) {
            Some(values) => values
                .iter()
                .map(|value| format!("\"{}\"", crate::js::to_string(value)))
                .collect::<Vec<_>>()
                .join(" | "),
            None => "string".to_owned(),
        },
        "number" | "integer" => "number".to_owned(),
        "boolean" => "boolean".to_owned(),
        "null" => "null".to_owned(),
        "array" => match schema.get("items") {
            Some(items) => format!("{}[]", type_text(items)),
            None => "any[]".to_owned(),
        },
        "object" => match schema.get("properties").and_then(Value::as_object).filter(|map| !map.is_empty()) {
            Some(properties) => format!("{{{}}}", fields_text(properties, schema)),
            None => "object".to_owned(),
        },
        _ => "any".to_owned(),
    }
}

fn fields_text(properties: &Map, schema: &Value) -> String {
    let required: Vec<&str> =
        schema.get("required").and_then(Value::as_array).unwrap_or_default().iter().filter_map(Value::as_str).collect();
    properties
        .iter()
        .map(|(key, value)| {
            let mark = if required.contains(&key.as_str()) { "" } else { "?" };
            format!("{key}{mark}: {}", type_text(value))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The defaults a Query of a tool starts from, from its output schema.
fn default_for(schema: &Value) -> Value {
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => Value::from(""),
        Some("number" | "integer") => Value::Number(0.0),
        Some("boolean") => Value::Bool(false),
        Some("array") => Value::Array(Vec::new()),
        Some("object") => schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| properties.iter().map(|(key, value)| (key.clone(), default_for(value))).collect())
            .unwrap_or_else(|| Value::Object(Map::new())),
        _ => Value::Null,
    }
}

fn tool_signature(tool: &ToolSpec) -> String {
    let args = match tool.input_schema.get("properties").and_then(Value::as_object).filter(|map| !map.is_empty()) {
        Some(properties) => fields_text(properties, &tool.input_schema),
        None => String::new(),
    };
    let returns = tool.output_schema.as_ref().map(|schema| format!(" → {}", type_text(schema))).unwrap_or_default();
    let mut line = format!("- {}({args}){returns}", tool.name);
    if let Some(description) = tool.description.as_ref().filter(|description| !description.is_empty()) {
        line.push_str(&format!("\n  {description}"));
    }
    line
}

fn tools_section(tools: &[Tool]) -> String {
    let mut lines = vec![
        "## Available Tools".to_owned(),
        String::new(),
        "Use these with Query() for read operations or Mutation() for write operations. The LLM decides which is \
         appropriate based on the tool's purpose."
            .to_owned(),
        String::new(),
    ];
    let specs: Vec<&ToolSpec> = tools
        .iter()
        .filter_map(|tool| match tool {
            Tool::Spec(spec) => Some(spec),
            Tool::Name(_) => None,
        })
        .collect();
    lines.extend(tools.iter().filter_map(|tool| match tool {
        Tool::Name(name) => Some(format!("- {name}")),
        Tool::Spec(_) => None,
    }));
    lines.extend(specs.iter().map(|spec| tool_signature(spec)));
    let with_output: Vec<(&str, &Value)> =
        specs.iter().filter_map(|spec| Some((spec.name.as_str(), spec.output_schema.as_ref()?))).collect();
    if !with_output.is_empty() {
        lines.push(String::new());
        lines.push("### Default values for Query results".to_owned());
        lines.push(String::new());
        lines.push("Use these shapes as minimal Query defaults:".to_owned());
        for (name, schema) in with_output {
            lines.push(format!("- {name}: `{}`", default_for(schema).to_json_string()));
        }
    }
    lines.push(String::new());
    lines.push(
        "CRITICAL: Use ONLY the tools listed above in Query() and Mutation() calls. Do NOT invent or guess tool names. \
         If the user asks for functionality that doesn't match any available tool, use realistic mock data instead of \
         fabricating a tool call."
            .to_owned(),
    );
    lines.join("\n")
}
