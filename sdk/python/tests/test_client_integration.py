"""Mock-based integration tests covering every Memory client endpoint."""

from __future__ import annotations

import json
import uuid
from datetime import datetime, timezone

import httpx
import pytest
import respx

BASE = "http://localhost:7474"

# ---------------------------------------------------------------------------
# Shared fixtures
# ---------------------------------------------------------------------------


def _memory_result(content: str = "test content", **kw) -> dict:
    return {
        "id": str(uuid.uuid4()),
        "content": content,
        "importance": kw.get("importance", 5.0),
        "tags": kw.get("tags", []),
        "memory_type": kw.get("memory_type", "fact"),
        "created_at": datetime.now(timezone.utc).isoformat(),
        "source_session": None,
        "similarity": kw.get("similarity", 0.85),
        "decay_score": kw.get("decay_score", 1.0),
        "reasoning": kw.get("reasoning", None),
    }


def _store_response(**kw) -> dict:
    return {
        "id": str(uuid.uuid4()),
        "importance": kw.get("importance", 7.0),
        "tags": kw.get("tags", []),
        "created_at": datetime.now(timezone.utc).isoformat(),
    }


# ---------------------------------------------------------------------------
# store()
# ---------------------------------------------------------------------------


class TestStore:
    @pytest.mark.asyncio
    async def test_store_minimal(self):
        from rememhq import Memory

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories").mock(
                return_value=httpx.Response(201, json=_store_response())
            )
            async with Memory(base_url=BASE) as m:
                r = await m.store("hello world")
            assert r.importance == 7.0

    @pytest.mark.asyncio
    async def test_store_sends_tags_and_importance(self):
        from rememhq import Memory

        captured = {}

        async def handler(request: httpx.Request):
            captured["body"] = json.loads(request.content)
            return httpx.Response(
                201, json=_store_response(importance=9.0, tags=["a", "b"])
            )

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                r = await m.store("tagged", tags=["a", "b"], importance=9.0)

        assert captured["body"]["tags"] == ["a", "b"]
        assert captured["body"]["importance"] == 9.0
        assert r.importance == 9.0

    @pytest.mark.asyncio
    async def test_store_sends_ttl_and_type(self):
        from rememhq import Memory
        from rememhq.models import MemoryType

        captured = {}

        async def handler(request: httpx.Request):
            captured["body"] = json.loads(request.content)
            return httpx.Response(201, json=_store_response())

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                await m.store("proc", memory_type=MemoryType.PROCEDURE, ttl_days=30)

        assert captured["body"]["ttl_days"] == 30
        assert captured["body"]["memory_type"] == "procedure"

    @pytest.mark.asyncio
    async def test_store_http_error_raises(self):
        from rememhq import Memory

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories").mock(
                return_value=httpx.Response(500, text="oops")
            )
            async with Memory(base_url=BASE) as m:
                with pytest.raises(httpx.HTTPStatusError):
                    await m.store("fail")


# ---------------------------------------------------------------------------
# recall()
# ---------------------------------------------------------------------------


class TestRecall:
    @pytest.mark.asyncio
    async def test_recall_returns_list(self):
        from rememhq import Memory

        payload = [_memory_result("memory A"), _memory_result("memory B")]
        async with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/recall").mock(
                return_value=httpx.Response(200, json=payload)
            )
            async with Memory(base_url=BASE) as m:
                results = await m.recall("test query")
        assert len(results) == 2
        assert results[0].content == "memory A"

    @pytest.mark.asyncio
    async def test_recall_passes_query_params(self):
        from rememhq import Memory

        captured = {}

        async def handler(request: httpx.Request):
            captured["params"] = dict(request.url.params)
            return httpx.Response(200, json=[])

        async with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/recall").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                await m.recall("my query", limit=3, filter_tags=["t1", "t2"])

        assert captured["params"]["q"] == "my query"
        assert captured["params"]["limit"] == "3"
        assert captured["params"]["filter_tags"] == "t1,t2"

    @pytest.mark.asyncio
    async def test_recall_with_memory_type_filter(self):
        from rememhq import Memory
        from rememhq.models import MemoryType

        captured = {}

        async def handler(request: httpx.Request):
            captured["params"] = dict(request.url.params)
            return httpx.Response(200, json=[])

        async with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/recall").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                await m.recall("query", memory_type=MemoryType.PROCEDURE)

        assert captured["params"]["memory_type"] == "procedure"

    @pytest.mark.asyncio
    async def test_recall_empty_response(self):
        from rememhq import Memory

        async with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/recall").mock(
                return_value=httpx.Response(200, json=[])
            )
            async with Memory(base_url=BASE) as m:
                results = await m.recall("nothing")
        assert results == []


# ---------------------------------------------------------------------------
# search()
# ---------------------------------------------------------------------------


class TestSearch:
    @pytest.mark.asyncio
    async def test_search_returns_list(self):
        from rememhq import Memory

        payload = [_memory_result()]
        async with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/search").mock(
                return_value=httpx.Response(200, json=payload)
            )
            async with Memory(base_url=BASE) as m:
                results = await m.search("deploy")
        assert len(results) == 1

    @pytest.mark.asyncio
    async def test_search_passes_limit(self):
        from rememhq import Memory

        captured = {}

        async def handler(request: httpx.Request):
            captured["params"] = dict(request.url.params)
            return httpx.Response(200, json=[])

        async with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/search").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                await m.search("query", limit=5)

        assert captured["params"]["limit"] == "5"


# ---------------------------------------------------------------------------
# update()
# ---------------------------------------------------------------------------


