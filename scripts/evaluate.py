#!/usr/bin/env python3
"""Retrieval regression runner using only Python's standard library.

Dataset format (JSONL):
{"id":"case-id","query":"...","relevant_source_ids":["uuid"],"must_abstain":false}
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path


def post_json(url: str, api_key: str, payload: dict) -> dict:
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "authorization": f"Bearer {api_key}",
            "content-type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def validate_case(case: dict, line_number: int) -> None:
    required = {"id", "query", "relevant_source_ids"}
    missing = required - case.keys()
    if missing:
        raise ValueError(f"line {line_number}: missing {sorted(missing)}")
    if not isinstance(case["relevant_source_ids"], list):
        raise ValueError(f"line {line_number}: relevant_source_ids must be an array")


def main() -> int:
    parser = argparse.ArgumentParser(description="Evaluate Personal Knowledge OS retrieval")
    parser.add_argument("dataset", type=Path)
    parser.add_argument("--base-url", default=os.getenv("PKOS_BASE_URL", "http://localhost:8080"))
    parser.add_argument("--api-key", default=os.getenv("PKOS_API_KEY"))
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument("--min-recall", type=float, default=0.0)
    parser.add_argument("--min-mrr", type=float, default=0.0)
    args = parser.parse_args()

    if not args.api_key:
        parser.error("--api-key or PKOS_API_KEY is required")
    if args.k < 1 or args.k > 50:
        parser.error("--k must be between 1 and 50")

    cases = []
    for line_number, raw in enumerate(args.dataset.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw.strip():
            continue
        case = json.loads(raw)
        validate_case(case, line_number)
        cases.append(case)

    if not cases:
        print("dataset is empty", file=sys.stderr)
        return 2

    hit_count = 0
    reciprocal_rank_sum = 0.0
    abstention_correct = 0
    abstention_cases = 0

    for case in cases:
        try:
            result = post_json(
                f"{args.base_url.rstrip('/')}/v1/search",
                args.api_key,
                {"query": case["query"], "limit": args.k, "include_memories": True},
            )
        except urllib.error.URLError as exc:
            print(f"request failed for {case['id']}: {exc}", file=sys.stderr)
            return 3

        relevant = set(case["relevant_source_ids"])
        ranked_sources: list[str] = []
        for hit in result.get("hits", []):
            source_id = hit.get("source_id")
            if source_id:
                ranked_sources.append(source_id)
            ranked_sources.extend(hit.get("evidence_source_ids", []))

        rank = next(
            (index + 1 for index, source_id in enumerate(ranked_sources) if source_id in relevant),
            None,
        )
        if rank is not None:
            hit_count += 1
            reciprocal_rank_sum += 1.0 / rank

        must_abstain = bool(case.get("must_abstain", False))
        if must_abstain:
            abstention_cases += 1
            if not result.get("hits"):
                abstention_correct += 1

        print(
            json.dumps(
                {
                    "id": case["id"],
                    "hit": rank is not None,
                    "rank": rank,
                    "degraded": result.get("degraded", False),
                    "methods": result.get("methods", []),
                },
                ensure_ascii=False,
            )
        )

    recall = hit_count / len(cases)
    mrr = reciprocal_rank_sum / len(cases)
    summary = {
        "cases": len(cases),
        f"recall@{args.k}": recall,
        "mrr": mrr,
        "abstention_accuracy": (abstention_correct / abstention_cases) if abstention_cases else None,
    }
    print(json.dumps(summary, ensure_ascii=False))

    if recall < args.min_recall or mrr < args.min_mrr:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
