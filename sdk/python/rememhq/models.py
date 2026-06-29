"""Pydantic v2 models for the remem REST API."""

from __future__ import annotations

from datetime import datetime
from enum import Enum
from typing import Optional
from uuid import UUID

from pydantic import BaseModel, Field


class MemoryType(str, Enum):
    FACT = "fact"
    PROCEDURE = "procedure"
    PREFERENCE = "preference"
    DECISION = "decision"


class ForgetMode(str, Enum):
    DELETE = "delete"
    DECAY = "decay"
    ARCHIVE = "archive"


class StoreResponse(BaseModel):
    id: UUID
    importance: float
    tags: list[str]
    created_at: datetime


class CompactResponse(BaseModel):
    compressed_context: str
    original_length: int
    compressed_length: int


class MemoryResult(BaseModel):
    id: UUID
    content: str
    importance: float
    tags: list[str]
    memory_type: MemoryType
    created_at: datetime
    source_session: Optional[str] = None
    similarity: float = 0.0
    decay_score: float = 1.0
    reasoning: Optional[str] = None


class ConsolidationReport(BaseModel):
    session_id: str
    new_facts: int
    updated_facts: int
    contradictions: list[Contradiction] = Field(default_factory=list)
    knowledge_graph_updates: list[KnowledgeGraphUpdate] = Field(default_factory=list)


class Contradiction(BaseModel):
    existing_memory_id: UUID
    new_content: str
    existing_content: str
    explanation: str


class KnowledgeGraphUpdate(BaseModel):
    subject: str
    predicate: str
    object: str


# Fix forward references
ConsolidationReport.model_rebuild()

class MemoryStoreRecord(BaseModel):
    id: str
    name: str
    description: Optional[str] = None
    created_at: datetime
    archived_at: Optional[datetime] = None

class MemoryVersionRecord(BaseModel):
    id: str
    store_id: str
    memory_id: UUID
    operation: str
    content: str
    content_sha256: str
    created_at: datetime
