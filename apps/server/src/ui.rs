//! What the server keeps and tells of the programs agents show in their replies: the state the
//! user leaves them in, and what a message sends the agent of them.

use motile_protocol::wire::{Item, ProgramState};
use serde_json::{Value, json};

/// How much of a program's state the server keeps, as JSON.
pub const MAX_STATE: usize = 256 * 1024;
const CONTEXT: &str = "context and form values = ";

/// A message as the agent reads it: its text, then what its program sent with it and the state of
/// each program the user changed, a line each.
pub fn told(text: &str, context: Option<&Value>, programs: &[ProgramState]) -> String {
    let changed = programs
        .iter()
        .map(|program| json!({ "item": program.item_id, "block": program.block, "state": program.state }));
    let mut told = text.to_string();
    for value in context.cloned().into_iter().chain(changed) {
        if !told.is_empty() {
            told.push('\n');
        }
        told.push_str(CONTEXT);
        told.push_str(&value.to_string());
    }
    told
}

/// The state of every program of these items, a block at a time.
pub fn states(items: &[Item]) -> Vec<ProgramState> {
    let states = items.iter().flat_map(|item| {
        item.ui.iter().map(|(block, state)| ProgramState {
            item_id: item.id.clone(),
            block: *block,
            state: state.clone(),
        })
    });
    states.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_tells_what_its_program_sent_then_each_changed_program() {
        let context = json!(["User clicked: Book it", { "people": 4 }, { "table": 7 }]);
        let changed = ProgramState { item_id: "a1".into(), block: 2, state: json!({ "$days": "30" }) };

        let told = told("Book it", Some(&context), &[changed]);

        assert_eq!(
            told,
            "Book it\n\
             context and form values = [\"User clicked: Book it\",{\"people\":4},{\"table\":7}]\n\
             context and form values = {\"block\":2,\"item\":\"a1\",\"state\":{\"$days\":\"30\"}}"
        );
        assert_eq!(super::told("Hi", None, &[]), "Hi");
        assert_eq!(super::told("", Some(&json!(1)), &[]), "context and form values = 1");
    }

    #[test]
    fn every_block_of_an_item_has_its_own_state() {
        let mut item: Item = serde_json::from_value(json!({
            "id": "a1", "seq": 1, "rev": 2, "created_at": 0.0, "type": "assistant", "text": "",
        }))
        .unwrap();
        item.ui.insert(3, json!({ "x": 1 }));
        item.ui.insert(1, json!(null));

        let blocks: Vec<u32> = states(&[item]).iter().map(|state| state.block).collect();

        assert_eq!(blocks, [1, 3]);
    }
}
