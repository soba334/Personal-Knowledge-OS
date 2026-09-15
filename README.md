# Personal Knowledge OS

A production-oriented, source-grounded personal memory service for AI applications.

**Core invariant:** source is truth. Memories, embeddings, lexical indexes and graph edges are derived state and can be rebuilt from immutable source records.

## v1 capabilities

- append-only source ingestion with SHA-256 integrity metadata
- deterministic chunking and asynchronous embedding jobs
- OpenAI-compatible embedding provider (optional)
- memory candidates with evidence, manual approval/rejection, supersession and temporal validity
- temporal entities and relations
- hybrid lexical + dense retrieval with Reciprocal Rank Fusion (RRF)
- PostgreSQL 18 + VectorChord + PGroonga deployment
- durable PostgreSQL job queue (`FOR UPDATE SKIP LOCKED`)
- audit log and retrieval telemetry
- health/readiness endpoints
- migrations, CI, Docker, tests and operational docs

## Quick start

```bash
cp .env.example .env
docker compose up --build -d postgres
cargo run
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
    "content":"We decided to keep source records immutable and treat memory as derived state.",
    "occurred_at":"2026-09-15T12:00:00Z"
  }'
```

### Search

```bash
curl -X POST http://localhost:8080/v1/search \
  -H "authorization: Bearer $PKOS_API_KEY" \
  -H 'content-type: application/json' \
  -d '{
    "query":"source of truth memory",
    "limit":10
  }'
```

## Repository layout

```text
src/
  api/          HTTP boundary
  services/     retrieval, embeddings, memory and worker logic
  config.rs     validated runtime configuration
  models.rs     API/domain DTOs
  storage.rs    SQL persistence boundary
migrations/     source-of-truth schema
Dockerfile      API image
docker/         PostgreSQL extension image
docs/           architecture and operations
```

See [`docs/00_README.md`](docs/00_README.md) for the full design map.
