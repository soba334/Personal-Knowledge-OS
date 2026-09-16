#!/usr/bin/env python3
"""End-to-end test for the PKOS MCP gateway using MCP 2026-07-28 requests."""

from __future__ import annotations

import argparse
import json
import time
import urllib.error
import urllib.request
from typing import Any

PROTOCOL_VERSION = "2026-07-28"
SENTINEL = "For CI, I decided to let AI capture durable knowledge automatically."
MEMORY_STATEMENT = "AI should capture durable knowledge automatically."


def client_meta() -> dict[str, Any]:
    return {
        "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "pkos-ci-probe", "version": "1.0.0"},
        "io.modelcontextprotocol/clientCapabilities": {},
    }


def parse_mcp_payload(payload: bytes, content_type: str) -> Any:
    text = payload.decode("utf-8", errors="replace").strip()
    if not text:
        raise AssertionError(f"MCP returned an empty body (content-type={content_type!r})")

    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass

    frames: list[str] = []
    current: list[str] = []
    for line in text.splitlines():
        if line.startswith("data:"):
            current.append(line[5:].lstrip())
        elif not line.strip() and current:
            frames.append("\n".join(current))
            current = []
    if current:
        frames.append("\n".join(current))

    for frame in reversed(frames):
        try:
            return json.loads(frame)
        except json.JSONDecodeError:
            continue
    raise AssertionError(
        f"MCP response was neither JSON nor parseable SSE (content-type={content_type!r}): {text[:1000]!r}"
    )


def http_mcp(
    base_url: str,
    token: str | None,
    method: str,
    path: str,
    body: dict[str, Any] | None = None,
    *,
    mcp_method: str | None = None,
    mcp_name: str | None = None,
    expected_status: int = 200,
) -> tuple[Any, str]:
    headers = {"accept": "application/json, text/event-stream"}
    if token is not None:
        headers["authorization"] = f"Bearer {token}"
    if body is not None:
        headers["content-type"] = "application/json"
    if mcp_method:
        headers["MCP-Protocol-Version"] = PROTOCOL_VERSION
        headers["Mcp-Method"] = mcp_method
    if mcp_name:
        headers["Mcp-Name"] = mcp_name

    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        f"{base_url.rstrip('/')}{path}", method=method, headers=headers, data=data
    )
    try:
        with urllib.request.urlopen(req, timeout=20) as response:
            status = response.status
            payload = response.read()
            content_type = response.headers.get("content-type", "")
    except urllib.error.HTTPError as exc:
        status = exc.code
        payload = exc.read()
        content_type = exc.headers.get("content-type", "")

    if status != expected_status:
        rendered = payload.decode("utf-8", errors="replace")
        raise AssertionError(f"{method} {path}: expected {expected_status}, got {status}: {rendered}")

    if not payload:
        return None, content_type
    if path == "/mcp" and expected_status < 400:
        return parse_mcp_payload(payload, content_type), content_type
    try:
        return json.loads(payload), content_type
    except json.JSONDecodeError:
        return payload.decode("utf-8", errors="replace"), content_type


def rpc(base_url: str, token: str, request_id: int, method: str, params: dict[str, Any] | None = None) -> Any:
    merged_params = dict(params or {})
    merged_params["_meta"] = client_meta()
    payload: dict[str, Any] = {
        "jsonrpc": "2.0",
        "id": request_id,
        "method": method,
        "params": merged_params,
    }
    response, _ = http_mcp(
        base_url,
        token,
        "POST",
        "/mcp",
        payload,
        mcp_method=method,
        mcp_name=merged_params.get("name") if method == "tools/call" else None,
    )
    if response.get("error"):
        raise AssertionError(f"MCP {method} returned error: {response['error']}")
    return response["result"]


