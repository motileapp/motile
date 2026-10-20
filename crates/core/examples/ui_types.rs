//! Writes the Swift and Kotlin copies of the OpenUI tree's types (`render/openui/tree.rs`), read
//! from the Rust source with their serde attributes, as each client reads the core's JSON: Swift
//! from `JSONSerialization`'s dictionaries, Kotlin from `kotlinx.serialization`'s `JsonObject`.
//! A type is named with `Ui` before it unless it has it. `scripts/ui-types.sh` runs it.
//!
//!     cargo run -p motile-core --example ui_types -- <swift file> <kotlin file>

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use syn::{Attribute, Expr, Fields, GenericArgument, Item, Lit, Meta, PathArguments, Type};

const ROOT: &str = "UiNode";
const SOURCES: [&str; 2] = ["src/render/openui/tree.rs", "src/render/markdown.rs"];

#[derive(Clone, Debug)]
enum Ty {
    Str,
    Bool,
    Float,
    Double,
    Int,
    Opt(Box<Ty>),
    List(Box<Ty>),
    Named(String),
    /// Any JSON, as the program gave it.
    Json,
    /// A prose block, as the clients' typesetters read it.
    Prose,
    /// `canvas::scene::Color`, which serializes itself.
    Color,
}

struct Field {
    name: String,
    json: String,
    doc: Vec<String>,
    ty: Ty,
    flatten: bool,
}

struct Variant {
    name: String,
    json: String,
    doc: Vec<String>,
    fields: Vec<Field>,
}

enum Def {
    Struct { doc: Vec<String>, fields: Vec<Field> },
    Unit { doc: Vec<String>, variants: Vec<Variant> },
    Tagged { doc: Vec<String>, tag: String, variants: Vec<Variant> },
}

struct Defs {
    defs: BTreeMap<String, Def>,
    aliases: BTreeMap<String, Ty>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [swift_path, kotlin_path] = args.as_slice() else {
        eprintln!("usage: ui_types <swift file> <kotlin file>");
        std::process::exit(2);
    };
    let mut defs = Defs { defs: BTreeMap::new(), aliases: BTreeMap::new() };
    let manifest = env!("CARGO_MANIFEST_DIR");
    for source in SOURCES {
        let text = std::fs::read_to_string(format!("{manifest}/{source}")).expect("the source");
        let file = syn::parse_file(&text).expect("valid Rust");
        defs.read(&file.items);
    }
    let order = defs.reachable(ROOT);
    std::fs::write(swift_path, swift(&defs, &order)).expect("the Swift file");
    std::fs::write(kotlin_path, kotlin(&defs, &order)).expect("the Kotlin file");
}

fn docs(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(pair) if pair.path.is_ident("doc") => match &pair.value {
                Expr::Lit(syn::ExprLit { lit: Lit::Str(text), .. }) => Some(text.value().trim().to_owned()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// `#[serde(key = "value")]`s and bare `#[serde(word)]`s.
fn serde(attrs: &[Attribute]) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
        let _ = attr.parse_nested_meta(|meta| {
            let key = meta.path.get_ident().map(ToString::to_string).unwrap_or_default();
            if let Ok(value) = meta.value() {
                if let Ok(Lit::Str(text)) = value.parse::<Lit>() {
                    found.insert(key, text.value());
                } else {
                    let _ = value.parse::<Expr>();
                }
            } else {
                found.insert(key, String::new());
            }
            Ok(())
        });
    }
    found
}

fn derives_serialize(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("derive")
            && attr.meta.require_list().is_ok_and(|list| list.tokens.to_string().contains("Serialize"))
    })
}

/// serde's `snake_case`: an underscore before each capital but the first, then lower case.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            out.push('_');
        }
        out.extend(character.to_lowercase());
    }
    out
}

fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for (index, character) in name.chars().enumerate() {
        if character == '_' {
            upper = true;
        } else if index == 0 {
            out.extend(character.to_lowercase());
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

/// `PascalCase` to `lowerCamel`, keeping a run of capitals together: `YouTube` → `youTube`.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|first| first.to_lowercase().chain(chars).collect()).unwrap_or_default()
}

fn type_name(name: &str) -> String {
    if name.starts_with("Ui") { name.to_owned() } else { format!("Ui{name}") }
}

impl Defs {
    fn read(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Type(alias) => {
                    let name = alias.ident.to_string();
                    if let Some(ty) = self.ty(&alias.ty) {
                        self.aliases.insert(name, ty);
                    }
                }
                Item::Struct(item) if derives_serialize(&item.attrs) => {
                    let fields = self.fields(&item.fields);
                    self.defs.insert(item.ident.to_string(), Def::Struct { doc: docs(&item.attrs), fields });
                }
                Item::Enum(item) if derives_serialize(&item.attrs) => {
                    let attrs = serde(&item.attrs);
                    let rename = |name: &str| match attrs.get("rename_all").map(String::as_str) {
                        Some("snake_case") => snake(name),
                        _ => name.to_owned(),
                    };
                    let variants = item
                        .variants
                        .iter()
                        .map(|variant| Variant {
                            name: variant.ident.to_string(),
                            json: serde(&variant.attrs)
                                .get("rename")
                                .cloned()
                                .unwrap_or_else(|| rename(&variant.ident.to_string())),
                            doc: docs(&variant.attrs),
                            fields: self.fields(&variant.fields),
                        })
                        .collect();
                    let doc = docs(&item.attrs);
                    let def = match attrs.get("tag") {
                        Some(tag) => Def::Tagged { doc, tag: tag.clone(), variants },
                        None => Def::Unit { doc, variants },
                    };
                    self.defs.insert(item.ident.to_string(), def);
                }
                _ => {}
            }
        }
    }

    fn fields(&self, fields: &Fields) -> Vec<Field> {
        fields
            .iter()
            .filter(|field| !serde(&field.attrs).contains_key("skip"))
            .map(|field| {
                let name = field.ident.as_ref().expect("named fields").to_string();
                let attrs = serde(&field.attrs);
                Field {
                    json: attrs.get("rename").cloned().unwrap_or_else(|| name.clone()),
                    name,
                    doc: docs(&field.attrs),
                    ty: self.ty(&field.ty).unwrap_or_else(|| panic!("a type the generator reads: {:?}", field.ident)),
                    flatten: attrs.contains_key("flatten"),
                }
            })
            .collect()
    }

    fn ty(&self, ty: &Type) -> Option<Ty> {
        match ty {
            Type::Array(array) => Some(Ty::List(Box::new(self.ty(&array.elem)?))),
            Type::Path(path) => {
                let segment = path.path.segments.last()?;
                let name = segment.ident.to_string();
                let argument = || match &segment.arguments {
                    PathArguments::AngleBracketed(arguments) => match arguments.args.first() {
                        Some(GenericArgument::Type(inner)) => self.ty(inner),
                        _ => None,
                    },
                    _ => None,
                };
                Some(match name.as_str() {
                    "String" | "str" => Ty::Str,
                    "bool" => Ty::Bool,
                    "f32" => Ty::Float,
                    "f64" => Ty::Double,
                    "u8" | "u16" | "u32" | "u64" | "usize" | "i32" | "i64" => Ty::Int,
                    "Option" => Ty::Opt(Box::new(argument()?)),
                    "Vec" => Ty::List(Box::new(argument()?)),
                    "Arc" | "Box" => argument()?,
                    "Value" => Ty::Json,
                    "Prose" => Ty::Prose,
                    "Color" => Ty::Color,
                    "Spans" => Ty::List(Box::new(Ty::Int)),
                    _ => self.aliases.get(&name).cloned().unwrap_or(Ty::Named(name)),
                })
            }
            _ => None,
        }
    }

    /// The types `root` reaches, each once, in the order they are reached.
    fn reachable(&self, root: &str) -> Vec<String> {
        fn walk(defs: &Defs, ty: &Ty, seen: &mut BTreeSet<String>, order: &mut Vec<String>) {
            match ty {
                Ty::Opt(inner) | Ty::List(inner) => walk(defs, inner, seen, order),
                Ty::Named(name) => visit(defs, name, seen, order),
                _ => {}
            }
        }
        fn visit(defs: &Defs, name: &str, seen: &mut BTreeSet<String>, order: &mut Vec<String>) {
            if !seen.insert(name.to_owned()) {
                return;
            }
            order.push(name.to_owned());
            let def = defs.defs.get(name).unwrap_or_else(|| panic!("no type {name}"));
            let fields: Vec<&Field> = match def {
                Def::Struct { fields, .. } => fields.iter().collect(),
                Def::Unit { .. } => Vec::new(),
                Def::Tagged { variants, .. } => variants.iter().flat_map(|variant| &variant.fields).collect(),
            };
            for field in fields {
                walk(defs, &field.ty, seen, order);
            }
        }
        let mut order = Vec::new();
        visit(self, root, &mut BTreeSet::new(), &mut order);
        order
    }

    /// Whether reading a value of the type can fail: an unknown tag, or a field that can.
    fn failable(&self, ty: &Ty) -> bool {
        self.failable_in(ty, &mut BTreeSet::new())
    }

    fn failable_in(&self, ty: &Ty, visiting: &mut BTreeSet<String>) -> bool {
        if let Ty::Color = ty {
            return true;
        }
        let Ty::Named(name) = ty else { return false };
        if !visiting.insert(name.clone()) {
            return false;
        }
        let failable = match &self.defs[name] {
            Def::Tagged { .. } => true,
            Def::Unit { .. } => false,
            Def::Struct { fields, .. } => fields.iter().any(|field| self.failable_in(&field.ty, visiting)),
        };
        visiting.remove(name);
        failable
    }

    fn fields_failable(&self, fields: &[Field]) -> bool {
        fields.iter().any(|field| self.failable(&field.ty))
    }
}

