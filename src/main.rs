mod browser;
mod crawl;
mod document;
mod domain;
mod extract;
mod fetch;
mod index;
mod map;
mod markdown;
mod plan;
mod promo;
mod robots;
mod search;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use fetch::HttpClient;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "swiftel", version, about = "Public web intelligence, from your terminal", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Find public web pages (SearXNG or keyless engine fallback).
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// URL of a JSON-enabled SearXNG instance.
        #[arg(long)]
        searxng: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Retrieve a public HTML page and its provenance.
    Fetch {
        url: String,
        /// Controlled localhost fixtures ONLY; bypasses private-address protection.
        #[arg(long)]
        allow_private: bool,
        #[arg(long)]
        json: bool,
    },
    /// Crawl pages on one origin, respecting robots.txt and small budgets.
    Crawl {
        url: String,
        #[arg(long, default_value_t = 10)]
        max_pages: usize,
        #[arg(long, default_value_t = 1)]
        max_depth: usize,
        /// Controlled localhost fixtures ONLY; bypasses private-address protection.
        #[arg(long)]
        allow_private: bool,
        #[arg(long)]
        json: bool,
    },
    /// Discover URLs from robots sitemaps and same-origin links, without extracting every page.
    Map {
        url: String,
        #[arg(long, default_value_t = 100)]
        max_urls: usize,
        #[arg(long, default_value_t = 5)]
        max_pages: usize,
        /// Controlled localhost fixtures ONLY.
        #[arg(long)]
        allow_private: bool,
        #[arg(long)]
        json: bool,
    },
    /// Convert a local or public PDF/DOCX/XLSX/PPTX to Markdown and structured JSON.
    Document {
        source: String,
        /// Controlled localhost fixtures ONLY.
        #[arg(long)]
        allow_private: bool,
        #[arg(long)]
        json: bool,
    },
    /// Rank fetched public sources by lexical evidence (no AI-generated assertions).
    Research {
        query: String,
        /// Restrict to a bounded same-origin crawl rather than search providers.
        #[arg(long)]
        site: Option<String>,
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long, default_value_t = 10)]
        max_pages: usize,
        #[arg(long, default_value_t = 2)]
        max_depth: usize,
        #[arg(long)]
        searxng: Option<String>,
        /// Controlled localhost fixtures ONLY.
        #[arg(long)]
        allow_private: bool,
        #[arg(long)]
        json: bool,
    },
    /// Render and interact with a page through a locally published Servo WebDriver.
    Browse {
        url: String,
        #[arg(long, default_value = "http://127.0.0.1:7000/")]
        webdriver: String,
        /// JSON plan: restricted click/type/wait actions; no arbitrary JS.
        #[arg(long, conflicts_with = "prompt")]
        actions: Option<PathBuf>,
        /// Ask a local Ollama model to propose a validated plan (preview by default).
        #[arg(long, requires = "model")]
        prompt: Option<String>,
        /// Installed Ollama model name; sent only to 127.0.0.1:11434.
        #[arg(long, requires = "prompt")]
        model: Option<String>,
        /// Execute the proposed AI plan; without this, only preview the plan.
        #[arg(long, requires = "prompt")]
        execute_ai_plan: bool,
        /// Browser subresources bypass Swiftel's HTTP SSRF guard; use isolated egress.
        #[arg(long)]
        acknowledge_browser_network_risk: bool,
        /// Controlled localhost fixtures ONLY.
        #[arg(long)]
        allow_private: bool,
        #[arg(long)]
        json: bool,
    },
    /// Search for codes printed on public software offer pages (not checkout-tested).
    Promo {
        brand: String,
        #[arg(long)]
        searxng: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Passive public certificate-transparency and archive lookups.
    Domain {
        domain: String,
        #[arg(long)]
        json: bool,
    },
}

