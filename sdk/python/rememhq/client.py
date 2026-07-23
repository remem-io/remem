"""Async-first Python client for the remem REST API."""

from __future__ import annotations

from datetime import datetime
from uuid import UUID

import httpx

from rememhq.config import RememConfig
from rememhq.models import (
    ConsolidationReport,
    CompactResponse,
    ForgetMode,
    MemoryResult,
    MemoryType,
    StoreResponse,
    MemoryStoreRecord,
    MemoryVersionRecord,
)


class Memory:
    """Async client for remem — reasoning memory layer for AI agents.

    Example:
        >>> m = Memory(project="my-agent", reasoning_model="claude-sonnet-4-5")
        >>> await m.store("User prefers dark mode", tags=["preferences"])
        >>> results = await m.recall("what are the user's preferences?")
    """

    def __init__(
        self,
        project: str = "default",
        reasoning_model: str = "claude-sonnet-4-5",
        scoring_model: str = "claude-haiku-4-5",
        base_url: str | None = None,
        api_key: str | None = None,
        timeout: float = 30.0,
    ):
        config = RememConfig(
            project=project,
            reasoning_model=reasoning_model,
            scoring_model=scoring_model,
            timeout=timeout,
        )
        if base_url:
            config.base_url = base_url
        if api_key:
            config.api_key = api_key

        self._config = config
        headers: dict[str, str] = {"Content-Type": "application/json"}
        if config.api_key:
            headers["Authorization"] = f"Bearer {config.api_key}"

        self._client = httpx.AsyncClient(
            base_url=config.base_url,
            headers=headers,
            timeout=config.timeout,
        )
        self.stores = MemoryStoresClient(self._client)

    async def store(
        self,
        content: str,
        *,
        tags: list[str] | None = None,
        importance: float | None = None,
        ttl_days: int | None = None,
        memory_type: MemoryType = MemoryType.FACT,
    ) -> StoreResponse:
        """Store a new memory with automatic LLM importance scoring."""
        payload: dict = {
            "content": content,
            "tags": tags or [],
            "memory_type": memory_type.value,
        }
        if importance is not None:
            payload["importance"] = importance
        if ttl_days is not None:
            payload["ttl_days"] = ttl_days

        resp = await self._client.post("/v1/memories", json=payload)
        resp.raise_for_status()
        return StoreResponse.model_validate(resp.json())

    async def store_batch(
        self,
        items: list[dict],
    ) -> list[StoreResponse]:
        """Store multiple memories sequentially. Each dict can contain keys for `store`."""
        results = []
        for item in items:
            res = await self.store(**item)
            results.append(res)
        return results

    async def recall(
        self,
        query: str,
        *,
        limit: int = 8,
        filter_tags: list[str] | None = None,
        since: datetime | None = None,
        memory_type: MemoryType | None = None,
    ) -> list[MemoryResult]:
        """Guided recall — LLM re-ranks candidates for relevance."""
        params: dict = {"q": query, "limit": limit}
        if filter_tags:
            params["filter_tags"] = ",".join(filter_tags)
        if since:
            params["since"] = since.isoformat()
        if memory_type:
            params["memory_type"] = memory_type.value

        resp = await self._client.get("/v1/memories/recall", params=params)
        resp.raise_for_status()
        return [MemoryResult.model_validate(r) for r in resp.json()]

    async def search(
        self,
        query: str,
        *,
        limit: int = 20,
        filter_tags: list[str] | None = None,
    ) -> list[MemoryResult]:
        """Hybrid vector + keyword search without LLM re-ranking."""
        params: dict = {"q": query, "limit": limit}
        if filter_tags:
            params["filter_tags"] = ",".join(filter_tags)

        resp = await self._client.get("/v1/memories/search", params=params)
        resp.raise_for_status()
        return [MemoryResult.model_validate(r) for r in resp.json()]

    async def update(
        self,
        id: UUID | str,
        *,
        content: str | None = None,
        importance: float | None = None,
        tags: list[str] | None = None,
    ) -> dict:
        """Update an existing memory."""
        payload: dict = {}
        if content is not None:
            payload["content"] = content
        if importance is not None:
            payload["importance"] = importance
        if tags is not None:
            payload["tags"] = tags

        resp = await self._client.patch(f"/v1/memories/{id}", json=payload)
        resp.raise_for_status()
        return resp.json()

    async def forget(
        self,
        id: UUID | str,
        *,
        mode: ForgetMode = ForgetMode.DELETE,
    ) -> dict:
        """Delete, decay, or archive a memory."""
        resp = await self._client.delete(
            f"/v1/memories/{id}", params={"mode": mode.value}
        )
        resp.raise_for_status()
        return resp.json()

    async def consolidate(
        self,
        session_id: str,
        *,
        model: str | None = None,
    ) -> ConsolidationReport:
        """Trigger consolidation over a session's working memory."""
        payload: dict = {}
        if model:
            payload["model"] = model

        resp = await self._client.post(
            f"/v1/sessions/{session_id}/consolidate", json=payload
        )
        resp.raise_for_status()
        return ConsolidationReport.model_validate(resp.json())

    async def decay(self, factor: float = 0.9) -> dict:
        """Apply importance-weighted decay to all active memories."""
        resp = await self._client.post("/v1/memories/decay", json={"factor": factor})
        resp.raise_for_status()
        return resp.json()

    async def compact_context(
        self,
        conversation_text: str,
        *,
        focus_areas: list[str] | None = None,
    ) -> CompactResponse:
        """Compact a conversation trace to save context window tokens."""
        payload: dict = {"conversation_text": conversation_text}
        if focus_areas is not None:
            payload["focus_areas"] = focus_areas

        resp = await self._client.post("/v1/memories/compact", json=payload)
        resp.raise_for_status()
        return CompactResponse.model_validate(resp.json())

    async def close(self) -> None:
        """Close the HTTP client."""
        await self._client.aclose()

    async def __aenter__(self) -> "Memory":
        return self

    async def __aexit__(self, *args) -> None:
        await self.close()


