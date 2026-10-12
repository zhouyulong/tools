---
name: ccf-prioritized-literature-search
description: Prioritize recent, high-quality computer-science papers when users ask for top journals/conferences, top-tier, frontier, high-quality, or new literature. Prefer CCF A venues, then controlled CCF B and SCI Q1 fallbacks; do not use for ordinary broad literature searches.
---

# CCF-prioritized literature search

Use this skill for computer-science paper discovery when the request conveys both quality and recency, including terms such as “顶刊”, “顶会”, “顶级”, “高质量”, “优质”, “前沿”, “最新”, or “新近”. Do not apply it merely because the user requests papers, asks about a supplied paper, or explicitly asks for classic/historical work.

## Time window

Treat “new” as papers whose first public scholarly date (online-first or official publication date) falls in the 12 months before the search date. A date range or other recency requirement from the user overrides this default. State the period used in the result.

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

When several candidates have the same tier, prefer the more recent paper, then the closer topical fit. Do not fill a request with marginally relevant papers just to reach a number; say how many qualified results were found.

## CCF B SCI一区 cache

To create or refresh the reusable CCF B journal cache, run [scripts/refresh_ccf_b_sci_q1_cache.py](scripts/refresh_ccf_b_sci_q1_cache.py) with the key configured in the environment. The refresh script preserves raw, redacted API evidence and labels uncertain results as unverified; it must not turn uncertainty into a positive SCI一区 result. If the key is absent, follow the configuration and console-opening procedure above rather than creating a guessed cache.

## Result reporting

For each recommended paper, report its publication date, venue, and one compact venue-evidence label: `CCF A`, `CCF B conference`, `CCF B + SCI一区`, or `SCI一区 (EasyScholar)`. Include the searched date window and disclose any shortfall or unavailable SCI verification. Do not claim “top-tier” for a venue whose evidence was not established.
