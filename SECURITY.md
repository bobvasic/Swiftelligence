# Security policy

Please report vulnerabilities privately through the [Swiftelligence repository security advisory feature](https://github.com/bobvasic/Swiftelligence/security/advisories). Do not publish exploit details in an issue.

## Enforced boundaries

Swiftel's ordinary HTTP client accepts public HTTP(S) content only by default: it rejects credentials in URLs, non-public/reserved destinations and nonstandard ports, checks resolved addresses, pins DNS per request, validates each redirect, disables implicit proxies, and caps time and bytes. `crawl` and `map` are same-origin, robots-aware, and bounded. `--allow-private` is solely for controlled local fixtures and intentionally relaxes the private-address defense.

## Optional Servo browser

`browse` is opt-in. Its WebDriver endpoint must remain on host loopback. An initial Rust HTTP preflight validates the top-level URL, and restricted anchor-click targets are checked before clicking. This is **not** a complete browser SSRF defense: browser subresources, JavaScript fetches, redirects and DNS resolution are outside the Rust HTTP client's per-hop checks. Docker's default bridge is not a public-only egress firewall. Use an independently enforced public-only egress policy and a low-privilege isolated runtime for untrusted sites; never run with sensitive mounts or host credentials. A GET anchor may have side effects. Browser response status is unknown (`0`), while the hash identifies the rendered DOM, not the original network response.

Local Ollama planning uses a bounded excerpt and ID-addressable controls and emits validated typed actions. Preview is the default; execution requires `--execute-ai-plan`. A malicious page can still influence a model and site behavior. Inspect plans and avoid confidential content.

## Documents and research

PDF/Office archives are bounded by input/decompression declarations but parsing is in-process and does not guarantee CPU/memory safety. Use OS process limits on hostile files. Scanned PDFs are not OCRed; extracted text does not guarantee completeness. Evidence hashes are observations of received bytes, not a signed chain of custody. Do not infer identity, legality, or absence from partial search coverage.

There is no telemetry or hosted account. Queries are visible to selected search engines, destination hosts, resolvers, and CT/archive providers. A local model receives page excerpts only when explicitly configured. See [detailed boundaries](docs/security/BOUNDARIES.md).
