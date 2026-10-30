use crate::fetch::HttpClient;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeSet;
use url::Url;

#[derive(Debug, Serialize)]
pub struct ArchiveSnapshot {
    pub url: String,
    pub timestamp: String,
    pub status: String,
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DomainReport {
    pub domain: String,
    pub retrieved_at: DateTime<Utc>,
    pub ct_source: &'static str,
    pub hostnames: Vec<String>,
    pub archive: Option<ArchiveSnapshot>,
    pub errors: Vec<String>,
    pub note: &'static str,
}

pub async fn domain(input: &str) -> Result<DomainReport> {
    let domain = input.trim().trim_end_matches('.').to_ascii_lowercase();
    validate_domain(&domain)?;
    let mut report = DomainReport {
        domain: domain.clone(),
        retrieved_at: Utc::now(),
        ct_source: "https://crt.sh/",
        hostnames: Vec::new(),
        archive: None,
        errors: Vec::new(),
        note: "Certificate records and the closest available archive snapshot are historical observations, not proof of current ownership or active hosts.",
    };
    let client = HttpClient::new(false);
    let mut ct = Url::parse("https://crt.sh/")?;
    ct.query_pairs_mut()
        .append_pair("q", &format!("%.{domain}"))
        .append_pair("output", "json");
    match client.get(ct.as_str(), 2 * 1024 * 1024, None).await {
        Ok(resp) if resp.status == 200 => {
            match serde_json::from_slice::<serde_json::Value>(&resp.body) {
                Ok(value) => {
                    let mut names = BTreeSet::new();
                    for item in value.as_array().into_iter().flatten() {
                        if let Some(raw) = item.get("name_value").and_then(|v| v.as_str()) {
                            for name in raw.lines() {
                                let name = name
                                    .trim()
                                    .trim_start_matches("*.")
                                    .trim_end_matches('.')
                                    .to_ascii_lowercase();
                                if name == domain || name.ends_with(&format!(".{domain}")) {
                                    names.insert(name);
                                }
                            }
                        }
                    }
                    report.hostnames = names.into_iter().take(500).collect();
                }
                Err(error) => report
                    .errors
                    .push(format!("crt.sh non-JSON response: {error}")),
            }
        }
        Ok(resp) => report.errors.push(format!("crt.sh HTTP {}", resp.status)),
        Err(error) => report.errors.push(format!("crt.sh unavailable: {error:#}")),
    }
    let mut archive = Url::parse("https://archive.org/wayback/available")?;
    archive.query_pairs_mut().append_pair("url", &domain);
    match client.get(archive.as_str(), 512 * 1024, None).await {
        Ok(resp) if resp.status == 200 => {
            match serde_json::from_slice::<serde_json::Value>(&resp.body) {
                Ok(value) => {
                    let closest = &value["archived_snapshots"]["closest"];
                    if let (Some(url), Some(timestamp)) =
                        (closest["url"].as_str(), closest["timestamp"].as_str())
                    {
                        report.archive = Some(ArchiveSnapshot {
                            url: url.to_string(),
                            timestamp: timestamp.to_string(),
                            status: closest["status"].as_str().unwrap_or("").to_string(),
                            source: "https://archive.org/wayback/available",
                        });
                    }
                }
                Err(error) => report
                    .errors
                    .push(format!("Wayback non-JSON response: {error}")),
            }
        }
        Ok(resp) => report.errors.push(format!("Wayback HTTP {}", resp.status)),
        Err(error) => report
            .errors
            .push(format!("Wayback unavailable: {error:#}")),
    }
    Ok(report)
}

fn validate_domain(value: &str) -> Result<()> {
    if value.len() > 253 || !value.contains('.') || value.parse::<std::net::IpAddr>().is_ok() {
        bail!("expected a public DNS domain, e.g. example.com");
    }
    for label in value.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            bail!("invalid domain label");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn domain_input_is_strict() {
        assert!(validate_domain("example.com").is_ok());
        for value in [
            "http://example.com",
            "example.com/path",
            "127.0.0.1",
            "a..com",
            "-bad.com",
            "example.com:8080",
        ] {
            assert!(validate_domain(value).is_err(), "{value}");
        }
    }
}
