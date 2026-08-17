# Graph Engineering

How to build the agent system that replaced loops.

A single AI agent in a loop can plan, code, test, and review. But the moment the task gets complex, it chokes. The context window fills up, reasoning degrades, and you're back in the chair reading every diff.

The people shipping faster in 2026 stopped giving one agent five jobs. They split the work into specialists: one agent codes, another reviews, a third tests. Then they wired them into a system that routes work, shares state, and verifies output without a human in the chair.

That system is a graph. And in July 2026, the AI engineering community gave the practice a name: **Graph engineering**: designing the organization your agents run in.

Each layer built on the last. None replaced the one before it. Graph engineering is the fourth layer.

## 01. Three primitives. That's the whole thing.
- **Nodes** do the work. Each node is one specialized agent with one job. A planner, a coder, a reviewer, a test runner. Each gets its own context window, its own instructions, its own set of tools. What it can't see, it can't break.
- **Edges** route between them. An edge is the rule that decides what happens after a node finishes. "If the classifier says bug, send to the fixer. If it says flake, send to the retry agent." Edges can be unconditional (always go here next) or conditional (check a value, then choose a path).
- **State** is what flows along the edges. A shared data object that carries what's been done, what failed, and what comes next. Each node reads the slice it needs, does its work, writes its output back to the state, and the next node picks it up.

A loop is the smallest possible graph: one node with an edge back to itself. So graph engineering does not replace loop engineering. It's the layer directly above. You learn loops first. When one agent doing five jobs becomes the bottleneck, you split the jobs into a graph.

## 02. The 4-question filter.
Most tasks do not need a graph. A single well-built loop handles the majority of real work. The people dismissing graph engineering as a buzzword have a point you should take seriously: unnecessary complexity is not engineering. It's overhead.

Before you build anything, answer four questions. If any answer is no, keep the loop.
1. Does the task naturally split into independent subtasks?
2. Do different steps require different models or contexts?
3. Is a maker-checker split necessary for safety or quality?
4. Is the current single-agent loop failing or hitting a ceiling?

If your loop works, don't break it with a graph. If your loop is the bottleneck, the graph is the fix.

## 03. Sketch the org chart before touching code.
The mistake most people make with graphs: they open Claude Code and start building agents before knowing what the graph should look like. That's writing functions before knowing what the program does.

Start with three questions on paper:
1. What is the final output? Be specific. "Fixed CI" is weak. "All tests pass, lint clean, PR opened with summary of what failed and what was changed" is a target the graph can verify against.
2. Which subtasks are independent? Draw them side by side. If the env-secret fix and the flaky-test retry don't share any input, they can run at the same time. If the fixer needs the classifier's output, draw an arrow.
3. Where does quality get checked? Every path through the graph should hit at least one gate before reaching the output. No gate means no verification.

Start on paper. Boxes for agents. Arrows for routing. This sketch becomes your blueprint. Don't skip it. The 10 minutes you spend here saves hours of rewiring later.

## 04. One agent, one verb. No generalists.
Each node in the graph is one agent with one job. Not a generalist that does everything. A specialist that does one thing well.

Why this matters: a generalist agent loads the full context of the entire task into one window. When the window fills up (and it always fills up on complex tasks), reasoning degrades. The model starts losing track of what it was doing.

A specialist gets a scoped context window with only the information it needs. It sees less, but it sees clearly. Anthropic's own multi-agent research confirmed this: splitting agents into specialists with isolated contexts showed a 90.2% improvement over a single-agent baseline on internal research benchmarks.

Three rules for every node:
1. **One node, one verb.** "Classify." "Fix." "Review." "Retry." If a node description has two verbs, it's two nodes.
2. **Scope the tools.** A reviewer node should not have access to write files. A fixer node should not have access to merge PRs. Scoped tools prevent a specialist from stepping outside its lane.
3. **Match model to task.** The classifier routes failures and needs to be fast. Use a cheap model (Haiku, or Sonnet at low effort). The bug fixer needs to read test failures, understand the code, and write a targeted patch. Use a frontier model at high effort.

