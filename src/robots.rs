use crate::fetch::HttpClient;
use anyhow::{Result, bail};
use regex::Regex;
use std::time::Duration;
use url::Url;

#[derive(Debug, Default)]
pub struct Robots {
    groups: Vec<Group>,
    sitemaps: Vec<String>,
}

#[derive(Debug, Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
    delay: Option<f64>,
}

#[derive(Debug)]
struct Rule {
    allow: bool,
    pattern: String,
}

impl Robots {
    pub fn parse(input: &str) -> Self {
        let mut robots = Self::default();
        let mut current = Group::default();
        for raw in input.lines().take(3_000) {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            match key.as_str() {
                "sitemap" if robots.sitemaps.len() < 20 => {
                    robots.sitemaps.push(value.to_string());
                }
                "user-agent" => {
                    if !current.rules.is_empty() || current.delay.is_some() {
                        robots.groups.push(current);
                        current = Group::default();
                    }
                    current.agents.push(value.to_ascii_lowercase());
                }
                "allow" | "disallow" if !current.agents.is_empty() && !value.is_empty() => {
                    current.rules.push(Rule {
                        allow: key == "allow",
                        pattern: value.to_string(),
                    });
                }
                "crawl-delay" if !current.agents.is_empty() => {
                    current.delay = value
                        .parse::<f64>()
                        .ok()
                        .filter(|d| d.is_finite() && *d >= 0.0);
                }
                _ => {}
            }
        }
        if !current.agents.is_empty() {
            robots.groups.push(current);
        }
        robots
    }

    pub fn sitemaps(&self) -> &[String] {
        &self.sitemaps
    }

    fn selected(&self) -> Vec<&Group> {
        let specific: Vec<_> = self
            .groups
            .iter()
            .filter(|g| {
                g.agents
                    .iter()
                    .any(|a| a == "swiftel" || a == "swiftel/0.1")
            })
            .collect();
        if !specific.is_empty() {
            return specific;
        }
        self.groups
            .iter()
            .filter(|g| g.agents.iter().any(|a| a == "*"))
            .collect()
    }

    pub fn allowed(&self, url: &Url) -> bool {
        let path = match url.query() {
            Some(query) => format!("{}?{}", url.path(), query),
            None => url.path().to_string(),
        };
        let mut winning: Option<(usize, bool)> = None;
        for rule in self.selected().iter().flat_map(|g| g.rules.iter()) {
            if matches_path(&rule.pattern, &path) {
                let specificity = rule
                    .pattern
                    .trim_end_matches('$')
                    .chars()
                    .filter(|c| *c != '*')
                    .count();
                if winning.is_none_or(|(prev, allowed)| {
                    specificity > prev || (specificity == prev && rule.allow && !allowed)
                }) {
                    winning = Some((specificity, rule.allow));
                }
            }
        }
        winning.is_none_or(|(_, allowed)| allowed)
    }

    pub fn delay(&self) -> Result<Duration> {
        let seconds = self
            .selected()
            .iter()
            .filter_map(|g| g.delay)
            .fold(1.0f64, f64::max);
        if seconds > 10.0 {
            bail!("robots crawl-delay exceeds 10 seconds; refusing a long crawl");
        }
        Ok(Duration::from_secs_f64(seconds))
    }
}

fn matches_path(rule: &str, path: &str) -> bool {
    let anchored = rule.ends_with('$');
    let pattern = rule
        .trim_end_matches('$')
        .split('*')
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(".*");
    let expression = format!("^{pattern}{}", if anchored { "$" } else { "" });
    Regex::new(&expression).is_ok_and(|regex| regex.is_match(path))
}

pub async fn load(client: &HttpClient, start: &Url) -> Result<Robots> {
    let url = start.join("/robots.txt")?;
    let scope = start.origin().ascii_serialization();
    let response = client.get(url.as_str(), 256 * 1024, Some(&scope)).await?;
    match response.status {
        200 => Ok(Robots::parse(&String::from_utf8_lossy(&response.body))),
        404 | 410 => Ok(Robots::default()),
        status => bail!("robots.txt returned HTTP {status}; crawl refused"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specific_rules_override_wildcard_and_longest_allow_wins() {
        let robots = Robots::parse(
            "User-agent: *\nDisallow: /\nUser-agent: Swiftel\nDisallow: /private\nAllow: /private/public\nCrawl-delay: 2\n",
        );
        assert!(robots.allowed(&Url::parse("https://example.org/public").unwrap()));
        assert!(!robots.allowed(&Url::parse("https://example.org/private/user").unwrap()));
        assert!(robots.allowed(&Url::parse("https://example.org/private/public/a").unwrap()));
        assert_eq!(robots.delay().unwrap().as_secs(), 2);
    }

    #[test]
    fn wildcard_and_end_anchor() {
        let robots = Robots::parse("User-agent: *\nDisallow: /*.pdf$\n");
        assert!(!robots.allowed(&Url::parse("https://example.org/doc.pdf").unwrap()));
        assert!(robots.allowed(&Url::parse("https://example.org/doc.pdf?download=1").unwrap()));
    }
}
