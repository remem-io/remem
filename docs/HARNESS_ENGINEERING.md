# Harness Engineering

The bottleneck is everything around the model: who gives it tools, who checks its work, what it remembers, what it is allowed to touch, and whether anyone can see what it did. That layer has a name now. It is called the harness.

Each wave did not replace the one before it. It absorbed it. Context engineering absorbed prompt engineering: what you feed the model matters more than how you phrase the question. Harness engineering absorbs both: the harness decides what context to load, what prompt to send, and then adds the seven things neither prompts nor context can do alone: tool control, verification, memory, permissions, observability, routing, and feedback.

88% of enterprise AI agent projects fail to reach production. The failure is almost never the model. It is context rot, missing guardrails, no state persistence, or feedback loops that never close. Two teams running the same model with different harness designs get dramatically different outcomes. The harness is the differentiator, not the model.

## What a harness actually is

The model contains the raw intelligence. The harness makes that intelligence reliable. It is the execution layer between the model and the real world: tool dispatch, verification, memory, permissions, and logging.

A production harness has seven layers. Each one prevents a different class of failure.

### 1. Tool Orchestration
The single most important rule: never let the model call tools directly. The model returns a structured tool call (function name, arguments). The harness validates the schema, checks permissions, executes, and injects the result back. This prevents prompt injection from escalating to arbitrary code execution.

This is the draft-commit pattern. Dangerous actions are first drafted, then explicitly committed. The model suggests "delete this file." The harness holds that action and asks for confirmation. Read-only operations go through immediately. Write operations wait.

```yaml
tools:
  file_read: auto        # no confirmation needed
  file_write: auto       # safe, versioned
  file_delete: confirm   # requires human approval
  git_commit: auto
  git_push: confirm      # irreversible
  shell_exec: confirm    # arbitrary execution, always hold
  api_call: auto         # read-only endpoints
  api_mutate: confirm    # state-changing endpoints
```

Whether it is a successful API response, a permission denial, or a timeout, the agent always receives a structured observation back. No dangling promises. No silent failures. The harness turns every tool interaction into a clean request-response cycle the agent can reason about, instead of letting the agent parse raw shell output and guess what happened.

### 2. Verification Loops
Every production harness separates the maker from the checker. The model that wrote the code is too generous grading its own homework. A second pass with different instructions catches what the first one talked itself into.

Real example: in April 2026, Claude Code had a quality degradation. Anthropic traced it to three independent harness-level changes: a reasoning-effort downgrade, a caching bug that dropped thinking history, and an aggressive verbosity-limiting prompt. None were model problems. All three were harness problems. The model was the same. The surrounding system changed, and quality collapsed. Anthropic published a full postmortem. The lesson was not "fix the model." The lesson was "your harness is your product quality, treat it that way."

The verification loop does not have to be expensive. A fast, cheap model can handle most checks (does it compile, do the tests pass, is the diff inside scope). Reserve the expensive model for judgment calls where the cheap one lacks the reasoning depth. The split itself is what matters, not the price tag on the verifier.

```yaml
verify:
  enabled: true
  mode: separate_agent   # never self-grade
  criteria:
    - all tests pass
    - no new lint warnings
    - no files modified outside task scope
  on_fail: reject_with_reason
  on_pass: proceed_to_commit
```

### 3. Context & Memory
Context engineering taught us that what you feed the model matters more than how you ask. Harness engineering takes the next step: the harness decides automatically what enters the context window, instead of relying on the human to get it right every time.

A production harness compacts context between turns. It does not shove the entire conversation history into every call. It summarizes completed subtasks, drops resolved tool outputs, and keeps only what the current step needs. Without this, long-running agents fill their window, start losing information, and every reasoning step gets worse.

Memory works the same way. The harness persists what matters (CLAUDE.md, STATE.md, constraints) and loads it automatically at session start. The agent that made a mistake last week cannot make the same mistake this week because the lesson lives in a file the harness reads before doing anything. Without persistent memory, every session is a blank slate. With it, the system compounds.

The practical test: if your agent is in turn 40 and still knows the constraints you set in turn 1, your context layer is working. If it forgot them by turn 15, it is not.

### 4. Guardrails
Guardrails intercept model outputs before they reach a user or downstream system and validate them against policy. In 2026, this is no longer optional. Colorado's AI Act took effect June 30. The EU AI Act's high-risk provisions applied from August. SOC 2 auditors now ask for evidence of runtime controls on AI outputs.

A common mistake: treating all four categories with the same weight for every agent. A customer-facing chatbot and an agent with database write access do not need identical controls. The harness should scale guardrail strictness to the risk level of the task, not apply maximum friction everywhere.

Four categories:
- **Behavioral**: Content safety, tone, formatting rules.
- **Data**: PII detection, data classification. The agent cannot leak customer data into a log or PR description.
- **Tool & action**: Permission scoping, draft-commit for irreversible operations.
- **Operational**: Token budgets, time limits, retry caps. The agent cannot burn $200 on a task you expected to cost $5.

```yaml
guardrails:
  scope_lock:
    - do not touch files outside /src/[task-scope]/
    - do not modify CI config without human approval
  budget:
    max_tokens_per_run: 500000
    max_cost_per_run: $2.00
    max_retries: 3
  data:
    block_pii_in_output: true
    redact_secrets_in_logs: true
  action:
    require_approval: [git_push, deploy, db_migrate]
```

