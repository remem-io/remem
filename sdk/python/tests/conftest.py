"""Shared test fixtures for the remem Python SDK."""

import pytest


@pytest.fixture
def base_url():
    """Default API base URL for testing."""
    return "http://localhost:7474"


@pytest.fixture
def mock_memory_data():
    """Sample memory data for testing."""
    return {
        "content": "Alice is a software engineer",
        "tags": ["bio", "profession"],
        "importance": 7.5,
        "memory_type": "fact",
    }


@pytest.fixture
def mock_recall_response():
    """Sample recall response structure.

    Matches the real API's ``PaginatedResponse<MemoryResult>`` envelope
    (see rememhq-api/src/models.rs) — a ``data`` array plus a
    ``next_cursor``, not a bare JSON array.
    """
    return {
        "data": [
            {
                "id": "00000000-0000-0000-0000-000000000001",
                "content": "Alice is a software engineer",
                "importance": 7.5,
                "tags": ["bio"],
                "memory_type": "fact",
                "created_at": "2026-01-01T00:00:00Z",
                "source_session": None,
                "similarity": 0.95,
                "reasoning": "Directly relevant to query about profession",
            }
        ],
        "next_cursor": None,
    }
