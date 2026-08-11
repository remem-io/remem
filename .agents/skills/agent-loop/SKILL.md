---
name: agent-loop
description: The 7 rules for self-improving agent loops using agents-cli. Use this when improving LLM reasoning or prompt quality.
---

# Agent Optimization Loop Rules

When optimizing prompts, agent behavior, or LLM reasoning (like the `remem` Reasoning Engine), you MUST follow these 7 rules to ensure genuine improvement rather than metric gaming.

### The 7 Rules

1. **Start with one case, not a suite**: Add one failing case. Fix it (expect 5-10 iterations). Add the next case only once the first holds.
2. **Make judges explain themselves**: A boolean or number is not enough. The judge must output a reason string. The next iteration is written based on this reason.
3. **Use code wherever the answer is deterministic**: If checking for a valid JSON schema or tool call, use Python/Rust asserts, not an LLM judge. Save LLM judges for tone and completeness.
4. **Score behavior, not paths**: Do not enforce exact trajectories (e.g., forcing the agent to check the weather before checking location if the order doesn't matter).
5. **Treat a flaky case as a finding**: If identical runs yield different scores, the agent or the judge is non-deterministic. Debug this instead of deleting the case.
6. **Never let the proposer move the bar**: Do not lower thresholds, edit the expected output, or drop a case to achieve a passing score. Use a held-out test set to verify real gains.
7. **Auto-optimize once, at the end**: Prompt optimization is expensive. Use it only for wording, not for missing tool calls.

### Usage in `remem`

1. Define your metric as a Python script in `evals/agent_loop/metrics/`. It must return a `pass` (0/1) and a `reason`.
2. Define ONE failing case in `evals/agent_loop/datasets/`.
3. Run the evaluation:
   ```bash
   uvx google-agents-cli eval generate --dataset evals/agent_loop/datasets/your_case.json -o artifacts/traces/
   uvx google-agents-cli eval grade --traces artifacts/traces/ --config evals/agent_loop/eval_config.yaml
   ```
4. Read the failure reason, modify the Rust prompt in `rememhq-core`, rebuild, and repeat.
