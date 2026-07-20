use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use rememhq_core::config::RememConfig;
use rememhq_core::providers::{ChatMessage, ChatRole, Tool};
use rememhq_core::reasoning::{ReasoningEngine, ReasoningEvent};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde_json::json;
use std::time::Duration;

/// Truncate `s` to at most `max_chars` characters, keeping the front.
///
/// Slices on character boundaries (via `char_indices`) rather than a raw
/// byte offset — naive byte-index slicing (`&s[0..n]`) panics with "byte
/// index n is not a char boundary" if the cut lands in the middle of a
/// multi-byte UTF-8 character.
fn truncate_chars_front(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

/// Keep at most the last `max_chars` characters of `s`, on character
/// boundaries. Used for the directory path, where the *end* of the path is
/// usually the more useful part to show when truncating.
fn truncate_chars_back(s: &str, max_chars: usize) -> &str {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s;
    }
    let skip = char_count - max_chars;
    match s.char_indices().nth(skip) {
        Some((byte_idx, _)) => &s[byte_idx..],
        // nth(skip) is None exactly when skip >= char_count, which (given
        // the guard above) only happens when max_chars == 0 — i.e. "keep
        // zero characters." Unlike truncate_chars_front's None case (which
        // correctly means "shorter than requested, return it all"), this
        // one means "requested nothing," so it must return "", not `s`.
        None => "",
    }
}

