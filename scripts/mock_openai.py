#!/usr/bin/env python3
"""Tiny OpenAI-compatible chat-completions mock for CI.

It deliberately returns one grounded memory only when the CI sentinel appears in
captured source text. This lets integration tests exercise the entire automatic
memory pipeline without an external model dependency.
"""

from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

SENTINEL = "For CI, I decided to let AI capture durable knowledge automatically."
MEMORY_STATEMENT = "AI should capture durable knowledge automatically."


class Handler(BaseHTTPRequestHandler):
    server_version = "PKOSMockOpenAI/1.0"

    def log_message(self, fmt: str, *args: Any) -> None:
        print(f"mock-openai: {fmt % args}", flush=True)

    def _json(self, status: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler API
        if self.path == "/healthz":
            self._json(200, {"status": "ok"})
            return
        self._json(404, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler API
        if self.path != "/v1/chat/completions":
            self._json(404, {"error": "not found"})
            return

        length = int(self.headers.get("content-length", "0"))
        try:
            request = json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError:
            self._json(400, {"error": "invalid json"})
            return

        rendered = json.dumps(request, ensure_ascii=False)
        memories: list[dict[str, Any]] = []
        if SENTINEL in rendered:
            memories.append(
                {
                    "memory_type": "decision",
                    "statement": MEMORY_STATEMENT,
                    "evidence_quote": SENTINEL,
                    "confidence": 0.99,
                    "importance": 0.9,
                    "valid_from": None,
                    "supersedes_memory_id": None,
                }
            )

        self._json(
            200,
            {
                "id": "chatcmpl-pkos-ci",
                "object": "chat.completion",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": json.dumps({"memories": memories}),
                        },
                        "finish_reason": "stop",
                    }
                ],
            },
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=18080)
    args = parser.parse_args()
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f"mock-openai listening on {args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
