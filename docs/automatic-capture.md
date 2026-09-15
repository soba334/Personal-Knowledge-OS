# Automatic Knowledge Capture

Personal Knowledge OS can accept normalized AI conversation events and turn grounded, durable facts into active memory without manual note-taking.

## Flow

```text
AI client / agent / connector
        |
        | POST /v1/captures
        v
raw source (truth)
        |
        +--> chunk + lexical/vector indexing
        |
        +--> durable extract_memories job
                 |
                 v
          OpenAI-compatible LLM
                 |
                 v
          deterministic verifier
          - allowed memory type
          - exact/whitespace-normalized source quote
          - finite confidence/importance
          - basic secret-material rejection
          - valid supersession target
                 |
                 v
          threshold gate
                 |
                 +--> active memory (auto-promote)
                 +--> candidate or discard (policy)
```

The model never writes directly to the `active` memory table. It proposes structured candidates; the service validates them and performs the state transition.

## One-time configuration

Set an OpenAI-compatible chat-completions provider. `PKOS_MEMORY_BASE_URL` and `PKOS_MEMORY_API_KEY` fall back to `PKOS_OPENAI_BASE_URL` and `PKOS_OPENAI_API_KEY`.

```env
PKOS_MEMORY_EXTRACTION_ENABLED=true
PKOS_MEMORY_AUTO_PROMOTE_ENABLED=true
PKOS_MEMORY_AUTO_PROMOTE_MIN_CONFIDENCE=0.92
PKOS_MEMORY_AUTO_PROMOTE_MIN_IMPORTANCE=0.65
PKOS_MEMORY_KEEP_UNPROMOTED_CANDIDATES=false
```

When automatic promotion is enabled and `PKOS_MEMORY_KEEP_UNPROMOTED_CANDIDATES=false`, low-confidence or low-importance proposals are discarded instead of creating a review backlog.

## Capture API

AI clients should call `POST /v1/captures` after a completed turn or short conversation batch. Use a stable `external_id` supplied by the client when retries are possible; the existing source uniqueness constraint then provides idempotency.

```json
{
  "kind": "conversation",
  "title": "Chat session",
  "provider": "my-agent",
  "session_id": "session-123",
  "external_id": "session-123:turn-42",
  "occurred_at": "2026-09-15T14:00:00Z",
  "messages": [
    {"role": "user", "content": "Bifrostでいきたいです"},
    {"role": "assistant", "content": "Bifrostを採用方針として整理します"}
  ],
  "metadata": {"surface": "chat"}
}
```

The full normalized conversation is stored as a source before any model call. If extraction or the upstream LLM fails, the source remains durable and the PostgreSQL job queue retries extraction.

## Integration rule

Connectors should capture automatically; the person should not have to decide what to save. Prefer one capture per completed interaction with a stable event ID. Do not send raw credential stores, environment files, password-manager exports, or other secret-bearing sources to the capture API.

The extractor is explicitly instructed not to treat assistant-only claims as personal facts and not to obey instructions embedded inside captured source content.