### 5. Observability
An agent that worked last week can fail this week without any code change. The model got a silent update, an API shifted its response format, or usage hit a path nobody tested. Without observability, you find out when production breaks. With it, you detect drift before it causes harm.

Organizations that can trace every agent action and detect behavioral drift are the ones that can confidently expand agent autonomy over time. Without traces, every expansion of scope is a bet. With them, it is an evidence-based decision.

- **Every tool call and its result**. What the agent tried, what happened, how long it took. The audit trail.
- **Cost per accepted change**. Not total tokens. Cost per useful output. If accepted-change rate drops below 50%, the harness is spending more than it saves.
- **Drift detection**. Compare this week's output distribution to last week's. Catch a silent model regression before your users do.

### 6. Routing & Model Selection
Not every subtask needs the same model. A lint check does not need the same reasoning power as an architecture review. A harness without routing sends everything through the most expensive model and hopes the budget holds. A harness with routing picks the cheapest model that can handle each subtask and reserves the expensive one for where it matters.

The practical split: use a fast, cheap model for classification, formatting, and mechanical checks. Use the strong model for planning, complex reasoning, and anything that requires judgment. The harness routes automatically based on task type, not on the developer remembering to switch models mid-workflow.

```yaml
routing:
  classify_issue:
    model: haiku
    effort: low
    # fast, cheap, handles 90% of triage
  plan_fix:
    model: sonnet
    effort: medium
    # needs reasoning but not frontier-grade
  write_code:
    model: sonnet
    effort: high
    # balance of speed and quality
  verify_output:
    model: opus
    effort: high
    # expensive but catches what cheaper models miss
  format_pr_description:
    model: haiku
    effort: low
    # mechanical task, no reasoning needed
```

### 7. Feedback & Self-Improvement
A static harness runs the same way on run 50 as it did on run 1. A feedback-enabled harness learns from every rejection, every timeout, every budget overshoot and writes the lesson into a constraints file that every future run reads automatically.

This is not model fine-tuning. The weights do not change. What changes is the system around the model: the rules tighten, the routing improves, the context gets more precise. Over a few weeks, the verifier has less and less to catch because the constraints already prevent the mistakes it used to flag.

```yaml
# CONSTRAINTS.md - loaded before every run
# Auto-generated from verifier rejections

## From run 3 (2026-08-10)
- never disable a failing test, escalate instead
- do not modify files outside the task's stated scope

## From run 5 (2026-08-12)
- every claimed metric must link to a source
- if two sub-agents contradict, flag both, do not pick a side

## From run 8 (2026-08-14)
- PR descriptions must include "What changed" and "Why"
- do not add dependencies without checking license compatibility
```

## Why 88% of agents fail without one
The number comes from enterprise deployment data. Nearly 9 out of 10 AI agent projects stall before reaching production, and the root cause is almost never "the model is not smart enough." It is the infrastructure around the model: missing layers, skipped checks, unmonitored drift. Here is what actually breaks:

- **Context rot**. Long sessions lose constraints. The rule from turn 3 is gone by turn 50. The agent was told "never modify billing code." By turn 50, the summarization has dropped that instruction. The agent modifies billing code.
- **No verification**. Agent grades its own work. The grade is always generous. A study of self-evaluating agents found they approve their own output 94% of the time. A separate verifier with different instructions drops that to 61%. The 33% gap is the bugs that would have shipped.
- **Tool escalation**. Prompt injection triggers a shell command. No permission layer stops it. In one documented case, a crafted GitHub issue description caused an agent to run an arbitrary script embedded in the issue body, because the harness had no tool-level permission check.
- **Silent model updates**. Provider ships a change. Agent behavior drifts. Nobody notices for two weeks because there are no behavioral baselines to compare against.
- **No state persistence**. Every session starts from zero. Same mistakes repeated daily. Same context re-derived at full token cost. A team reported spending 40% of their agent budget on context the agent had already processed in previous sessions.
- **Budget explosion**. Loop retries 40 times on a failing task. No cap stops it. The invoice arrives Monday.

Every one of these is a harness problem, not a model problem. Swap the model and the same failures happen. Fix the harness and the same model starts working. The cheapest time to design the harness is before the first real user touches the agent, not after the first incident report.

## Where a harness does not help
- A harness cannot fix a bad task definition. If the goal is vague, the best harness in the world will reliably produce the wrong thing.
- Guardrails rot. Permissions set once and never reviewed become a false sense of security. Re-audit every 30 days.
- Over-harnessing kills speed. Every approval gate adds latency. A harness designed for a bank will make a hackathon feel like filing taxes. Match the weight to the risk.
- The harness is not the product. It is infrastructure. Nobody buys your harness. They buy what the agent produces.

## Conclusion
Prompt engineering taught us how to talk to models. Context engineering taught us what to show them. Harness engineering is the part that gets them into production: who gives them tools, who checks their work, what they remember, what they can touch, and whether anyone can see what happened.

Every enterprise still evaluating AI agents through a "which model should we pick" lens is asking last year's question. The model is the engine. The harness is the car. Nobody ships an engine without a car around it.

You do not need all seven layers on day one. Start with tool permissions (layer 1) and a basic verification check (layer 2). Add memory when sessions start repeating themselves. Add guardrails when the agent touches anything a customer can see. Add observability when you need to prove it is working. Add routing when the token bill gets uncomfortable. Add feedback when you want the system to stop making the same mistake twice.

Seven layers. Tool orchestration, verification, context, guardrails, observability, routing, feedback. Skip one and you have a demo. Build all seven and you have a system that gets better every time it runs.