class StoreMemoriesClient:
    def __init__(self, client: httpx.AsyncClient):
        self._client = client

    async def list(self, store_id: str | UUID) -> list[MemoryResult]:
        resp = await self._client.get(f"/v1/memory_stores/{store_id}/memories")
        resp.raise_for_status()
        return [MemoryResult.model_validate(r) for r in resp.json()]

    async def create(
        self, store_id: str | UUID, path: str, content: str
    ) -> MemoryResult:
        resp = await self._client.post(
            f"/v1/memory_stores/{store_id}/memories",
            json={"path": path, "content": content},
        )
        resp.raise_for_status()
        return MemoryResult.model_validate(resp.json())

    async def get(self, store_id: str | UUID, path_or_id: str | UUID) -> MemoryResult:
        resp = await self._client.get(
            f"/v1/memory_stores/{store_id}/memories/{path_or_id}"
        )
        resp.raise_for_status()
        return MemoryResult.model_validate(resp.json())

    async def update(
        self, store_id: str | UUID, path_or_id: str | UUID, content: str
    ) -> MemoryResult:
        resp = await self._client.post(
            f"/v1/memory_stores/{store_id}/memories/{path_or_id}",
            json={"content": content},
        )
        resp.raise_for_status()
        return MemoryResult.model_validate(resp.json())

    async def list_versions(
        self, store_id: str | UUID, path_or_id: str | UUID
    ) -> list[MemoryVersionRecord]:
        resp = await self._client.get(
            f"/v1/memory_stores/{store_id}/memories/{path_or_id}/versions"
        )
        resp.raise_for_status()
        return [MemoryVersionRecord.model_validate(r) for r in resp.json()]


class MemoryStoresClient:
    def __init__(self, client: httpx.AsyncClient):
        self._client = client
        self.memories = StoreMemoriesClient(client)

    async def create(
        self, name: str, description: str | None = None
    ) -> MemoryStoreRecord:
        payload = {"name": name}
        if description:
            payload["description"] = description
        resp = await self._client.post("/v1/memory_stores", json=payload)
        resp.raise_for_status()
        return MemoryStoreRecord.model_validate(resp.json())

    async def list(self) -> list[MemoryStoreRecord]:
        resp = await self._client.get("/v1/memory_stores")
        resp.raise_for_status()
        return [MemoryStoreRecord.model_validate(r) for r in resp.json()]

    async def get(self, store_id: str | UUID) -> MemoryStoreRecord:
        resp = await self._client.get(f"/v1/memory_stores/{store_id}")
        resp.raise_for_status()
        return MemoryStoreRecord.model_validate(resp.json())

    async def archive(self, store_id: str | UUID) -> None:
        resp = await self._client.post(f"/v1/memory_stores/{store_id}/archive")
        resp.raise_for_status()
