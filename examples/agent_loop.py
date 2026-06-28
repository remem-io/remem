import asyncio
from datetime import datetime
from rememhq.client import Memory
from rememhq.models import MemoryType

async def main():
    print("🧠 Starting remem Agent Loop Simulation...")
    
    # Initialize the client. We assume the REST API is running locally on port 7474
    # Make sure your REMEM_API_KEY environment variable matches what the server expects.
    async with Memory(
        project="test-agent-01",
        base_url="http://127.0.0.1:7474",
        timeout=10.0
    ) as memory:
        
        print("\n--- Day 1: Storing Observations ---")
        observations = [
            ("The user prefers dark mode in all applications.", MemoryType.PREFERENCE, ["ui", "preferences"]),
            ("The codebase is written in Rust and uses Axum for the API.", MemoryType.FACT, ["architecture", "backend"]),
            ("When deploying to production, always use the GHCR docker image.", MemoryType.PROCEDURE, ["deployment", "devops"]),
            ("The user loves coffee but hates tea.", MemoryType.FACT, ["preferences", "food"]),
        ]
        
        for content, mem_type, tags in observations:
            print(f"Storing: '{content}'")
            try:
                result = await memory.store(
                    content=content,
                    memory_type=mem_type,
                    tags=tags
                )
                print(f"  ✅ Stored with ID {result.id} | Importance: {result.importance}")
            except Exception as e:
                print(f"  ❌ Failed to store: {e}")
                
        print("\n--- Day 2: Recalling Context ---")
        query = "How should I deploy the backend, and what is it written in?"
        print(f"Agent wants to know: '{query}'")
        
        try:
            results = await memory.recall(query, limit=2)
            print(f"\nRecall returned {len(results)} results:")
            for i, res in enumerate(results):
                print(f"  {i+1}. [{res.memory_type.value}] {res.content}")
                print(f"     Tags: {res.tags} | Relevance/Similarity: {res.similarity:.2f}")
                if res.reasoning:
                    print(f"     Reasoning: {res.reasoning}")
        except Exception as e:
            print(f"❌ Failed to recall: {e}")
            
        print("\n--- Day 3: Consolidating Session ---")
        session_id = "session_x99"
        print(f"Triggering consolidation for session {session_id}...")
        try:
            report = await memory.consolidate(session_id)
            print(f"  ✅ Consolidation Report:")
            print(f"     New Facts: {report.new_facts}")
            print(f"     Updated Facts: {report.updated_facts}")
            if report.contradictions:
                print(f"     Contradictions found: {len(report.contradictions)}")
        except Exception as e:
            # Note: Consolidate might fail if the server's consolidation logic is not fully hooked up in v0.1
            print(f"  ℹ️ Consolidation status: {e}")

if __name__ == "__main__":
    asyncio.run(main())