const HEADER: &str = "Written by scripts/ui-types.sh from crates/core/src/render/openui/tree.rs: do not edit.";

fn swift(defs: &Defs, order: &[String]) -> String {
    let mut out = format!("// {HEADER}\n\nimport Foundation\n{SWIFT_COLOR}");
    for name in order {
        out.push('\n');
        match &defs.defs[name] {
            Def::Struct { doc, fields } => swift_struct(&mut out, defs, &type_name(name), doc, fields, ""),
            Def::Unit { doc, variants } => {
                swift_doc(&mut out, doc, "");
                let _ = writeln!(out, "enum {}: String {{", type_name(name));
                for variant in variants {
                    swift_doc(&mut out, &variant.doc, "    ");
                    let case = lower_first(&variant.name);
                    if case == variant.json {
                        let _ = writeln!(out, "    case {case}");
                    } else {
                        let _ = writeln!(out, "    case {case} = \"{}\"", variant.json);
                    }
                }
                out.push_str("}\n");
            }
            Def::Tagged { doc, tag, variants } => {
                let enum_name = type_name(name);
                swift_doc(&mut out, doc, "");
                let _ = writeln!(out, "indirect enum {enum_name} {{");
                for variant in variants {
                    swift_doc(&mut out, &variant.doc, "    ");
                    let case = lower_first(&variant.name);
                    if variant.fields.is_empty() {
                        let _ = writeln!(out, "    case {case}");
                    } else {
                        let _ = writeln!(out, "    case {case}({})", variant.name);
                    }
                }
                out.push_str("\n    init?(json: JSON) {\n");
                let _ = writeln!(out, "        switch json[\"{tag}\"] as? String {{");
                for variant in variants {
                    let case = lower_first(&variant.name);
                    let _ = writeln!(out, "        case \"{}\":", variant.json);
                    if variant.fields.is_empty() {
                        let _ = writeln!(out, "            self = .{case}");
                    } else if defs.fields_failable(&variant.fields) {
                        let _ = writeln!(
                            out,
                            "            guard let value = {}(json: json) else {{ return nil }}",
                            variant.name
                        );
                        let _ = writeln!(out, "            self = .{case}(value)");
                    } else {
                        let _ = writeln!(out, "            self = .{case}({}(json: json))", variant.name);
                    }
                }
                out.push_str("        default:\n            return nil\n        }\n    }\n");
                for variant in variants.iter().filter(|variant| !variant.fields.is_empty()) {
                    out.push('\n');
                    swift_struct(&mut out, defs, &variant.name, &[], &variant.fields, "    ");
                }
                out.push_str("}\n");
            }
        }
    }
    out
}

fn swift_doc(out: &mut String, doc: &[String], indent: &str) {
    for line in doc {
        let _ = writeln!(out, "{indent}/// {line}");
    }
}

fn swift_type(ty: &Ty) -> String {
    match ty {
        Ty::Str => "String".into(),
        Ty::Bool => "Bool".into(),
        Ty::Float | Ty::Double => "Double".into(),
        Ty::Int => "Int".into(),
        Ty::Opt(inner) => format!("{}?", swift_type(inner)),
        Ty::List(inner) => format!("[{}]", swift_type(inner)),
        Ty::Named(name) => type_name(name),
        Ty::Json => "Any?".into(),
        Ty::Prose => "JSON".into(),
        Ty::Color => "UiColor".into(),
    }
}

