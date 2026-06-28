use rememhq_core::config::RememConfig;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::providers::{ChatMessage, ChatRole, Tool};
use std::io::Write;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};

pub async fn run_agent(engine: ReasoningEngine, config: &RememConfig) -> anyhow::Result<()> {
    println!("remem AI Companion Terminal v{}", env!("CARGO_PKG_VERSION"));
    println!("Provider: {}", config.reasoning.provider);
    println!("Type 'quit' or 'exit' to end the session.\n");

    let tools = vec![
        Tool {
            name: "execute_command".to_string(),
            description: "Execute a terminal command (e.g. ls, git, cargo). Returns the command output.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command line string to execute in the shell"
                    }
                },
                "required": ["command"]
            }),
        }
    ];

    let mut messages: Vec<ChatMessage> = vec![];
    
    // Add initial system prompt
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: "You are a helpful AI terminal companion running on the user's computer. You can help them write code, run commands, and answer questions. Use the execute_command tool to run shell commands.".to_string(),
        tool_calls: None,
        tool_call_id: None,
    });

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    loop {
        print!("remem> ");
        std::io::stdout().flush()?;

        let input = match lines.next_line().await? {
            Some(line) => line,
            None => break,
        };

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("quit") || input.eq_ignore_ascii_case("exit") {
            println!("Goodbye!");
            break;
        }

        // Before sending to LLM, recall relevant memories
        if let Ok(memories) = engine.recall(input, 3, &[], None, None).await {
            if !memories.is_empty() {
                let mut context = String::from("Relevant past memories:\n");
                for mem in memories {
                    context.push_str(&format!("- {}\n", mem.content));
                }
                messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: context,
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }

        messages.push(ChatMessage {
            role: ChatRole::User,
            content: input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        // Agent loop
        loop {
            // Call the LLM provider
            let response = engine.provider.chat(&messages, &tools, &config.reasoning.reasoning_model).await?;
            let mut assistant_msg = response.message.clone();
            
            if !assistant_msg.content.is_empty() {
                println!("\n{}\n", assistant_msg.content);
            }

            messages.push(assistant_msg.clone());

            // Handle tool calls
            if let Some(tool_calls) = assistant_msg.tool_calls.take() {
                for tc in tool_calls {
                    if tc.name == "execute_command" {
                        if let Some(cmd_str) = tc.arguments.get("command").and_then(|v| v.as_str()) {
                            println!("> Executing: {}", cmd_str);
                            
                            let output = if cfg!(target_os = "windows") {
                                std::process::Command::new("powershell")
                                    .args(["-Command", cmd_str])
                                    .output()
                            } else {
                                std::process::Command::new("sh")
                                    .arg("-c")
                                    .arg(cmd_str)
                                    .output()
                            };

                            let result_content = match output {
                                Ok(output) => {
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    let mut out = String::new();
                                    if !stdout.is_empty() { out.push_str(&stdout); }
                                    if !stderr.is_empty() { out.push_str("\nSTDERR:\n"); out.push_str(&stderr); }
                                    if out.is_empty() { out.push_str("Command executed successfully with no output."); }
                                    out
                                },
                                Err(e) => format!("Failed to execute command: {}", e),
                            };

                            messages.push(ChatMessage {
                                role: ChatRole::Tool,
                                content: result_content,
                                tool_calls: None,
                                tool_call_id: Some(tc.id),
                            });
                        }
                    }
                }
            } else {
                // No more tool calls, exit the inner loop to wait for user input
                break;
            }
        }
    }

    Ok(())
}
