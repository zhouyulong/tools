# Transfer guide

This archive is a portable, directory-based agent skill. Copy the enclosed `ccf-prioritized-literature-search` directory into the destination harness's skill directory, preserving its internal structure. The harness must load its `SKILL.md` as the skill entry point; `agents/openai.yaml` is optional Codex UI metadata and can be ignored by other harnesses.

The included CCF A/B data is derived from the 2026 seventh-edition CCF catalog. The SCI cache was verified through EasyScholar on 2026-10-11. Freshness is not a fixed day count: JCR editions appear in late June and CAS divisions from December, and a new edition needs about one month before EasyScholar reflects it. Entries inside that one-month sync window are reported as pending; entries older than the sync date must be re-queried. A grade that changed confirms the new edition immediately. Check with `python3 scripts/cache_freshness.py` and update with `python3 scripts/refresh_ccf_b_sci_q1_cache.py --incremental`. It contains no secret key.

To use or refresh EasyScholar verification, provide the receiving process with an `EASY_SCHOLAR_KEY` environment variable. The scripts require Python 3.9+ and only the standard library. If the variable is unavailable, direct the user to https://www.easyscholar.cc/console/user/open to obtain their own key; do not request that they paste it into chat or persist it in the skill.

From the skill directory, a full cache refresh is:

```text
python scripts/refresh_ccf_b_sci_q1_cache.py --confirm-batch
```

The refresh makes one network request per CCF B journal and conservatively marks a journal as SCI一区 only when EasyScholar returns explicit evidence.
