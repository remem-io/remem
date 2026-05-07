//! Stdio transport — reads JSON-RPC from stdin, writes to stdout.
//!
//! This is the primary transport for MCP integration with
//! Claude Code, Cursor, and other IDE-based agents.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Run the stdio JSON-RPC transport loop.
///
/// Reads newline-delimited JSON from stdin and writes responses to stdout.
/// Logging goes to stderr to avoid contaminating the JSON-RPC channel.
pub async fn run_stdio_loop<F, Fut>(handler: F) -> anyhow::Result<()>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Option<String>>,
{
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        if let Some(response) = handler(line).await {
            stdout.write_all(response.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}
