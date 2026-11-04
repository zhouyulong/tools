---
name: ccf-prioritized-literature-search
description: Prioritize high-quality computer-science papers when users ask for top journals/conferences, top-tier, high-quality, or frontier literature. Prefer CCF A venues, then controlled CCF B and SCI Q1 fallbacks; apply a date window only when the user specifies one, and do not use for ordinary broad literature searches.
---

# CCF-prioritized literature search

Use this skill for computer-science paper discovery when the request conveys quality, including terms such as “顶刊”, “顶会”, “顶级”, “高质量”, “优质”, “前沿”, “最新”, or “新近”. Recency words such as “最新” or “新近” still select this skill for their quality intent, but only a date range the user states explicitly becomes a search constraint. Do not apply it merely because the user requests papers, asks about a supplied paper, or explicitly asks for classic/historical work.

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

For SCI一区 evidence, first consult [references/ccf-b-sci-q1-cache.json](references/ccf-b-sci-q1-cache.json). Only use entries marked `verified_sci_q1` and checked within the past 90 days. If no current cached verdict is available, query EasyScholar with [scripts/easyscholar_rank.py](scripts/easyscholar_rank.py). Read `EASY_SCHOLAR_KEY` only from the environment; never print, store, or include it in a URL shown to the user. If the variable is unavailable, tell the user to configure `EASY_SCHOLAR_KEY` and open [the EasyScholar open-API console](https://www.easyscholar.cc/console/user/open) in a user-visible browser so they can obtain their own secret key. A journal is SCI一区 only when the API response explicitly supports that claim. An unavailable key, failed request, or unclear response is not evidence of SCI一区.

## Selection order

Apply the requested topic, date window, and any user-specified venue constraints before this ranking. Select relevant papers in this order, stopping when the requested number is reached:

1. CCF A journals and conferences.
2. CCF B conferences, only when CCF A results are insufficient. For artificial intelligence, human-computer interaction and ubiquitous computing, and cross-disciplinary/comprehensive/emerging computing, include all topic-relevant conferences from the dedicated CCF B supplement index in this tier.
3. CCF B journals verified by EasyScholar as SCI一区, only when more journal results are needed.
4. Other journals verified by EasyScholar as SCI一区, only when the preceding groups still leave the request short.

Do not use unranked conferences as a substitute for CCF A/B conferences. If the user asks only for journals or only for conferences, apply the corresponding branches only. Preserve topic relevance: a venue rank never makes an off-topic paper suitable.

When several candidates have the same tier, prefer the closer topical fit, then the more recent paper. Do not fill a request with marginally relevant papers just to reach a number; say how many qualified results were found.

## CCF B SCI一区 cache

To create or refresh the reusable CCF B journal cache, run [scripts/refresh_ccf_b_sci_q1_cache.py](scripts/refresh_ccf_b_sci_q1_cache.py) with the key configured in the environment. The refresh script preserves raw, redacted API evidence and labels uncertain results as unverified; it must not turn uncertainty into a positive SCI一区 result. If the key is absent, follow the configuration and console-opening procedure above rather than creating a guessed cache.

## Result reporting

For each recommended paper, report its publication date, venue, and one compact venue-evidence label: `CCF A`, `CCF B conference`, `CCF B + SCI一区`, or `SCI一区 (EasyScholar)`. Include the date window actually applied, written at half-year granularity (e.g. `2024 H2–2025 H1`, or “no date limit” when the user gave none), and disclose any shortfall or unavailable SCI verification. Do not claim “top-tier” for a venue whose evidence was not established.
