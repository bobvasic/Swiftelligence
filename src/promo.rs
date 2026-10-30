use crate::extract::fetch_page;
use crate::fetch::HttpClient;
use crate::search;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Serialize)]
pub struct PromoHit {
    pub code: String,
    pub source_url: String,
    pub source_title: String,
    pub excerpt: String,
    pub retrieved_at: DateTime<Utc>,
    pub status: &'static str,
    pub source_type: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PromoReport {
    pub brand: String,
    pub queries: Vec<String>,
    pub pages_checked: usize,
    pub codes: Vec<PromoHit>,
    pub warnings: Vec<String>,
    pub page_errors: Vec<String>,
    pub note: &'static str,
}

pub async fn promo(brand: &str, searxng: Option<&str>) -> Result<PromoReport> {
    let brand = brand.trim();
    if brand.is_empty() || brand.len() > 100 {
        bail!("brand must be between 1 and 100 characters");
    }
    let domain = if brand.contains('.') && !brand.contains(' ') {
        Some(brand.to_ascii_lowercase())
    } else {
        None
    };
    let queries = if domain.is_some() {
        vec![
            format!("site:{brand} \"promo code\" OR \"coupon code\""),
            format!("\"{brand}\" \"event code\" OR \"redeem coupon\""),
        ]
    } else {
        vec![
            format!("\"{brand}\" \"promo code\" official"),
            format!("\"{brand}\" \"redeem coupon\" hackathon event"),
        ]
    };
    let mut report = PromoReport {
        brand: brand.into(),
        queries: queries.clone(),
        pages_checked: 0,
        codes: Vec::new(),
        warnings: Vec::new(),
        page_errors: Vec::new(),
        note: "Published strings only; eligibility, expiry, and redemption are NOT verified. No result is not proof of no offer.",
    };
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for query in &queries {
        match search::search(query, 6, searxng).await {
            Ok(results) => {
                report.warnings.extend(results.warnings);
                for hit in results.hits {
                    let Ok(url) = Url::parse(&hit.url) else {
                        continue;
                    };
                    let host = url.host_str().unwrap_or_default();
                    if is_aggregator(host) {
                        continue;
                    }
                    if seen.insert(hit.url.clone()) {
                        let first_party = domain
                            .as_ref()
                            .is_some_and(|d| host == d || host.ends_with(&format!(".{d}")));
                        candidates.push((!first_party, hit.url));
                    }
                }
            }
            Err(error) => report
                .warnings
                .push(format!("search failed for {query}: {error:#}")),
        }
    }
    candidates.sort_by_key(|(third_party, _)| *third_party);
    let client = HttpClient::new(false);
    let mut codes = HashSet::new();
    for (third_party, url) in candidates.into_iter().take(8) {
        match fetch_page(&client, &url, None).await {
            Ok(page) => {
                report.pages_checked += 1;
                for (code, excerpt) in extract_codes(&page.text) {
                    if codes.insert(code.to_ascii_lowercase()) {
                        report.codes.push(PromoHit {
                            code,
                            source_url: page.url.clone(),
                            source_title: page.title.clone(),
                            excerpt,
                            retrieved_at: page.retrieved_at,
                            status: "published_unverified",
                            source_type: if third_party {
                                "public_page"
                            } else {
                                "supplied_domain"
                            },
                        });
                    }
                }
            }
            Err(error) => report.page_errors.push(format!("{url}: {error:#}")),
        }
    }
    if report.codes.is_empty() {
        report.warnings.push("No literal code verified in fetched page bodies within this bounded search; check official event and partner terms manually".into());
    }
    Ok(report)
}

fn is_aggregator(host: &str) -> bool {
    [
        "hotdeals.com",
        "coupons.com",
        "retailmenot.com",
        "honey.com",
        "simplycodes.com",
        "worthepenny.com",
        "tenereteam.com",
        "couponbirds.com",
    ]
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

pub fn extract_codes(text: &str) -> Vec<(String, String)> {
    let patterns = [
        r#"(?i)\b(?:promo(?:tional)?|coupon|discount|referral|event)[ \t]+code[ \t]*(?::|=|-|—|is)?[ \t]*["'`]?([a-z0-9][a-z0-9_-]{4,23})\b"#,
        r#"(?i)\b(?:use|enter|redeem)[ \t]+(?:the[ \t]+)?(?:(?:promo|coupon|discount)[ \t]+)?code[ \t]*["'`]?([a-z0-9][a-z0-9_-]{4,23})\b"#,
    ];
    let ignored = [
        "AVAILABLE",
        "EXPIRED",
        "DISCOUNT",
        "PROMO",
        "COUPON",
        "REDEEM",
        "HERE",
        "BELOW",
        "ABOVE",
        "VALID",
        "SAVE",
        "FIRST",
        "ENTER",
        "START",
        "ONLY",
        "WORKS",
        "WHICH",
        "WHEN",
        "SHOWN",
        "FIELD",
        "SHARED",
        "APPLY",
    ];
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for line in text.lines() {
        if line.len() > 1000 {
            continue;
        }
        for pattern in patterns {
            let Ok(regex) = Regex::new(pattern) else {
                continue;
            };
            for capture in regex.captures_iter(line) {
                let Some(value) = capture.get(1) else {
                    continue;
                };
                let code = value.as_str();
                if ignored.contains(&code.to_ascii_uppercase().as_str())
                    || code.chars().all(|c| c.is_ascii_digit())
                {
                    continue;
                }
                if !seen.insert(code.to_ascii_lowercase()) {
                    continue;
                }
                let excerpt = line.chars().take(240).collect::<String>();
                results.push((code.to_string(), excerpt));
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codes_need_literal_context() {
        let text = "Annual pricing saves 20%.\nCoupon code: LAUNCH2026 for eligible new accounts.\nUse code HELLO-2026 at checkout.\nPromo code is available later.\n";
        let codes = extract_codes(text);
        assert_eq!(codes.len(), 2);
        assert!(codes.iter().any(|(code, _)| code == "LAUNCH2026"));
        assert!(codes.iter().any(|(code, _)| code == "HELLO-2026"));
    }
}
