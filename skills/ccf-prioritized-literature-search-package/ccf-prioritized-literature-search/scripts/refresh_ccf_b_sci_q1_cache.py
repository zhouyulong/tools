#!/usr/bin/env python3
"""Build a conservative EasyScholar evidence cache for CCF B journals."""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

from easyscholar_rank import query


def is_explicit_sci_q1(response: object) -> bool:
    """Return true only for an unambiguous SCI 1区/Q1 statement in API data."""
    rendered = json.dumps(response, ensure_ascii=False).lower()
    compact = re.sub(r"\s+", "", rendered)
    direct = ("sci一区", "sci1区", "sci-q1", "sciq1")
    if any(token in compact for token in direct):
        return True
    return bool(re.search(r'"sci[^"}]*"\s*:\s*"?(?:1区|q1)"?', rendered))


def main() -> int:
    script_dir = Path(__file__).resolve().parent
    reference_dir = script_dir.parent / "references"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=reference_dir / "ccf-2026-b-journals.json")
    parser.add_argument("--output", type=Path, default=reference_dir / "ccf-b-sci-q1-cache.json")
    parser.add_argument("--delay", type=float, default=0.5, help="Seconds between API requests.")
    parser.add_argument("--max", type=int, help="Limit the number of journals; useful for a trial run.")
    parser.add_argument("--confirm-batch", action="store_true", help="Required for a complete refresh of all CCF B journals.")
    args = parser.parse_args()
    source = json.loads(args.source.read_text(encoding="utf-8"))
    journals = source["journals"]
    if args.max is None and not args.confirm_batch:
        parser.error("A full API refresh requires --confirm-batch. Use --max for a small trial.")
    if args.max is not None:
        journals = journals[: args.max]
    verified, unresolved = [], []
    for position, journal in enumerate(journals, 1):
        try:
            evidence = query(journal["full_name"])
            record = {**journal, "checked_at": evidence["checked_at"], "easyscholar_response": evidence["response"]}
            if is_explicit_sci_q1(evidence["response"]):
                record["verification"] = "verified_sci_q1"
                verified.append(record)
            else:
                record["verification"] = "not_verified_sci_q1"
                unresolved.append(record)
        except RuntimeError as error:
            unresolved.append({**journal, "verification": "query_failed", "error": str(error)})
        if position < len(journals):
            time.sleep(args.delay)
    output = {
        "schema_version": 1,
        "catalog": source["catalog"],
        "status": "complete" if len(journals) == len(source["journals"]) else "partial",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_journal_count": len(source["journals"]),
        "queried_journal_count": len(journals),
        "journals": verified,
        "unverified": unresolved,
    }
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"verified_sci_q1={len(verified)}; unresolved={len(unresolved)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
