//! Rust WebDriver client for an isolated, headless Servo browser.
//! The driver MUST be bound/published only on loopback. Never expose it to the Internet.
use crate::{
    extract::{self, Page},
    fetch::{self, HttpClient, PAGE_LIMIT},
};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use reqwest::{Client, Method};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use url::Url;

const ELEMENT: &str = "element-6066-11e4-a52e-4f735466cecf";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Click { selector: String },
    Type { selector: String, value: String },
    Wait { milliseconds: u64 },
}

#[derive(Debug, Serialize)]
pub struct Control {
    pub selector: String,
    pub kind: &'static str,
    pub label: String,
    pub target: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BrowserReport {
    pub engine: &'static str,
    pub actions_executed: usize,
    pub page: Page,
    pub controls: Vec<Control>,
    pub ai_plan: Option<Vec<Action>>,
    pub planner: Option<String>,
    pub note: &'static str,
}

/// Only expose elements with simple ID selectors; never propose arbitrary JS/forms.
fn controls(html: &str, source: &str) -> Vec<Control> {
    let doc = Html::parse_document(html);
    let Ok(selector) = Selector::parse("a[id][href],input[id],textarea[id]") else {
        return Vec::new();
    };
    let base = Url::parse(source).ok();
    doc.select(&selector)
        .filter_map(|node| {
            let id = node.value().attr("id")?;
            if id.len() > 64
                || id.is_empty()
                || !id.as_bytes()[0].is_ascii_alphabetic()
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                return None;
            }
            let name = node.value().name();
            let target = if name == "a" {
                let url = base.as_ref()?.join(node.value().attr("href")?).ok()?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.origin() != base.as_ref()?.origin()
                {
                    return None;
                }
                Some(url.to_string())
            } else {
                if name == "input"
                    && !matches!(
                        node.value().attr("type").unwrap_or("text"),
                        "text" | "search"
                    )
                {
                    return None;
                }
                None
            };
            let label = node
                .value()
                .attr("aria-label")
                .or_else(|| node.value().attr("placeholder"))
                .map(str::to_owned)
                .unwrap_or_else(|| node.text().collect::<Vec<_>>().join(" "));
            Some(Control {
                selector: format!("#{id}"),
                kind: if name == "a" { "click" } else { "type" },
                label: label.chars().take(100).collect(),
                target,
            })
        })
        .take(64)
        .collect()
}

pub fn load_actions(path: &Path) -> Result<Vec<Action>> {
    if std::fs::metadata(path)?.len() > 32 * 1024 {
        bail!("action plan exceeds 32KiB");
    }
    let actions: Vec<Action> = serde_json::from_slice(&std::fs::read(path)?)?;
    validate_actions(&actions)?;
    Ok(actions)
}

pub fn validate_actions(actions: &[Action]) -> Result<()> {
    if actions.is_empty() {
        bail!("action plan is empty");
    }
    if actions.len() > 20 {
        bail!("action plan exceeds 20 steps");
    }
    for action in actions {
        match action {
            Action::Click { selector } | Action::Type { selector, .. }
                if selector.is_empty() || selector.len() > 256 =>
            {
                bail!("selector must be 1..=256 bytes")
            }
            Action::Type { value, .. } if value.len() > 1024 => bail!("typed value exceeds 1KiB"),
            Action::Wait { milliseconds } if *milliseconds > 2000 => {
                bail!("wait exceeds 2 seconds")
            }
            _ => {}
        }
    }
    Ok(())
}

fn webdriver_base(input: &str) -> Result<Url> {
    let url = Url::parse(input).context("invalid WebDriver URL")?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("WebDriver must be http://127.0.0.1:PORT/ (loopback only)");
    }
    Ok(url)
}

async fn command(
    client: &Client,
    base: &Url,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let url = base.join(path)?;
    let mut request = client.request(method, url);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    if response
        .content_length()
        .is_some_and(|size| size > 3 * 1024 * 1024)
    {
        bail!("WebDriver reply exceeds budget");
    }
    let status = response.status();
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > 3 * 1024 * 1024 {
            bail!("WebDriver reply exceeds budget");
        }
        bytes.extend_from_slice(&chunk);
    }
    let result: Value = serde_json::from_slice(&bytes).context("non-JSON WebDriver reply")?;
    if !status.is_success() {
        bail!(
            "WebDriver HTTP {}: {}",
            status,
            result["value"]["message"]
                .as_str()
                .unwrap_or("command failed")
        );
    }
    Ok(result["value"].clone())
}

async fn text_value(client: &Client, base: &Url, path: &str) -> Result<String> {
    command(client, base, Method::GET, path, None)
        .await?
        .as_str()
        .map(str::to_owned)
        .context("missing WebDriver text value")
}

async fn verify_current(
    client: &Client,
    base: &Url,
    session: &str,
    scope: &str,
    allow_private: bool,
) -> Result<String> {
    let current = text_value(client, base, &format!("session/{session}/url")).await?;
    let parsed = Url::parse(&current)?;
    fetch::validate_url(&parsed, allow_private)?;
    if parsed.origin().ascii_serialization() != scope {
        bail!("browser navigated outside original origin: {current}");
    }
    Ok(current)
}

