//! Bounded PDF/Office extraction; all parsing occurs in Rust.
use crate::fetch::HttpClient;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};

const MAX_INPUT: usize = 8 * 1024 * 1024;
const MAX_DECOMPRESSED: u64 = 32 * 1024 * 1024;
const MAX_OUTPUT: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct DocumentReport {
    pub source: String,
    pub http_status: Option<u16>,
    pub format: String,
    pub markdown: String,
    pub structured: Value,
    pub sha256: String,
    pub retrieved_at: DateTime<Utc>,
    pub warnings: Vec<String>,
}

fn check_office_zip(bytes: &[u8]) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > 2_000 {
        bail!("office archive has too many entries");
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)?;
        total = total.saturating_add(entry.size());
        if total > MAX_DECOMPRESSED || entry.size() > MAX_INPUT as u64 {
            bail!("office archive exceeds decompressed size budget");
        }
        if entry.size() > 4096 && entry.size() / entry.compressed_size().max(1) > 200 {
            bail!("office archive compression ratio exceeds budget");
        }
    }
    Ok(())
}

pub fn parse_bytes(
    source: &str,
    bytes: &[u8],
    retrieved_at: DateTime<Utc>,
) -> Result<DocumentReport> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT {
        bail!("document must be 1..=8MiB");
    }
    let hash = format!("{:x}", Sha256::digest(bytes));
    let (format, markdown, structured, warnings) = if bytes.starts_with(b"%PDF-") {
        let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
            .context("PDF text extraction failed")?;
        if pages.len() > 200 {
            bail!("PDF has more than 200 pages");
        }
        if pages.iter().all(|p| p.trim().is_empty()) {
            bail!("PDF contains no extractable text; scanned pages require OCR (not available)");
        }
        let mut md = String::new();
        for (i, text) in pages.iter().enumerate() {
            if md.len() + text.len() > MAX_OUTPUT {
                bail!("PDF text exceeds 2MiB output limit");
            }
            md.push_str(&format!("## Page {}\n\n{}\n\n", i + 1, text.trim()));
        }
        let warnings = if pages.iter().any(|p| p.trim().is_empty()) {
            vec!["Some PDF pages have no text layer; those pages were not OCRed".to_string()]
        } else {
            Vec::new()
        };
        ("pdf".to_string(), md, json!({"pages": pages}), warnings)
    } else if bytes.starts_with(b"PK\x03\x04") {
        check_office_zip(bytes)?;
        let kind =
            undoc::detect_format_from_bytes(bytes).context("not a supported Office package")?;
        if !kind.is_enabled() {
            bail!(
                "{} is not enabled; supported: DOCX, XLSX, PPTX",
                kind.extension()
            );
        }
        let doc = undoc::parse_bytes(bytes).context("office parse failed")?;
        let md = undoc::render::to_markdown(&doc, &undoc::render::RenderOptions::default())?;
        if md.trim().is_empty() {
            bail!("office document has no extractable text");
        }
        if md.len() > MAX_OUTPUT {
            bail!("office Markdown exceeds 2MiB output limit");
        }
        let json = undoc::render::to_json_default(&doc)?;
        if json.len() > MAX_OUTPUT {
            bail!("office JSON exceeds 2MiB output limit");
        }
        (
            kind.extension().to_string(),
            md,
            serde_json::from_str(&json)?,
            Vec::new(),
        )
    } else {
        bail!("unsupported document bytes (expected PDF, DOCX, XLSX or PPTX)");
    };
    Ok(DocumentReport {
        source: source.into(),
        http_status: None,
        format,
        markdown,
        structured,
        sha256: hash,
        retrieved_at,
        warnings,
    })
}

pub fn parse_file(path: &Path) -> Result<DocumentReport> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_INPUT as u64 {
        bail!("document file must be 1..=8MiB");
    }
    let bytes = std::fs::read(path)?;
    parse_bytes(&path.display().to_string(), &bytes, Utc::now())
}

pub async fn parse_url(client: &HttpClient, url: &str) -> Result<DocumentReport> {
    let response = client.get(url, MAX_INPUT, None).await?;
    if !(200..300).contains(&response.status) {
        bail!("HTTP {} at {}", response.status, response.url);
    }
    let source = response.url;
    let status = response.status;
    let retrieved_at = response.retrieved_at;
    tokio::task::spawn_blocking(move || {
        let mut report = parse_bytes(&source, &response.body, retrieved_at)?;
        report.http_status = Some(status);
        Ok(report)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_html_and_oversized_input() {
        assert!(parse_bytes("test", b"<html/>", Utc::now()).is_err());
        assert!(parse_bytes("test", &vec![0; MAX_INPUT + 1], Utc::now()).is_err());
    }
    #[test]
    fn real_format_fixtures_render_to_markdown_and_json() {
        for (source, bytes, needle) in [
            (
                "pdf",
                include_bytes!("../tests/fixtures/evidence.pdf").as_slice(),
                "Rust PDF evidence",
            ),
            (
                "docx",
                include_bytes!("../tests/fixtures/evidence.docx").as_slice(),
                "Rust analysis",
            ),
            (
                "xlsx",
                include_bytes!("../tests/fixtures/evidence.xlsx").as_slice(),
                "Rust workbook evidence",
            ),
            (
                "pptx",
                include_bytes!("../tests/fixtures/evidence.pptx").as_slice(),
                "Rust presentation evidence",
            ),
        ] {
            let report = parse_bytes(source, bytes, Utc::now()).unwrap();
            assert_eq!(report.format, source);
            assert!(report.markdown.contains(needle), "{source}");
            assert!(report.structured.to_string().contains("Rust"), "{source}");
            assert_eq!(report.sha256.len(), 64);
        }
        assert!(
            parse_bytes(
                "empty",
                include_bytes!("../tests/fixtures/empty.pdf"),
                Utc::now()
            )
            .is_err()
        );
    }

    #[test]
    fn office_bomb_budget_rejects_high_expansion() {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        use std::io::Write;
        writer.write_all(&vec![b'X'; 2 * 1024 * 1024]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(check_office_zip(&bytes).is_err());
    }
}
