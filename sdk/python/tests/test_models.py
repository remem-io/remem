"""Unit tests for remem SDK Pydantic models."""

import uuid
from datetime import datetime
from rememhq.models import MemoryType, MemoryResult


class TestMemoryType:
    def test_fact_value(self):
        assert MemoryType.FACT == "fact"

    def test_procedure_value(self):
        assert MemoryType.PROCEDURE == "procedure"

    def test_preference_value(self):
        assert MemoryType.PREFERENCE == "preference"

    def test_decision_value(self):
        assert MemoryType.DECISION == "decision"


class TestMemoryResult:
    def test_create_minimal(self):
        result = MemoryResult(
            id=uuid.uuid4(),
            content="test content",
            importance=5.0,
            tags=[],
            memory_type=MemoryType.FACT,
            created_at=datetime.utcnow(),
        )
        assert result.content == "test content"
        assert result.importance == 5.0
        assert result.similarity == 0.0
        assert result.decay_score == 1.0
        assert result.reasoning is None

    def test_create_with_tags(self):
        result = MemoryResult(
            id=uuid.uuid4(),
            content="tagged",
            importance=5.0,
            tags=["a", "b"],
            memory_type=MemoryType.FACT,
            created_at=datetime.utcnow(),
        )
        assert len(result.tags) == 2
        assert "a" in result.tags

    def test_serialization(self):
        result = MemoryResult(
            id=uuid.uuid4(),
            content="test",
            importance=5.0,
            tags=[],
            memory_type=MemoryType.FACT,
            created_at=datetime.utcnow(),
        )
        data = result.model_dump()
        assert "content" in data
        assert "importance" in data
        assert "similarity" in data

    def test_optional_reasoning(self):
        result = MemoryResult(
            id=uuid.uuid4(),
            content="no reasoning",
            importance=5.0,
            tags=[],
            memory_type=MemoryType.FACT,
            created_at=datetime.utcnow(),
            similarity=0.5,
            reasoning="because",
        )
        assert result.reasoning == "because"