## 05. Edges decide who gets what.
Edges are the routing rules that connect nodes. They decide three things: who gets the output next, under what condition, and what data travels along the edge.

There are three types of edges. Every graph uses at least two. The key insight: hard constraints should not live inside prompts. If a routing rule is critical ("never send payment-related failures to the auto-fixer"), enforce it in the edge logic, not in the agent's instructions. Prompts are advisory. Edges are deterministic. A prompt might be ignored on turn 47 of a long session. An edge is code. It doesn't forget.

## 06. State: the file the graph writes between runs.
State is the memory of the graph. Without it, each node starts from zero and each run forgets what happened before. With state, nodes pick up where the last one left off, and tomorrow's run knows what today's run already tried.

The state object is a shared data structure that flows along the edges. Each node reads the slice of state it needs, does its work, writes its output back, and the next node picks up the updated state.

Design rule: **whitelist, don't broadcast.** Each node receives only the slice of state it needs.

## 07. The node that says done or not done.
This is the piece that separates a working graph from an expensive fan of unverified agents agreeing with each other.

Without a gate, the graph produces output and calls it done. With a gate, the graph produces output and then something objective checks whether the output is actually correct. The gate is the only node that decides "done" or "not done." Every other node does work. The gate judges work.

Karpathy's rule applies directly here: if a task is verifiable, it is optimizable.

Three types of gates, from strongest to weakest:
1. **Hard gate.** A test suite, a type checker, a linter, a build. It passes or fails. No opinion, no judgment, no bias. This is the strongest gate.
2. **Soft gate.** A second model asked to review the output. Better than no gate, but weaker than a test suite.
3. **Human gate.** A human reviews before merge, deploy, or any irreversible action. The graph surfaces the work. The human makes the final call.

## 08. Five shapes. Every graph is assembled from these.
Every graph you will ever build is assembled from five composable patterns:
- **Route at entry:** classifier sends each failure type to the right specialist.
- **Chain within each path:** specialist fixes, then gate verifies.
- **Evaluate on retry:** if gate fails, fixer gets feedback and iterates.

The cheapest structural win in most graphs is routing. A single classifier at the entry that sends easy tasks to a cheap model and hard tasks to a frontier model can cut your token bill in half without changing output quality.

## 09. Ship it: /goal, /loop, Routines.
The graph is designed. The nodes are built. The edges are wired. The gate is set. Now run it.

There are three levels of autonomy. Each one removes you further from the loop. Start at level one. Only promote when you trust the graph.
- **Level 1: Manual run with `/goal`.** You type the command once. The graph runs until the condition you set is met.
- **Level 2: Scheduled runs with `/loop`.** The graph checks for new failures on a cadence. You're no longer the trigger.
- **Level 3: Routines.** Run in the cloud on infrastructure. Triggered by schedules, API calls, or webhooks.

## 10. The debt the graph creates.
Two problems get sharper as the graph gets better. Both scale with success.
- **Comprehension debt** grows every time the graph ships code you didn't write. The gap between what exists in your repository and what you actually understand charges compound interest.
- **Cognitive surrender** is subtler. When the graph runs itself, it's tempting to stop forming an opinion and accept whatever comes back. Designing the graph is the cure when you do it with judgment. It's the accelerant when you do it to avoid thinking.

The mitigations are not technical:
- **Read the diffs.** Every single one.
- **Spot-check the gate.** Periodically pick a few PRs the graph approved and manually verify the gate actually caught what it was supposed to catch.
- **Keep the graph on small, machine-checkable tasks.**
- **Pair-design the graph with a teammate.** A second pair of eyes when designing catches blind spots.

## Conclusion
Karpathy showed us the context window is the programming surface. Cherny showed us the loop automates the context engineering. Steinberger asked the question that moved the frame one more level: what happens when many loops connect into one system?

But the honest version of this story is the same one every article in this space eventually tells: most developers don't need a graph yet. Not until the loop hits a ceiling, the task breaks into parallel subtasks, and the maker-checker split needs structural enforcement. If you haven't built a working loop, go build one first. A graph without a working loop underneath it is just organized complexity.
