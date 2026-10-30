mod crawl;
mod domain;
mod extract;
mod fetch;
mod promo;
mod robots;
mod search;

use anyhow::Result;
use clap::{Parser, Subcommand};
use fetch::HttpClient;
use serde::Serialize;

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