/// Explicit browser opt-in: page subresources are NOT protected by HttpClient's SSRF guard.
pub async fn browse(
    http: &HttpClient,
    input: &str,
    webdriver_url: &str,
    actions: &[Action],
) -> Result<BrowserReport> {
    let base = webdriver_base(webdriver_url)?;
    let url = Url::parse(input)?;
    fetch::validate_url(&url, http.allow_private())?;
    let scope = url.origin().ascii_serialization();
    // Validate the top-level URL, resolved address and redirects with the pinned HTTP client.
    http.get(input, PAGE_LIMIT, Some(&scope)).await?;
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(20))
        .build()?;
    let session = command(
        &client,
        &base,
        Method::POST,
        "session",
        Some(json!({
            "capabilities": {"alwaysMatch": {"browserName": "servo"}}
        })),
    )
    .await?;
    if session["capabilities"]["browserName"] != "servo" {
        bail!("driver did not launch Servo");
    }
    let id = session["sessionId"]
        .as_str()
        .context("WebDriver did not return a session ID")?
        .to_owned();
    let result = async {
        command(&client, &base, Method::POST, &format!("session/{id}/timeouts"),
            Some(json!({"pageLoad": 12000, "script": 5000, "implicit": 0}))).await?;
        command(&client, &base, Method::POST, &format!("session/{id}/url"), Some(json!({"url": input}))).await?;
        verify_current(&client, &base, &id, &scope, http.allow_private()).await?;
        for action in actions {
            match action {
                Action::Wait { milliseconds } => tokio::time::sleep(Duration::from_millis(*milliseconds)).await,
                Action::Click { selector } | Action::Type { selector, .. } => {
                    let found = command(&client, &base, Method::POST, &format!("session/{id}/element"),
                        Some(json!({"using": "css selector", "value": selector}))).await?;
                    let element = found[ELEMENT].as_str().context("element not found")?;
                    let path = format!("session/{id}/element/{element}");
                    match action {
                        Action::Click { .. } => {
                            let name = text_value(&client, &base, &format!("{path}/name")).await?;
                            if name != "a" { bail!("click is restricted to links (anchors), not buttons/forms"); }
                            let href = text_value(&client, &base, &format!("{path}/attribute/href")).await?;
                            let current = verify_current(&client, &base, &id, &scope, http.allow_private()).await?;
                            let target = Url::parse(&href).or_else(|_| Url::parse(&current)?.join(&href))?;
                            fetch::validate_url(&target, http.allow_private())?;
                            if target.origin().ascii_serialization() != scope { bail!("cross-origin click refused"); }
                            http.get(target.as_str(), PAGE_LIMIT, Some(&scope)).await?;
                            command(&client, &base, Method::POST, &format!("{path}/click"), Some(json!({}))).await?;
                        }
                        Action::Type { value, .. } => {
                            let name = text_value(&client, &base, &format!("{path}/name")).await?;
                            if name != "textarea" && name != "input" { bail!("type is restricted to text/search inputs or textareas"); }
                            if name == "input" {
                                let kind = text_value(&client, &base, &format!("{path}/attribute/type")).await?;
                                if !matches!(kind.as_str(), "text" | "search") { bail!("input type {kind} is not permitted"); }
                            }
                            command(&client, &base, Method::POST, &format!("{path}/value"), Some(json!({"text": value}))).await?;
                        }
                        Action::Wait { .. } => unreachable!(),
                    }
                    verify_current(&client, &base, &id, &scope, http.allow_private()).await?;
                }
            }
        }
        let final_url = verify_current(&client, &base, &id, &scope, http.allow_private()).await?;
        let source = text_value(&client, &base, &format!("session/{id}/source")).await?;
        if source.len() > PAGE_LIMIT { bail!("rendered page exceeds 2MiB"); }
        let available_controls = controls(&source, &final_url);
        let page = extract::page_from_html(&source, &final_url, 0, Utc::now(), source.as_bytes())?;
        Ok(BrowserReport { engine: "servo", actions_executed: actions.len(), page,
            controls: available_controls, ai_plan: None, planner: None,
            note: "Rendered DOM SHA-256; HTTP status is unknown (0). Browser subresources require isolated public-only egress." })
    }.await;
    let _ = command(
        &client,
        &base,
        Method::DELETE,
        &format!("session/{id}"),
        None,
    )
    .await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn driver_must_be_loopback() {
        assert!(webdriver_base("http://127.0.0.1:7058/").is_ok());
        for url in [
            "http://0.0.0.0:7058/",
            "http://example.org:7058/",
            "http://localhost:7058/",
            "https://127.0.0.1:7058/",
            "http://127.0.0.1:7058/evil",
        ] {
            assert!(webdriver_base(url).is_err(), "{url}");
        }
    }
}
