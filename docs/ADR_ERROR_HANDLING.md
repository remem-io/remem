# Architectural Decision Record (ADR) 001: Error Handling Strategy

## Status
Accepted

## Context
The `remem` Rust codebase contains both library core components (`rememhq-core`, `libremem`, domain validators) and application entrypoints / CLI / REST / MCP transport servers (`rememhq-api`, `rememhq-mcp`, `rememhq-cli`).

Historically, Rust applications often mix `anyhow` and `thiserror` without explicit boundaries, leading to potential inconsistency across crates and modules.

## Decision
We enforce a strict separation of concerns for error handling across the repository:

1. **`thiserror` for Library Boundaries and Typed Errors**:
   - Use `thiserror::Error` for domain-specific, programmatic, or public library error types (e.g. `rememhq-core::harness::validator::ValidationError`, storage engines, provider error enums).
   - Use when callers need to match on error variants (e.g. distinguishing `InvalidSchema`, `TypeMismatch`, `NotFound`, or `PermissionDenied`).

2. **`anyhow` for Application Glue & Internal Execution**:
   - Use `anyhow::Result` for application code, CLI commands, HTTP request handlers, MCP tool runners, and internal pipeline glue.
   - Use when errors are propagated up to loggers, user displays, or top-level HTTP status converters without requiring programmatic matching.

## Consequences
- Prevents error idiom drift as new crates, tools, and reasoning loops are added.
- Ensures public/library error contracts stay strongly typed while keeping application glue lightweight and concise.