/// Swift that reads `value`, an `Any?`, as the type; optional when reading it can fail.
fn swift_read(defs: &Defs, ty: &Ty, value: &str) -> String {
    match ty {
        Ty::Str => format!("{value} as? String ?? \"\""),
        Ty::Bool => format!("{value} as? Bool ?? false"),
        Ty::Float | Ty::Double => format!("({value} as? NSNumber)?.doubleValue ?? 0"),
        Ty::Int => format!("({value} as? NSNumber)?.intValue ?? 0"),
        Ty::Opt(inner) => match inner.as_ref() {
            Ty::Str => format!("{value} as? String"),
            Ty::Bool => format!("{value} as? Bool"),
            Ty::Float | Ty::Double => format!("({value} as? NSNumber)?.doubleValue"),
            Ty::Int => format!("({value} as? NSNumber)?.intValue"),
            Ty::Color => format!("UiColor(json: {value})"),
            Ty::Named(name) => match &defs.defs[name] {
                Def::Unit { .. } => format!("({value} as? String).flatMap({}.init(rawValue:))", type_name(name)),
                _ if defs.failable(inner) => format!("({value} as? JSON).flatMap({}.init(json:))", type_name(name)),
                _ => format!("({value} as? JSON).map({}.init(json:))", type_name(name)),
            },
            other => format!("{value} == nil ? nil : {}", swift_read(defs, other, value)),
        },
        Ty::List(inner) => {
            let map = if defs.failable(inner) { "compactMap" } else { "map" };
            format!("({value} as? [Any] ?? []).{map} {{ {} }}", swift_read(defs, inner, "$0"))
        }
        Ty::Named(name) => match &defs.defs[name] {
            Def::Unit { variants, .. } => {
                format!(
                    "{}(rawValue: {value} as? String ?? \"\") ?? .{}",
                    type_name(name),
                    lower_first(&variants[0].name)
                )
            }
            _ => format!("{}(json: {value} as? JSON ?? [:])", type_name(name)),
        },
        Ty::Json => value.to_owned(),
        Ty::Prose => format!("{value} as? JSON ?? [:]"),
        Ty::Color => format!("UiColor(json: {value})"),
    }
}

fn swift_struct(out: &mut String, defs: &Defs, name: &str, doc: &[String], fields: &[Field], indent: &str) {
    swift_doc(out, doc, indent);
    let _ = writeln!(out, "{indent}struct {name} {{");
    for field in fields {
        swift_doc(out, &field.doc, &format!("{indent}    "));
        let _ = writeln!(out, "{indent}    let {}: {}", camel(&field.name), swift_type(&field.ty));
    }
    let failable = defs.fields_failable(fields);
    let _ = writeln!(out, "\n{indent}    init{}(json: JSON) {{", if failable { "?" } else { "" });
    for field in fields {
        let property = camel(&field.name);
        let read = if field.flatten {
            format!("{}(json: json)", swift_type(&field.ty))
        } else {
            swift_read(defs, &field.ty, &format!("json[\"{}\"]", field.json))
        };
        if defs.failable(&field.ty) {
            let _ = writeln!(out, "{indent}        guard let {property} = {read} else {{ return nil }}");
            let _ = writeln!(out, "{indent}        self.{property} = {property}");
        } else {
            let _ = writeln!(out, "{indent}        {property} = {read}");
        }
    }
    let _ = writeln!(out, "{indent}    }}\n{indent}}}");
}

