# Personal Knowledge OS MCP

Remote MCP gateway for connecting ChatGPT and other MCP hosts to Personal Knowledge OS.

The gateway intentionally does **not** write PostgreSQL directly. Every tool delegates to the existing PKOS HTTP API so owner scoping, Source provenance, automatic memory extraction, and evidence verification remain single-sourced in the Rust service.

## Tools

- `remember` — save verbatim user evidence through the capture pipeline
- `capture_conversation` — save role-preserving conversation evidence
- `search_knowledge` — hybrid Source + active-memory search
- `get_source` — retrieve exact Source evidence
- `list_memories` / `get_memory` — inspect derived memory and evidence links
- `recent_timeline` — inspect recent Source events

Write tools are additive and never activate Memory directly. They create Source/Capture records and let the existing PKOS worker apply extraction, grounding verification, thresholds, deduplication, and supersession.

## Configuration

| Variable | Purpose |
| --- | --- |
| `PKOS_MCP_BEARER_TOKEN` | external Bearer credential accepted by `/mcp`; use a separate high-entropy secret |
| `PKOS_API_KEY` | internal credential used by the gateway to call the Rust API |
| `PKOS_MCP_API_BASE_URL` | PKOS HTTP API, default `http://api:8080` |
| `PKOS_MCP_BIND` | listen address, default `0.0.0.0` |
| `PKOS_MCP_PORT` | listen port, default `8787` |
| `PKOS_MCP_PUBLIC_HOSTNAME` | public HTTPS hostname allowed by Host/Origin guards |
| `PKOS_MCP_ALLOWED_HOSTS` | optional comma-separated additional Host values |
| `PKOS_MCP_ALLOWED_ORIGINS` | optional comma-separated additional Origin hostnames |

## Endpoints

- `POST /mcp` — stateless MCP endpoint (2026-07-28 SDK v2, with legacy stateless compatibility)
- `GET /healthz` — liveness
- `/.well-known/*` — deliberately returns 404 because this deployment uses static Bearer auth rather than OAuth

Expose `/mcp` through HTTPS. Do not expose PostgreSQL publicly, and preferably keep the Rust API private so the MCP gateway is the only internet-facing application surface.
