use owo_colors::OwoColorize;
use rememhq_core::config::RememConfig;
use rememhq_core::providers::{ChatMessage, ChatRole, Tool};
use rememhq_core::reasoning::ReasoningEngine;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde_json::json;

pub async fn run_agent(engine: ReasoningEngine, config: &RememConfig) -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let provider = &config.reasoning.provider;
    let model = &config.reasoning.reasoning_model;
    let dir_str = config.project_data_dir().display().to_string();

    // Truncate strings to fit the box if they are too long
    let trunc_version = if version.len() > 40 {
        &version[0..40]
    } else {
        version
    };
    let trunc_provider = if provider.len() > 10 {
        &provider[0..10]
    } else {
        provider
    };
    let trunc_model = if model.len() > 25 {
        &model[0..25]
    } else {
        model
    };
    let trunc_dir = if dir_str.len() > 41 {
        format!("...{}", &dir_str[dir_str.len() - 38..])
    } else {
        dir_str
    };

    println!(
        "{}",
        "╭────────────────────────────────────────────────────────────╮".dimmed()
    );
    println!(
        "{} {} {:<40} {}",
        "│".dimmed(),
        ">_ remem AI".bold(),
        trunc_version,
        "│".dimmed()
    );
    println!(
        "{}",
        "│                                                            │".dimmed()
    );
    println!(
        "{} provider:  {:<10}  {:<25} {}",
        "│".dimmed(),
        trunc_provider,
        trunc_model.blue(),
        "│".dimmed()
    );
    println!(
        "{} directory: {:<41} {}",
        "│".dimmed(),
        trunc_dir,
        "│".dimmed()
    );
    println!(
        "{}",
        "╰────────────────────────────────────────────────────────────╯".dimmed()
    );
    println!(
        "{}",
        "Tip: Type 'quit' or 'exit' to end the session.\n".bold()
    );

    let tools = vec![Tool {
        name: "execute_command".to_string(),
        description:
            "Execute a terminal command (e.g. ls, git, cargo). Returns the command output."
                .to_string(),
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
    }];

    let mut messages: Vec<ChatMessage> = vec![];

    // Add initial system prompt
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: "You are a helpful AI terminal companion running on the user's computer. You can help them write code, run commands, and answer questions. Use the execute_command tool to run shell commands.".to_string(),
        tool_calls: None,
        tool_call_id: None,
    });

    let mut rl = DefaultEditor::new()?;

    loop {
        let prompt = "❯ ".to_string();

        let readline = tokio::task::block_in_place(|| rl.readline(&prompt));

        let input = match readline {
            Ok(line) => {
                let _ = rl.add_history_entry(line.as_str());
                line
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
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
            let response = engine
                .provider
                .chat(&messages, &tools, &config.reasoning.reasoning_model)
                .await?;
            let mut assistant_msg = response.message.clone();

            if !assistant_msg.content.is_empty() {
                let mut display_content = assistant_msg.content.clone();
                // Dim out thought blocks
                if let Some(start) = display_content.find("<thought>") {
                    if let Some(end_offset) = display_content[start..].find("</thought>") {
                        let end = start + end_offset + 10;
                        let thought_str = &display_content[start..end].to_string();
                        let dimmed_thought = thought_str.dimmed().to_string();
                        display_content = display_content.replace(thought_str, &dimmed_thought);
                    }
                }

                println!("\n{}\n", display_content);
            }

            messages.push(assistant_msg.clone());

            // Handle tool calls
            if let Some(tool_calls) = assistant_msg.tool_calls.take() {
                for tc in tool_calls {
                    if tc.name == "execute_command" {
                        if let Some(cmd_str) = tc.arguments.get("command").and_then(|v| v.as_str())
                        {
                            let exec_text = format!("> Executing: {}", cmd_str);
                            println!("{}", exec_text.blue().bold());

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
                                    if !stdout.is_empty() {
                                        out.push_str(&stdout);
                                    }
                                    if !stderr.is_empty() {
                                        out.push_str("\nSTDERR:\n");
                                        out.push_str(&stderr);
                                    }
                                    if out.is_empty() {
                                        out.push_str(
                                            "Command executed successfully with no output.",
                                        );
                                    }

                                    println!("{}", "  Command finished.".dimmed());
                                    out
                                }
                                Err(e) => {
                                    let err = format!("Failed to execute command: {}", e);
                                    println!("{}", err.red());
                                    err
                                }
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
