---
name: ccf-prioritized-literature-search
description: Prioritize high-quality computer-science papers when users ask for top journals/conferences, top-tier, high-quality, or frontier literature, or ask what CCF grade or SCI quartile a paper or venue has. Prefer CCF A venues, then controlled CCF B and SCI Q1 fallbacks; always pair a CCF journal grade with its SCI grade, apply a date window only when the user specifies one, and do not use for ordinary broad literature searches.
---

# CCF-prioritized literature search

Use this skill for computer-science paper discovery when the request conveys quality, including terms such as “顶刊”, “顶会”, “顶级”, “高质量”, “优质”, “前沿”, “最新”, or “新近”. Recency words such as “最新” or “新近” still select this skill for their quality intent, but only a date range the user states explicitly becomes a search constraint. Do not apply it merely because the user requests papers or explicitly asks for classic/historical work.

## Venue grade lookup

Also use this skill when the request is about how a venue or a specific paper is ranked, even if no quality or recency word appears — for example “这篇论文什么等级”, “这个会是 CCF 几类”, “这篇是 CCF A 吗”, “这个期刊是 SCI 一区吗”, or “帮我判断这个 venue 的档次”.

- Identify the venue by abbreviation and full title, then read [references/ccf-2026-a-b-venues.json](references/ccf-2026-a-b-venues.json) before answering.
- Return the grade with its evidence label (`CCF A`, `CCF B conference`, `CCF B + SCI一区`, `SCI一区 (EasyScholar)`, or `unranked`). `unranked` means the venue was not found in the catalog, not that it is a poor venue.
- For any journal, this rule requires returning the SCI grade as well; follow the journal SCI grade requirement below. Never infer a quartile without explicit evidence.
- This lookup needs no date window and no candidate-paper search; do not pad the answer with unrelated papers.

## Journal SCI grade requirement

This is mandatory for every journal, without exception. Whenever a CCF journal is named in a result, a comparison, or a grade lookup, report its SCI grade alongside the CCF grade in the same entry. Never report a CCF journal grade alone.

- Scope applies to CCF A journals and CCF B journals alike. Conferences are exempt: they have no SCI grade, so state that they are journals-only ranked and do not invent a quartile.
- Full coverage means all 148 CCF journals (37 A + 111 B). The bundled cache [references/ccf-b-sci-q1-cache.json](references/ccf-b-sci-q1-cache.json) covers CCF B journals only and holds 45 verified entries; it contains no CCF A journal. Any journal absent from a current, verified cache entry requires a live query, including every CCF A journal. To extend coverage for CCF A journals, run the refresh script with `--source references/ccf-2026-a-journals.json` and `--output references/ccf-a-sci-cache.json`, mirroring the B file layout; keep it separate so the B cache is not overwritten.
- Query with [scripts/easyscholar_rank.py](scripts/easyscholar_rank.py). Read `EASY_SCHOLAR_KEY` only from the environment; never print, store, or place it in a URL shown to the user.
- “SCI 等级” is ambiguous, so every grade must name its system inline. Never write a bare `SCI Q1` or `2区`: the reader cannot tell which system it came from. Report both systems whenever the data supports them, each with an explicit prefix:
  - JCR: from the `sci` field. Write `JCR Q1`–`JCR Q4`, optionally with the year or edition if known, e.g. `JCR Q2`.
  - 中科院: two editions that often disagree, so keep them apart. `sciBase` is 大类（基础版）, `sciUp` is 大类（升级版）, `sciUpSmall` is 小类（升级版）. Write `中科院 大类（基础版）X区` / `中科院 大类（升级版）X区` / `中科院 小类 Y区`, e.g. `中科院 大类（升级版）计算机科学4区（小类 计算机：人工智能4区）`. If both editions exist and differ, report both rather than picking one; the base and upgraded editions are not interchangeable.
  - The API also returns `xr`/`xrSmall` and a 顶刊/TOP designation. Do not guess what an unidentified field means or fold it into a division; report it only when you can name the system, otherwise leave it out and say so.
- If only one system is available, label that one and say the other is unavailable. An unlabelled grade is a defect, not a shortcut.
- Do not treat these as interchangeable and never convert one into the other. A CCF A journal can sit in a low JCR quartile; that is a legitimate, reportable result. Never present “Q1” or “一区” without saying which system produced it.
- Never infer an SCI grade from CCF rank, journal name, publisher, or impact factor.
- If the key is missing or a query fails, write `SCI 等级未验证` with the reason, then give the console link from below. This is the only permitted substitute; silently omitting the SCI grade is a violation of this rule.

## Date window

There is no default time window. Do not filter papers by publication or online-first date unless the user states an explicit date range, year, or other period.

- When the user gives a period (“近三年”, “2022–…”, “2023 年以后”, “最近半年”), use exactly that period. If it is ambiguous, state the interpretation you applied.
- When the user gives no period, search without one. Do not implicitly fall back to “the last 12 months” or any other unstated window.
- Always state the window actually used, including “no date limit” when none was requested.

### Half-year granularity

Six months is the finest granularity the user may specify, and the finest you should resolve a window to. Interpret and report every window on half-year boundaries; do not narrow to months or days.

- Half-year units: `2024 H1` = 2024-01-01 to 2024-06-30; `2024 H2` = 2024-07-01 to 2024-12-31.
- Relative periods are counted back from the search date in half-year steps: “最近半年” is the current half-year block plus the one before it (the 12 months preceding the search date), “最近一年” is the four most recent half-year blocks, and so on.
- A whole year (“2023”, “2023 年”) means both halves of that year. A month or quarter given by the user (e.g. “2024 年 3 月”) is widened to the containing half-year, and you must say which half-year you used.
- When reporting, write the window as half-year ranges, e.g. `2023 H2–2024 H1` or `2024 H1`. Never claim month-level precision.

