import asyncio
from rememhq.client import Memory
from rememhq.models import MemoryType

async def main():
    print("🧠 Starting remem RAG Example...")
    
    # Initialize the client.
    async with Memory(
        project="rag-demo-project",
        base_url="http://127.0.0.1:7474"
    ) as memory:
        
        print("\n--- Step 1: Ingesting Documents (RAG) ---")
        documents = [
            "Rust's ownership system guarantees memory safety without a garbage collector.",
            "Axum is a web application framework that focuses on ergonomics and modularity.",
            "Vector databases are optimized for storing and retrieving high-dimensional embeddings.",
            "Model Context Protocol (MCP) standardizes how AI assistants communicate with external tools."
        ]
        
        for doc in documents:
            print(f"Ingesting: '{doc[:40]}...'")
            await memory.store(
                content=doc,
                memory_type=MemoryType.FACT,
                tags=["documentation", "rag"]
            )
            
        print("\n--- Step 2: Retrieving Context for Generation ---")
        user_question = "How does Rust ensure memory safety?"
        print(f"User Query: '{user_question}'")
        
        # Retrieve the most relevant documents based on semantic similarity + LLM reasoning
        results = await memory.recall(user_question, limit=2)
        
        print("\nRetrieved Context:")
        context_chunks = []
        for i, res in enumerate(results):
            print(f"  {i+1}. {res.content} (Relevance: {res.similarity:.2f})")
            context_chunks.append(res.content)
            
        print("\n--- Step 3: LLM Generation (Mock) ---")
        # In a real app, you would pass `context_chunks` + `user_question` to an LLM provider (like OpenAI or Anthropic)
        prompt = f"Given the context: {' | '.join(context_chunks)}, answer the question: {user_question}"
        print(f"Prompt constructed for LLM: \n{prompt}")
        
        mock_response = "Based on the provided context, Rust ensures memory safety through its ownership system, which does not require a garbage collector."
        print(f"\nMock LLM Response: {mock_response}")

if __name__ == "__main__":
    asyncio.run(main())
