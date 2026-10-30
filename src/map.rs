//! Same-origin discovery from robots sitemaps and visible links.
use crate::extract::fetch_page;
use crate::fetch::HttpClient;
use crate::robots::{self, Robots};
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use url::Url;

#[derive(Debug, Serialize)]
pub struct MapEntry {
    pub url: String,
    pub discovered_via: &'static str,
    pub depth: usize,
}

#[derive(Debug, Serialize)]
pub struct MapReport {
    pub seed: String,
    pub retrieved_at: DateTime<Utc>,
    pub pages_inspected: usize,
    pub sitemaps_inspected: usize,
    pub urls: Vec<MapEntry>,
    pub errors: Vec<String>,
    pub note: &'static str,
}

fn scoped(raw: &str, origin: &str, policy: &Robots) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.origin().ascii_serialization() != origin
        || !policy.allowed(&url)
    {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn xml_links(body: &[u8]) -> Result<(bool, Vec<String>)> {
    let xml = std::str::from_utf8(body)?;
    let tree = roxmltree::Document::parse(xml)?;
    let root = tree.root_element().tag_name().name();
    if !matches!(root, "urlset" | "sitemapindex") {
        bail!("not an XML sitemap");
    }
    let parent = if root == "urlset" { "url" } else { "sitemap" };
    let links = tree
        .root_element()
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == parent)
        .filter_map(|n| {
            n.children()
                .find(|c| c.is_element() && c.tag_name().name() == "loc")
        })
        .filter_map(|n| n.text())
        .take(2_000)
        .map(str::to_string)
        .collect();
    Ok((root == "sitemapindex", links))
}