## Venue evidence

Read [references/ccf-2026-a-b-venues.json](references/ccf-2026-a-b-venues.json) before ranking venues. It contains the CCF seventh-edition A/B listings extracted from the supplied official 2026 catalog. Match a venue by its abbreviation and full title, and retain the source page for review. Treat a missing or ambiguous match as unranked; do not infer a CCF grade.

For any search in artificial intelligence, human-computer interaction and ubiquitous computing, or cross-disciplinary/comprehensive/emerging computing, also read [references/ccf-b-ai-hci-cross-conferences.json](references/ccf-b-ai-hci-cross-conferences.json). Its 28 CCF B conferences are a required supplementary candidate pool for those areas, rather than an optional or easily overlooked fallback.

For SCI一区 evidence, first consult [references/ccf-b-sci-q1-cache.json](references/ccf-b-sci-q1-cache.json). Only use entries marked `verified_sci_q1` whose freshness verdict from the rules above is `current` or `due_soon`. If no current cached verdict is available, query EasyScholar with [scripts/easyscholar_rank.py](scripts/easyscholar_rank.py). Read `EASY_SCHOLAR_KEY` only from the environment; never print, store, or include it in a URL shown to the user. If the variable is unavailable, tell the user to configure `EASY_SCHOLAR_KEY` and open [the EasyScholar open-API console](https://www.easyscholar.cc/console/user/open) in a user-visible browser so they can obtain their own secret key. A journal is SCI一区 only when the API response explicitly supports that claim. An unavailable key, failed request, or unclear response is not evidence of SCI一区.

## Selection order

Apply the requested topic, date window, and any user-specified venue constraints before this ranking. Select relevant papers in this order, stopping when the requested number is reached:

1. CCF A journals and conferences.
2. CCF B conferences, only when CCF A results are insufficient. For artificial intelligence, human-computer interaction and ubiquitous computing, and cross-disciplinary/comprehensive/emerging computing, include all topic-relevant conferences from the dedicated CCF B supplement index in this tier.
3. CCF B journals verified by EasyScholar as SCI一区, only when more journal results are needed.
4. Other journals verified by EasyScholar as SCI一区, only when the preceding groups still leave the request short.

Do not use unranked conferences as a substitute for CCF A/B conferences. If the user asks only for journals or only for conferences, apply the corresponding branches only. Preserve topic relevance: a venue rank never makes an off-topic paper suitable.

When several candidates have the same tier, prefer the closer topical fit, then the more recent paper. Do not fill a request with marginally relevant papers just to reach a number; say how many qualified results were found.

## Grade freshness

SCI grades change only when a ranking body publishes a new edition. Counting days from the last check is meaningless, and EasyScholar echoes no version field, so a successful re-query does not prove the API has caught up. Re-querying right at a release can return last edition's grade and stamp it with today's date, laundering stale data into a fresh-looking entry. Freshness is therefore judged against two marks per cycle.

- JCR quartile: Clarivate publishes annually, historically late June. Use 06-20 as the release mark.
- 中科院分区: published annually, historically from December onward. Use 12-01 as the release mark.
- Redundancy: allow one month after each release mark for EasyScholar to sync. Until that sync date the entry is `sync_window`, an unconfirmed window rather than a usable current grade.
- A value that actually changed is self-evidencing and confirms the new edition at once, without waiting out the month. An unchanged value proves nothing: “same grade” and “API not yet synced” look identical.
- States: `current` (confirmed), `sync_window` (inside the month after a release; report only as pending), `stale` (past the sync date without a fresh read; must be re-queried), `unknown_age` (missing timestamp; treat as stale).
- Report the check date with every journal SCI grade, and mark a `sync_window` grade as `SCI 等级待确认（YYYY-MM 新版同步期内）`.
- Run `python3 scripts/cache_freshness.py` for the cache-level verdict, or import `assess(checked_at, today, grade_changed)` from `scripts/cache_freshness.py` for a single entry.

## CCF B SCI一区 cache

To create or refresh the reusable CCF B journal cache, run [scripts/refresh_ccf_b_sci_q1_cache.py](scripts/refresh_ccf_b_sci_q1_cache.py) with the key configured in the environment. This cache stores full EasyScholar responses plus a derived `sci_grade` block (`jcr_quartile`, `cas_major`, `cas_minor`, `impact_factor`), so a cached journal can supply either grading system without a new request. Keep it current with `--incremental`, which re-queries only entries whose cycle has rolled over and reuses the rest; extend the same cache when its coverage is insufficient for a journal you must report. The refresh script preserves raw, redacted API evidence and labels uncertain results as unverified; it must not turn uncertainty into a positive SCI一区 result. If the key is absent, follow the configuration and console-opening procedure above rather than creating a guessed cache.

## Result reporting

For each recommended paper, report its publication date, venue, and one compact venue-evidence label: `CCF A`, `CCF B conference`, `CCF B + SCI一区`, or `SCI一区 (EasyScholar)`. When the venue is a journal, append both labelled SCI grades in the same entry (e.g. `CCF A · JCR Q2 · 中科院 大类 计算机科学2区`), or `SCI 等级未验证` with the reason. Include the date window actually applied, written at half-year granularity (e.g. `2024 H2–2025 H1`, or “no date limit” when the user gave none), and disclose any shortfall or unavailable SCI verification. Do not claim “top-tier” for a venue whose evidence was not established.
