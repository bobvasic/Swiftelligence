//! Local, deterministic BM25 ranking over *fetched* source bodies.
use crate::extract::Page;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Serialize)]
pub struct EvidenceHit {
    pub url: String,
    pub title: String,
    pub sha256: String,
    pub retrieved_at: DateTime<Utc>,
    pub score: f64,
    pub snippet: String,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResearchReport {
    pub query: String,
    pub provider: String,
    pub pages_checked: usize,
    pub hits: Vec<EvidenceHit>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub note: &'static str,
}

fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.chars().count() > 1)
        .map(str::to_lowercase)
        .take(15_000)
        .collect()
}

pub fn rank(query: &str, pages: &[Page], limit: usize) -> Result<Vec<EvidenceHit>> {
    if query.trim().is_empty() || query.len() > 500 || !(1..=100).contains(&limit) {
        bail!("query must be 1..=500 characters, limit 1..=100");
    }
    let query_terms: BTreeSet<_> = terms(query).into_iter().collect();
    if query_terms.is_empty() {
        bail!("query has no indexable words");
    }
    let docs: Vec<_> = pages.iter().map(|p| terms(&p.text)).collect();
    let avg_len =
        (docs.iter().map(Vec::len).sum::<usize>() as f64 / docs.len().max(1) as f64).max(1.0);
    let mut df = HashMap::<&str, usize>::new();
    for words in &docs {
        let distinct: HashSet<&str> = words.iter().map(String::as_str).collect();
        for term in &query_terms {
            if distinct.contains(term.as_str()) {
                *df.entry(term).or_default() += 1;
            }
        }
    }
    let mut hits = Vec::new();
    for (page, words) in pages.iter().zip(docs) {
        let mut frequencies = HashMap::<&str, usize>::new();
        for word in &words {
            *frequencies.entry(word).or_default() += 1;
        }
        let title = terms(&page.title);
        let mut matched = Vec::new();
        let mut score = 0.0;
        for term in &query_terms {
            let freq = *frequencies.get(term.as_str()).unwrap_or(&0) as f64;
            let title_match = title.contains(term);
            if freq == 0.0 && !title_match {
                continue;
            }
            matched.push(term.clone());
            let n = pages.len() as f64;
            let d = *df.get(term.as_str()).unwrap_or(&0) as f64;
            let idf = (1.0 + (n - d + 0.5) / (d + 0.5)).ln();
            let tf = freq * 2.2 / (freq + 1.2 * (0.25 + 0.75 * words.len() as f64 / avg_len));
            score += idf * (tf + if title_match { 2.0 } else { 0.0 });
        }
        if matched.is_empty() {
            continue;
        }
        matched.sort();
        let lower = page.text.to_lowercase();
        let index = matched
            .iter()
            .filter_map(|term| lower.find(term))
            .min()
            .unwrap_or(0);
        let from = page.text.floor_char_boundary(index.saturating_sub(80));
        let to = page
            .text
            .ceil_char_boundary((index + 220).min(page.text.len()));
        let snippet = page.text.get(from..to).unwrap_or("").replace('\n', " ");
        hits.push(EvidenceHit {
            url: page.url.clone(),
            title: page.title.clone(),
            sha256: page.sha256.clone(),
            retrieved_at: page.retrieved_at,
            score,
            snippet,
            matched_terms: matched,
        });
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.url.cmp(&b.url)));
    hits.truncate(limit);
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranks_verified_bodies_and_keeps_provenance() {
        let pages = [
            crate::extract::page_from_html("<html><title>Rust guide</title><body><p>Rust browser engine implementation guide.</p></body></html>", "https://example.org/a", 200, Utc::now(), b"a").unwrap(),
            crate::extract::page_from_html("<html><title>Other</title><body><p>Unrelated article about mountains.</p></body></html>", "https://example.org/b", 200, Utc::now(), b"b").unwrap()
        ];
        let result = rank("rust engine", &pages, 5).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].url, "https://example.org/a");
        assert!(result[0].snippet.contains("browser engine"));
        assert_eq!(result[0].sha256.len(), 64);
    }
}
