//! Method names. Kept as constants so the app, CLI and extension host agree.

/// Vault / agent-facing methods (served by the desktop app).
pub const STATUS: &str = "status";
pub const REQUEST_PERMISSIONS: &str = "request_permissions";
pub const GET_KEY: &str = "get_key";
pub const REPORT_KEY_INVALID: &str = "report_key_invalid";
pub const LIST_SERVICES: &str = "list_services";
pub const LIST_KEYS: &str = "list_keys";
pub const START_COLLECTION: &str = "start_collection";

/// Extension bridge: sent by the native host right after connecting.
pub const EXT_HELLO: &str = "ext.hello";
/// Extension → app notifications (tab closed, user pressed stop, …).
pub const EXT_EVENT: &str = "ext.event";
/// App → extension requests are `browser.<tool_name>`.
pub const BROWSER_PREFIX: &str = "browser.";
/// App → extension: begin/end a collection run (creates/cleans the tab group).
pub const BROWSER_SESSION_BEGIN: &str = "browser.session_begin";
pub const BROWSER_SESSION_END: &str = "browser.session_end";
/// App → extension: ping.
pub const BROWSER_PING: &str = "browser.ping";
