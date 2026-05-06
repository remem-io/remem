"""Configuration for the remem Python SDK."""

from __future__ import annotations

import os
from dataclasses import dataclass, field


@dataclass
class RememConfig:
    """Configuration for connecting to a remem server."""

    base_url: str = field(
        default_factory=lambda: os.environ.get(
            "REMEM_BASE_URL", "http://localhost:7474"
        )
    )
    api_key: str | None = field(default_factory=lambda: os.environ.get("REMEM_API_KEY"))
    project: str = field(
        default_factory=lambda: os.environ.get("REMEM_PROJECT", "default")
    )
    reasoning_model: str = field(
        default_factory=lambda: os.environ.get(
            "REMEM_REASONING_MODEL", "claude-sonnet-4-5"
        )
    )
    scoring_model: str = field(
        default_factory=lambda: os.environ.get(
            "REMEM_SCORING_MODEL", "claude-haiku-4-5"
        )
    )
    timeout: float = 30.0
