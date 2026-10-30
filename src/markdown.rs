//! Deterministic, source-preserving HTML → Markdown. No generated claims.
use scraper::{ElementRef, Html, Selector};
use url::Url;

const LIMIT: usize = 60_000;

fn clean(node: ElementRef<'_>) -> String {
    node.descendants()
        .filter_map(|n| {
            let text = n.value().as_text()?;
            (!n.ancestors().filter_map(ElementRef::wrap).any(|p| {
                matches!(
                    p.value().name(),
                    "script" | "style" | "noscript" | "nav" | "footer" | "header" | "form"
                )
            }))
            .then(|| text.to_string())
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

pub fn convert(html: &str, source: &str) -> String {
    let doc = Html::parse_document(html);
    let Ok(roots) = Selector::parse("main, article, body") else {
        return String::new();
    };
    let Some(root) = doc.select(&roots).next() else {
        return String::new();
    };
    let Ok(blocks) = Selector::parse("h1,h2,h3,h4,h5,h6,p,li,pre,blockquote,tr") else {
        return String::new();
    };
    let Ok(cells) = Selector::parse("th,td") else {
        return String::new();
    };
    let base = Url::parse(source).ok();
    let mut out = String::new();
    for node in root.select(&blocks) {
        if node
            .ancestors()
            .filter_map(ElementRef::wrap)
            .skip(1)
            .any(|p| {
                matches!(
                    p.value().name(),
                    "script" | "style" | "noscript" | "nav" | "footer" | "header" | "form"
                )
            })
        {
            continue;
        }
        let tag = node.value().name();
        let text = clean(node);
        if text.is_empty() {
            continue;
        }
        let line = match tag {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = tag.as_bytes()[1] - b'0';
                format!("{} {}", "#".repeat(usize::from(level)), escape(&text))
            }
            "li" => format!("- {}", escape(&text)),
            "blockquote" => format!("> {}", escape(&text)),
            "pre" => format!("```\n{}\n```", text.replace("```", "` ` `")),
            "tr" => {
                let values: Vec<_> = node
                    .select(&cells)
                    .map(clean)
                    .filter(|s| !s.is_empty())
                    .collect();
                if values.is_empty() {
                    continue;
                }
                format!(
                    "| {} |",
                    values
                        .iter()
                        .map(|s| escape(s))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            }
            _ => {
                // Keep link targets explicit for provenance. Only visible HTTP(S) links.
                let mut line = escape(&text);
                if let Ok(anchors) = Selector::parse("a[href]") {
                    for a in node.select(&anchors).take(10) {
                        let Some(href) = a.value().attr("href") else {
                            continue;
                        };
                        let Some(url) = base.as_ref().and_then(|base| base.join(href).ok()) else {
                            continue;
                        };
                        if !matches!(url.scheme(), "http" | "https")
                            || !url.username().is_empty()
                            || url.password().is_some()
                        {
                            continue;
                        }
                        let label = clean(a);
                        if !label.is_empty() {
                            line.push_str(&format!(" [{}]({})", escape(&label), url));
                        }
                    }
                }
                line
            }
        };
        if out.len() + line.len() + 2 > LIMIT {
            break;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_preserves_headings_tables_links_and_drops_scripts() {
        let md = convert(
            "<html><body><main><h1>Report</h1><p>Source <a href='/a'>A</a></p><table><tr><th>Year</th><th>Value</th></tr><tr><td>2026</td><td>42</td></tr></table><script>secret()</script></main></body></html>",
            "https://example.org/",
        );
        assert!(md.contains("# Report"));
        assert!(md.contains("[A](https://example.org/a)"));
        assert!(md.contains("| Year | Value |"));
        assert!(!md.contains("secret()"));
    }
}