/// Discovery is bounded by two budgets: HTML page reads and sitemap reads.
pub async fn map(
    client: &HttpClient,
    seed: &str,
    max_urls: usize,
    max_pages: usize,
) -> Result<MapReport> {
    if !(1..=500).contains(&max_urls) || max_pages > 25 {
        bail!("max-urls must be 1..=500 and max-pages 0..=25");
    }
    let mut start = Url::parse(seed)?;
    crate::fetch::validate_url(&start, client.allow_private())?;
    start.set_fragment(None);
    let origin = start.origin().ascii_serialization();
    let policy = robots::load(client, &start).await?;
    let delay = policy.delay()?;
    let mut report = MapReport {
        seed: start.to_string(),
        retrieved_at: Utc::now(),
        pages_inspected: 0,
        sitemaps_inspected: 0,
        urls: Vec::new(),
        errors: Vec::new(),
        note: "Bounded discovery, not a complete index; blocked and unlinked pages may be absent.",
    };
    let mut found = HashSet::new();
    if let Some(url) = scoped(start.as_str(), &origin, &policy) {
        found.insert(url.clone());
        report.urls.push(MapEntry {
            url,
            discovered_via: "seed",
            depth: 0,
        });
    } else {
        bail!("seed is disallowed by robots.txt");
    }
    let default_sitemap = start.join("/sitemap.xml")?.to_string();
    let mut sitemaps: VecDeque<(String, usize)> = policy
        .sitemaps()
        .iter()
        .filter_map(|s| scoped(s, &origin, &policy))
        .map(|s| (s, 0))
        .collect();
    sitemaps.push_back((default_sitemap, 0));
    let mut seen_maps = HashSet::new();
    while let Some((url, depth)) = sitemaps.pop_front() {
        if report.sitemaps_inspected >= 8 || report.urls.len() >= max_urls {
            break;
        }
        if !seen_maps.insert(url.clone()) || scoped(&url, &origin, &policy).is_none() {
            continue;
        }
        if report.sitemaps_inspected > 0 {
            tokio::time::sleep(delay).await;
        }
        report.sitemaps_inspected += 1;
        match client.get(&url, 1024 * 1024, Some(&origin)).await {
            Ok(response) if response.status == 200 => match xml_links(&response.body) {
                Ok((index, links)) => {
                    for raw in links {
                        let Some(link) = scoped(&raw, &origin, &policy) else {
                            continue;
                        };
                        if index {
                            if depth < 2 && seen_maps.len() + sitemaps.len() < 16 {
                                sitemaps.push_back((link, depth + 1));
                            }
                        } else if found.insert(link.clone()) {
                            report.urls.push(MapEntry {
                                url: link,
                                discovered_via: "sitemap",
                                depth: 0,
                            });
                            if report.urls.len() >= max_urls {
                                break;
                            }
                        }
                    }
                }
                Err(error) => report.errors.push(format!("{url}: {error}")),
            },
            Ok(response)
                if url == start.join("/sitemap.xml")?.as_str() && response.status == 404 => {}
            Ok(response) => report
                .errors
                .push(format!("{url}: HTTP {}", response.status)),
            Err(error) => report.errors.push(format!("{url}: {error:#}")),
        }
    }
    let mut queue = VecDeque::from([(start.to_string(), 0)]);
    let mut queued = HashSet::from([start.to_string()]);
    let mut visited = HashSet::new();
    while let Some((url, depth)) = queue.pop_front() {
        if report.pages_inspected >= max_pages || report.urls.len() >= max_urls {
            break;
        }
        if !visited.insert(url.clone()) || scoped(&url, &origin, &policy).is_none() {
            continue;
        }
        if report.pages_inspected + report.sitemaps_inspected > 0 {
            tokio::time::sleep(delay).await;
        }
        report.pages_inspected += 1;
        match fetch_page(client, &url, Some(&origin)).await {
            Ok(page) => {
                for raw in page.links {
                    if let Some(link) = scoped(&raw, &origin, &policy) {
                        if found.insert(link.clone()) {
                            report.urls.push(MapEntry {
                                url: link.clone(),
                                discovered_via: "link",
                                depth: depth + 1,
                            });
                        }
                        if depth < 2 && queue.len() < 50 && queued.insert(link.clone()) {
                            queue.push_back((link, depth + 1));
                        }
                        if report.urls.len() >= max_urls {
                            break;
                        }
                    }
                }
            }
            Err(error) => report.errors.push(format!("{url}: {error:#}")),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn sitemap_discovery_still_inspects_linked_page() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let origin = format!("http://127.0.0.1:{port}");
        let fixture_origin = origin.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let n = stream.read(&mut request).await.unwrap();
                let text = String::from_utf8_lossy(&request[..n]);
                let path = text.split_whitespace().nth(1).unwrap_or("/");
                let (code, body) = match path {
                    "/robots.txt" => (
                        "200 OK",
                        format!(
                            "User-agent: *\nDisallow: /secret\nSitemap: {fixture_origin}/sitemap.xml\n"
                        ),
                    ),
                    "/sitemap.xml" => (
                        "200 OK",
                        format!("<urlset><url><loc>{fixture_origin}/next</loc></url></urlset>"),
                    ),
                    "/" => (
                        "200 OK",
                        "<html><body><p>Home</p><a href='/next'>Next</a></body></html>".to_string(),
                    ),
                    "/next" => (
                        "200 OK",
                        "<html><body><p>Next page</p><a href='/third'>Third</a></body></html>"
                            .to_string(),
                    ),
                    _ => ("404 Not Found", "missing".to_string()),
                };
                let response = format!(
                    "HTTP/1.1 {code}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let report = map(&HttpClient::new(true), &format!("{origin}/"), 10, 2)
            .await
            .unwrap();
        server.abort();
        assert_eq!(report.pages_inspected, 2);
        assert!(
            report
                .urls
                .iter()
                .any(|e| e.url == format!("{origin}/third"))
        );
        assert!(report.urls.iter().all(|e| !e.url.ends_with("/secret")));
    }
    #[test]
    fn sitemap_index_and_namespace_are_parsed() {
        let (index, links) = xml_links(br#"<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><sitemap><loc>https://example.org/s2.xml</loc></sitemap></sitemapindex>"#).unwrap();
        assert!(index);
        assert_eq!(links, vec!["https://example.org/s2.xml"]);
        let (index, links) =
            xml_links(br#"<urlset><url><loc>https://example.org/a</loc></url></urlset>"#).unwrap();
        assert!(!index);
        assert_eq!(links, vec!["https://example.org/a"]);
    }
    #[test]
    fn rejects_foreign_and_disallowed_urls() {
        let policy = Robots::parse("User-agent: *\nDisallow: /secret\n");
        assert!(scoped("https://example.org/secret", "https://example.org", &policy).is_none());
        assert!(scoped("https://other.org/", "https://example.org", &policy).is_none());
        assert_eq!(
            scoped("https://example.org/a#b", "https://example.org", &policy).unwrap(),
            "https://example.org/a"
        );
    }
}
