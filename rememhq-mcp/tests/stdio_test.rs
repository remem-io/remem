use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[tokio::test]
async fn test_mcp_stdio_end_to_end() -> anyhow::Result<()> {
    let binary_path = env!("CARGO_BIN_EXE_rememhq-mcp");

    let mut child = Command::new(binary_path)
        .arg("--project")
        .arg("test-mcp-smoke")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    let stdout = child.stdout.as_mut().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // 1. Send initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "clientInfo": {
                "name": "IntegrationTestAgent",
                "version": "1.0.0"
            }
        }
    });

    let mut init_line = serde_json::to_string(&init_req)?;
    init_line.push('\n');
    stdin.write_all(init_line.as_bytes()).await?;
    stdin.flush().await?;

    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).await?;
    let resp: serde_json::Value = serde_json::from_str(&resp_line)?;

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["serverInfo"]["name"], "rememhq-mcp");

    // 2. Send tools/list
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let mut list_line = serde_json::to_string(&list_req)?;
    list_line.push('\n');
    stdin.write_all(list_line.as_bytes()).await?;
    stdin.flush().await?;

    resp_line.clear();
    reader.read_line(&mut resp_line).await?;
    let list_resp: serde_json::Value = serde_json::from_str(&resp_line)?;
    let tools = list_resp["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert!(tools.len() >= 19, "expected at least 19 tools listed");

    // 3. Test CallToolResult envelope unification on mem_set_mode
    let mode_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "mem_set_mode",
            "arguments": {
                "mode": "debugging"
            }
        }
    });
    let mut mode_line = serde_json::to_string(&mode_req)?;
    mode_line.push('\n');
    stdin.write_all(mode_line.as_bytes()).await?;
    stdin.flush().await?;

    resp_line.clear();
    reader.read_line(&mut resp_line).await?;
    let mode_resp: serde_json::Value = serde_json::from_str(&resp_line)?;
    let content = &mode_resp["result"]["content"];
    assert!(content.is_array(), "result.content must be an array");
    assert_eq!(content[0]["type"], "text");

    // 4. Test tool-level domain error handling with isError: true
    let err_req = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "mem_store",
            "arguments": {} // missing content
        }
    });
    let mut err_line = serde_json::to_string(&err_req)?;
    err_line.push('\n');
    stdin.write_all(err_line.as_bytes()).await?;
    stdin.flush().await?;

    resp_line.clear();
    reader.read_line(&mut resp_line).await?;
    let err_resp: serde_json::Value = serde_json::from_str(&resp_line)?;

    // Should return result with isError: true, NOT a jsonrpc error object
    assert!(err_resp["error"].is_null());
    assert_eq!(err_resp["result"]["isError"], true);
    assert!(err_resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Error: Missing content"));

    child.kill().await?;
    Ok(())
}