fn kotlin(defs: &Defs, order: &[String]) -> String {
    let mut out = format!(
        "// {HEADER}\n\npackage app.motile.core\n\nimport kotlinx.serialization.json.JsonArray\nimport kotlinx.serialization.json.JsonElement\n\
         import kotlinx.serialization.json.JsonNull\nimport kotlinx.serialization.json.JsonObject\nimport kotlinx.serialization.json.JsonPrimitive\n\
         import kotlinx.serialization.json.booleanOrNull\nimport kotlinx.serialization.json.doubleOrNull\nimport kotlinx.serialization.json.longOrNull\n"
    );
    for name in order {
        out.push('\n');
        match &defs.defs[name] {
            Def::Struct { doc, fields } => kotlin_class(&mut out, defs, &type_name(name), doc, fields, "", None),
            Def::Unit { doc, variants } => {
                let enum_name = type_name(name);
                kotlin_doc(&mut out, doc, "");
                let _ = writeln!(out, "enum class {enum_name}(val raw: String) {{");
                let entries: Vec<String> =
                    variants.iter().map(|variant| format!("    {}(\"{}\")", variant.name, variant.json)).collect();
                let _ = writeln!(out, "{};\n", entries.join(",\n"));
                let _ = writeln!(out, "    companion object {{");
                let _ = writeln!(
                    out,
                    "        fun of(raw: String?): {enum_name}? = entries.firstOrNull {{ it.raw == raw }}"
                );
                out.push_str("    }\n}\n");
            }
            Def::Tagged { doc, tag, variants } => {
                let interface = type_name(name);
                kotlin_doc(&mut out, doc, "");
                let _ = writeln!(out, "sealed interface {interface} {{");
                for variant in variants {
                    if variant.fields.is_empty() {
                        kotlin_doc(&mut out, &variant.doc, "    ");
                        let _ = writeln!(out, "    data object {} : {interface}", variant.name);
                    } else {
                        kotlin_class(
                            &mut out,
                            defs,
                            &variant.name,
                            &variant.doc,
                            &variant.fields,
                            "    ",
                            Some(&interface),
                        );
                    }
                    out.push('\n');
                }
                let _ = writeln!(out, "    companion object {{");
                let _ = writeln!(out, "        fun from(json: JSON): {interface}? = when (json.string(\"{tag}\")) {{");
                for variant in variants {
                    let read = if variant.fields.is_empty() {
                        variant.name.clone()
                    } else {
                        format!("{}.from(json)", variant.name)
                    };
                    let _ = writeln!(out, "            \"{}\" -> {read}", variant.json);
                }
                out.push_str("            else -> null\n        }\n    }\n}\n");
            }
        }
    }
    out.push_str(KOTLIN_COLOR);
    out.push_str(KOTLIN_READERS);
    out
}

const SWIFT_COLOR: &str = r#"
/// A colour: the text's own ("current"), a token by name, a token's wash, a layer of the surface
/// it lies on, or `[r, g, b, a]`.
enum UiColor {
    case current
    case token(String)
    case wash(String)
    case surface(String)
    case rgba([Int])

    init?(json value: Any?) {
        switch value {
        case let name as String:
            self = name == "current" ? .current : .token(name)
        case let object as JSON:
            if let token = object["wash"] as? String {
                self = .wash(token)
            } else if let layer = object["surface"] as? String {
                self = .surface(layer)
            } else {
                return nil
            }
        case let channels as [NSNumber]:
            self = .rgba(channels.map(\.intValue))
        default:
            return nil
        }
    }
}
"#;

const KOTLIN_COLOR: &str = "
/** A colour: the text's own (\"current\"), a token by name, a token's wash, a layer of the surface it lies on, or `[r, g, b, a]`. */
sealed interface UiColor {
    data object Current : UiColor

    data class Token(val name: String) : UiColor

    data class Wash(val name: String) : UiColor

    data class Surface(val layer: String) : UiColor

    data class Rgba(val channels: List<Int>) : UiColor

