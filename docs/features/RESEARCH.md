# Discovery and research

## Commands

```bash
swiftel search 'rust browser engines' --limit 5 --json
swiftel map https://example.com --max-urls 100 --max-pages 5 --json
swiftel crawl https://example.com --max-pages 10 --max-depth 2 --json
swiftel research 'rust browser' --site https://example.com --max-pages 10 --max-depth 2 --limit 8 --json
swiftel research 'rust browser' --limit 8 --json
```

`map` is URL discovery, not scraping: seed, robots sitemaps (including nested sitemap indexes up to the built-in limits), and same-origin HTML links. `--max-urls` is 1–500 and `--max-pages` is 0–25; at most eight sitemap reads are attempted. A 404 on the conventional `/sitemap.xml` is normal and not treated as a failure. Robots restrictions apply to discovery and page reads. Page/source errors are listed individually.

`crawl` extracts same-origin HTML only. Its maximum depth is 3 and page budget 100; this is deliberately bounded, not a global/deep-web crawler. The seed's robots policy is loaded before traversal. Search/provider coverage, challenge pages, login-only sources, and network timeouts remain possible gaps. A robots `crawl-delay` above ten seconds causes refusal rather than a long-running crawl.

`research` ranks only body text successfully fetched. For a `--site` crawl, it uses the crawl results. Otherwise it searches via optional `SWIFTEL_SEARXNG_URL` (JSON-enabled) or the keyless fallback, fetches matching public HTML and supported document URLs, then calculates BM25-like lexical relevance. The JSON report contains query, provider, pages checked, source URLs, timestamps, SHA-256 digests, excerpts, matched terms, warnings and failures. No LLM synthesis, semantic inference, identity match, truth verification, or persistent cross-run index is implied. Empty/partial results never establish absence from the wider web.

The score is a ranking aid, not evidentiary confidence. For corroboration, inspect original documents and independent sources. See [research playbook](../RESEARCH_PLAYBOOK.md) and [boundaries](../security/BOUNDARIES.md).
