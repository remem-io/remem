"""Async-first Python client for the remem REST API."""

from __future__ import annotations

from datetime import datetime
from uuid import UUID

import httpx

from rememhq.config import RememConfig
from rememhq.models import (
    ConsolidationReport,
    ForgetMode,
    MemoryResult,
    MemoryType,
    StoreResponse,
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

    async def close(self) -> None:
        """Close the HTTP client."""
        await self._client.aclose()

    async def __aenter__(self) -> "Memory":
        return self

    async def __aexit__(self, *args) -> None:
        await self.close()
