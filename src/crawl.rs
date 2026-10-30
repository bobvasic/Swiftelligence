use crate::extract::{Page, fetch_page};
use crate::fetch::HttpClient;
use crate::robots;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use url::Url;

#[derive(Debug, Serialize)]
pub struct CrawlError {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct CrawlReport {
    pub seed: String,
    pub retrieved_at: DateTime<Utc>,
    pub attempted: usize,
    pub pages: Vec<Page>,
    pub errors: Vec<CrawlError>,
}

pub async fn crawl(
    client: &HttpClient,
    seed: &str,
    max_pages: usize,
    max_depth: usize,
) -> Result<CrawlReport> {
    if !(1..=100).contains(&max_pages) {
        bail!("max-pages must be between 1 and 100");
    }
    if max_depth > 3 {
        bail!("max-depth must be between 0 and 3");
    }
    let mut start = Url::parse(seed)?;
    start.set_fragment(None);
    let origin = start.origin().ascii_serialization();
    let policy = robots::load(client, &start).await?;
    let delay = policy.delay()?;
    let mut queue = VecDeque::from([(start.to_string(), 0usize)]);
    let mut seen = HashSet::from([start.to_string()]);
    let mut report = CrawlReport {
        seed: start.to_string(),
        retrieved_at: Utc::now(),
        attempted: 0,
        pages: Vec::new(),
        errors: Vec::new(),
    };
    while let Some((url, depth)) = queue.pop_front() {
        if report.attempted >= max_pages {
            break;
        }
        let parsed = Url::parse(&url)?;
        if !policy.allowed(&parsed) {
            report.errors.push(CrawlError {
                url,
                error: "disallowed by robots.txt".to_string(),
            });
            continue;
        }
        if report.attempted > 0 {
            tokio::time::sleep(delay).await;
        }
        report.attempted += 1;
        match fetch_page(client, &url, Some(&origin)).await {
            Ok(page) => {
                if depth < max_depth {
                    for link in &page.links {
                        if let Ok(parsed) = Url::parse(link)
                            && parsed.origin().ascii_serialization() == origin
                            && seen.insert(link.clone())
                        {
                            queue.push_back((link.clone(), depth + 1));
                        }
                    }
                }
                report.pages.push(page);
            }
            Err(error) => report.errors.push(CrawlError {
                url,
                error: format!("{error:#}"),
            }),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn local_fixture_respects_robots_scope_and_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0u8; 4096];
                let read = socket.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request.split_whitespace().nth(1).unwrap_or("/");
                let (status, body) = match path {
                    "/robots.txt" => ("200 OK", "User-agent: Swiftel\nDisallow: /secret\n".to_string()),
                    "/" => ("200 OK", "<html><title>Home</title><body><main><p>Welcome</p><a href='/about'>About</a><a href='/secret'>Secret</a><a href='http://127.0.0.1:9/outside'>Outside</a></main></body></html>".to_string()),
                    "/about" => ("200 OK", "<html><title>About</title><body><p>About page.</p></body></html>".into()),
                    _ => ("404 Not Found", "missing".into()),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let url = format!("http://127.0.0.1:{port}/");
        let report = crawl(&HttpClient::new(true), &url, 3, 1).await.unwrap();
        server.abort();
        assert_eq!(report.attempted, 2);
        assert_eq!(report.pages.len(), 2);
        assert!(report.errors.iter().any(|e| e.error.contains("robots.txt")));
        assert!(report.pages.iter().all(|p| p.url.starts_with(&url)));
    }
}
