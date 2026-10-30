# Contributing

Swiftel is built for verifiable, passive research. Contributions are welcome if they keep the binary useful without paid accounts or hidden dependencies.

1. Open an issue describing the user problem and the source's public API or protocol. Avoid submitting private target information or live case material.
2. Keep each change narrow. Add a deterministic fixture test for parsers and a regression test for security-sensitive URL, redirect, robots, or crawl logic.
3. Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets`.
4. Document whether an integration is keyless, self-hosted, or requires a third party, and label coverage gaps. Never commit tokens, cookies, account IDs, screenshots from private sessions, or personal evidence.
5. Respect `robots.txt`, server rate limits, and legal scope. Do not add CAPTCHA solving, stealth-bypass defaults, active scanning, or credential/identity-enumeration workflows.

The license is MIT; contributions are licensed under MIT when merged.
