//! A program row's tree kept in step with the core's `ui_patch` events. A node is found by its id,
//! through the lists of nodes the core diffs (`children`, `items`, `inlines`), and only the nodes
//! on its way are copied.

use std::sync::Arc;

use motile_core::render::openui::UiOp;
use motile_core::render::openui::tree::{Nodes, UiKind, UiNode};
use serde_json::{Map, Value};

const LISTS: [&str; 3] = ["children", "items", "inlines"];

/// Applies the ops in order to the tree and the row's errors. An op the tree doesn't fit is
/// skipped.
pub fn apply(tree: &mut Option<Box<UiNode>>, errors: &mut Vec<String>, ops: &[UiOp]) {
    for op in ops {
        match op {
            UiOp::Root { node } => *tree = node.as_ref().and_then(read).map(Box::new),
            UiOp::Errors { errors: now } => errors.clone_from(now),
            op => {
                let Some(root) = tree.as_deref_mut() else { continue };
                if !apply_to(root, op) {
                    tracing::debug!("a program's patch doesn't fit its tree: {op:?}");
                }
            }
        }
    }
}

fn apply_to(root: &mut UiNode, op: &UiOp) -> bool {
    match op {
        UiOp::Replace { id, node } => {
            let Some(node) = read(node) else { return false };
            let Some(path) = path_to(root, id) else { return false };
            *at_mut(root, &path) = node;
            true
        }
        UiOp::Props { id, set, unset } => {
            let Some(path) = path_to(root, id) else { return false };
            let target = at_mut(root, &path);
            let Some(changed) = with_props(target, set, unset) else { return false };
            *target = changed;
            true
        }
        UiOp::Insert { parent, key, index, node } => {
            let Some(node) = read(node) else { return false };
            let Some(path) = path_to(root, parent) else { return false };
            let Some(list) = list_mut(&mut at_mut(root, &path).kind, key) else { return false };
            list.insert((*index).min(list.len()), Arc::new(node));
            true
        }
        UiOp::Remove { id } => {
            let Some(mut path) = path_to(root, id) else { return false };
            let Some((key, index)) = path.pop() else { return false };
            let Some(list) = list_mut(&mut at_mut(root, &path).kind, key) else { return false };
            list.remove(index);
            true
        }
        UiOp::Root { .. } | UiOp::Errors { .. } => true,
    }
}

fn read(node: &Value) -> Option<UiNode> {
    serde_json::from_value(node.clone()).inspect_err(|error| tracing::debug!("a program's node: {error}")).ok()
}

/// The node with its keys in `set` changed and those in `unset` gone. Its lists of nodes stay
/// as they are unless they are set, so only the node itself is written out and read again.
fn with_props(node: &UiNode, set: &Map<String, Value>, unset: &[String]) -> Option<UiNode> {
    let mut shallow = node.clone();
    let mut kept = Vec::new();
    for key in LISTS {
        if let Some(list) = list_mut(&mut shallow.kind, key) {
            kept.push((key, std::mem::take(list)));
        }
    }
    let mut value = serde_json::to_value(&shallow).ok()?;
    let object = value.as_object_mut()?;
    for key in unset {
        object.remove(key);
    }
    for (key, changed) in set {
        object.insert(key.clone(), changed.clone());
    }
    let mut changed = read(&value)?;
    for (key, list) in kept {
        if !set.contains_key(key)
            && let Some(slot) = list_mut(&mut changed.kind, key)
        {
            *slot = list;
        }
    }
    Some(changed)
}

/// Where the node `id` is: the list and the index of each step down to it from the root.
fn path_to(root: &UiNode, id: &str) -> Option<Vec<(&'static str, usize)>> {
    if root.id == id {
        return Some(Vec::new());
    }
    let within = |node: &UiNode| node.id.is_empty() || id.starts_with(&format!("{}/", node.id)) || node.id == id;
    for key in LISTS {
        let Some(list) = list(&root.kind, key) else { continue };
        for (index, child) in list.iter().enumerate() {
            if !within(child) {
                continue;
            }
            if let Some(mut rest) = path_to(child, id) {
                rest.insert(0, (key, index));
                return Some(rest);
            }
        }
    }
    None
}

