use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use passvalet_agent::executor::{BrowserExecutor, ExecutorError, ToolOutput};
use passvalet_agent::ladder::ModelLadder;
use passvalet_agent::playbook::PlaybookSet;
use passvalet_agent::provider::*;
use passvalet_agent::run::{
    RunControl, RunEvent, RunKind, RunOutcome, RunRequest, Runner, SecretSink,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

#[derive(Default)]
struct RecordingProvider(Mutex<Vec<CompletionRequest>>);

#[async_trait]
impl LlmProvider for RecordingProvider {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let mut requests = self.0.lock().unwrap();
        requests.push(req.clone());
        let name = if requests.len() == 1 {
            "read_page"
        } else {
            "done"
        };
        Ok(CompletionResponse {
            tool_calls: vec![ToolCall {
                id: requests.len().to_string(),
                name: name.into(),
                arguments: json!({}),
            }],
            ..Default::default()
        })
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenaiCompat
    }
}

struct BrowserWithPrivateImage;

#[async_trait]
impl BrowserExecutor for BrowserWithPrivateImage {
    async fn session_begin(&self, _: &str, _: &str) -> Result<String, ExecutorError> {
        Ok("1".into())
    }
    async fn call(&self, _: &str, _: &str, _: Value) -> Result<ToolOutput, ExecutorError> {
        Ok(ToolOutput {
            text: "Settings button".into(),
            image_png_base64: Some("private-screenshot-bytes".into()),
            ..Default::default()
        })
    }
    async fn session_end(&self, _: &str, _: bool) -> Result<(), ExecutorError> {
        Ok(())
    }
}

struct NoStore;

#[async_trait]
impl SecretSink for NoStore {
    async fn store(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: Option<String>,
    ) -> Result<bool, String> {
        panic!("read-only test must not write secrets")
    }
}

struct PausingProvider;

#[async_trait]
impl LlmProvider for PausingProvider {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        Ok(CompletionResponse {
            tool_calls: vec![ToolCall {
                id: "pause".into(),
                name: if req.messages.len() == 1 {
                    "need_user"
                } else {
                    "done"
                }
                .into(),
                arguments: json!({}),
            }],
            ..Default::default()
        })
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenaiCompat
    }
}

#[tokio::test]
async fn immediate_resume_and_abort_are_not_lost() {
    for abort in [false, true] {
        // Started occupies the slot, so sending Step blocks until we consume NeedUser.
        let (events, mut receiver) = mpsc::channel(1);
        let control = RunControl::default();
        let runner = Runner {
            provider: Arc::new(PausingProvider),
            executor: Arc::new(BrowserWithPrivateImage),
            sink: Arc::new(NoStore),
            events,
            control: control.clone(),
            ladder: ModelLadder::new(vec!["test".into()]),
            max_tokens: 100,
        };
        let task = tokio::spawn(runner.run(RunRequest {
            run_id: "pause-test".into(),
            service: "openai".into(),
            kind: RunKind::Collect {
                key_types: vec!["api_key".into()],
            },
            playbook: PlaybookSet::builtin().get("openai").unwrap().clone(),
            hints: vec![],
        }));
        assert!(matches!(
            receiver.recv().await,
            Some(RunEvent::Started { .. })
        ));
        assert!(matches!(
            receiver.recv().await,
            Some(RunEvent::NeedUser { .. })
        ));
        if abort {
            control.abort();
        } else {
            control.resume();
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while receiver.recv().await.is_some() {}
            task.await.unwrap()
        })
        .await
        .expect("early resume/abort must not leave the run waiting forever");
        if abort {
            assert_eq!(result, RunOutcome::Aborted);
        } else {
            assert!(
                matches!(result, RunOutcome::Partial { missing, .. } if missing == vec!["api_key"])
            );
        }
    }
}

#[tokio::test]
async fn unredacted_browser_images_never_reach_the_model() {
    let provider = Arc::new(RecordingProvider::default());
    let (events, _receiver) = mpsc::channel(16);
    let runner = Runner {
        provider: provider.clone(),
        executor: Arc::new(BrowserWithPrivateImage),
        sink: Arc::new(NoStore),
        events,
        control: RunControl::default(),
        ladder: ModelLadder::new(vec!["test".into()]),
        max_tokens: 100,
    };
    runner
        .run(RunRequest {
            run_id: "privacy-test".into(),
            service: "openai".into(),
            kind: RunKind::Collect {
                key_types: vec!["api_key".into()],
            },
            playbook: PlaybookSet::builtin().get("openai").unwrap().clone(),
            hints: vec![],
        })
        .await;
    let requests = provider.0.lock().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "must inspect the request after browser output"
    );
    let conversation = serde_json::to_string(&requests[1].messages).unwrap();
    assert!(
        conversation.contains("Settings button"),
        "safe text must remain usable"
    );
    assert!(!conversation.contains("private-screenshot-bytes"));
    assert!(!passvalet_agent::tools::is_browser_tool("screenshot"));
    for request in requests.iter() {
        for message in &request.messages {
            for part in message
                .content
                .iter()
                .chain(message.tool_results.iter().flat_map(|r| &r.content))
            {
                assert!(
                    !matches!(part, ContentPart::Image { .. }),
                    "raw screenshot reached the model"
                );
            }
        }
        assert!(!request.tools.iter().any(|tool| tool.name == "screenshot"));
    }
}
