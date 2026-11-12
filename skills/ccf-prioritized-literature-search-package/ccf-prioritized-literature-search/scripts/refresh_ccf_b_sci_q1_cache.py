#!/usr/bin/env python3
"""Build or update the EasyScholar SCI grade cache for CCF journals.

Two modes:

- Full build: re-queries every journal in the source list. Use a dedicated --output path.
- Incremental (--incremental): keeps entries that cache_freshness.assess() still calls
  current or due_soon, and only re-queries the ones whose grading cycle has rolled over.
  This is the normal upkeep path; it keeps SCI grades from drifting past a new JCR or
  CAS release without re-spending API quota on journals that cannot have changed.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

from cache_freshness import assess
from easyscholar_rank import query


def is_explicit_sci_q1(response: object) -> bool:
    """Return true only for an unambiguous SCI 1区/Q1 statement in API data."""
    rendered = json.dumps(response, ensure_ascii=False).lower()
    compact = re.sub(r"\s+", "", rendered)
    direct = ("sci一区", "sci1区", "sci-q1", "sciq1")
    if any(token in compact for token in direct):
        return True
    return bool(re.search(r'"sci[^"}]*"\s*:\s*"?(?:1区|q1)"?', rendered))


def _official_rank(response: object) -> dict:
    """Pull the EasyScholar officialRank.select block, tolerating missing keys."""
    try:
        data = response["data"]  # type: ignore[index]
        select = data["officialRank"]["select"]
        return select if isinstance(select, dict) else {}
    except (KeyError, TypeError):
        return {}


def _official_rank_all(response: object) -> dict:
    """The full officialRank.all block, which keeps fields select drops (e.g. sciUpSmall)."""
    try:
        all_ranks = response["data"]["officialRank"]["all"]  # type: ignore[index]
        return all_ranks if isinstance(all_ranks, dict) else {}
    except (KeyError, TypeError):
        return {}


def extract_sci_grade(response: object) -> dict:
    """Split the two mutually incompatible grading systems instead of conflating them."""
    merged = {**_official_rank_all(response), **_official_rank(response)}
    jcr = merged.get("sci")
    quartile = jcr.strip().upper() if isinstance(jcr, str) else None
    return {
        # JCR quartile, labelled so it is never confused with a CAS division.
        "jcr_quartile": quartile if quartile in ("Q1", "Q2", "Q3", "Q4") else None,
        # CAS publishes two editions that frequently disagree; keep both.
        "cas_major_base": merged.get("sciBase"),
        "cas_major_upgraded": merged.get("sciUp"),
        "cas_minor_upgraded": merged.get("sciUpSmall"),
        "impact_factor": merged.get("sciif"),
    }


def grade_signature(grade: dict | None) -> tuple:
    """Comparable fingerprint of a grade, ignoring volatile fields like impact factor."""
    grade = grade or {}
    return (
        grade.get("jcr_quartile"),
        grade.get("cas_major_base"),
        grade.get("cas_major_upgraded"),
        grade.get("cas_minor_upgraded"),
    )


def classify_change(previous: dict | None, grade: dict, checked_at: str, today) -> dict:
    """Decide whether a new reading proves the API moved to a new edition.

    A changed value proves it: last edition could not have produced this one. An unchanged
    value proves nothing, because "still the same grade" and "API has not synced yet" are
    indistinguishable from the outside.
    """
    prev_sig = grade_signature((previous or {}).get("sci_grade"))
    new_sig = grade_signature(grade)
    prev_known = previous is not None and any(prev_sig)
    changed = bool(prev_known and prev_sig != new_sig)
    if not prev_known:
        note = "无上一版可比对，仅凭本次读数，需等同步确认点。"
    elif changed:
        note = "等级较上一版已变化，可确认为新版数据。"
    else:
        note = "读数与上一版相同，无法区分“未更新”与“易学术尚未同步”，需等同步确认点。"
    return {
        "grade_changed": changed,
        "previous_signature": list(prev_sig) if prev_known else None,
        "signature": list(new_sig),
        "change_note": note,
    }


def main() -> int:
    script_dir = Path(__file__).resolve().parent
    reference_dir = script_dir.parent / "references"
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--source", type=Path, default=reference_dir / "ccf-2026-b-journals.json")
    parser.add_argument("--output", type=Path, default=reference_dir / "ccf-b-sci-q1-cache.json")
    parser.add_argument("--delay", type=float, default=0.5, help="Seconds between API requests.")
    parser.add_argument("--max", type=int, help="Limit the number of journals; useful for a trial run.")
    parser.add_argument("--confirm-batch", action="store_true", help="Required for a full refresh.")
    parser.add_argument("--today", help="Override today (YYYY-MM-DD) when deciding freshness; for testing.")
    parser.add_argument("--incremental", action="store_true",
                        help="Re-query only entries whose grading cycle has rolled over; keep the rest.")
    args = parser.parse_args()

    source = json.loads(args.source.read_text(encoding="utf-8"))
    journals = source.get("journals") or source.get("venues") or []

    today = None
    if args.today:
        from datetime import date as _date
        today = _date.fromisoformat(args.today)

    previous = {}
    if args.incremental:
        if not args.output.exists():
            parser.error("--incremental needs an existing --output cache to update.")
        old = json.loads(args.output.read_text(encoding="utf-8"))
        for entry in (old.get("journals") or []) + (old.get("unverified") or []):
            key = entry.get("full_name") or entry.get("short_name")
            if key:
                previous[key] = entry

    if args.max is None and not args.confirm_batch and not args.incremental:
        parser.error("A full API refresh requires --confirm-batch. Use --max for a small trial.")

    pending, kept = [], []
    for journal in journals:
        key = journal.get("full_name") or journal.get("short_name")
        entry = previous.get(key)
        if entry is not None and assess(entry.get("checked_at"), today, bool(entry.get("grade_changed")))["status"] == "current":
            if not entry.get("sci_grade"):
                entry = {**entry, "sci_grade": extract_sci_grade(entry.get("easyscholar_response"))}
            entry = {**entry, "grade_changed": bool(entry.get("grade_changed"))}
            entry = {**entry, "freshness": assess(entry.get("checked_at"), today, entry["grade_changed"])}
            kept.append(entry)
        else:
            pending.append(journal)
    if args.max is not None:
        pending = pending[: args.max]

    verified, unresolved = [], []
    for position, journal in enumerate(pending, 1):
        try:
            evidence = query(journal["full_name"])
            grade = extract_sci_grade(evidence["response"])
            prior = previous.get(journal.get("full_name") or journal.get("short_name"))
            change = classify_change(prior, grade, evidence["checked_at"], today)
            record = {
                **journal,
                "checked_at": evidence["checked_at"],
                **change,
                "freshness": assess(evidence["checked_at"], today, change["grade_changed"]),
                "sci_grade": grade,
                "easyscholar_response": evidence["response"],
            }
            if is_explicit_sci_q1(evidence["response"]):
                record["verification"] = "verified_sci_q1"
                verified.append(record)
            else:
                record["verification"] = "not_verified_sci_q1"
                unresolved.append(record)
        except RuntimeError as error:
            fallback = previous.get(journal.get("full_name") or journal.get("short_name"))
            if fallback is not None:
                prior = {**fallback, "refreshed_at": datetime.now(timezone.utc).isoformat(), "refresh_error": str(error)}
                verdict = assess(prior.get("checked_at"), today, bool(prior.get("grade_changed")))
                prior["freshness"] = verdict
                if verdict["status"] != "current":
                    prior["verification"] = "stale_" + str(prior.get("verification", "unknown"))
                unresolved.append(prior)
            else:
                unresolved.append({**journal, "verification": "query_failed", "error": str(error)})
        if position < len(pending):
            time.sleep(args.delay)

    for entry in kept:
        (verified if entry.get("verification") == "verified_sci_q1" else unresolved).append(entry)

    output = {
        "schema_version": 1,
        "catalog": source["catalog"],
        "status": "complete" if len(verified) + len(unresolved) >= len(journals) else "partial",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_journal_count": len(journals),
        "queried_journal_count": len(pending),
        "reused_journal_count": len(kept),
        "journals": verified,
        "unverified": unresolved,
    }
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"queried={len(pending)} reused={len(kept)} verified_sci_q1={len(verified)} unresolved={len(unresolved)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
