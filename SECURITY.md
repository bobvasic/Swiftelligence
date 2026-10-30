# Security policy

Please report security vulnerabilities privately through the repository's security advisory feature after the project has been published. Until then, do not include exploit details in a public issue.

## Boundaries

Swiftel is intended for public HTTP(S) content only. Default URL checks reject local/private/reserved addresses, credentials in URLs, and unsupported schemes. Redirects and resolved addresses must be revalidated. Crawl respects robots directives and bounded request budgets. Never use `--allow-private` on untrusted URLs; it exists for controlled localhost fixtures and intentionally relaxes network isolation.

Results are observations, not verified identity or legal conclusions. Do not ingest secrets, private account data, breached data, or internal network endpoints. Avoid running CLI with privileged OS credentials. Audit contributions that introduce network clients, redirects, proxy support, or browser execution particularly carefully.

No telemetry or hosted account is part of the project. Queries are still disclosed to the chosen search engine, page host, DNS resolver, and CT/archive providers; self-host SearXNG and use your own resolver when network metadata matters.
