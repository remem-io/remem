import asyncio
from rememhq.client import Memory
from rememhq.models import MemoryType

async def main():
    print("🧠 Starting remem Multi-Agent Example...")
    
    # We will simulate two agents working on the same project
    # Agent 1: The Researcher
    # Agent 2: The Writer
    
    # Both connect to the same memory project to share state
    project_name = "multi-agent-demo"
    
    async with Memory(project=project_name, base_url="http://127.0.0.1:7474") as researcher_memory, \
               Memory(project=project_name, base_url="http://127.0.0.1:7474") as writer_memory:
        
        print("\n--- Researcher Agent ---")
        findings = [
            ("The target audience for our new product is remote software engineers.", MemoryType.FACT, ["audience", "product"]),
            ("Key value proposition: Saves 2 hours of context switching per day.", MemoryType.FACT, ["value_prop", "product"]),
        ]
        
        for content, mem_type, tags in findings:
            print(f"Researcher storing: '{content}'")
            await researcher_memory.store(content=content, memory_type=mem_type, tags=tags)
            
        print("\n--- Writer Agent ---")
        query = "What is the key value proposition and who is the audience?"
        print(f"Writer wants to know: '{query}'")
        
        results = await writer_memory.recall(query)
        print(f"\nWriter found {len(results)} shared memories:")
        for res in results:
            print(f"  - [{res.memory_type.value}] {res.content}")
            if res.reasoning:
                print(f"    (Reasoning: {res.reasoning})")

if __name__ == "__main__":
    asyncio.run(main())
