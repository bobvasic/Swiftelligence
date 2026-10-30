# Component architecture

The `swiftel` binary routes commands in `src/main.rs`; no hosted Swiftel service or account is required.

| Module | Boundary |
| --- | --- |
| `search` | Keyless public search or optional JSON-enabled SearXNG, with partial-coverage warnings. |
| `fetch` | Public-address checks, DNS pinning, bounded HTTP bodies, manual redirect revalidation and same-origin scope. |
| `robots` | Swiftel-specific robots matching, sitemap declarations and crawl-delay parsing. |
| `extract`, `markdown` | Deterministic HTML text, links and LLM-readable Markdown, preserving fetched-source SHA-256. |
| `crawl`, `map` | Same-origin traversal and URL discovery with separate budgets. |
| `index` | In-memory BM25 ranking and source-grounded excerpts; no persisted global index. |
| `document` | Rust PDF text-layer and Office package decoding, Markdown and structured JSON. |
| `browser` | Optional local Servo WebDriver session, restricted anchor click and text input. |
| `plan` | Optional local Ollama JSON proposal, checked against rendered page controls; preview before execution. |
| `promo`, `domain` | Literal published-code leads and passive CT/archive lookup. |

The operational paths are:

1. `map` loads robots for the seed origin, inspects bounded sitemap indexes, then HTML links; it emits discovered URLs with method and depth. A discovered URL is not a fetched page or evidence that a page is accessible.
2. `crawl` traverses same-origin HTML within page/depth budgets, honoring robots and delay. `research --site` feeds its pages into `index`; ordinary `research` uses provider-discovered URLs, fetches public HTML or supported document URLs, then ranks only actual fetched text.
3. `document` accepts a local file or validated public HTTP(S) URL and returns source, SHA-256 of input bytes, retrieval timestamp, Markdown, format and structured JSON. It does not perform OCR.
4. `browse` validates the top-level URL via the HTTP client, then renders it in a separate Servo process. Its SHA-256 covers the rendered DOM, not a network response. It reports status `0` because WebDriver does not expose a reliable HTTP response code. Browser subrequests are not protected by the Rust HTTP client's URL/DNS checks.
5. With `browse --prompt`, a first browser snapshot provides permitted ID-addressable controls to a local Ollama model. Swiftel validates its JSON actions against those controls. Only `--execute-ai-plan` opens a second browser session and executes them. The model is a proposer, never a policy engine.

All budgets are per invocation, not a distributed scheduling/throughput guarantee. Document parsing is in-process: compressed ZIP sizes are checked before parsing, but a malicious parser input may still consume CPU or memory. Run untrusted files with OS resource limits.
