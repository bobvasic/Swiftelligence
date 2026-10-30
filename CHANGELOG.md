# Changelog

## [Unreleased]

### Added

- Same-origin sitemap/link `map`, lexical source-grounded `research`, deterministic HTML-to-Markdown, and bounded PDF/DOCX/XLSX/PPTX `document` commands.
- Optional Servo browser runtime with restricted WebDriver actions and local-only Ollama action proposals. AI plans preview by default and require explicit execution.
- Hierarchical architecture, research, document, browser runtime and security documentation with Mermaid diagrams.

### Security

- Restrict WebDriver to host loopback, enforce explicit browser-network-risk acknowledgement, validate top-level URLs and anchor targets, and cap driver replies and action plans.
- Document that browser subresources/scripts bypass Rust HTTP SSRF checks; Docker containment is not an egress firewall. Document parser resource limits do not provide a full sandbox.

### Limitations

- No OCR, universal document fidelity, autonomous unrestricted browser agent, hosted search index or deep-web access. Browser HTTP status remains unknown, reported as `0`.

## [0.1.0] - 2026-10-30

### Added

- Keyless public web search with optional JSON-enabled SearXNG and a bounded fallback.
- Public HTML retrieval with title, text, links, final URL, SHA-256 and retrieval time.
- Same-origin crawl with robots.txt rules, crawl delay, limits, and per-page errors.
- Literal promo-code leads from fetched public pages, always marked redemption-unverified.
- Passive certificate-transparency and archive lookups for domains.
- JSON output, MIT license, CI, security policy and anonymized research playbook.

### Security

- Reject private/reserved destinations and embedded URL credentials by default; revalidate redirects and pin resolved addresses.
- Cap response size, timeouts, redirects, crawl pages and depth. No telemetry, account login, active scanning or CAPTCHA bypass.

### Limitations

- No JavaScript rendering, global search index, commercial search SLA or verified coupon redemption. Upstream search/CT/archive services may rate-limit or fail; partial coverage is reported, not represented as exhaustive.

[0.1.0]: https://github.com/bobvasic/Swiftelligence/releases/tag/v0.1.0
