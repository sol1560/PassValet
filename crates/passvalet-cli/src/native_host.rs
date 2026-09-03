//! Chrome native-messaging host: relays JSON-RPC messages between the extension (stdin/stdout,
//! 4-byte little-endian length prefix) and the desktop app (Unix socket, newline-delimited).

use anyhow::{anyhow, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::launch;

const MAX_FRAME: u32 = 64 * 1024 * 1024;

pub async fn run() -> Result<()> {
    let socket_path = passvalet_ipc::IpcClient::socket_path();
    let stream = connect_socket(&socket_path).await?;
    let (sock_rd, mut sock_wr) = stream.into_split();
    let mut sock_rd = tokio::io::BufReader::new(sock_rd);

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let to_socket = async {
        let mut len_buf = [0u8; 4];
        loop {
            if stdin.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_buf);
            if len == 0 || len > MAX_FRAME {
                tracing::error!("bad frame length {len}");
                break;
            }
            let mut body = vec![0u8; len as usize];
            if stdin.read_exact(&mut body).await.is_err() {
                break;
            }
            // Ensure single line: the extension sends compact JSON; strip stray newlines anyway.
            body.retain(|&b| b != b'\n' && b != b'\r');
            body.push(b'\n');
            if sock_wr.write_all(&body).await.is_err() {
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    let to_stdout = async {
        use tokio::io::AsyncBufReadExt;
        let mut line = String::new();
        loop {
            line.clear();
            match sock_rd.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let trimmed = line.trim_end_matches(['\n', '\r']);
            if trimmed.is_empty() {
                continue;
            }
            let len = (trimmed.len() as u32).to_le_bytes();
            if stdout.write_all(&len).await.is_err() {
                break;
            }
            if stdout.write_all(trimmed.as_bytes()).await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        r = to_socket => r,
        r = to_stdout => r,
    }
}

async fn connect_socket(path: &std::path::Path) -> Result<UnixStream> {
    if let Ok(s) = UnixStream::connect(path).await {
        return Ok(s);
    }
    if !launch::launch_app() {
        return Err(anyhow!("PassValet app not running at {}", path.display()));
    }
    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if let Ok(s) = UnixStream::connect(path).await {
            return Ok(s);
        }
    }
    Err(anyhow!("PassValet app did not start"))
}
