//! Collection / rotation runs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use passvalet_agent::playbook::PlaybookSet;
use passvalet_agent::provider::ProviderConfig;
use passvalet_agent::run::{RunControl, RunEvent, RunKind, RunOutcome, RunRequest, Runner, SecretSink};
use passvalet_agent::{providers, ModelLadder};
use passvalet_core::model::{AuditEvent, NewSecret, SecretSource};
use passvalet_core::{services, Vault};
use serde::Serialize;
use tauri::{Emitter, Runtime};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize)]
pub struct RunInfo {
    pub run_id: String,
    pub service: String,
    pub kind: RunKind,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished: Option<RunOutcome>,
    pub events: Vec<RunEvent>,
}

struct Entry {
    info: RunInfo,
    control: RunControl,
    waiters: Vec<oneshot::Sender<RunOutcome>>,
}

#[derive(Default)]
pub struct RunRegistry {
    runs: Mutex<HashMap<String, Entry>>,
    pub playbooks: Mutex<Option<PlaybookSet>>,
}

impl RunRegistry {
    pub fn playbooks(&self) -> PlaybookSet {
        let mut g = self.playbooks.lock().unwrap();
        if g.is_none() {
            let dir = passvalet_core::paths::app_dir().join("playbooks");
            *g = Some(PlaybookSet::builtin().with_overrides(&dir));
        }
        g.clone().unwrap()
    }

    pub fn list(&self) -> Vec<RunInfo> {
        let mut v: Vec<RunInfo> = self.runs.lock().unwrap().values().map(|e| e.info.clone()).collect();
        v.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        v
    }

    #[allow(dead_code)]
    pub fn get(&self, run_id: &str) -> Option<RunInfo> {
        self.runs.lock().unwrap().get(run_id).map(|e| e.info.clone())
    }

    pub fn active(&self) -> Option<RunInfo> {
        self.runs
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.info.finished.is_none())
            .map(|e| e.info.clone())
            .next()
    }

    pub fn abort(&self, run_id: &str) -> bool {
        match self.runs.lock().unwrap().get(run_id) {
            Some(e) => {
                e.control.abort();
                true
            }
            None => false,
        }
    }

    pub fn resume(&self, run_id: &str) -> bool {
        match self.runs.lock().unwrap().get(run_id) {
            Some(e) => {
                e.control.resume();
                true
            }
            None => false,
        }
    }

    /// Wait for a run to finish (used by rotation over IPC).
    pub fn wait(&self, run_id: &str) -> Option<oneshot::Receiver<RunOutcome>> {
        let mut g = self.runs.lock().unwrap();
        let e = g.get_mut(run_id)?;
        let (tx, rx) = oneshot::channel();
        if let Some(out) = &e.info.finished {
            let _ = tx.send(out.clone());
        } else {
            e.waiters.push(tx);
        }
        Some(rx)
    }

    fn record(&self, ev: &RunEvent) {
        let run_id = match ev {
            RunEvent::Started { run_id, .. }
            | RunEvent::Thought { run_id, .. }
            | RunEvent::Step { run_id, .. }
            | RunEvent::Captured { run_id, .. }
            | RunEvent::Escalated { run_id, .. }
            | RunEvent::NeedUser { run_id, .. }
            | RunEvent::Finished { run_id, .. } => run_id.clone(),
        };
        let mut g = self.runs.lock().unwrap();
        if let Some(e) = g.get_mut(&run_id) {
            if e.info.events.len() < 500 {
                e.info.events.push(ev.clone());
            }
            if let RunEvent::Finished { outcome, .. } = ev {
                e.info.finished = Some(outcome.clone());
                for w in e.waiters.drain(..) {
                    let _ = w.send(outcome.clone());
                }
            }
        }
    }

    pub fn clear_finished(&self) {
        self.runs.lock().unwrap().retain(|_, e| e.info.finished.is_none());
    }
}

/// Writes captured values into the vault.
pub struct VaultSink {
    pub vault: Arc<Mutex<Vault>>,
    pub source: SecretSource,
}

#[async_trait]
impl SecretSink for VaultSink {
    async fn store(
        &self,
        run_id: &str,
        service: &str,
        key_type: &str,
        value: &str,
        label: Option<String>,
    ) -> Result<bool, String> {
        let valid = services::value_matches_pattern(service, key_type, value);
        let mut metadata = serde_json::Map::new();
        metadata.insert("run_id".into(), serde_json::Value::String(run_id.to_string()));
        let mut v = self.vault.lock().unwrap();
        v.put_secret(NewSecret {
            service: service.to_string(),
            key_type: key_type.to_string(),
            value: value.to_string(),
            label,
            source: self.source,
            metadata,
        })
        .map_err(|e| e.to_string())?;
        Ok(valid)
    }
}

