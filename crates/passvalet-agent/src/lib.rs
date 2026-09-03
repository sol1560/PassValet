//! Collection agent.
//!
//! The model drives a browser through a small, model-agnostic tool set (a11y-tree reads by
//! `ref`, clicks, typing, screenshots) executed by the Chrome extension. Secrets never reach the
//! model: `capture_secret` extracts them on the extension side and only a masked preview is
//! returned to the conversation. Every tool output is passed through [`redact`] as a second line
//! of defence.

pub mod executor;
pub mod ladder;
pub mod playbook;
pub mod provider;
pub mod providers;
pub mod redact;
pub mod run;
pub mod tools;

pub use executor::{BrowserExecutor, ToolOutput};
pub use ladder::ModelLadder;
pub use playbook::{Playbook, PlaybookSet};
pub use provider::{LlmProvider, ProviderConfig, ProviderKind};
pub use run::{RunEvent, RunKind, RunOutcome, RunRequest, Runner, SecretSink};
