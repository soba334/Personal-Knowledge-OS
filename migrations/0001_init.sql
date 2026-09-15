CREATE EXTENSION IF NOT EXISTS vchord CASCADE;
CREATE EXTENSION IF NOT EXISTS pgroonga;

CREATE TABLE IF NOT EXISTS sources (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    kind text NOT NULL CHECK (length(kind) BETWEEN 1 AND 64),
    title text,
    content text NOT NULL,
    external_id text,
    source_uri text,
    sha256 text NOT NULL CHECK (length(sha256)=64),
    occurred_at timestamptz,
    ingested_at timestamptz NOT NULL DEFAULT now(),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (owner_id, kind, external_id)
);
CREATE INDEX IF NOT EXISTS sources_owner_time_idx ON sources(owner_id, occurred_at DESC NULLS LAST, ingested_at DESC);

CREATE TABLE IF NOT EXISTS chunks (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    source_id uuid NOT NULL REFERENCES sources(id) ON DELETE RESTRICT,
    position integer NOT NULL CHECK(position>=0),
    char_start integer NOT NULL CHECK(char_start>=0),
    char_end integer NOT NULL CHECK(char_end>=char_start),
    content text NOT NULL,
    embedding vector(1536),
    embedding_model text,
    embedded_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(source_id, position)
);
CREATE INDEX IF NOT EXISTS chunks_owner_source_idx ON chunks(owner_id,source_id,position);
CREATE INDEX IF NOT EXISTS chunks_content_pgroonga_idx ON chunks USING pgroonga(content);
CREATE INDEX IF NOT EXISTS chunks_embedding_vchordrq_idx ON chunks USING vchordrq (embedding vector_cosine_ops);

CREATE TABLE IF NOT EXISTS memories (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    memory_type text NOT NULL CHECK(length(memory_type) BETWEEN 1 AND 64),
    statement text NOT NULL,
    status text NOT NULL CHECK(status IN ('candidate','active','superseded','rejected','disputed')),
    confidence real NOT NULL DEFAULT 0.8 CHECK(confidence BETWEEN 0 AND 1),
    importance real NOT NULL DEFAULT 0.5 CHECK(importance BETWEEN 0 AND 1),
    recurrence_count integer NOT NULL DEFAULT 1 CHECK(recurrence_count>=1),
    valid_from timestamptz,
    valid_until timestamptz,
    supersedes_id uuid REFERENCES memories(id) ON DELETE RESTRICT,
    embedding vector(1536),
    embedding_model text,
    embedded_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK(valid_until IS NULL OR valid_from IS NULL OR valid_until>=valid_from)
);
CREATE INDEX IF NOT EXISTS memories_owner_status_idx ON memories(owner_id,status,updated_at DESC);
CREATE INDEX IF NOT EXISTS memories_statement_pgroonga_idx ON memories USING pgroonga(statement);
CREATE INDEX IF NOT EXISTS memories_embedding_vchordrq_idx ON memories USING vchordrq (embedding vector_cosine_ops);

CREATE TABLE IF NOT EXISTS memory_evidence (
    memory_id uuid NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    source_id uuid NOT NULL REFERENCES sources(id) ON DELETE RESTRICT,
    chunk_id uuid REFERENCES chunks(id) ON DELETE RESTRICT,
    evidence_text text,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(memory_id,source_id)
);

CREATE TABLE IF NOT EXISTS entities (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    entity_type text NOT NULL,
    canonical_name text NOT NULL,
    attributes jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS entities_owner_name_idx ON entities(owner_id,canonical_name);

CREATE TABLE IF NOT EXISTS entity_aliases (
    entity_id uuid NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    alias text NOT NULL,
    PRIMARY KEY(entity_id,alias)
);

CREATE TABLE IF NOT EXISTS relations (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    subject_entity_id uuid NOT NULL REFERENCES entities(id) ON DELETE RESTRICT,
    predicate text NOT NULL,
    object_entity_id uuid NOT NULL REFERENCES entities(id) ON DELETE RESTRICT,
    valid_from timestamptz,
    valid_until timestamptz,
    confidence real NOT NULL DEFAULT 0.8 CHECK(confidence BETWEEN 0 AND 1),
    source_id uuid REFERENCES sources(id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK(subject_entity_id<>object_entity_id),
    CHECK(valid_until IS NULL OR valid_from IS NULL OR valid_until>=valid_from)
);
CREATE INDEX IF NOT EXISTS relations_subject_idx ON relations(owner_id,subject_entity_id,predicate);
CREATE INDEX IF NOT EXISTS relations_object_idx ON relations(owner_id,object_entity_id,predicate);

CREATE TABLE IF NOT EXISTS jobs (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    kind text NOT NULL,
    payload jsonb NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','done','failed')),
    attempts integer NOT NULL DEFAULT 0 CHECK(attempts>=0),
    available_at timestamptz NOT NULL DEFAULT now(),
    lease_until timestamptz,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS jobs_ready_idx ON jobs(owner_id,status,available_at,created_at);

CREATE TABLE IF NOT EXISTS audit_log (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    event_type text NOT NULL,
    subject_type text NOT NULL,
    subject_id uuid NOT NULL,
    actor text NOT NULL,
    details jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS audit_owner_time_idx ON audit_log(owner_id,created_at DESC);

CREATE TABLE IF NOT EXISTS retrieval_runs (
    id uuid PRIMARY KEY,
    owner_id uuid NOT NULL,
    query text NOT NULL,
    methods text[] NOT NULL,
    degraded boolean NOT NULL,
    latency_ms bigint NOT NULL CHECK(latency_ms>=0),
    top_k integer NOT NULL CHECK(top_k>0),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS retrieval_runs_owner_time_idx ON retrieval_runs(owner_id,created_at DESC);
