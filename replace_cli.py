import os

files_to_check = [
    "AGENTS.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
    "README.md",
    "rememhq-cli/src/bin/remem.rs",
    "rememhq-cli/src/commands/init.rs",
    "rememhq-cli/src/main.rs",
    "docs/AGENT_SETUP.md",
    "LLM.md",
    "rememhq-mcp/README.md",
    "rememhq-mcp/src/main.rs",
    "rememhq-mcp/src/transport/stdio.rs",
    ".agents/plugins/remem/README.md",
    ".agents/plugins/remem/plugin.json"
]

replacements = [
    ("gemini-cli", "antigravity-cli"),
    ("Gemini CLI", "Antigravity CLI"),
    ("GeminiCli", "AntigravityCli"),
    ("gemini_cli", "antigravity_cli"),
    ("init_gemini", "init_antigravity"),
    ("Gemini, or any MCP", "Antigravity, or any MCP"),
    (".gemini/config", ".antigravity/config"),
    (".gemini/settings.json", ".antigravity/settings.json"),
    ("AgentClient::Gemini", "AgentClient::Antigravity")
]

for filepath in files_to_check:
    path = os.path.join(r"c:\Users\frimp\Documents\remem", filepath)
    if os.path.exists(path):
        with open(path, "r", encoding="utf-8") as f:
            content = f.read()
        
        new_content = content
        for old, new in replacements:
            new_content = new_content.replace(old, new)
            
        if new_content != content:
            with open(path, "w", encoding="utf-8") as f:
                f.write(new_content)
            print(f"Updated {filepath}")
