#!/usr/bin/env python3
"""Query EasyScholar without exposing EASY_SCHOLAR_KEY."""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

ENDPOINT = "https://www.easyscholar.cc/open/getPublicationRank"
CONSOLE_URL = "https://www.easyscholar.cc/console/user/open"
SENSITIVE_TOKENS = ("secret", "key", "token", "authorization", "password")


def redact(value):
    if isinstance(value, dict):
        return {
            str(key): "[redacted]" if any(token in str(key).lower() for token in SENSITIVE_TOKENS) else redact(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [redact(item) for item in value]
    return value


def query(publication_name: str, timeout: float = 25.0) -> dict:
    secret = os.environ.get("EASY_SCHOLAR_KEY")
    if not secret:
        raise RuntimeError(
            "EASY_SCHOLAR_KEY is not set. Configure this environment variable with your EasyScholar secret key "
            f"from {CONSOLE_URL}."
        )
    parameters = urlencode({"secretKey": secret, "publicationName": publication_name})
    request = Request(f"{ENDPOINT}?{parameters}", headers={"Accept": "application/json", "User-Agent": "Codex CCF literature skill"})
    try:
        with urlopen(request, timeout=timeout) as response:
            body = response.read().decode("utf-8", errors="replace")
            status = response.status
    except HTTPError as error:
        raise RuntimeError(f"EasyScholar returned HTTP {error.code}.") from error
    except URLError as error:
        raise RuntimeError(f"EasyScholar request failed: {error.reason}") from error
    try:
        payload = json.loads(body)
    except json.JSONDecodeError:
        payload = {"non_json_response": body}
    return {
        "publication_name": publication_name,
        "checked_at": datetime.now(timezone.utc).isoformat(),
        "http_status": status,
        "response": redact(payload),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("publication_name")
    parser.add_argument("--output", type=Path, help="Write JSON to this path instead of stdout.")
    parser.add_argument("--timeout", type=float, default=25.0)
    args = parser.parse_args()
    try:
        result = query(args.publication_name, args.timeout)
    except RuntimeError as error:
        print(str(error), file=sys.stderr)
        return 2
    rendered = json.dumps(result, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
