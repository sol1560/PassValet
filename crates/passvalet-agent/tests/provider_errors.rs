use passvalet_agent::provider::{CompletionRequest, Message, ProviderConfig, ProviderKind};
use passvalet_agent::providers;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

async fn response_error(kind: ProviderKind, status: u16, body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = providers::build(&ProviderConfig {
        kind,
        base_url: format!("http://{}", listener.local_addr().unwrap()),
        api_key: None,
        models: vec!["test".into()],
    });
    let server = async {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(stream.read_line(&mut line).await.unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        assert!(length < 8192);
        stream.read_exact(&mut vec![0; length]).await.unwrap();
        let response = format!(
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.get_mut().write_all(response.as_bytes()).await.unwrap();
    };
    let request = CompletionRequest {
        model: "test".into(),
        system: "test".into(),
        messages: vec![Message::user_text("test")],
        tools: vec![],
        max_tokens: 10,
        temperature: 0.0,
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(provider.complete(&request), server)
    })
    .await
    .expect("local HTTP test should finish");
    result.unwrap_err().to_string()
}

#[tokio::test]
async fn malformed_unicode_response_does_not_panic() {
    for kind in [ProviderKind::OpenaiCompat, ProviderKind::Anthropic] {
        let error = response_error(kind, 200, "错".repeat(200)).await;
        assert!(error.starts_with("bad response:"));
    }
}

#[tokio::test]
async fn server_error_does_not_echo_credentials() {
    let key = format!("sk-{}", "A1".repeat(24));
    for kind in [ProviderKind::OpenaiCompat, ProviderKind::Anthropic] {
        for status in [200, 401] {
            let error = response_error(kind, status, format!("invalid token {key}")).await;
            assert!(!error.contains(&key), "provider errors must not echo credentials");
        }
    }
}
