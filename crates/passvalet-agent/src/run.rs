//! The agent loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, Notify};

use crate::executor::{BrowserExecutor, ExecutorError, ToolOutput};
use crate::ladder::ModelLadder;
use crate::playbook::Playbook;
use crate::provider::*;
use crate::redact;
use crate::tools;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunKind {
    Collect { key_types: Vec<String> },
    Rotate { key_type: String },
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub run_id: String,
    pub service: String,
    pub kind: RunKind,
    pub playbook: Playbook,
    /// Extra context (e.g. project name the user picked).
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEvent {
    Started {
        run_id: String,
        service: String,
        model: String,
    },
    Thought {
        run_id: String,
        text: String,
    },
    Step {
        run_id: String,
        step: u32,
        tool: String,
        args: String,
        result: String,
        is_error: bool,
    },
    Captured {
        run_id: String,
        key_type: String,
        fingerprint: String,
        valid: bool,
    },
    Escalated {
        run_id: String,
        from: String,
        to: String,
    },
    NeedUser {
        run_id: String,
        message: String,
    },
    Finished {
        run_id: String,
        outcome: RunOutcome,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RunOutcome {
    Success {
        captured: Vec<String>,
        summary: String,
    },
    Partial {
        captured: Vec<String>,
        missing: Vec<String>,
        reason: String,
    },
    Failed {
        reason: String,
    },
    Aborted,
}

/// Where captured values go (the vault). Returns whether the value matched the expected format.
#[async_trait]
pub trait SecretSink: Send + Sync {
    async fn store(
        &self,
        run_id: &str,
        service: &str,
        key_type: &str,
        value: &str,
        label: Option<String>,
    ) -> Result<bool, String>;
}

/// Shared control handles for a run.
#[derive(Clone, Default)]
pub struct RunControl {
    abort: Arc<AtomicBool>,
    resume: Arc<Notify>,
}

impl RunControl {
    pub fn abort(&self) {
        self.abort.store(true, Ordering::SeqCst);
        self.resume.notify_waiters();
    }
    pub fn is_aborted(&self) -> bool {
        self.abort.load(Ordering::SeqCst)
    }
    /// User clicked "Continue" after a `need_user` pause.
    pub fn resume(&self) {
        self.resume.notify_waiters();
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.resume.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_aborted() {
                return;
            }
            notified.await;
        }
    }
}

pub struct Runner {
    pub provider: Arc<dyn LlmProvider>,
    pub executor: Arc<dyn BrowserExecutor>,
    pub sink: Arc<dyn SecretSink>,
    pub events: mpsc::Sender<RunEvent>,
    pub control: RunControl,
    pub ladder: ModelLadder,
    pub max_tokens: u32,
}

const GENERAL_RULES: &str = r#"You are PassValet's collection agent operating inside the user's own, already signed-in browser tab. Your job is to navigate a SaaS dashboard and capture API keys into the user's local vault.

Hard rules:
- You never see key values. To save a key, call `capture_secret` with the ref of the element that displays it (or source="clipboard" right after clicking a Copy button). Values in page text are shown to you masked as [REDACTED …].
- Prefer `read_page` with filter="interactive" to orient yourself; use `find` to locate specific controls. Screenshots are unavailable because they may expose key values. If page controls cannot be identified from text, ask the user for help.
- Act on elements by ref. After navigation or clicks that change the page, re-read the page before acting again — refs are invalidated by navigation.
- Do not change account settings, billing, or delete anything unless the task explicitly says to rotate a key. Never create resources other than API keys/tokens.
- If you land on a login page, a 2FA prompt, a captcha, or an org/project chooser you cannot resolve from the task hints, call `need_user` with a short instruction for the user, then continue after they return.
- If the dashboard shows a key only once at creation time, make sure you capture it before closing the dialog.
- When every requested key is captured (or you have verified some do not exist), call `done`. If blocked, call `fail` with the reason.
- Be efficient: batch reasoning, avoid redundant navigation, and stop as soon as the task is complete."#;

impl Runner {
    fn system_prompt(&self, req: &RunRequest) -> String {
        let (phase, task) = match &req.kind {
            RunKind::Collect { key_types } => (
                req.playbook.collect.instructions.clone(),
                format!(
                    "TASK: collect these {} keys: {}. Save each with `capture_secret` using exactly these key_type ids.",
                    req.playbook.label,
                    key_types.join(", ")
                ),
            ),
            RunKind::Rotate { key_type } => (
                req.playbook
                    .rotate
                    .as_ref()
                    .map(|r| r.instructions.clone())
                    .unwrap_or_default(),
                format!(
                    "TASK: rotate the {} key `{}`: create a replacement key, capture it with `capture_secret` (key_type=\"{}\"), then revoke/delete the OLD key only after the new one is captured. Never delete the newly created key.",
                    req.playbook.label, key_type, key_type
                ),
            ),
        };
        let mut s = String::new();
        s.push_str(GENERAL_RULES);
        s.push_str("\n\n# Service playbook: ");
        s.push_str(&req.playbook.label);
        s.push('\n');
        s.push_str(&phase);
        s.push_str("\n\n# ");
        s.push_str(&task);
        if !req.hints.is_empty() {
            s.push_str("\n\nUser hints:\n");
            for h in &req.hints {
                s.push_str("- ");
                s.push_str(h);
                s.push('\n');
            }
        }
        s
    }

    async fn emit(&self, ev: RunEvent) {
        let _ = self.events.send(ev).await;
    }

    pub async fn run(mut self, req: RunRequest) -> RunOutcome {
        // 通用点击工具无法强制先保存新值、再精确撤销旧值。
        // 在实现服务专用的安全撤销路径前，不得把轮换交给模型执行。
        if matches!(req.kind, RunKind::Rotate { .. }) {
            let outcome = RunOutcome::Failed {
                reason: "暂不支持安全自动轮换：尚无法保证新密钥保存后只撤销对应的旧密钥。未操作浏览器，请手动轮换。".into(),
            };
            self.emit(RunEvent::Finished {
                run_id: req.run_id,
                outcome: outcome.clone(),
            })
            .await;
            return outcome;
        }
        let outcome = self.run_inner(&req).await;
        // Destroy everything on the extension side regardless of outcome.
        let keep_tabs = matches!(
            outcome,
            RunOutcome::Failed { .. } | RunOutcome::Partial { .. }
        );
        if let Err(e) = self.executor.session_end(&req.run_id, keep_tabs).await {
            tracing::warn!("session_end failed: {e}");
        }
        self.emit(RunEvent::Finished {
            run_id: req.run_id.clone(),
            outcome: outcome.clone(),
        })
        .await;
        outcome
    }

    async fn run_inner(&mut self, req: &RunRequest) -> RunOutcome {
        let run_id = req.run_id.clone();
        let wanted: Vec<String> = match &req.kind {
            RunKind::Collect { key_types } => key_types.clone(),
            RunKind::Rotate { key_type } => vec![key_type.clone()],
        };
        let max_steps = match &req.kind {
            RunKind::Collect { .. } => req.playbook.max_steps,
            RunKind::Rotate { .. } => req
                .playbook
                .rotate
                .as_ref()
                .map(|r| r.max_steps)
                .unwrap_or(req.playbook.max_steps),
        };

        self.emit(RunEvent::Started {
            run_id: run_id.clone(),
            service: req.service.clone(),
            model: self.ladder.current().to_string(),
        })
        .await;

        let tab = match self
            .executor
            .session_begin(&run_id, &req.playbook.start_url)
            .await
        {
            Ok(t) => t,
            Err(ExecutorError::Aborted) => return RunOutcome::Aborted,
            Err(e) => {
                return RunOutcome::Failed {
                    reason: format!("could not open browser tab: {e}"),
                }
            }
        };

        let system = self.system_prompt(req);
        let tool_defs = tools::all_tools();
        let mut messages: Vec<Message> = vec![Message::user_text(format!(
            "The browser tab {tab} is open at {}. Begin. Start by reading the page.",
            req.playbook.start_url
        ))];
        let mut captured: Vec<String> = Vec::new();
        let mut step: u32 = 0;
        let mut idle_rounds = 0u32;

        loop {
            if self.control.is_aborted() {
                return RunOutcome::Aborted;
            }
            if step >= max_steps {
                return self.partial(
                    &captured,
                    &wanted,
                    format!("step budget ({max_steps}) exhausted"),
                );
            }

            let creq = CompletionRequest {
                model: self.ladder.current().to_string(),
                system: system.clone(),
                messages: messages.clone(),
                tools: tool_defs.clone(),
                max_tokens: self.max_tokens,
            };
            let response = tokio::select! {
                biased;
                _ = self.control.cancelled() => return RunOutcome::Aborted,
                response = self.provider.complete(&creq) => response,
            };
            let resp = match response {
                Ok(r) => r,
                Err(e) if e.is_retryable() => {
                    tracing::warn!("provider error: {e}");
                    if let Some(next) = self.ladder.force_escalate() {
                        self.emit(RunEvent::Escalated {
                            run_id: run_id.clone(),
                            from: creq.model.clone(),
                            to: next,
                        })
                        .await;
                        continue;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    step += 1;
                    continue;
                }
                Err(e) => {
                    return self.partial(&captured, &wanted, format!("model error: {e}"));
                }
            };

            if let Some(t) = &resp.text {
                self.emit(RunEvent::Thought {
                    run_id: run_id.clone(),
                    text: redact::redact(t),
                })
                .await;
            }

            if resp.tool_calls.is_empty() {
                idle_rounds += 1;
                messages.push(Message::assistant(resp.text.clone(), vec![]));
                if idle_rounds >= 3 {
                    return self.partial(&captured, &wanted, "model stopped calling tools".into());
                }
                messages.push(Message::user_text(
                    "Continue using tools. When finished call `done`; if blocked call `fail` or `need_user`.",
                ));
                step += 1;
                continue;
            }
            idle_rounds = 0;
            messages.push(Message::assistant(
                resp.text.clone(),
                resp.tool_calls.clone(),
            ));

            let mut results: Vec<ToolResult> = Vec::new();
            let mut step_had_error = false;
            let mut terminal: Option<RunOutcome> = None;

            for tc in &resp.tool_calls {
                if step >= max_steps {
                    return self.partial(
                        &captured,
                        &wanted,
                        format!("step budget ({max_steps}) exhausted"),
                    );
                }
                step += 1;
                if self.control.is_aborted() {
                    return RunOutcome::Aborted;
                }
                let args_summary = summarize_args(&tc.arguments);

                match tc.name.as_str() {
                    tools::DONE => {
                        let summary = tc.arguments["summary"].as_str().unwrap_or("").to_string();
                        let missing: Vec<String> = wanted
                            .iter()
                            .filter(|k| !captured.contains(k))
                            .cloned()
                            .collect();
                        terminal = Some(if missing.is_empty() {
                            RunOutcome::Success {
                                captured: captured.clone(),
                                summary,
                            }
                        } else {
                            RunOutcome::Partial {
                                captured: captured.clone(),
                                missing,
                                reason: summary,
                            }
                        });
                        break;
                    }
                    tools::FAIL => {
                        let reason = tc.arguments["reason"].as_str().unwrap_or("").to_string();
                        terminal = Some(self.partial(&captured, &wanted, reason));
                        break;
                    }
                    tools::NEED_USER => {
                        let message = tc.arguments["message"].as_str().unwrap_or("").to_string();
                        // Register before publishing the pause so an immediate reply is not lost.
                        let resume = self.control.resume.clone();
                        let notified = resume.notified();
                        tokio::pin!(notified);
                        notified.as_mut().enable();
                        self.emit(RunEvent::NeedUser {
                            run_id: run_id.clone(),
                            message: message.clone(),
                        })
                        .await;
                        self.emit(RunEvent::Step {
                            run_id: run_id.clone(),
                            step,
                            tool: tc.name.clone(),
                            args: args_summary,
                            result: "waiting for user".into(),
                            is_error: false,
                        })
                        .await;
                        // Wait for resume or abort.
                        if self.control.is_aborted() {
                            return RunOutcome::Aborted;
                        }
                        notified.await;
                        if self.control.is_aborted() {
                            return RunOutcome::Aborted;
                        }
                        results.push(ToolResult {
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            content: vec![ContentPart::Text(
                                "The user has completed the step. Re-read the page and continue."
                                    .into(),
                            )],
                            is_error: false,
                        });
                    }
                    "capture_secret" => {
                        let key_type = tc.arguments["key_type"].as_str().unwrap_or("").to_string();
                        let label = tc.arguments["label"].as_str().map(|s| s.to_string());
                        let out = if wanted.contains(&key_type) {
                            self.executor
                                .call(&run_id, "capture_secret", tc.arguments.clone())
                                .await
                        } else {
                            Ok(ToolOutput::error("key_type is not part of this request"))
                        };
                        let (text, is_error) = match out {
                            Ok(ToolOutput {
                                secret: Some(value),
                                ..
                            }) if !value.trim().is_empty() => {
                                let value = value.trim().to_string();
                                match self
                                    .sink
                                    .store(&run_id, &req.service, &key_type, &value, label)
                                    .await
                                {
                                    Ok(true) => {
                                        if !captured.contains(&key_type) {
                                            captured.push(key_type.clone());
                                        }
                                        self.emit(RunEvent::Captured {
                                            run_id: run_id.clone(),
                                            key_type: key_type.clone(),
                                            fingerprint: redact::mask(&value),
                                            valid: true,
                                        })
                                        .await;
                                        let remaining: Vec<&String> =
                                            wanted.iter().filter(|k| !captured.contains(k)).collect();
                                        (
                                            format!(
                                                "Captured {key_type} ({} chars, preview {}), format OK. Remaining: {}",
                                                value.chars().count(),
                                                redact::mask(&value),
                                                if remaining.is_empty() { "none — call done".to_string() } else { remaining.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ") }
                                            ),
                                            false,
                                        )
                                    }
                                    Ok(false) => ("Value rejected: format does not match the expected pattern. Check the element and try again.".into(), true),
                                    Err(e) => (format!("vault refused the value: {e}"), true),
                                }
                            }
                            Ok(o) => (
                                if o.text.is_empty() {
                                    "no value found at that ref (empty). Reveal the key first or use source=\"clipboard\" after clicking Copy.".to_string()
                                } else {
                                    redact::redact(&o.text)
                                },
                                true,
                            ),
                            Err(ExecutorError::Aborted) => return RunOutcome::Aborted,
                            Err(e) => (format!("capture failed: {e}"), true),
                        };
                        if is_error {
                            step_had_error = true;
                        }
                        self.emit(RunEvent::Step {
                            run_id: run_id.clone(),
                            step,
                            tool: tc.name.clone(),
                            args: args_summary,
                            result: text.clone(),
                            is_error,
                        })
                        .await;
                        results.push(ToolResult {
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            content: vec![ContentPart::Text(text)],
                            is_error,
                        });
                    }
                    name if tools::is_browser_tool(name) => {
                        let out = self
                            .executor
                            .call(&run_id, name, tc.arguments.clone())
                            .await;
                        let (content, text, is_error) = match out {
                            Ok(o) => {
                                let mut text = o.text;
                                if let Some(bs) = &o.browser_state {
                                    text.push_str(&format!(
                                        "\n[tab {} · {} · {}]",
                                        bs.tab_id, bs.title, bs.url
                                    ));
                                }
                                let text = redact::redact(&text);
                                // Browser images have not been redacted. Never forward them, even
                                // when an executor unexpectedly attaches one to a text tool.
                                let content = vec![ContentPart::Text(text.clone())];
                                (content, text, o.is_error)
                            }
                            Err(ExecutorError::Aborted) => return RunOutcome::Aborted,
                            Err(e) => {
                                let t = format!("{e}");
                                (vec![ContentPart::Text(t.clone())], t, true)
                            }
                        };
                        if is_error {
                            step_had_error = true;
                        }
                        self.emit(RunEvent::Step {
                            run_id: run_id.clone(),
                            step,
                            tool: tc.name.clone(),
                            args: args_summary,
                            result: truncate(&text, 400),
                            is_error,
                        })
                        .await;
                        results.push(ToolResult {
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            content,
                            is_error,
                        });
                    }
                    other => {
                        step_had_error = true;
                        results.push(ToolResult {
                            call_id: tc.id.clone(),
                            name: tc.name.clone(),
                            content: vec![ContentPart::Text(format!("unknown tool {other}"))],
                            is_error: true,
                        });
                    }
                }
            }

            if let Some(outcome) = terminal {
                return outcome;
            }

            if step_had_error {
                if let Some(next) = self.ladder.note_failure() {
                    self.emit(RunEvent::Escalated {
                        run_id: run_id.clone(),
                        from: creq.model.clone(),
                        to: next,
                    })
                    .await;
                }
            } else {
                self.ladder.note_success();
            }

            if !results.is_empty() {
                messages.push(Message::tool_results(results));
            }
            // Keep context bounded: drop old screenshots.
            prune_images(&mut messages, 2);
        }
    }

    fn partial(&self, captured: &[String], wanted: &[String], reason: String) -> RunOutcome {
        let missing: Vec<String> = wanted
            .iter()
            .filter(|k| !captured.contains(k))
            .cloned()
            .collect();
        if captured.is_empty() {
            RunOutcome::Failed { reason }
        } else {
            RunOutcome::Partial {
                captured: captured.to_vec(),
                missing,
                reason,
            }
        }
    }
}

fn summarize_args(v: &Value) -> String {
    let s = v.to_string();
    truncate(&redact::redact(&s), 200)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}…")
    }
}

/// Keep only the last `keep` images in the conversation to bound token usage.
fn prune_images(messages: &mut [Message], keep: usize) {
    let mut seen = 0usize;
    for m in messages.iter_mut().rev() {
        for r in m.tool_results.iter_mut().rev() {
            r.content.retain(|c| match c {
                ContentPart::Image { .. } => {
                    seen += 1;
                    seen <= keep
                }
                _ => true,
            });
            if r.content.is_empty() {
                r.content.push(ContentPart::Text(
                    "(screenshot removed to save context)".into(),
                ));
            }
        }
        m.content.retain(|c| match c {
            ContentPart::Image { .. } => {
                seen += 1;
                seen <= keep
            }
            _ => true,
        });
    }
}