    companion object {
        fun from(json: JsonElement?): UiColor? = when (json) {
            is JsonObject -> json[\"wash\"].uiOptString()?.let(::Wash) ?: json[\"surface\"].uiOptString()?.let(::Surface)
            is JsonArray -> Rgba(json.map { it.uiOptInt() ?: 0 })
            else -> json.uiOptString()?.let { if (it == \"current\") Current else Token(it) }
        }
    }
}
";

const KOTLIN_READERS: &str = "
private fun JsonElement?.uiPrimitive(): JsonPrimitive? = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }

private fun JsonElement?.uiOptString(): String? = uiPrimitive()?.takeIf { it.isString }?.content

private fun JsonElement?.uiOptBool(): Boolean? = uiPrimitive()?.booleanOrNull

private fun JsonElement?.uiOptDouble(): Double? = uiPrimitive()?.doubleOrNull

private fun JsonElement?.uiOptInt(): Int? = uiPrimitive()?.let { it.longOrNull?.toInt() ?: it.doubleOrNull?.toInt() }

private fun JsonElement?.uiList(): List<JsonElement> = (this as? JsonArray) ?: emptyList()

private fun JsonElement?.uiObject(): JSON? = this as? JsonObject
";

fn kotlin_doc(out: &mut String, doc: &[String], indent: &str) {
    match doc {
        [] => {}
        [line] => {
            let _ = writeln!(out, "{indent}/** {line} */");
        }
        lines => {
            let _ = writeln!(out, "{indent}/**");
            for line in lines {
                let _ = writeln!(out, "{indent} * {line}");
            }
            let _ = writeln!(out, "{indent} */");
        }
    }
}

fn kotlin_type(ty: &Ty) -> String {
    match ty {
        Ty::Str => "String".into(),
        Ty::Bool => "Boolean".into(),
        Ty::Float => "Float".into(),
        Ty::Double => "Double".into(),
        Ty::Int => "Int".into(),
        Ty::Opt(inner) => format!("{}?", kotlin_type(inner)),
        Ty::List(inner) => format!("kotlin.collections.List<{}>", kotlin_type(inner)),
        Ty::Named(name) => type_name(name),
        Ty::Json => "JsonElement".into(),
        Ty::Prose => "JSON".into(),
        Ty::Color => "UiColor".into(),
    }
}

/// Kotlin that reads `value`, a `JsonElement?`, as the type; nullable when reading it can fail.
fn kotlin_read(defs: &Defs, ty: &Ty, value: &str) -> String {
    match ty {
        Ty::Str => format!("{value}.uiOptString() ?: \"\""),
        Ty::Bool => format!("{value}.uiOptBool() ?: false"),
        Ty::Float => format!("{value}.uiOptDouble()?.toFloat() ?: 0f"),
        Ty::Double => format!("{value}.uiOptDouble() ?: 0.0"),
        Ty::Int => format!("{value}.uiOptInt() ?: 0"),
        Ty::Opt(inner) => match inner.as_ref() {
            Ty::Str => format!("{value}.uiOptString()"),
            Ty::Bool => format!("{value}.uiOptBool()"),
            Ty::Float => format!("{value}.uiOptDouble()?.toFloat()"),
            Ty::Double => format!("{value}.uiOptDouble()"),
            Ty::Int => format!("{value}.uiOptInt()"),
            Ty::Color => format!("UiColor.from({value})"),
            Ty::Named(name) => match &defs.defs[name] {
                Def::Unit { .. } => format!("{}.of({value}.uiOptString())", type_name(name)),
                _ => format!("{value}.uiObject()?.let {{ {}.from(it) }}", type_name(name)),
            },
            other => format!("{value}?.takeIf {{ it !is JsonNull }}?.let {{ {} }}", kotlin_read(defs, other, "it")),
        },
        Ty::List(inner) => {
            let map = if defs.failable(inner) { "mapNotNull" } else { "map" };
            format!("{value}.uiList().{map} {{ {} }}", kotlin_read(defs, inner, "it"))
        }
        Ty::Named(name) => match &defs.defs[name] {
            Def::Unit { variants, .. } => {
                format!("{}.of({value}.uiOptString()) ?: {}.{}", type_name(name), type_name(name), variants[0].name)
            }
            _ => format!("{}.from({value}.uiObject() ?: EmptyJSON)", type_name(name)),
        },
        Ty::Json => format!("{value} ?: JsonNull"),
        Ty::Prose => format!("{value}.uiObject() ?: EmptyJSON"),
        Ty::Color => format!("UiColor.from({value})"),
    }
}

fn kotlin_class(
    out: &mut String,
    defs: &Defs,
    name: &str,
    doc: &[String],
    fields: &[Field],
    indent: &str,
    parent: Option<&str>,
) {
    kotlin_doc(out, doc, indent);
    let _ = writeln!(out, "{indent}data class {name}(");
    for field in fields {
        kotlin_doc(out, &field.doc, &format!("{indent}    "));
        let _ = writeln!(out, "{indent}    val {}: {},", camel(&field.name), kotlin_type(&field.ty));
    }
    let parent = parent.map(|parent| format!(" : {parent}")).unwrap_or_default();
    let failable = defs.fields_failable(fields);
    let _ = writeln!(out, "{indent}){parent} {{");
    let _ = writeln!(out, "{indent}    companion object {{");
    let _ = writeln!(out, "{indent}        fun from(json: JSON): {name}{} {{", if failable { "?" } else { "" });
    let _ = writeln!(out, "{indent}            return {name}(");
    for field in fields {
        let read = if field.flatten {
            format!("{}.from(json)", kotlin_type(&field.ty))
        } else {
            kotlin_read(defs, &field.ty, &format!("json[\"{}\"]", field.json))
        };
        let read = if defs.failable(&field.ty) { format!("{read} ?: return null") } else { read };
        let _ = writeln!(out, "{indent}                {} = {read},", camel(&field.name));
    }
    let _ = writeln!(out, "{indent}            )\n{indent}        }}\n{indent}    }}\n{indent}}}");
}