/// The node at the end of `path`, copied on the way where it is shared.
fn at_mut<'a>(root: &'a mut UiNode, path: &[(&'static str, usize)]) -> &'a mut UiNode {
    let mut node = root;
    for (key, index) in path {
        let list = list_mut(&mut node.kind, key).expect("the path follows the tree");
        node = Arc::make_mut(&mut list[*index]);
    }
    node
}

fn list<'a>(kind: &'a UiKind, key: &str) -> Option<&'a Nodes> {
    match (kind, key) {
        (
            UiKind::Col { children, .. }
            | UiKind::Row { children, .. }
            | UiKind::Box { children, .. }
            | UiKind::Grid { children, .. }
            | UiKind::Card { children, .. }
            | UiKind::Carousel { children, .. }
            | UiKind::Pressable { children, .. }
            | UiKind::Form { children, .. }
            | UiKind::ListItem { children, .. },
            "children",
        ) => Some(children),
        (UiKind::List { items, .. }, "items") => Some(items),
        (UiKind::Text { inlines, .. }, "inlines") => Some(inlines),
        _ => None,
    }
}

fn list_mut<'a>(kind: &'a mut UiKind, key: &str) -> Option<&'a mut Nodes> {
    match (kind, key) {
        (
            UiKind::Col { children, .. }
            | UiKind::Row { children, .. }
            | UiKind::Box { children, .. }
            | UiKind::Grid { children, .. }
            | UiKind::Card { children, .. }
            | UiKind::Carousel { children, .. }
            | UiKind::Pressable { children, .. }
            | UiKind::Form { children, .. }
            | UiKind::ListItem { children, .. },
            "children",
        ) => Some(children),
        (UiKind::List { items, .. }, "items") => Some(items),
        (UiKind::Text { inlines, .. }, "inlines") => Some(inlines),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use motile_core::render::openui::diff;
    use serde_json::json;

    use super::*;

    fn golden(name: &str) -> Value {
        let path = format!("{}/../../crates/core/fixtures/openui/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn patched(before: &Value, after: &Value) -> Value {
        let mut ops = Vec::new();
        diff(Some(before), Some(after), &mut ops);
        let mut tree = Some(Box::new(serde_json::from_value::<UiNode>(before.clone()).unwrap()));
        apply(&mut tree, &mut Vec::new(), &ops);
        serde_json::to_value(tree.unwrap()).unwrap()
    }

    #[test]
    fn a_tree_patched_with_the_cores_diff_is_the_tree_it_was_diffed_to() {
        let text = golden("text");
        let mut changed = text.clone();
        changed["children"][1]["text"]["text"] = json!("Another line.");
        changed["children"].as_array_mut().unwrap().pop();
        changed["children"][6] = json!({"id": "children/6", "component": "Spacer"});
        changed["children"][3]["children"].as_array_mut().unwrap().push(json!({
            "id": "children/3/children/3", "component": "Divider"
        }));
        assert_eq!(patched(&text, &changed), changed);
        assert_eq!(patched(&changed, &text), text);
    }

    #[test]
    fn a_tree_grows_and_changes_its_component_through_patches() {
        let (layout, inputs) = (golden("layout"), golden("inputs"));
        assert_eq!(patched(&layout, &inputs), inputs);
        let mut empty = layout.clone();
        empty["children"] = json!([]);
        assert_eq!(patched(&empty, &layout), layout);
    }

    #[test]
    fn errors_and_a_new_root_replace_what_the_row_had() {
        let mut tree = Some(Box::new(serde_json::from_value::<UiNode>(golden("text")).unwrap()));
        let mut errors = Vec::new();
        let ops = vec![UiOp::Errors { errors: vec!["Unknown component".into()] }, UiOp::Root { node: None }];
        apply(&mut tree, &mut errors, &ops);
        assert!(tree.is_none());
        assert_eq!(errors, ["Unknown component"]);
    }
}
