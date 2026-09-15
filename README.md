# Personal Knowledge OS

A production-oriented, source-grounded personal memory service for AI applications.

**Core invariant:** source is truth. Memories, embeddings, lexical indexes, and graph edges are derived or source-linked state and must remain auditable back to durable evidence.

## v1 capabilities

- source ingestion with SHA-256 integrity metadata
- deterministic overlapping chunking and asynchronous embedding jobs
- OpenAI-compatible embedding provider with lexical-only degradation
- source-grounded memory candidates with manual approval/rejection
- temporal memory supersession (`active -> superseded`) without history loss
- temporal entity/relation graph with explicit validity windows and source links
- source timeline ordered by occurrence time with ingestion-time fallback
- hybrid lexical + dense retrieval with Reciprocal Rank Fusion (RRF)
- PostgreSQL 18 + VectorChord + PGroonga deployment
- durable PostgreSQL job queue using `FOR UPDATE SKIP LOCKED`, leases, and retry backoff
- audit log and retrieval telemetry
- constant-time Bearer authentication scoped to `PKOS_OWNER_ID`
- health/readiness endpoints, Docker deployment, CI, and retrieval evaluation harness

> Automatic memory extraction is intentionally non-activating in v1. Model output must pass a verifier/review boundary before it may become active memory.

> Graph writes are also explicit API operations in v1. The service does not allow unverified LLM output to silently rewrite entity or relation history.

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

When the embedding provider is unavailable, the endpoint continues with lexical retrieval and returns `degraded: true` instead of taking retrieval offline.

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

### Create a temporal graph relation

Create entities first:

```bash
curl -X POST http://localhost:8080/v1/entities \
  -H "authorization: Bearer $PKOS_API_KEY" \
  -H 'content-type: application/json' \
  -d '{
    "entity_type":"project",
    "canonical_name":"Personal Knowledge OS",
    "attributes":{"status":"active"},
    "aliases":["PKOS"]
  }'
```

Then create an evidence-linked relation between two entity IDs:

```bash
curl -X POST http://localhost:8080/v1/relations \
  -H "authorization: Bearer $PKOS_API_KEY" \
  -H 'content-type: application/json' \
  -d '{
    "subject_entity_id":"<subject-uuid>",
    "predicate":"uses",
    "object_entity_id":"<object-uuid>",
    "valid_from":"2026-09-15T12:00:00Z",
    "confidence":1.0,
    "source_id":"<source-uuid>"
  }'
```

Relations are not deleted to represent change. Close an old fact with `POST /v1/relations/{id}/close`; historical graph queries remain available through `include_historical=true`.

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
| GET | `/v1/entities` | list/filter entities and aliases |
| POST | `/v1/entities` | create a validated entity |
| GET | `/v1/entities/{id}` | retrieve an entity |
| GET | `/v1/entities/{id}/graph` | retrieve incoming/outgoing graph as-of a timestamp |
| POST | `/v1/relations` | create a temporal relation, optionally linked to source evidence |
| GET | `/v1/relations/{id}` | retrieve a relation |
| POST | `/v1/relations/{id}/close` | end a relation validity interval without deleting history |
| GET | `/v1/timeline` | list source events in reverse chronological order |

All `/v1/*` routes require `Authorization: Bearer <PKOS_API_KEY>`. Owner scope is taken from trusted runtime configuration (`PKOS_OWNER_ID`), never from request input.

## Retrieval evaluation

A Golden JSONL template lives at `eval/golden.example.jsonl`. The evaluator reports Recall@K, MRR, and abstention accuracy and can fail CI/automation when thresholds are not met.

```bash
python3 scripts/evaluate.py \
  --base-url http://localhost:8080 \
  --api-key "$PKOS_API_KEY" \
  --dataset eval/golden.example.jsonl \
  --k 10
```

Populate a private or sanitized Golden dataset representative of real use before using retrieval thresholds as a release gate.

## Operational invariants

- **Evidence first:** source records are durable evidence; memories cannot be created without existing evidence source IDs.
- **No silent activation:** model-generated memory stays outside the active memory set until an explicit verification/approval boundary passes.
- **Temporal history:** superseded memories and closed relations remain queryable instead of being destructively overwritten.
- **Tenant boundary:** application reads/writes are scoped to configured `PKOS_OWNER_ID`.
- **Graceful retrieval degradation:** embedding failure does not disable lexical retrieval.
- **Durable async work:** embedding jobs use PostgreSQL leases, `SKIP LOCKED`, retries, and terminal failure state.
- **Auditable mutation:** source ingestion, memory lifecycle changes, and graph mutations emit audit records.
- **Bounded APIs:** text lengths, pagination limits, confidence ranges, relation time intervals, and graph identity constraints are validated.

## Production checklist

Before exposing the service beyond a trusted private network:

1. Generate a high-entropy `PKOS_API_KEY` and store it in a secret manager rather than source control.
2. Use managed/encrypted PostgreSQL storage or encrypted host volumes and test restore procedures.
3. Put TLS and network access control in front of the API; do not expose PostgreSQL publicly.
4. Configure provider timeouts/quotas and alert on repeated job failures, `degraded=true` retrievals, and readiness failures.
5. Back up source records, memories, graph history, and audit records; test point-in-time restore where supported.
6. Build a representative Golden retrieval set and establish release thresholds before changing embeddings, chunking, lexical backend, or ranking weights.
7. Keep migrations forward-only in production and validate them against a copy of production-shaped data before deployment.

## Repository layout

```text
src/
  api/               HTTP boundary and input validation
  services/          chunking, retrieval, embeddings, and worker logic
  config.rs          validated runtime configuration
  models.rs          API/domain DTOs
  storage.rs         source/search/memory/job persistence
  storage_graph.rs   temporal graph and timeline persistence
migrations/          forward database schema migrations
scripts/evaluate.py  retrieval regression evaluator
eval/                Golden dataset templates
Dockerfile           API image
docker/              PostgreSQL extension image
```
