/**
 * remem Agent Loop Example (TypeScript)
 * 
 * This script demonstrates the typical memory lifecycle of an AI agent:
 * 1. Store: The agent learns a new fact during a conversation.
 * 2. Recall: The agent retrieves relevant facts to answer a question.
 * 3. Consolidate: The agent processes its working memory into long-term graphs and resolves contradictions.
 * 
 * Usage:
 * Ensure the remem API is running locally: `cargo run -p rememhq-api`
 * Then run this script: `npx tsx examples/agent_loop.ts`
 */

import { Memory } from "../sdk/typescript/src/index.js";

async function main() {
  console.log("=== remem TypeScript Agent Loop Simulation ===\n");

  // Initialize the memory client
  // By default, it connects to http://localhost:7474 and reads REMEM_API_KEY if present
  const memory = new Memory({ project: "ts-agent-demo" });

  const sessionId = "session_ts_01";

  // --- Step 1: Store ---
  console.log("1. Storing new observations...");
  
  const obs1 = "The user prefers to write backend code in Rust.";
  console.log(`   -> Storing: "${obs1}"`);
  const r1 = await memory.store(obs1, { tags: ["user_preference", "coding"] });
  console.log(`      Stored with ID: ${r1.id} (Importance: ${r1.importance})`);

  const obs2 = "The user is currently evaluating Google Gemini 1.5 Flash for reasoning.";
  console.log(`   -> Storing: "${obs2}"`);
  const r2 = await memory.store(obs2, { tags: ["current_task", "llm"] });
  console.log(`      Stored with ID: ${r2.id} (Importance: ${r2.importance})\n`);

  // --- Step 2: Recall ---
  console.log("2. Recalling relevant context...");
  const query = "What languages does the user like for backend development?";
  console.log(`   -> Query: "${query}"`);
  
  const results = await memory.recall(query, { limit: 2 });
  console.log(`   -> Found ${results.length} relevant memories:`);
  for (const res of results) {
    console.log(`      - [Similarity: ${res.similarity.toFixed(2)}] ${res.content}`);
  }
  console.log();

  // --- Step 3: Consolidate ---
  console.log("3. Consolidating session memory...");
  console.log(`   -> Triggering consolidation for session: ${sessionId}`);
  
  try {
    const report = await memory.consolidate(sessionId, "gemini-1.5-flash");
    console.log("   -> Consolidation Complete:");
    console.log(`      New facts extracted: ${report.new_facts}`);
    console.log(`      Updated facts: ${report.updated_facts}`);
    console.log(`      Contradictions resolved: ${report.contradictions.length}`);
    console.log(`      Knowledge graph updates: ${report.knowledge_graph_updates.length}`);
  } catch (err: any) {
    console.log(`   -> Consolidation skipped or failed (ensure API handles session consolidation): ${err.message}`);
  }

  console.log("\n=== Simulation Complete ===");
}

main().catch(console.error);