pub struct StartArgs {
    pub service: String,
    pub kind: RunKind,
    pub hints: Vec<String>,
    pub provider: ProviderConfig,
}

/// Kick off a run; events are forwarded to the UI as `run:event`.
pub fn start<R: Runtime>(
    app: tauri::AppHandle<R>,
    registry: Arc<RunRegistry>,
    vault: Arc<Mutex<Vault>>,
    executor: Arc<dyn passvalet_agent::BrowserExecutor>,
    args: StartArgs,
) -> Result<String, String> {
    let playbooks = registry.playbooks();
    let playbook = playbooks
        .get(&args.service)
        .cloned()
        .ok_or_else(|| format!("没有 {} 的采集剧本", args.service))?;
    if let RunKind::Rotate { key_type } = &args.kind {
        if !playbook.supports_rotation(key_type) {
            return Err(format!("{} 的 {} 不支持自动轮换", args.service, key_type));
        }
    }
    if registry.active().is_some() {
        return Err("已有一个采集任务在运行，请先等待其完成或中止".into());
    }
    if args.provider.api_key.is_none() && !args.provider.base_url.contains("127.0.0.1") && !args.provider.base_url.contains("localhost") {
        return Err("请先在设置中填写模型 API key（ZenMux）".into());
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    let source = match &args.kind {
        RunKind::Collect { .. } => SecretSource::Collected,
        RunKind::Rotate { .. } => SecretSource::Rotated,
    };
    {
        let v = vault.lock().unwrap();
        let ev = match &args.kind {
            RunKind::Collect { .. } => AuditEvent::CollectionStarted,
            RunKind::Rotate { .. } => AuditEvent::RotationStarted,
        };
        let _ = v.audit(ev, Some("passvalet-agent"), Some(&args.service), None, None, Some(&run_id));
    }

    let control = RunControl::default();
    registry.runs.lock().unwrap().insert(
        run_id.clone(),
        Entry {
            info: RunInfo {
                run_id: run_id.clone(),
                service: args.service.clone(),
                kind: args.kind.clone(),
                started_at: chrono::Utc::now(),
                finished: None,
                events: vec![],
            },
            control: control.clone(),
            waiters: vec![],
        },
    );

    let (tx, mut rx) = mpsc::channel::<RunEvent>(256);
    let provider = providers::build(&args.provider);
    let runner = Runner {
        provider,
        executor,
        sink: Arc::new(VaultSink {
            vault: vault.clone(),
            source,
        }),
        events: tx,
        control,
        ladder: ModelLadder::new(if args.provider.models.is_empty() {
            vec!["google/gemini-3.7-flash".into()]
        } else {
            args.provider.models.clone()
        }),
        max_tokens: 2048,
    };
    let req = RunRequest {
        run_id: run_id.clone(),
        service: args.service.clone(),
        kind: args.kind.clone(),
        playbook,
        hints: args.hints,
    };

    // event forwarder
    let reg2 = registry.clone();
    let app2 = app.clone();
    let vault2 = vault.clone();
    let service2 = args.service.clone();
    let kind2 = args.kind.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = rx.recv().await {
            reg2.record(&ev);
            if let RunEvent::Finished { outcome, .. } = &ev {
                let v = vault2.lock().unwrap();
                let (event, detail) = match (&kind2, outcome) {
                    (RunKind::Collect { .. }, RunOutcome::Success { .. }) => (AuditEvent::CollectionCompleted, "success"),
                    (RunKind::Collect { .. }, RunOutcome::Partial { .. }) => (AuditEvent::CollectionCompleted, "partial"),
                    (RunKind::Collect { .. }, RunOutcome::Aborted) => (AuditEvent::CollectionAborted, "aborted"),
                    (RunKind::Collect { .. }, RunOutcome::Failed { .. }) => (AuditEvent::CollectionAborted, "failed"),
                    (RunKind::Rotate { .. }, RunOutcome::Success { .. }) => (AuditEvent::RotationCompleted, "success"),
                    (RunKind::Rotate { .. }, _) => (AuditEvent::RotationFailed, "failed"),
                };
                let _ = v.audit(event, Some("passvalet-agent"), Some(&service2), None, None, Some(detail));
            }
            let _ = app2.emit("run:event", &ev);
        }
    });

    tauri::async_runtime::spawn(async move {
        let _ = runner.run(req).await;
    });
    Ok(run_id)
}