fn configured_searxng(arg: Option<String>) -> Option<String> {
    arg.or_else(|| {
        std::env::var("SWIFTEL_SEARXNG_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
    })
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Search {
            query,
            limit,
            searxng,
            json,
        } => {
            let report =
                search::search(&query, limit, configured_searxng(searxng).as_deref()).await?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Search: {} ({} hits via {})",
                    report.query,
                    report.hits.len(),
                    report.provider
                );
                for hit in &report.hits {
                    println!("  {}\n  {}", hit.title, hit.url);
                }
                for warning in &report.warnings {
                    eprintln!("Note: {warning}");
                }
            }
        }
        Command::Fetch {
            url,
            allow_private,
            json,
        } => {
            if allow_private {
                eprintln!(
                    "Warning: private-address protection disabled for controlled local fixture"
                );
            }
            let page = extract::fetch_page(&HttpClient::new(allow_private), &url, None).await?;
            if json {
                print_json(&page)?;
            } else {
                println!(
                    "{}\n{} | HTTP {} | {} | sha256 {}\n\n{}",
                    page.title,
                    page.url,
                    page.status,
                    page.retrieved_at.to_rfc3339(),
                    page.sha256,
                    page.text
                );
            }
        }
        Command::Crawl {
            url,
            max_pages,
            max_depth,
            allow_private,
            json,
        } => {
            if allow_private {
                eprintln!(
                    "Warning: private-address protection disabled for controlled local fixture"
                );
            }
            let report =
                crawl::crawl(&HttpClient::new(allow_private), &url, max_pages, max_depth).await?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Crawl: {} ({} pages, {} attempted, {} skipped/failed)",
                    report.seed,
                    report.pages.len(),
                    report.attempted,
                    report.errors.len()
                );
                for page in &report.pages {
                    println!("  {} | {}", page.url, page.title);
                }
                for error in &report.errors {
                    eprintln!("  {}: {}", error.url, error.error);
                }
            }
        }
        Command::Map {
            url,
            max_urls,
            max_pages,
            allow_private,
            json,
        } => {
            let report =
                map::map(&HttpClient::new(allow_private), &url, max_urls, max_pages).await?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Map: {} ({} URLs, {} HTML pages, {} sitemaps)",
                    report.seed,
                    report.urls.len(),
                    report.pages_inspected,
                    report.sitemaps_inspected
                );
                for entry in &report.urls {
                    println!("  {} [{}]", entry.url, entry.discovered_via);
                }
                for error in &report.errors {
                    eprintln!("  {error}");
                }
            }
        }
        Command::Document {
            source,
            allow_private,
            json,
        } => {
            let report = if source.starts_with("http://") || source.starts_with("https://") {
                document::parse_url(&HttpClient::new(allow_private), &source).await?
            } else if source.contains("://") {
                bail!("only local files and HTTP(S) URLs are supported");
            } else {
                document::parse_file(std::path::Path::new(&source))?
            };
            if json {
                print_json(&report)?;
            } else {
                println!("{}", report.markdown);
            }
        }
        Command::Research {
            query,
            site,
            limit,
            max_pages,
            max_depth,
            searxng,
            allow_private,
            json,
        } => {
            if !(1..=20).contains(&limit) {
                bail!("limit must be 1..=20");
            }
            let (provider, pages, errors, warnings) = if let Some(url) = site {
                let report =
                    crawl::crawl(&HttpClient::new(allow_private), &url, max_pages, max_depth)
                        .await?;
                let errors = report
                    .errors
                    .into_iter()
                    .map(|e| format!("{}: {}", e.url, e.error))
                    .collect();
                ("site_crawl".to_string(), report.pages, errors, Vec::new())
            } else {
                let found =
                    search::search(&query, limit, configured_searxng(searxng).as_deref()).await?;
                let mut pages = Vec::new();
                let mut errors = Vec::new();
                let client = HttpClient::new(false);
                for hit in found.hits {
                    let path = match url::Url::parse(&hit.url) {
                        Ok(parsed) => parsed.path().to_ascii_lowercase(),
                        Err(error) => {
                            errors.push(format!("{}: invalid source URL: {error}", hit.url));
                            continue;
                        }
                    };
                    if [".pdf", ".docx", ".xlsx", ".pptx"]
                        .iter()
                        .any(|ext| path.ends_with(ext))
                    {
                        match document::parse_url(&client, &hit.url).await {
                            Ok(doc) => pages.push(extract::Page {
                                url: doc.source.clone(),
                                title: doc.source.clone(),
                                text: doc.markdown.clone(),
                                markdown: doc.markdown,
                                links: Vec::new(),
                                status: doc.http_status.unwrap_or(0),
                                retrieved_at: doc.retrieved_at,
                                sha256: doc.sha256,
                            }),
                            Err(error) => errors.push(format!("{}: {error:#}", hit.url)),
                        }
                    } else {
                        match extract::fetch_page(&client, &hit.url, None).await {
                            Ok(page) => pages.push(page),
                            Err(error) => errors.push(format!("{}: {error:#}", hit.url)),
                        }
                    }
                }
                (found.provider, pages, errors, found.warnings)
            };
            let hits = index::rank(&query, &pages, limit)?;
            let report = index::ResearchReport {
                query,
                provider,
                pages_checked: pages.len(),
                hits,
                errors,
                warnings,
                note: "Only fetched source bodies are ranked. Search and crawl coverage are incomplete; no AI synthesis or claim verification is implied.",
            };
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Research: {} ({} sources fetched, {} matches)",
                    report.query,
                    report.pages_checked,
                    report.hits.len()
                );
                for hit in &report.hits {
                    println!("  {:.2} {}\n    {}", hit.score, hit.url, hit.snippet);
                }
                for error in &report.errors {
                    eprintln!("  {error}");
                }
            }
        }
        Command::Browse {
            url,
            webdriver,
            actions,
            prompt,
            model,
            execute_ai_plan,
            acknowledge_browser_network_risk,
            allow_private,
            json,
        } => {
            if !acknowledge_browser_network_risk {
                bail!(
                    "browser subresources bypass HTTP SSRF checks; run Servo in an isolated network and pass --acknowledge-browser-network-risk"
                );
            }
            let plan = actions
                .as_ref()
                .map(|p| browser::load_actions(p))
                .transpose()?
                .unwrap_or_default();
            let client = HttpClient::new(allow_private);
            let report = if let Some(prompt) = prompt {
                let model = model.expect("clap requires model with prompt");
                let mut snapshot = browser::browse(&client, &url, &webdriver, &[]).await?;
                let proposed = plan::propose(&prompt, &model, &snapshot).await?;
                if execute_ai_plan {
                    let mut executed =
                        browser::browse(&client, &url, &webdriver, &proposed).await?;
                    executed.ai_plan = Some(proposed);
                    executed.planner = Some(model);
                    executed
                } else {
                    snapshot.ai_plan = Some(proposed);
                    snapshot.planner = Some(model);
                    snapshot
                }
            } else {
                browser::browse(&client, &url, &webdriver, &plan).await?
            };
            if json {
                print_json(&report)?;
            } else {
                if let Some(plan) = &report.ai_plan {
                    eprintln!(
                        "AI plan ({}): {}",
                        if execute_ai_plan {
                            "executed"
                        } else {
                            "preview only"
                        },
                        serde_json::to_string(plan)?
                    );
                }
                println!("{}", report.page.markdown);
            }
        }
        Command::Promo {
            brand,
            searxng,
            json,
        } => {
            let report = promo::promo(&brand, configured_searxng(searxng).as_deref()).await?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Promo research: {} ({} pages inspected, {} published code leads)",
                    report.brand,
                    report.pages_checked,
                    report.codes.len()
                );
                for code in &report.codes {
                    println!(
                        "  {} — {}\n    {}",
                        code.code, code.source_url, code.excerpt
                    );
                }
                for warning in &report.warnings {
                    eprintln!("Note: {warning}");
                }
                for error in &report.page_errors {
                    eprintln!("Page not verified: {error}");
                }
                println!("{}", report.note);
            }
        }
        Command::Domain { domain, json } => {
            let report = domain::domain(&domain).await?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Domain: {} ({} CT hostnames)",
                    report.domain,
                    report.hostnames.len()
                );
                for hostname in &report.hostnames {
                    println!("  {hostname}");
                }
                if let Some(snapshot) = &report.archive {
                    println!("Closest archive: {} ({})", snapshot.url, snapshot.timestamp);
                }
                for error in &report.errors {
                    eprintln!("Source error: {error}");
                }
                println!("{}", report.note);
            }
        }
    }
    Ok(())
}
