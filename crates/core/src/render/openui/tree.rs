//! A program's tree as the clients draw it. For now each node is its component with its props as
//! the program computed them; the resolution of each component replaces this.

use motile_openui::{Element, Step, Value};
use serde::Serialize;
use serde_json::{Map, json};

use super::{ResolveCtx, child, node_id};

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct UiNode {
    pub id: String,
    pub component: String,
    /// What a bound control shows is the value the user left it at; a component inside a prop is
    /// written `{"node": id}`, and is one of the children.
    pub props: serde_json::Value,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<UiNode>,
}

pub fn resolve(root: &Element, ctx: &ResolveCtx) -> UiNode {
    node(root, &[], ctx)
}

fn node(element: &Element, path: &[Step], ctx: &ResolveCtx) -> UiNode {
    let mut children = Vec::new();
    let mut props = Map::new();
    for (name, value) in &element.props {
        let shown = ctx.runtime.value_at(path, name).unwrap_or_else(|| value.clone());
        props.insert(name.clone(), prop(&shown, &child(path, Step::Key(name.clone())), ctx, &mut children));
    }
    if let Some(error) = ctx.runtime.error_at(path) {
        props.insert("error".into(), json!(error));
    }
    let text = |name: &str| element.prop(name).and_then(Value::as_str);
    let images =
        |query: &str| ctx.images(query).map(|found| found.iter().map(|media| media.id.clone()).collect::<Vec<_>>());
    match element.component.as_str() {
        "SearchImage" => {
            props.insert("media".into(), json!(images(text("query").unwrap_or_default())));
        }
        "Image" if ctx.node(path).is_some_and(|node| node.fallback) => {
            props.insert("media".into(), json!(images(text("alt").unwrap_or_default())));
        }
        "Map" if element.prop("route").and_then(Value::as_bool) == Some(true) => {
            let markers = element.prop("markers").and_then(Value::as_array).unwrap_or_default();
            let stops: Vec<[f64; 2]> = markers
                .iter()
                .filter_map(|marker| {
                    let marker = marker.as_element()?;
                    Some([marker.prop("lat")?.as_number()?, marker.prop("lng")?.as_number()?])
                })
                .collect();
            props.insert("route".into(), json!(ctx.route(&stops)));
        }
        _ => {}
    }
    UiNode { id: node_id(path), component: element.component.clone(), props: props.into(), children }
}

fn prop(value: &Value, path: &[Step], ctx: &ResolveCtx, children: &mut Vec<UiNode>) -> serde_json::Value {
    match value {
        Value::Element(element) => {
            let resolved = node(element, path, ctx);
            let reference = json!({ "node": resolved.id });
            children.push(resolved);
            reference
        }
        Value::Plan(_) => json!("action"),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| prop(item, &child(path, Step::Index(index)), ctx, children))
            .collect(),
        Value::Object(map) => map
            .iter()
            .map(|(key, item)| (key.clone(), prop(item, &child(path, Step::Key(key.clone())), ctx, children)))
            .collect::<Map<_, _>>()
            .into(),
        value => value.to_json(),
    }
}
