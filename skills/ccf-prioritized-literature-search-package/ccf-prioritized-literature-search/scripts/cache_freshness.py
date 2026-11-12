#!/usr/bin/env python3
"""Decide whether a cached SCI grade may be presented as the current edition.

Two facts drive this module:

1. SCI grades change only when a ranking body publishes a new edition. Counting days from
   the last check is therefore meaningless.
2. EasyScholar does not echo a version or edition field, so a successful re-query does NOT
   prove the API has caught up with a new official edition. Re-querying right after a
   release can hand back last edition's grade and stamp it with today's date - laundering
   stale data into an apparently fresh entry. This is the real hazard, not mere age.

So freshness is judged against two marks per cycle:

    released_at  - earliest plausible official release of the new edition
    confirmed_at - released_at + lag, the point after which the API is assumed synced

A one-month lag after each official release is the redundancy: long enough for EasyScholar
to publish the new edition, short enough that the skill rarely withholds a usable grade.
The lag only guards the window right after a release; it is not a general lifetime.

A value that actually CHANGED is self-evidencing: the API must have moved to a new edition,
so a change confirmed after released_at promotes the entry to current without waiting out the
lag. A value that did NOT change proves nothing, because "unchanged" and "API not yet
synced" look identical.
"""

from __future__ import annotations

import argparse
import json
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

POLICY_VERSION = 3

# (name, release month, release day, lag days) - release is the earliest plausible date.
RELEASE_CYCLES = (
    ("JCR", 6, 20, 30),
    ("CAS", 12, 1, 30),
)


def _as_date(value) -> date:
    if isinstance(value, datetime):
        return value.date()
    if isinstance(value, date):
        return value
    text = str(value).strip().replace("Z", "+00:00")
    try:
        return datetime.fromisoformat(text).date()
    except ValueError:
        return datetime.strptime(text[:10], "%Y-%m-%d").date()


def cycle_marks(now: date) -> list[dict]:
    """Latest release and its confirmation mark, per cycle."""
    marks = []
    for name, month, day, lag in RELEASE_CYCLES:
        year = now.year if (now.month, now.day) >= (month, day) else now.year - 1
        released = date(year, month, day)
        marks.append({
            "cycle": name,
            "released_at": released,
            "confirmed_at": released + timedelta(days=lag),
            "lag_days": lag,
        })
    return marks


def assess(checked_at, now: date | None = None, changed_since_previous: bool = False) -> dict:
    """Judge one observation.

    checked_at is when the grade was last read from the API.
    changed_since_previous is True only when this read differs from the value cached before
    the current edition was released. It is what lets a genuine update skip the lag.
    """
    now = now or datetime.now(timezone.utc).date()
    try:
        checked = _as_date(checked_at)
    except (TypeError, ValueError):
        return {
            "status": "unknown_age",
            "checked_at": None,
            "per_cycle": [],
            "message": "校验时间缺失或无法解析，按过期处理。",
        }

    per_cycle = []
    worst = "current"
    for mark in cycle_marks(now):
        confirmed = mark["confirmed_at"]
        released = mark["released_at"]
        if checked >= confirmed:
            state = "current"
        elif changed_since_previous and checked >= released:
            state = "current_by_change"
        elif checked >= released:
            state = "sync_window"
        elif now >= confirmed:
            state = "stale"
        else:
            state = "sync_window"
        per_cycle.append({
            "cycle": mark["cycle"],
            "released_at": released.isoformat(),
            "confirmed_at": confirmed.isoformat(),
            "lag_days": mark["lag_days"],
            "state": state,
        })
        order = {"current": 0, "current_by_change": 0, "sync_window": 1, "stale": 2}
        if order[state] > order.get(worst, 0):
            worst = state

    if worst == "stale":
        cycle = next(c for c in per_cycle if c["state"] == "stale")
        message = (f"校验于 {checked.isoformat()}，早于 {cycle['cycle']} 新版（{cycle['released_at']}）的同步确认点"
                   f"（{cycle['confirmed_at']}），须重新查询后才能作为当前等级。")
    elif worst == "sync_window":
        cycle = next(c for c in per_cycle if c["state"] == "sync_window")
        message = (f"校验于 {checked.isoformat()}，{cycle['cycle']} 新版已于 {cycle['released_at']} 发布，"
                   f"但在 {cycle['confirmed_at']} 前的同步冗余期内，易学术可能仍返回上一版等级，须标注为待确认。")
    elif any(c["state"] == "current_by_change" for c in per_cycle):
        message = f"校验于 {checked.isoformat()}，新旧等级已变化，可确认为新版数据。"
    else:
        message = f"校验于 {checked.isoformat()}，已过各分区同步确认点。"

    return {
        "status": worst,
        "checked_at": checked.isoformat(),
        "per_cycle": per_cycle,
        "message": message,
    }


def iter_entries(payload: dict):
    for key in ("journals", "unverified"):
        for entry in payload.get(key) or []:
            yield entry


def main() -> int:
    parser = argparse.ArgumentParser(description="Report which cached SCI grades are still current.")
    parser.add_argument("cache", type=Path, nargs="?", default=None)
    parser.add_argument("--today", help="Override today (YYYY-MM-DD) for testing.")
    parser.add_argument("--verbose", action="store_true", help="List entries that are not current.")
    args = parser.parse_args()

    cache = args.cache or Path(__file__).resolve().parent.parent / "references" / "ccf-b-sci-q1-cache.json"
    payload = json.loads(cache.read_text(encoding="utf-8"))
    now = _as_date(args.today) if args.today else datetime.now(timezone.utc).date()

    buckets: dict[str, int] = {}
    pending = []
    for entry in iter_entries(payload):
        verdict = assess(entry.get("checked_at"), now, bool(entry.get("grade_changed")))
        buckets[verdict["status"]] = buckets.get(verdict["status"], 0) + 1
        if verdict["status"] not in ("current",):
            pending.append((entry.get("short_name") or entry.get("full_name"), verdict))

    print(f"file        : {cache}")
    print(f"today       : {now.isoformat()}")
    print(f"policy v{POLICY_VERSION} : " + ", ".join(
        f"{n} 发布 {m:02d}-{d:02d} 确认 +{lag}天" for n, m, d, lag in RELEASE_CYCLES))
    print("status      : " + ", ".join(f"{k}={v}" for k, v in sorted(buckets.items())))
    for mark in cycle_marks(now):
        print(f"cycle       : {mark['cycle']} 新版 {mark['released_at'].isoformat()} 确认点 {mark['confirmed_at'].isoformat()}")
    if args.verbose:
        for name, verdict in pending:
            print(f"  - {name}: [{verdict['status']}] {verdict['message']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
