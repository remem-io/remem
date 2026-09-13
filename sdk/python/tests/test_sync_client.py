"""Tests for the synchronous client (SyncMemory).

SyncMemory previously only implemented store() and recall(), while the
async Memory client implemented the full API surface. These tests cover
the methods added to bring SyncMemory to parity, plus the paginated
response envelope shared with the async client.
"""

from __future__ import annotations

import uuid
from datetime import UTC, datetime

import httpx
import respx

BASE = "http://localhost:7474"


def _memory_result(content: str = "test content", **kw) -> dict:
    return {
        "id": str(uuid.uuid4()),
        "content": content,
        "importance": kw.get("importance", 5.0),
        "tags": kw.get("tags", []),
        "memory_type": kw.get("memory_type", "fact"),
        "created_at": datetime.now(UTC).isoformat(),
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
        "created_at": datetime.now(UTC).isoformat(),
    }


class TestSyncRecallSearch:
    def test_recall_unwraps_paginated_envelope(self):
        from rememhq import SyncMemory

        payload = {"data": [_memory_result("memory A"), _memory_result("memory B")], "next_cursor": None}
        with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/recall").mock(return_value=httpx.Response(200, json=payload))
            with SyncMemory(base_url=BASE) as m:
                results = m.recall("test query")
        assert len(results) == 2
        assert results[0].content == "memory A"

    def test_search_unwraps_paginated_envelope(self):
        from rememhq import SyncMemory

        payload = {"data": [_memory_result()], "next_cursor": None}
        with respx.mock(base_url=BASE) as mock:
            mock.get("/v1/memories/search").mock(return_value=httpx.Response(200, json=payload))
            with SyncMemory(base_url=BASE) as m:
                results = m.search("deploy")
        assert len(results) == 1


class TestSyncParityMethods:
    def test_update_sends_patch(self):
        from rememhq import SyncMemory

        mem_id = str(uuid.uuid4())
        with respx.mock(base_url=BASE) as mock:
            mock.patch(f"/v1/memories/{mem_id}").mock(
                return_value=httpx.Response(200, json={"id": mem_id, "content": "updated"})
            )
            with SyncMemory(base_url=BASE) as m:
                r = m.update(mem_id, content="updated")
        assert r["content"] == "updated"

    def test_forget_sends_delete(self):
        from rememhq import SyncMemory
        from rememhq.models import ForgetMode

        mem_id = str(uuid.uuid4())
        with respx.mock(base_url=BASE) as mock:
            mock.delete(f"/v1/memories/{mem_id}").mock(return_value=httpx.Response(200, json={"success": True}))
            with SyncMemory(base_url=BASE) as m:
                r = m.forget(mem_id, mode=ForgetMode.ARCHIVE)
        assert r["success"] is True

    def test_consolidate_returns_report(self):
        from rememhq import ConsolidationReport, SyncMemory

        payload = {
            "session_id": "sess-abc",
            "new_facts": 2,
            "updated_facts": 0,
            "contradictions": [],
            "knowledge_graph_updates": [],
        }
        with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/sessions/sess-abc/consolidate").mock(return_value=httpx.Response(200, json=payload))
            with SyncMemory(base_url=BASE) as m:
                r = m.consolidate("sess-abc")
        assert isinstance(r, ConsolidationReport)
        assert r.new_facts == 2

    def test_decay_sends_factor(self):
        from rememhq import SyncMemory

        with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memories/decay").mock(
                return_value=httpx.Response(200, json={"success": True, "archived_count": 1})
            )
            with SyncMemory(base_url=BASE) as m:
                r = m.decay(factor=0.5)
        assert r["archived_count"] == 1

    def test_get_health(self):
        from rememhq import SyncMemory

        with respx.mock(base_url=BASE) as mock:
            mock.get("/health").mock(return_value=httpx.Response(200, json={"status": "ok"}))
            with SyncMemory(base_url=BASE) as m:
                r = m.get_health()
        assert r["status"] == "ok"


class TestSyncStores:
    def test_stores_create_and_list(self):
        from rememhq import SyncMemory

        with respx.mock(base_url=BASE) as mock:
            mock.post("/v1/memory_stores").mock(
                return_value=httpx.Response(
                    201,
                    json={
                        "id": "store-1",
                        "name": "notes",
                        "description": None,
                        "created_at": datetime.now(UTC).isoformat(),
                        "archived_at": None,
                    },
                )
            )
            mock.get("/v1/memory_stores").mock(
                return_value=httpx.Response(
                    200,
                    json=[
                        {
                            "id": "store-1",
                            "name": "notes",
                            "description": None,
                            "created_at": datetime.now(UTC).isoformat(),
                            "archived_at": None,
                        }
                    ],
                )
            )
            with SyncMemory(base_url=BASE) as m:
                created = m.stores.create("notes")
                listed = m.stores.list()

        assert created.name == "notes"
        assert len(listed) == 1
        assert listed[0].id == "store-1"

    def test_store_memories_create_and_list(self):
        from rememhq import SyncMemory

        store_id = "store-1"
        with respx.mock(base_url=BASE) as mock:
            mock.post(f"/v1/memory_stores/{store_id}/memories").mock(
                return_value=httpx.Response(201, json=_memory_result("stored note"))
            )
            mock.get(f"/v1/memory_stores/{store_id}/memories").mock(
                return_value=httpx.Response(200, json=[_memory_result("stored note")])
            )
            with SyncMemory(base_url=BASE) as m:
                created = m.stores.memories.create(store_id, "notes/todo.md", "buy milk")
                listed = m.stores.memories.list(store_id)

        assert created.content == "stored note"
        assert len(listed) == 1