def call_tool(base_url: str, token: str, request_id: int, name: str, arguments: dict[str, Any]) -> Any:
    result = rpc(base_url, token, request_id, "tools/call", {"name": name, "arguments": arguments})
    if result.get("isError"):
        raise AssertionError(f"MCP tool {name} failed: {result}")
    content = result.get("content") or []
    if not content or content[0].get("type") != "text":
        raise AssertionError(f"MCP tool {name} returned no text payload: {result}")
    return json.loads(content[0]["text"])


def run(base_url: str, token: str) -> None:
    health, _ = http_mcp(base_url, None, "GET", "/healthz")
    if health.get("status") != "ok":
        raise AssertionError(f"MCP health is not ok: {health}")

    http_mcp(
        base_url,
        None,
        "POST",
        "/mcp",
        {"jsonrpc": "2.0", "id": 0, "method": "tools/list", "params": {"_meta": client_meta()}},
        mcp_method="tools/list",
        expected_status=401,
    )

    listed = rpc(base_url, token, 1, "tools/list")
    tools = {tool["name"]: tool for tool in listed.get("tools", [])}
    required = {
        "pkos_status",
        "remember",
        "capture_conversation",
        "search_knowledge",
        "get_source",
        "list_memories",
        "get_memory",
        "recent_timeline",
    }
    missing = sorted(required - tools.keys())
    if missing:
        raise AssertionError(f"MCP tools missing: {missing}")
    if tools["search_knowledge"].get("annotations", {}).get("readOnlyHint") is not True:
        raise AssertionError("search_knowledge must advertise readOnlyHint=true")
    if tools["remember"].get("annotations", {}).get("destructiveHint") is not False:
        raise AssertionError("remember must advertise destructiveHint=false")

    status = call_tool(base_url, token, 2, "pkos_status", {})
    if status.get("gateway") != "ok":
        raise AssertionError(f"pkos_status did not report gateway ok: {status}")

    remembered = call_tool(
        base_url,
        token,
        3,
        "remember",
        {
            "user_text": SENTINEL,
            "title": "MCP CI automatic memory",
            "occurred_at": "2026-01-12T12:00:00Z",
            "idempotency_key": "ci-mcp-remember-v1",
            "metadata": {"suite": "ci-mcp"},
        },
    )
    source_id = remembered["source"]["id"]
    if not remembered.get("memory_extraction_scheduled"):
        raise AssertionError("MCP remember did not schedule automatic memory extraction")

    source = call_tool(base_url, token, 4, "get_source", {"id": source_id})
    if SENTINEL not in source.get("content", ""):
        raise AssertionError("MCP get_source lost the verbatim user evidence")
    capture_meta = source.get("metadata", {}).get("pkos_capture", {})
    if capture_meta.get("provider") != "chatgpt-mcp":
        raise AssertionError(f"MCP source provenance missing: {source.get('metadata')}")

    search = call_tool(
        base_url,
        token,
        5,
        "search_knowledge",
        {"query": "capture durable knowledge automatically", "limit": 10},
    )
    if not any(hit.get("source_id") == source_id for hit in search.get("hits", [])):
        raise AssertionError("MCP-ingested Source is absent from search results")

    last_memories: list[dict[str, Any]] = []
    for attempt in range(40):
        last_memories = call_tool(
            base_url,
            token,
            100 + attempt,
            "list_memories",
            {"status": "active", "limit": 50},
        )
        for memory in last_memories:
            if memory.get("statement") != MEMORY_STATEMENT:
                continue
            if source_id not in memory.get("evidence_source_ids", []):
                raise AssertionError("MCP-created memory lost Source evidence")
            print(
                json.dumps(
                    {
                        "status": "ok",
                        "source_id": source_id,
                        "memory_id": memory["id"],
                        "tools": sorted(tools),
                    },
                    ensure_ascii=False,
                )
            )
            return
        time.sleep(0.5)
    raise AssertionError(f"MCP memory was not promoted; last memories={last_memories!r}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="http://127.0.0.1:8787")
    parser.add_argument("--token", required=True)
    args = parser.parse_args()
    run(args.base_url, args.token)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
