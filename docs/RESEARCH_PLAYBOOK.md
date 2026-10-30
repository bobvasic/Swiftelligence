# Research playbook

Swiftel distills repeatable practices from public-source company/domain research, historical-page analysis, social-platform triage, market/competitor discovery, and software-offer research. This is an anonymized methodology, not a collection of case files. No investigated subjects, private selectors, account identifiers, search logs, or evidence are shipped.

## Investigate by claim, not by tool

| Question | Preferred first source | Secondary source | Only then |
| --- | --- | --- | --- |
| What does a site say now? | Live first-party page | Dated official document | Public web index and page fetch |
| What did it say before? | Timestamped web archive | Historic official publication | Other archived copies |
| Which hosts were publicly named? | Certificate Transparency / public DNS | Archived URLs | Permissioned active discovery |
| What does an organization offer? | Official pricing and terms | Official event/partner page | Community lead, clearly labeled |
| Is a person or company the right entity? | Official or self-authored anchor | Independent corroboration | Unresolved namesake; do not merge |
| Is a redeemable code real? | Exact string in live original body | Terms + campaign status | Redemption untested until explicitly tested |

Search-result snippets, social claims, coupon aggregators, and indexed copies are leads. They are not proof of a page's current content. An HTTP 200 is not proof that a bot interstitial or loading shell contains the requested information. A service error or an incomplete search is not a negative finding.

## Source-routing lessons

- Structured sources first: public APIs, DNS and CT, archives, official registries, original vendor docs. Use web search for discovery, fetch the original for verification.
- Keep open-source tools modular: SearXNG for self-hosted metasearch; ProjectDiscovery/subfinder and crt.sh for passive host discovery; Maigret and Sherlock as *leads* for public usernames; Wayback/CDX for date-scoped history; ExifTool and SHA-256 for file metadata and integrity. Those tools are not bundled or required by Swiftel.
- Some existing workflows also used hosted services and browser rendering to access public, JS-dependent content. Swiftel deliberately uses no hosted paid API, account login, CAPTCHA solver, stealth evasion, active port scanner, or credential-collection module. A static CLI cannot replace a full interactive browser or a professionally maintained global search index.
- Bound the work: cap search queries, search results, crawl depth/pages/body bytes, redirects, and per-host request frequency. Collect only pages needed for the question.

## Offer and custom-code checklist

1. Search exact brand and canonical domain on official pricing, billing, developer docs, events, hackathons, partner, education, launch, and community pages.
2. Search phrases such as `"promo code"`, `"redeem coupon"`, `"referral code"`, and `"event code"` with the brand. Check the original live body, not just search results.
3. Classify a literal published string separately from automatic annual pricing, signup credits, partner application benefits, private referrals, and codes supplied after a paid bundle purchase.
4. Record a short verbatim excerpt, URL, retrieval time, campaign date/eligibility, and whether redemption was tested. If the event ended, a still-visible code may no longer work.
5. Treat coupon aggregators as leads with possible affiliate spam. Vendor-community user posts are not necessarily vendor statements.
6. If the source is blocked or a JS shell, mark it unverified rather than inventing its content. Never attempt an account, purchase, or checkout to test a code without authorization.
7. De-duplicate by the underlying offer (brand + benefit + eligibility + expiry), not just page URL. An old article rediscovered today is not a newly published offer.

## Evidence contract

For material assertions, carry `source URL`, `retrieved_at` (UTC), `query` (when applicable), `source kind`, an exact excerpt, `observation` versus `inference`, and `confidence`. Preserve failures and coverage gaps. Corroborate sensitive identity or allegation claims independently; a shared name, address, social handle, or adjacent graph node does not by itself establish identity or culpability.

Archive copies are historical evidence with their snapshot date, not current-page evidence. Date first-party pricing before quoting it. A technical fingerprint is an observation, not proof of a site's entire stack. Never enumerate private/breached data, collect unrelated personal details, or infer intent from posting frequency.

## Boundaries of this release

Swiftel is a small passive CLI for public search, HTML retrieval, same-site crawling, offer leads, and public domain metadata. It is not an investigator, threat scanner, person profiler, legal-conclusion engine, Firecrawl-compatible browser renderer, or replacement for every commercial search index. Prefer explicit errors and unverified status to false confidence.
