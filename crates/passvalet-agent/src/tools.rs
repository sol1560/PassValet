//! Model-agnostic browser tool definitions (JSON Schema). Mirrors the shape of Anthropic's
//! browser toolset so prompts and behaviours transfer, but works with any tool-calling model.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

fn tool(name: &str, description: &str, parameters: Value) -> ToolDef {
    ToolDef {
        name: name.into(),
        description: description.into(),
        parameters,
    }
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

/// Tools executed by the browser extension.
pub const BROWSER_TOOLS: &[&str] = &[
    "navigate",
    "read_page",
    "find",
    "get_page_text",
    "left_click",
    "double_click",
    "hover",
    "type",
    "key",
    "form_input",
    "scroll",
    "scroll_to",
    "wait",
    "list_tabs",
    "switch_tab",
    "capture_secret",
];

/// Tools handled by the agent loop itself.
pub const DONE: &str = "done";
pub const FAIL: &str = "fail";
pub const NEED_USER: &str = "need_user";

pub fn is_browser_tool(name: &str) -> bool {
    BROWSER_TOOLS.contains(&name)
}

pub fn all_tools() -> Vec<ToolDef> {
    let target_ref = json!({ "type": "string", "description": "Element reference from read_page/find, e.g. \"ref_12\"." });
    let coordinate = json!({ "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2, "description": "[x, y] viewport pixels from the last screenshot." });
    let tab_id = json!({ "type": "string", "description": "Tab id; defaults to the current tab." });

    vec![
        tool(
            "navigate",
            "Load a URL in the current tab, or \"back\" / \"forward\" / \"reload\".",
            obj(json!({ "url": { "type": "string" }, "tab_id": tab_id }), &["url"]),
        ),
        tool(
            "read_page",
            "Return the page's accessibility tree as indented text. Each interactive element is tagged like [ref_3] so you can act on it. Use filter=\"interactive\" to see only buttons/links/inputs (cheapest). Secret values are masked.",
            obj(
                json!({
                    "filter": { "type": "string", "enum": ["interactive", "all"], "description": "interactive = only visible interactive elements; all = every visible element (default)." },
                    "depth": { "type": "integer", "minimum": 1, "maximum": 30 },
                    "ref": { "type": "string", "description": "Restrict to this element's subtree." },
                    "tab_id": tab_id
                }),
                &[],
            ),
        ),
        tool(
            "find",
            "Search the page for elements matching a natural-language description (e.g. \"reveal service_role key button\"). Returns up to 20 tagged matches.",
            obj(json!({ "query": { "type": "string" }, "tab_id": tab_id }), &["query"]),
        ),
        tool(
            "get_page_text",
            "Return the visible text of the page (secrets masked). Good for reading instructions or error banners.",
            obj(json!({ "tab_id": tab_id }), &[]),
        ),
        tool(
            "left_click",
            "Click an element by ref (preferred) or by coordinate.",
            obj(json!({ "ref": target_ref, "coordinate": coordinate, "tab_id": tab_id }), &[]),
        ),
        tool(
            "double_click",
            "Double-click an element by ref or coordinate.",
            obj(json!({ "ref": target_ref, "coordinate": coordinate, "tab_id": tab_id }), &[]),
        ),
        tool(
            "hover",
            "Move the pointer over an element by ref or coordinate (reveals hover menus).",
            obj(json!({ "ref": target_ref, "coordinate": coordinate, "tab_id": tab_id }), &[]),
        ),
        tool(
            "type",
            "Type text at the current focus (click an input first).",
            obj(json!({ "text": { "type": "string" }, "tab_id": tab_id }), &["text"]),
        ),
        tool(
            "key",
            "Press a key or chord: \"Enter\", \"Escape\", \"Tab\", \"cmd+a\", \"ctrl+c\".",
            obj(json!({ "text": { "type": "string" }, "repeat": { "type": "integer", "minimum": 1, "maximum": 20 }, "tab_id": tab_id }), &["text"]),
        ),
        tool(
            "form_input",
            "Set an input/select/checkbox value directly by ref.",
            obj(json!({ "ref": target_ref, "value": { "type": ["string", "number", "boolean"] }, "tab_id": tab_id }), &["ref", "value"]),
        ),
        tool(
            "scroll",
            "Scroll the page (or the element under a coordinate).",
            obj(json!({ "direction": { "type": "string", "enum": ["up", "down", "left", "right"] }, "amount": { "type": "integer", "minimum": 1, "maximum": 10, "description": "wheel notches, default 3" }, "coordinate": coordinate, "tab_id": tab_id }), &["direction"]),
        ),
        tool(
            "scroll_to",
            "Scroll an element into view by ref.",
            obj(json!({ "ref": target_ref, "tab_id": tab_id }), &["ref"]),
        ),
        tool(
            "wait",
            "Wait for the page to settle (seconds, max 10).",
            obj(json!({ "seconds": { "type": "number", "minimum": 0.2, "maximum": 10 } }), &["seconds"]),
        ),
        tool("list_tabs", "List tabs that belong to this run.", obj(json!({}), &[])),
        tool(
            "switch_tab",
            "Make another run tab current.",
            obj(json!({ "tab_id": { "type": "string" } }), &["tab_id"]),
        ),
        tool(
            "capture_secret",
            "Securely capture a key value WITHOUT seeing it. Give the ref of the element that displays the key (text, input, code block) OR source=\"clipboard\" right after clicking a Copy button. PassValet stores it and tells you whether the format matched. Use this instead of reading the value.",
            obj(
                json!({
                    "key_type": { "type": "string", "description": "Which key this is, e.g. anon_key, service_role_key, secret_key, api_key." },
                    "ref": { "type": "string", "description": "Element showing the value." },
                    "source": { "type": "string", "enum": ["element", "clipboard"], "description": "Default element." },
                    "label": { "type": "string", "description": "Optional human label (key name shown in the dashboard)." },
                    "tab_id": tab_id
                }),
                &["key_type"],
            ),
        ),
        tool(
            DONE,
            "Finish the task. Call when every requested key has been captured (or you have captured all that exist).",
            obj(json!({ "summary": { "type": "string" } }), &["summary"]),
        ),
        tool(
            FAIL,
            "Give up: the task cannot be completed (e.g. permission denied, feature not present).",
            obj(json!({ "reason": { "type": "string" } }), &["reason"]),
        ),
        tool(
            NEED_USER,
            "Pause and ask the user to do something you cannot (log in, pass 2FA/captcha, choose between projects). The run continues after they click Continue.",
            obj(json!({ "message": { "type": "string" } }), &["message"]),
        ),
    ]
}