pub async fn run_agent(engine: ReasoningEngine, config: &RememConfig) -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let provider = &config.reasoning.provider;
    let model = &config.reasoning.reasoning_model;
    let dir_str = config.project_data_dir().display().to_string();

    // Truncate strings to fit the box if they are too long. Slicing by raw
    // byte index (the previous approach) panics with "byte index N is not a
    // char boundary" if the cut lands in the middle of a multi-byte UTF-8
    // character — very plausible for `dir_str` in particular, since project
    // directory paths routinely contain non-ASCII characters (accents, CJK,
    // etc.) in real usernames/folder names. truncate_chars_* below slice on
    // character boundaries instead, which is also what `{:<N}` padding
    // below actually counts against.
    let trunc_version = truncate_chars_front(version, 40);
    let trunc_provider = truncate_chars_front(provider, 10);
    let trunc_model = truncate_chars_front(model, 25);
    let trunc_dir = if dir_str.chars().count() > 41 {
        format!("...{}", truncate_chars_back(&dir_str, 38))
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

    let tools = vec![
        Tool {
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
        },
        Tool {
            name: "read_file".to_string(),
            description: "Read the contents of a file at the specified path.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The absolute or relative path to the file to read"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "write_file".to_string(),
            description:
                "Write content to a file at the specified path. Overwrites the file if it exists."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The absolute or relative path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The text content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        },
    ];

    let mut messages: Vec<ChatMessage> = vec![];

    // Add initial system prompt
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: "You are a helpful AI terminal companion running on the user's computer. You can help them write code, run commands, and answer questions. Use the execute_command tool to run shell commands.".to_string(),
        tool_calls: None,
        tool_call_id: None,
    });

    let mut rx = engine.event_bus.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                ReasoningEvent::ConsolidationStarted { session_id } => {
                    println!(
                        "{}",
                        format!("  [Memory] Consolidation started for {}...", session_id).dimmed()
                    );
                }
                ReasoningEvent::FactExtracted { content } => {
                    println!("{}", format!("  [Fact] {}", content).dimmed());
                }
                ReasoningEvent::ContradictionDetected {
                    existing_id,
                    new_content,
                } => {
                    println!(
                        "{}",
                        format!("  [Contradiction] {} -> {}", existing_id, new_content)
                            .yellow()
                            .dimmed()
                    );
                }
                ReasoningEvent::KnowledgeTripleFound {
                    subject,
                    predicate,
                    object,
                } => {
                    println!(
                        "{}",
                        format!("  [Graph] {} - {} - {}", subject, predicate, object).dimmed()
                    );
                }
                ReasoningEvent::ConsolidationCompleted { new_facts, .. } => {
                    println!(
                        "{}",
                        format!(
                            "  [Memory] Consolidation complete ({} new facts).",
                            new_facts
                        )
                        .dimmed()
                    );
                }
            }
        }
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
        if let Ok(memories) = engine.recall(input, 3, &[], None, None, None).await {
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
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                    .template("{spinner:.blue} {msg}")
                    .unwrap(),
            );
            spinner.set_message("Thinking...");
            spinner.enable_steady_tick(Duration::from_millis(100));

            // Call the LLM provider
            let response = engine
                .provider
                .chat(&messages, &tools, &config.reasoning.reasoning_model, None)
                .await;

            spinner.finish_and_clear();

            let response = response?;
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

                println!();
                termimad::print_text(&display_content);
                println!();

                if let Some(usage) = &response.usage {
                    let usage_str = format!(
                        "  [Tokens: {} prompt, {} completion, {} total]",
                        usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                    );
                    println!("{}", usage_str.dimmed());
                }
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
                    } else if tc.name == "read_file" {
                        if let Some(path_str) = tc.arguments.get("path").and_then(|v| v.as_str()) {
                            let exec_text = format!("> Reading file: {}", path_str);
                            println!("{}", exec_text.blue().bold());

                            let result_content = match std::fs::read_to_string(path_str) {
                                Ok(content) => content,
                                Err(e) => format!("Failed to read file: {}", e),
                            };

                            messages.push(ChatMessage {
                                role: ChatRole::Tool,
                                content: result_content,
                                tool_calls: None,
                                tool_call_id: Some(tc.id),
                            });
                        }
                    } else if tc.name == "write_file" {
                        if let (Some(path_str), Some(content_str)) = (
                            tc.arguments.get("path").and_then(|v| v.as_str()),
                            tc.arguments.get("content").and_then(|v| v.as_str()),
                        ) {
                            let exec_text = format!("> Writing file: {}", path_str);
                            println!("{}", exec_text.blue().bold());

                            let result_content = match std::fs::write(path_str, content_str) {
                                Ok(_) => format!("Successfully wrote to {}", path_str),
                                Err(e) => format!("Failed to write file: {}", e),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_chars_front_ascii() {
        assert_eq!(truncate_chars_front("hello world", 5), "hello");
        assert_eq!(truncate_chars_front("short", 40), "short");
        assert_eq!(truncate_chars_front("exact", 5), "exact");
    }

    #[test]
    fn test_truncate_chars_front_multibyte_no_panic() {
        // Regression test: the previous implementation sliced by raw byte
        // index (&s[0..n]), which panics if the cut lands mid-character.
        // 'é' is 2 bytes in UTF-8, so a byte-index cut at certain offsets
        // would land inside it; a char-index cut never does.
        let s = "café résumé naïve"; // contains multi-byte chars throughout
                                     // Should not panic for any max_chars value, including ones that
                                     // would have split a multi-byte char under the old byte-slicing.
        for n in 0..=s.chars().count() + 5 {
            let truncated = truncate_chars_front(s, n);
            assert!(truncated.chars().count() <= n.min(s.chars().count()));
        }
    }

    #[test]
    fn test_truncate_chars_back_ascii() {
        assert_eq!(truncate_chars_back("hello world", 5), "world");
        assert_eq!(truncate_chars_back("short", 40), "short");
    }

    #[test]
    fn test_truncate_chars_back_multibyte_no_panic() {
        // Mirrors test_truncate_chars_front_multibyte_no_panic, for the
        // "keep the tail" direction used for the directory path.
        let s = "/home/café/projects/日本語プロジェクト/naïve-résumé";
        for n in 0..=s.chars().count() + 5 {
            let truncated = truncate_chars_back(s, n);
            assert!(truncated.chars().count() <= n.min(s.chars().count()));
        }
        // The tail should actually be the tail, not garbage.
        assert!(s.ends_with(truncate_chars_back(s, 10)));
    }
}
