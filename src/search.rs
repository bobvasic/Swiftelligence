use crate::fetch::{HttpClient, ResponseData};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use serde::Serialize;
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub provider: String,
    pub query: String,
    pub retrieved_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SearchReport {
    pub query: String,
    pub provider: String,
    pub retrieved_at: DateTime<Utc>,
    pub hits: Vec<SearchHit>,
    pub warnings: Vec<String>,
}

pub async fn search(query: &str, limit: usize, searxng: Option<&str>) -> Result<SearchReport> {
    if query.trim().is_empty() {
        bail!("query must not be empty");
    }
    if !(1..=20).contains(&limit) {
        bail!("limit must be between 1 and 20");
    }
    if let Some(base) = searxng {
        return search_searxng(query, limit, base).await;
    }
    let client = HttpClient::new(false);
    let mut warnings = Vec::new();
    let ddg_url = search_url("https://html.duckduckgo.com/html/", query, None)?;
    let ddg = client.get(ddg_url.as_str(), 1024 * 1024, None).await;
    match ddg {
        Ok(response) if response.status == 200 => {
            let hits = parse_ddg(&response, query, limit);
            if !hits.is_empty() {
                return Ok(SearchReport {
                    query: query.to_string(),
                    provider: "duckduckgo_html".into(),
                    retrieved_at: response.retrieved_at,
                    hits,
                    warnings,
                });
            }
            warnings.push("DuckDuckGo returned no parseable results; trying Bing RSS (coverage may be incomplete)".into());
        }
        Ok(response) => warnings.push(format!(
            "DuckDuckGo HTTP {}; trying Bing RSS",
            response.status
        )),
        Err(error) => warnings.push(format!("DuckDuckGo unavailable ({error}); trying Bing RSS")),
    }
    let bing_url = search_url(
        "https://www.bing.com/search",
        query,
        Some(("format", "rss")),
    )?;
    let response = client.get(bing_url.as_str(), 1024 * 1024, None).await?;
    if response.status != 200 {
        bail!(
            "Bing RSS returned HTTP {}; search incomplete; {}",
            response.status,
            warnings.join("; ")
        );
    }
    let hits = parse_bing_rss(&response, query, limit)?;
    if hits.is_empty() {
        warnings.push("Bing RSS returned zero results; this does not establish absence".into());
    }
    Ok(SearchReport {
        query: query.to_string(),
        provider: "bing_rss".into(),
        retrieved_at: response.retrieved_at,
        hits,
        warnings,
    })
}

fn search_url(base: &str, query: &str, extra: Option<(&str, &str)>) -> Result<Url> {
    let mut url = Url::parse(base)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", query);
        if let Some((key, value)) = extra {
            pairs.append_pair(key, value);
        }
    }
    Ok(url)
}

async fn search_searxng(query: &str, limit: usize, base: &str) -> Result<SearchReport> {
    let mut endpoint = Url::parse(base).context("invalid SearXNG URL")?;
    endpoint.set_path("/search");
    endpoint.set_query(None);
    endpoint
        .query_pairs_mut()
        .append_pair("q", query)
        .append_pair("format", "json");
    let host = endpoint
        .host_str()
        .unwrap_or_default()
        .trim_matches(['[', ']']);
    let local = host == "localhost" || host == "127.0.0.1" || host == "::1";
    let response = HttpClient::new(local)
        .get(
            endpoint.as_str(),
            1024 * 1024,
            Some(&endpoint.origin().ascii_serialization()),
        )
        .await?;
    if response.status != 200 {
        bail!(
            "SearXNG returned HTTP {}; check instance JSON enablement/rate limit",
            response.status
        );
    }
    let payload: serde_json::Value = serde_json::from_slice(&response.body)
        .context("SearXNG did not return JSON; enable the JSON format")?;
    let mut warnings = Vec::new();
    if let Some(engines) = payload
        .get("unresponsive_engines")
        .and_then(|v| v.as_array())
        && !engines.is_empty()
    {
        warnings.push(format!(
            "{} unresponsive search engines; results incomplete",
            engines.len()
        ));
    }
    let mut seen = HashSet::new();
    let mut hits = Vec::new();
    for item in payload
        .get("results")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let Some(url) = item
            .get("url")
            .and_then(|v| v.as_str())
            .and_then(normalize_hit_url)
        else {
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        hits.push(SearchHit {
            url,
            title: item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            snippet: item
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            provider: "searxng".into(),
            query: query.into(),
            retrieved_at: response.retrieved_at,
        });
        if hits.len() == limit {
            break;
        }
    }
    if hits.is_empty() {
        warnings.push("Zero hits from this instance; not an exhaustive web negative".into());
    }
    Ok(SearchReport {
        query: query.into(),
        provider: "searxng".into(),
        retrieved_at: response.retrieved_at,
        hits,
        warnings,
    })
}

fn normalize_hit_url(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.username() != "" || url.password().is_some()
    {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn parse_ddg(response: &ResponseData, query: &str, limit: usize) -> Vec<SearchHit> {
    let html = Html::parse_document(&String::from_utf8_lossy(&response.body));
    let result = Selector::parse(".result").unwrap();
    let anchor = Selector::parse(".result__a").unwrap();
    let snippet = Selector::parse(".result__snippet").unwrap();
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    for item in html.select(&result) {
        let Some(a) = item.select(&anchor).next() else {
            continue;
        };
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let resolved = if href.starts_with("//") {
            format!("https:{href}")
        } else {
            href.to_string()
        };
        let target = Url::parse(&resolved).ok().and_then(|u| {
            if u.host_str()
                .is_some_and(|host| host.ends_with("duckduckgo.com"))
            {
                u.query_pairs()
                    .find(|(key, _)| key == "uddg")
                    .map(|(_, value)| value.into_owned())
            } else {
                Some(u.to_string())
            }
        });
        let Some(url) = target.and_then(|value| normalize_hit_url(&value)) else {
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        hits.push(SearchHit {
            url,
            title: a
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            snippet: item
                .select(&snippet)
                .next()
                .map(|s| s.text().collect::<Vec<_>>().join(" "))
                .unwrap_or_default()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            provider: "duckduckgo_html".into(),
            query: query.into(),
            retrieved_at: response.retrieved_at,
        });
        if hits.len() == limit {
            break;
        }
    }
    hits
}

fn parse_bing_rss(response: &ResponseData, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let body = std::str::from_utf8(&response.body).context("invalid RSS encoding")?;
    let doc = roxmltree::Document::parse(body).context("Bing RSS returned non-XML body")?;
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let field = |name: &str| {
            item.children()
                .find(|c| c.has_tag_name(name))
                .and_then(|c| c.text())
                .unwrap_or("")
        };
        let Some(url) = normalize_hit_url(field("link")) else {
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        hits.push(SearchHit {
            url,
            title: field("title").to_string(),
            snippet: field("description").to_string(),
            provider: "bing_rss".into(),
            query: query.into(),
            retrieved_at: response.retrieved_at,
        });
        if hits.len() == limit {
            break;
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_deduplicates_and_limits() {
        let payload = b"<rss><channel><item><title>A</title><link>https://example.com/a#top</link><description>one</description></item><item><title>B</title><link>https://example.com/a</link></item></channel></rss>";
        let resp = ResponseData {
            url: "https://www.bing.com/".into(),
            status: 200,
            content_type: "text/xml".into(),
            retrieved_at: Utc::now(),
            body: payload.to_vec(),
        };
        let hits = parse_bing_rss(&resp, "test", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/a");
    }
}
