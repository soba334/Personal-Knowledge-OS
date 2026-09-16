#!/usr/bin/env python3
"""End-to-end smoke test for a running Personal Knowledge OS stack.

Uses only the Python standard library so CI does not need extra packages.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

AUTO_MEMORY_SOURCE = "For CI, I decided to let AI capture durable knowledge automatically."
AUTO_MEMORY_STATEMENT = "AI should capture durable knowledge automatically."


def request(
    base_url: str,
    api_key: str,
    method: str,
    path: str,
    body: dict[str, Any] | None = None,
    *,
    authenticated: bool = True,
    expected_status: int = 200,
) -> Any:
    headers = {"accept": "application/json"}
    if authenticated:
        headers["authorization"] = f"Bearer {api_key}"
    data = None
    if body is not None:
        headers["content-type"] = "application/json"
        data = json.dumps(body).encode("utf-8")

    req = urllib.request.Request(
        f"{base_url.rstrip('/')}{path}",
        method=method,
        headers=headers,
        data=data,
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as response:
            status = response.status
            payload = response.read()
    except urllib.error.HTTPError as exc:
        status = exc.code
        payload = exc.read()

    if status != expected_status:
        rendered = payload.decode("utf-8", errors="replace")
        raise AssertionError(
            f"{method} {path}: expected HTTP {expected_status}, got {status}: {rendered}"
        )

    if not payload:
        return None
    return json.loads(payload)


def assert_has(items: list[dict[str, Any]], key: str, value: str, message: str) -> None:
    if not any(str(item.get(key)) == value for item in items):
        raise AssertionError(message)


def wait_for_automatic_memory(
    base_url: str,
    api_key: str,
    source_id: str,
    *,
    timeout_seconds: float = 20.0,
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    last_memories: list[dict[str, Any]] = []
    saw_matching_memory = False
    while time.monotonic() < deadline:
        last_memories = request(
            base_url,
            api_key,
            "GET",
            "/v1/memories?status=active&limit=50",
        )
        for memory in last_memories:
            if memory.get("statement") != AUTO_MEMORY_STATEMENT:
                continue
            saw_matching_memory = True
            # A previous test/source may already have activated the same normalized memory.
            # The worker reinforces that memory by attaching this Source evidence asynchronously,
            # so keep polling until the evidence link is visible instead of failing the race.
            if source_id not in memory.get("evidence_source_ids", []):
                continue
            if memory.get("status") != "active":
                raise AssertionError("automatic memory did not reach active state")
            return memory
        time.sleep(0.5)
    if saw_matching_memory:
        raise AssertionError(
            "automatic memory existed but was not reinforced with the captured source evidence "
            f"before timeout; source_id={source_id}; last active memories={last_memories!r}"
        )
    raise AssertionError(
        "automatic grounded memory was not promoted before timeout; "
        f"last active memories={last_memories!r}"
    )


def run(base_url: str, api_key: str, *, expect_auto_memory: bool) -> None:
    health = request(base_url, api_key, "GET", "/healthz", authenticated=False)
    if health.get("status") != "ok":
        raise AssertionError(f"healthz is not ok: {health}")

    request(
        base_url,
        api_key,
        "GET",
        "/v1/timeline",
        authenticated=False,
        expected_status=401,
    )

    source = request(
        base_url,
        api_key,
        "POST",
        "/v1/sources",
        {
            "kind": "note",
            "title": "CI temporal graph smoke evidence",
            "content": (
                "Personal Knowledge OS keeps source records as durable evidence. "
                "Temporal graph facts retain history instead of overwriting it."
            ),
            "external_id": "ci-smoke-source-v1",
            "source_uri": "urn:pkos:ci:smoke",
            "occurred_at": "2026-01-10T12:00:00Z",
            "metadata": {"suite": "ci-smoke"},
        },
    )
    source_id = source["id"]

    fetched_source = request(base_url, api_key, "GET", f"/v1/sources/{source_id}")
    if fetched_source["sha256"] != source["sha256"]:
        raise AssertionError("source integrity hash changed after persistence")

    search = request(
        base_url,
        api_key,
        "POST",
        "/v1/search",
        {"query": "durable evidence temporal graph", "limit": 10, "include_memories": True},
    )
    if not search["hits"]:
        raise AssertionError("lexical retrieval returned no hits for the ingested source")
    if not search.get("degraded"):
        raise AssertionError("search should report degraded=true when embeddings are intentionally disabled")

    capture = request(
        base_url,
        api_key,
        "POST",
        "/v1/captures",
        {
            "kind": "conversation",
            "title": "CI automatic knowledge capture",
            "provider": "ci-mock-agent",
            "session_id": "ci-session-1",
            "external_id": "ci-auto-capture-v1",
            "source_uri": "urn:pkos:ci:auto-capture",
            "occurred_at": "2026-01-11T12:00:00Z",
            "messages": [
                {"role": "user", "content": AUTO_MEMORY_SOURCE},
                {
                    "role": "assistant",
                    "content": "I will preserve that decision as source-grounded knowledge.",
                },
            ],
            "metadata": {"suite": "ci-auto-memory"},
        },
    )
    capture_source_id = capture["source"]["id"]
    fetched_capture = request(
        base_url,
        api_key,
        "GET",
        f"/v1/sources/{capture_source_id}",
    )
    if AUTO_MEMORY_SOURCE not in fetched_capture["content"]:
        raise AssertionError("captured conversation content was not persisted as source truth")

    automatic_memory: dict[str, Any] | None = None
    if expect_auto_memory:
        if not capture.get("memory_extraction_scheduled"):
            raise AssertionError("capture did not schedule automatic memory extraction")
        automatic_memory = wait_for_automatic_memory(
            base_url,
            api_key,
            capture_source_id,
        )

    memory = request(
        base_url,
        api_key,
        "POST",
        "/v1/memories/candidates",
        {
            "memory_type": "decision",
            "statement": "Temporal graph facts retain history instead of being overwritten.",
            "confidence": 1.0,
            "importance": 0.9,
            "valid_from": "2026-01-10T12:00:00Z",
            "evidence_source_ids": [source_id],
        },
    )
    if memory["status"] != "candidate":
        raise AssertionError("new memory bypassed candidate verification state")

    approved = request(
        base_url,
        api_key,
        "POST",
        f"/v1/memories/{memory['id']}/approve",
        {"supersedes_id": None},
    )
    if approved["status"] != "active":
        raise AssertionError("approved memory did not become active")
    if source_id not in approved["evidence_source_ids"]:
        raise AssertionError("approved memory lost its source evidence link")

    project = request(
        base_url,
        api_key,
        "POST",
        "/v1/entities",
        {
            "entity_type": "project",
            "canonical_name": "CI Smoke Project",
            "attributes": {"suite": "ci-smoke"},
            "aliases": ["CI Project"],
        },
    )
    database = request(
        base_url,
        api_key,
        "POST",
        "/v1/entities",
        {
            "entity_type": "technology",
            "canonical_name": "CI Smoke PostgreSQL",
            "attributes": {"suite": "ci-smoke"},
            "aliases": ["CI Postgres"],
        },
    )

    relation = request(
        base_url,
        api_key,
        "POST",
        "/v1/relations",
        {
            "subject_entity_id": project["id"],
            "predicate": "uses",
            "object_entity_id": database["id"],
            "valid_from": "2026-01-01T00:00:00Z",
            "confidence": 1.0,
            "source_id": source_id,
        },
    )

    graph_during = request(
        base_url,
        api_key,
        "GET",
        f"/v1/entities/{project['id']}/graph?as_of="
        + urllib.parse.quote("2026-01-15T00:00:00Z"),
    )
    assert_has(
        graph_during["outgoing"],
        "id",
        relation["id"],
        "active temporal relation is absent from graph query",
    )

    closed = request(
        base_url,
        api_key,
        "POST",
        f"/v1/relations/{relation['id']}/close",
        {"valid_until": "2026-02-01T00:00:00Z"},
    )
    if closed["valid_until"] != "2026-02-01T00:00:00Z":
        raise AssertionError("relation valid_until was not persisted")

    graph_after = request(
        base_url,
        api_key,
        "GET",
        f"/v1/entities/{project['id']}/graph?as_of="
        + urllib.parse.quote("2026-03-01T00:00:00Z"),
    )
    if any(item["id"] == relation["id"] for item in graph_after["outgoing"]):
        raise AssertionError("expired relation leaked into current as-of graph")

    graph_history = request(
        base_url,
        api_key,
        "GET",
        f"/v1/entities/{project['id']}/graph?include_historical=true",
    )
    assert_has(
        graph_history["outgoing"],
        "id",
        relation["id"],
        "closed relation disappeared from historical graph",
    )

    timeline = request(base_url, api_key, "GET", "/v1/timeline?limit=20")
    assert_has(timeline, "source_id", source_id, "ingested source is absent from timeline")
    assert_has(
        timeline,
        "source_id",
        capture_source_id,
        "automatic capture source is absent from timeline",
    )

    print(
        json.dumps(
            {
                "status": "ok",
                "source_id": source_id,
                "capture_source_id": capture_source_id,
                "automatic_memory_id": automatic_memory.get("id") if automatic_memory else None,
                "memory_id": memory["id"],
                "relation_id": relation["id"],
                "search_methods": search["methods"],
                "search_degraded": search["degraded"],
            },
            ensure_ascii=False,
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--api-key", required=True)
    parser.add_argument("--expect-auto-memory", action="store_true")
    args = parser.parse_args()
    try:
        run(args.base_url, args.api_key, expect_auto_memory=args.expect_auto_memory)
    except Exception as exc:  # noqa: BLE001 - smoke runner should emit one clear failure.
        print(f"smoke test failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
