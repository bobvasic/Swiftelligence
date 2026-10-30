# PDF and Office conversion

```bash
swiftel document ./report.pdf --json
swiftel document ./evidence.docx
swiftel document ./workbook.xlsx --json
swiftel document ./slides.pptx --json
swiftel document https://example.org/public-report.pdf --json
```

The Rust `document` module recognizes file bytes rather than trusting the extension. It handles PDF text layers, DOCX, XLSX and PPTX via `pdf-extract`, `undoc` and `zip`; there is no external Python/LibreOffice runtime in the CLI. Input is capped at 8 MiB; Office ZIP packages at 2,000 entries and 32 MiB declared decompressed bytes with per-entry and ratio caps; rendered Markdown and Office JSON at 2 MiB each; PDFs at 200 pages. URL downloads also use the normal bounded, redirect-revalidating HTTP client. JSON output contains source, format, Markdown, a structured representation, input-byte SHA-256, retrieval timestamp, and warnings.

Extraction is imperfect: PDF reading order and tables may be wrong; scanned/image-only PDFs are refused because no OCR is implemented; mixed text/image PDFs may have blank pages flagged. Office rendering may omit shapes, attachments, charts, formulas, macros, or layout information. Legacy `.doc`, `.xls`, `.ppt`, and other document types are not supported. Never treat output as the entire source, especially for legal or evidentiary use. Inspect the original bytes and cite the original URL or file.

Compressed-byte checks are not a complete resource sandbox. Parse untrusted documents under OS memory/CPU/process limits, without sensitive local files mounted into that environment. Never claim an integrity chain beyond the hash of bytes Swiftel actually received. `--allow-private` intentionally disables private-address protection and is for controlled local fixtures only.
