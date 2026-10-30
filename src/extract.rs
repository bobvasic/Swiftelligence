use crate::fetch::{HttpClient, PAGE_LIMIT};
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use url::Url;

#[derive(Debug, Serialize)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub text: String,
    pub links: Vec<String>,
    pub status: u16,
    pub retrieved_at: DateTime<Utc>,
    pub sha256: String,
}

pub async fn fetch_page(client: &HttpClient, input: &str, origin: Option<&str>) -> Result<Page> {
    let response = client.get(input, PAGE_LIMIT, origin).await?;
    if !(200..300).contains(&response.status) {
        bail!("HTTP {} at {}", response.status, response.url);
    }
    if !response.content_type.is_empty()
        && !response.content_type.starts_with("text/html")
        && !response.content_type.starts_with("application/xhtml+xml")
    {
        bail!(
            "unsupported content type at {}: {}",
            response.url,
            response.content_type
        );
    }
    if response.content_type.is_empty()
        && !response.body.starts_with(b"<!")
        && !response.body.starts_with(b"<html")
    {
        bail!("response is not recognizable HTML at {}", response.url);
    }
    let hash = format!("{:x}", Sha256::digest(&response.body));
    let html = String::from_utf8_lossy(&response.body);
    let (title, text, links) = extract_html(&html, &response.url);
    let heading = title.to_ascii_lowercase();
    if text.trim().is_empty()
        || [
            "just a moment",
            "attention required",
            "access denied",
            "verify you are human",
            "captcha",
        ]
        .iter()
        .any(|marker| heading.contains(marker))
        || (text.len() < 60
            && ["loading...", "enable javascript", "checking your browser"]
                .iter()
                .any(|marker| html.to_ascii_lowercase().contains(marker)))
    {
        bail!(
            "challenge or empty JS shell at {}; content not verified",
            response.url
        );
    }
    Ok(Page {
        url: response.url,
        title,
        text,
        links,
        status: response.status,
        retrieved_at: response.retrieved_at,
        sha256: hash,
    })
}

pub fn extract_html(html: &str, source: &str) -> (String, String, Vec<String>) {
    let document = Html::parse_document(html);
    let title_sel = Selector::parse("title").unwrap();
    let title = document
        .select(&title_sel)
        .next()
        .map(|node| compact(&node.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_default();

    let mut text = String::new();
    let sections = Selector::parse("main,article,body").unwrap();
    let parts = Selector::parse("h1,h2,h3,h4,p,li,blockquote,pre,dt,dd").unwrap();
    if let Some(root) = document.select(&sections).next() {
        for part in root.select(&parts) {
            let line = compact(&part.text().collect::<Vec<_>>().join(" "));
            if !line.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&line);
                if text.len() > 60_000 {
                    break;
                }
            }
        }
    }
    if text.is_empty()
        && let Some(root) = document.select(&Selector::parse("body").unwrap()).next()
    {
        let mut chunks = Vec::new();
        for node in root.descendants() {
            if let Some(value) = node.value().as_text()
                && !node
                    .ancestors()
                    .filter_map(scraper::ElementRef::wrap)
                    .any(|parent| {
                        matches!(
                            parent.value().name(),
                            "script" | "style" | "noscript" | "nav" | "footer"
                        )
                    })
            {
                chunks.push(value.to_string());
            }
        }
        text = compact(&chunks.join(" "));
    }
    text.truncate(text.floor_char_boundary(60_000));

    let base = Url::parse(source).ok();
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    let anchors = Selector::parse("a[href]").unwrap();
    for a in document.select(&anchors) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(base) = &base else { continue };
        let Ok(mut link) = base.join(href) else {
            continue;
        };
        if !matches!(link.scheme(), "http" | "https")
            || link.username() != ""
            || link.password().is_some()
        {
            continue;
        }
        link.set_fragment(None);
        let url = link.to_string();
        if seen.insert(url.clone()) {
            links.push(url);
        }
        if links.len() >= 500 {
            break;
        }
    }
    (title, text, links)
}

fn compact(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_preserves_source_and_drops_script() {
        let raw = "<html><head><title>  Example &amp; Company </title><script>secret()</script></head><body><main><h1>News</h1><p>A claim  in the original body.</p><a href='/about#top'>About</a></main></body></html>";
        let (title, text, links) = extract_html(raw, "https://example.com/");
        assert_eq!(title, "Example & Company");
        assert!(text.contains("A claim in the original body."));
        assert!(!text.contains("secret()"));
        assert_eq!(links, vec!["https://example.com/about"]);
    }
}
