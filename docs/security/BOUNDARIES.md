# Security boundaries

Enforced for standard HTTP fetches: HTTP(S)-only URLs, no embedded credentials, rejection of non-public destinations by default, DNS answer validation and per-hop pinning, manual redirect revalidation, no implicit proxy, request timeout, body byte cap. Crawl and map also scope to the seed origin, consult robots, and obey bounded budgets and crawl delay. `--allow-private` intentionally bypasses private-address checks and must be reserved for local fixtures.

Not enforced in the optional browser process: public-only DNS for subresources, redirect pinning, per-resource body caps, script-origin isolation, GET side-effect protection, authentication boundaries, CAPTCHA/anti-bot workarounds, or network egress firewalling. A Docker container with loopback-published WebDriver and dropped capabilities is not by itself a private-network firewall. Browser visits are opt-in and must run behind an independently enforced public-egress policy on sensitive systems. A real site can change DOM and URLs between preflight and browser navigation. The `browse` report's HTTP status is unknown (`0`).

AI planning is optional and local-only: model output is parsed into typed actions and checked against rendered controls, and execution requires `--execute-ai-plan`. Page content remains untrusted and may contain prompt injection. A GET link may still change data despite no form/button support. The local model receives a bounded page excerpt; do not browse confidential data just because the model endpoint is on loopback.

Document inputs are bounded by file/archive declarations but parsed in-process; malformed PDF or Office data can still trigger memory/CPU exhaustion or library defects. Use OS resource limits and avoid high-trust host mounts for untrusted documents. No OCR, verified source identity, or conclusion synthesis is provided. Hashes identify the bytes received (or rendered DOM for browser mode), not a signed chain of custody.

For responsible disclosure see [SECURITY.md](../../SECURITY.md). For reproducible runtime instructions see [Servo runtime](../runtime/SERVO.md).
