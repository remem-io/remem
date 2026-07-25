//! Stdio transport — reads JSON-RPC from stdin, writes to stdout.
//!
//! This is the primary transport for MCP integration with
//! Claude Code, Codex, Cursor, Copilot, Antigravity CLI, OpenCode,
//! and other IDE-based / CLI-based agents.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// Run the stdio JSON-RPC transport loop.
///
/// Reads newline-delimited JSON from stdin and writes responses to stdout.
/// Handlers are spawned as async tasks so slow requests (e.g. consolidation or recall)
/// do not block reading subsequent requests from stdin.
pub async fn run_stdio_loop<F, Fut>(handler: F) -> anyhow::Result<()>
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Option<String>> + Send + 'static,
{
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    let (tx, mut rx) = mpsc::channel::<String>(100);

    let writer_handle = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(response) = rx.recv().await {
            if let Err(e) = stdout.write_all(response.as_bytes()).await {
                tracing::error!("Failed to write to stdout: {}", e);
                break;
            }
            if let Err(e) = stdout.write_all(b"\n").await {
                tracing::error!("Failed to write newline to stdout: {}", e);
                break;
            }
            if let Err(e) = stdout.flush().await {
                tracing::error!("Failed to flush stdout: {}", e);
                break;
            }
        }
    });

    let handler = std::sync::Arc::new(handler);

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let handler = handler.clone();
        let tx = tx.clone();

        tokio::spawn(async move {
            if let Some(response) = handler(line).await {
                let _ = tx.send(response).await;
            }
        });
    }

    drop(tx);
    let _ = writer_handle.await;
    Ok(())
}