class TestUpdate:
    @pytest.mark.asyncio
    async def test_update_sends_patch(self):
        from rememhq import Memory

        mem_id = str(uuid.uuid4())
        captured = {}

        async def handler(request: httpx.Request):
            captured["method"] = request.method
            captured["body"] = json.loads(request.content)
            return httpx.Response(
                200,
                json={
                    "id": mem_id,
                    "content": "new content",
                    "importance": 8.0,
                    "tags": [],
                    "updated_at": "2026-01-01",
                },
            )

        async with respx.mock(base_url=BASE) as mock:
            mock.patch(f"/v1/memories/{mem_id}").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                r = await m.update(mem_id, content="new content", importance=8.0)

        assert captured["method"] == "PATCH"
        assert captured["body"]["content"] == "new content"
        assert r["importance"] == 8.0


# ---------------------------------------------------------------------------
# forget()
# ---------------------------------------------------------------------------


class TestForget:
    @pytest.mark.asyncio
    async def test_forget_delete_mode(self):
        from rememhq import Memory
        from rememhq.models import ForgetMode

        mem_id = str(uuid.uuid4())
        captured = {}

        async def handler(request: httpx.Request):
            captured["method"] = request.method
            captured["url"] = str(request.url)
            return httpx.Response(200, json={"success": True})

        async with respx.mock(base_url=BASE) as mock:
            mock.delete(f"/v1/memories/{mem_id}").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                r = await m.forget(mem_id, mode=ForgetMode.DELETE)

        assert captured["method"] == "DELETE"
        assert "mode=delete" in captured["url"]
        assert r["success"] is True

    @pytest.mark.asyncio
    async def test_forget_archive_mode(self):
        from rememhq import Memory
        from rememhq.models import ForgetMode

        mem_id = str(uuid.uuid4())
        captured = {}

        async def handler(request: httpx.Request):
            captured["url"] = str(request.url)
            return httpx.Response(200, json={"success": True})

        async with respx.mock(base_url=BASE) as mock:
            mock.delete(f"/v1/memories/{mem_id}").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                await m.forget(mem_id, mode=ForgetMode.ARCHIVE)

        assert "mode=archive" in captured["url"]


# ---------------------------------------------------------------------------
# consolidate()
# ---------------------------------------------------------------------------


class TestConsolidate:
    @pytest.mark.asyncio
    async def test_consolidate_returns_report(self):
        from rememhq import Memory, ConsolidationReport

        payload = {
            "session_id": "sess-abc",
            "new_facts": 3,
            "updated_facts": 1,
            "contradictions": [],
            "knowledge_graph_updates": [],
        }
        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/sessions/sess-abc/consolidate").mock(
                return_value=httpx.Response(200, json=payload)
            )
            async with Memory(base_url=BASE) as m:
                r = await m.consolidate("sess-abc")
        assert isinstance(r, ConsolidationReport)
        assert r.new_facts == 3
        assert r.session_id == "sess-abc"

    @pytest.mark.asyncio
    async def test_consolidate_sends_model(self):
        from rememhq import Memory

        captured = {}

        async def handler(request: httpx.Request):
            captured["body"] = json.loads(request.content)
            return httpx.Response(
                200,
                json={
                    "session_id": "s1",
                    "new_facts": 0,
                    "updated_facts": 0,
                    "contradictions": [],
                    "knowledge_graph_updates": [],
                },
            )

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/sessions/s1/consolidate").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                await m.consolidate("s1", model="gemini-2.0-flash")

        assert captured["body"].get("model") == "gemini-2.0-flash"


# ---------------------------------------------------------------------------
# decay()
# ---------------------------------------------------------------------------


class TestDecay:
    @pytest.mark.asyncio
    async def test_decay_sends_factor(self):
        from rememhq import Memory

        captured = {}

        async def handler(request: httpx.Request):
            captured["body"] = json.loads(request.content)
            return httpx.Response(200, json={"success": True, "archived_count": 2})

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories/decay").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                r = await m.decay(factor=0.7)

        assert captured["body"]["factor"] == pytest.approx(0.7)
        assert r["archived_count"] == 2


# ---------------------------------------------------------------------------
# Authentication
# ---------------------------------------------------------------------------


class TestAuth:
    @pytest.mark.asyncio
    async def test_api_key_sent_as_bearer(self):
        from rememhq import Memory

        captured = {}

        async def handler(request: httpx.Request):
            captured["auth"] = request.headers.get("authorization", "")
            return httpx.Response(201, json=_store_response())

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories").mock(side_effect=handler)
            async with Memory(base_url=BASE, api_key="secret-key") as m:
                await m.store("test")

        assert captured["auth"] == "Bearer secret-key"

    @pytest.mark.asyncio
    async def test_no_auth_header_without_key(self):
        from rememhq import Memory

        captured = {}

        async def handler(request: httpx.Request):
            captured["auth"] = request.headers.get("authorization")
            return httpx.Response(201, json=_store_response())

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories").mock(side_effect=handler)
            async with Memory(base_url=BASE) as m:
                await m.store("test")

        assert captured["auth"] is None


# ---------------------------------------------------------------------------
# Context manager
# ---------------------------------------------------------------------------


class TestContextManager:
    @pytest.mark.asyncio
    async def test_async_context_manager(self):
        from rememhq import Memory

        async with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories").mock(
                return_value=httpx.Response(201, json=_store_response())
            )
            async with Memory(base_url=BASE) as m:
                r = await m.store("ctx test")
            assert r.id is not None
