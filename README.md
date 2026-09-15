# Personal Knowledge OS

A production-oriented, source-grounded personal memory service for AI applications.

**Core invariant:** source is truth. Memories, embeddings, lexical indexes and graph edges are derived state and can be rebuilt from source records.

## v1 capabilities

- source ingestion with SHA-256 integrity metadata
- deterministic overlapping chunking and asynchronous embedding jobs
- OpenAI-compatible embedding provider with lexical-only degradation
- source-grounded memory candidates with manual approval/rejection
- temporal memory supersession (`active -> superseded`) without history loss
- temporal entity/relation schema for later graph expansion
- hybrid lexical + dense retrieval with Reciprocal Rank Fusion (RRF)
- PostgreSQL 18 + VectorChord + PGroonga deployment
- durable PostgreSQL job queue using `FOR UPDATE SKIP LOCKED`, leases and retry backoff
- audit log and retrieval telemetry
- constant-time Bearer authentication scoped to `PKOS_OWNER_ID`
- health/readiness endpoints, Docker deployment and CI

> Automatic memory extraction is intentionally non-activating in v1. Model output must pass a verifier/review boundary before it may become active memory.

## Quick start

```bash
cp .env.example .env
# Set PKOS_API_KEY to a random value of at least 32 bytes.
docker compose up --build
```

API defaults to `http://localhost:8080`.

### Ingest a source

```bash
curl -X POST http://localhost:8080/v1/sources \
  -H "authorization: Bearer $PKOS_API_KEY" \
  -H 'content-type: application/json' \
  -d '{
    "kind":"note",
    "title":"Architecture decision",
    "content":"We decided to keep source records as evidence and treat memory as derived state.",
    "occurred_at":"2026-09-15T12:00:00Z"
  }'
```

### Search

```bash
curl -X POST http://localhost:8080/v1/search \
  -H "authorization: Bearer $PKOS_API_KEY" \
  -H 'content-type: application/json' \
  -d '{"query":"source of truth memory","limit":10,"include_memories":true}'
```

### Create and approve a memory candidate

```bash
curl -X POST http://localhost:8080/v1/memories/candidates \
  -H "authorization: Bearer $PKOS_API_KEY" \
  -H 'content-type: application/json' \
  -d '{
    "memory_type":"decision",
    "statement":"Source records are the evidence layer; memories are derived state.",
    "confidence":1.0,
    "importance":0.9,
    "evidence_source_ids":["<source-uuid>"]
  }'

curl -X POST http://localhost:8080/v1/memories/<memory-uuid>/approve \
  -H "authorization: Bearer $PKOS_API_KEY" \
  -H 'content-type: application/json' \
  -d '{"supersedes_id":null}'
```

## HTTP API

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/healthz` | process liveness |
| GET | `/readyz` | database readiness |
| POST | `/v1/sources` | ingest source and enqueue derived indexing |
| GET | `/v1/sources/{id}` | retrieve source evidence |
| POST | `/v1/search` | hybrid source + active-memory retrieval |
| GET | `/v1/memories` | list memories by lifecycle state |
| POST | `/v1/memories/candidates` | create evidence-backed candidate |
| GET | `/v1/memories/{id}` | inspect memory and evidence links |
| POST | `/v1/memories/{id}/approve` | activate and optionally supersede old memory |
| POST | `/v1/memories/{id}/reject` | reject candidate |

## Repository layout

```text
src/
  api/          HTTP boundary
  services/     chunking, retrieval, embeddings and worker logic
  config.rs     validated runtime configuration
  models.rs     API/domain DTOs
  storage.rs    SQL persistence boundary
migrations/     source-of-truth schema
Dockerfile      API image
docker/         PostgreSQL extension image
```
