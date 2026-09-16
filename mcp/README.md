# Personal Knowledge OS MCP

MCP gateway for letting ChatGPT and other MCP hosts read from and write evidence into Personal Knowledge OS.

The gateway never writes PostgreSQL directly. Every tool delegates to the existing PKOS HTTP API, so owner scoping, Source provenance, automatic memory extraction, grounding verification, deduplication, and temporal memory rules stay single-sourced in the Rust service.

## Recommended ChatGPT architecture

Use the same boundary as `VerasolStudio/life-ops`:

```text
ChatGPT
   |
   | OpenAI Secure MCP Tunnel
   v
local tunnel-client
   |
   | Authorization: Bearer <local MCP secret>
   v
127.0.0.1:8787  PKOS MCP
   |
   | internal PKOS_API_KEY
   v
Docker network -> Rust API -> PostgreSQL
```

The MCP endpoint is published on host loopback only. Do not expose port 8787, 8080, or 5432 directly to the public internet.

ChatGPT itself should be configured with **authentication = none**. `tunnel-client` injects `Authorization: Bearer $PKOS_MCP_BEARER_TOKEN` on the final local hop, so neither the MCP secret nor `PKOS_API_KEY` needs to be stored in ChatGPT.

See [`../docs/chatgpt-mcp.md`](../docs/chatgpt-mcp.md) for setup.

## Tools

- `pkos_status` — verify MCP -> PKOS backend connectivity
- `remember` — save verbatim user evidence through the capture pipeline
- `capture_conversation` — save role-preserving conversation evidence
- `search_knowledge` — hybrid Source + active-memory search
- `get_source` — retrieve exact Source evidence
- `list_memories` / `get_memory` — inspect derived memory and evidence links
- `recent_timeline` — inspect recent Source events

The server instructions explicitly tell the model that it does not need to wait for the user to say “remember this”: when a durable fact is clearly grounded in user-authored text, it should call `remember` proactively.

Write tools are additive and never activate Memory directly. They create Source/Capture records and let the existing PKOS worker apply extraction, grounding verification, thresholds, deduplication, and supersession.

## Configuration

| Variable | Purpose |
| --- | --- |
| `PKOS_MCP_BEARER_TOKEN` | local MCP credential; use a different high-entropy secret from `PKOS_API_KEY` |
| `PKOS_API_KEY` | internal credential used only by the gateway to call the Rust API |
| `PKOS_MCP_API_BASE_URL` | PKOS HTTP API, default `http://api:8080` |
| `PKOS_MCP_BIND` | listen address; direct Node execution defaults to `127.0.0.1` |
| `PKOS_MCP_PORT` | listen port, default `8787` |
| `PKOS_MCP_PUBLIC_HOSTNAME` | optional direct-deployment hostname; not needed for Secure MCP Tunnel |
| `PKOS_MCP_ALLOWED_HOSTS` | optional comma-separated extra Host values |
| `PKOS_MCP_ALLOWED_ORIGINS` | optional comma-separated extra Origin hostnames |

Docker binds inside the MCP container on `0.0.0.0:8787`, but Compose publishes it only as `127.0.0.1:8787` on the host.

## Endpoints

- `POST /mcp` — stateless MCP endpoint using MCP 2026-07-28 SDK v2
- `GET /healthz` — liveness
- `/.well-known/*` — 404 by design; the recommended ChatGPT path uses Secure MCP Tunnel rather than OAuth discovery

The local MCP endpoint still requires its Bearer credential. Secure MCP Tunnel supplies it with `extra_headers`.
