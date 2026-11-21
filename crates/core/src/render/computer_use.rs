//! How a row says what an agent did on the server's screen through the computer use driver.

use serde_json::Value;

/// What the agents know the driver's MCP server by, which starts its tools' names.
pub const TOOL_PREFIX: &str = "mcp__cua-driver__";
/// The longest typed text a row quotes.
const QUOTED_CHARS: usize = 60;

/// What the call does while it runs and what it did, and the value it did it with: an app, keys,
/// text, a menu path or an address, which rows show as code. Phrases go in the verb.
pub struct Described {
    pub doing: String,
    pub did: String,
    pub target: String,
}

pub fn describe(tool: &str, input: &Value) -> Described {
    let text = |key: &str| input[key].as_str().unwrap_or_default().trim().to_string();
    let strings =
        |key: &str| -> Vec<&str> { input[key].as_array().into_iter().flatten().filter_map(Value::as_str).collect() };
    let said =
        |doing: &str, did: &str, target: String| Described { doing: doing.to_string(), did: did.to_string(), target };
    let phrase = |doing: &str, did: &str| said(doing, did, String::new());
    match tool {
        "get_window_state" => phrase("Looking at a window", "Looked at a window"),
        "get_desktop_state" => phrase("Looking at the screen", "Looked at the screen"),
        "zoom" => phrase("Zooming into a window", "Zoomed into a window"),
        "verify_state" => phrase("Checking a window", "Checked a window"),
        "list_apps" => phrase("Listing the apps", "Listed the apps"),
        "list_windows" => match text("app") {
            app if app.is_empty() => phrase("Listing the windows", "Listed the windows"),
            app => said("Listing the windows of", "Listed the windows of", app),
        },
        "launch_app" => match [text("name"), text("bundle_id")].into_iter().find(|name| !name.is_empty()) {
            Some(app) => said("Opening", "Opened", app),
            None => phrase("Opening an app", "Opened an app"),
        },
        "kill_app" => phrase("Quitting an app", "Quit an app"),
        "bring_to_front" => phrase("Bringing a window to the front", "Brought a window to the front"),
        "click" => phrase("Clicking in a window", "Clicked in a window"),
        "double_click" => phrase("Double-clicking in a window", "Double-clicked in a window"),
        "right_click" => phrase("Right-clicking in a window", "Right-clicked in a window"),
        "drag" => phrase("Dragging in a window", "Dragged in a window"),
        "move_cursor" => phrase("Moving the pointer", "Moved the pointer"),
        "scroll" => match text("direction").as_str() {
            direction @ ("up" | "down" | "left" | "right") => Described {
                doing: format!("Scrolling {direction}"),
                did: format!("Scrolled {direction}"),
                target: String::new(),
            },
            _ => phrase("Scrolling a window", "Scrolled a window"),
        },
        "type_text" | "browser_type" => said("Typing", "Typed", quoted(&text("text"))),
        "set_value" => said("Filling in", "Filled in", quoted(&text("value"))),
        "press_key" => {
            let keys: Vec<&str> = strings("modifiers").into_iter().chain(input["key"].as_str()).collect();
            said("Pressing", "Pressed", keys.join("+"))
        }
        "hotkey" => said("Pressing", "Pressed", strings("keys").join("+")),
        "invoke_menu" => said("Choosing", "Chose", strings("path").join(" › ")),
        "run_steps" => match input["steps"].as_array().map_or(0, Vec::len) {
            1 => phrase("Running a step", "Ran a step"),
            count => Described {
                doing: format!("Running {count} steps"),
                did: format!("Ran {count} steps"),
                target: String::new(),
            },
        },
        "run_script" => phrase("Running a script on the screen", "Ran a script on the screen"),
        "set_window_frame" => phrase("Moving a window", "Moved a window"),
        "clipboard_read" => phrase("Reading the clipboard", "Read the clipboard"),
        "clipboard_write" => phrase("Copying to the clipboard", "Copied to the clipboard"),
        "browser_navigate" => said("Opening", "Opened", text("url")),
        "browser_click" | "browser_pointer" | "browser_select" => {
            phrase("Clicking in a browser tab", "Clicked in a browser tab")
        }
        "get_browser_state" | "page" => phrase("Reading a browser tab", "Read a browser tab"),
        tool if tool.starts_with("browser_") => phrase("Using a browser tab", "Used a browser tab"),
        tool => said("Using the screen:", "Used the screen:", tool.to_string()),
    }
}

fn quoted(text: &str) -> String {
    let first = text.lines().next().unwrap_or_default();
    let mut shown: String = first.chars().take(QUOTED_CHARS).collect();
    if shown.len() < text.len() {
        shown.push('…');
    }
    format!("“{shown}”")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn did(tool: &str, input: Value) -> String {
        let described = describe(tool, &input);
        [described.did, described.target].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn each_action_on_the_screen_says_what_it_did() {
        assert_eq!(did("get_window_state", json!({"pid": 4, "window_id": 9})), "Looked at a window");
        assert_eq!(did("launch_app", json!({"bundle_id": "com.apple.Safari"})), "Opened com.apple.Safari");
        assert_eq!(did("launch_app", json!({"name": "Calculator"})), "Opened Calculator");
        assert_eq!(did("type_text", json!({"text": "hello world"})), "Typed “hello world”");
        assert_eq!(did("hotkey", json!({"keys": ["cmd", "s"]})), "Pressed cmd+s");
        assert_eq!(did("press_key", json!({"key": "return", "modifiers": ["shift"]})), "Pressed shift+return");
        assert_eq!(did("invoke_menu", json!({"path": ["File", "Save As…"]})), "Chose File › Save As…");
        assert_eq!(did("run_steps", json!({"steps": [{}, {}, {}]})), "Ran 3 steps");
        assert_eq!(did("scroll", json!({"direction": "down"})), "Scrolled down");
        assert_eq!(did("list_windows", json!({"app": "Finder"})), "Listed the windows of Finder");
        assert_eq!(did("future_tool", json!({})), "Used the screen: future_tool");
    }

    #[test]
    fn long_or_several_lines_of_typing_are_cut_short() {
        let typed = did("type_text", json!({"text": format!("{}\nsecond line", "a".repeat(80))}));
        assert_eq!(typed, format!("Typed “{}…”", "a".repeat(60)));
        assert_eq!(did("type_text", json!({"text": "one\ntwo"})), "Typed “one…”");
    }
}
