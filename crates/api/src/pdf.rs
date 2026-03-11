//! PDF-to-image rendering for vision LLM integration.
//!
//! Renders PDF pages to PNG images using `pdftoppm` (from poppler-utils),
//! returning base64-encoded data URLs suitable for multimodal LLM messages.

use base64::Engine as _;
use std::path::Path;

/// Maximum number of pages to render from a single PDF.
/// Each page image costs ~1000-2000 LLM tokens, so we cap to avoid token explosion.
const MAX_PAGES: usize = 10;

/// DPI for rendered page images. 150 gives good readability without huge images.
const RENDER_DPI: u32 = 150;

/// Render PDF pages to base64-encoded PNG data URLs.
///
/// Uses `pdftoppm` (poppler-utils) to rasterize each page. Returns up to
/// `MAX_PAGES` images. If the PDF has more pages, a text note is appended
/// as the last element.
///
/// Falls back to text extraction via `pdf-extract` if rendering fails.
pub async fn pdf_to_page_images(pdf_bytes: &[u8]) -> Result<PdfRenderResult, PdfRenderError> {
    // Write PDF to a temp file for pdftoppm
    let tmp_dir = tempfile::tempdir().map_err(|e| PdfRenderError::Io(e.to_string()))?;
    let pdf_path = tmp_dir.path().join("input.pdf");
    tokio::fs::write(&pdf_path, pdf_bytes)
        .await
        .map_err(|e| PdfRenderError::Io(e.to_string()))?;

    // Try pdftoppm first
    match render_with_pdftoppm(&pdf_path, tmp_dir.path()).await {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::warn!("pdftoppm rendering failed ({e}), falling back to text extraction");
            // Fallback: extract text using pdf-extract crate
            let text = extract_text_fallback(pdf_bytes);
            Ok(PdfRenderResult {
                page_images: Vec::new(),
                fallback_text: Some(text),
                total_pages: 0,
            })
        }
    }
}

/// Result of rendering a PDF.
pub struct PdfRenderResult {
    /// Base64-encoded PNG data URLs for each rendered page.
    pub page_images: Vec<String>,
    /// If image rendering failed, extracted text as fallback.
    pub fallback_text: Option<String>,
    /// Total number of pages in the PDF (may exceed `page_images.len()`).
    pub total_pages: usize,
}

#[derive(Debug)]
pub enum PdfRenderError {
    Io(String),
}

impl std::fmt::Display for PdfRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfRenderError::Io(msg) => write!(f, "PDF render I/O error: {msg}"),
        }
    }
}

/// Render pages using pdftoppm (poppler-utils).
async fn render_with_pdftoppm(
    pdf_path: &Path,
    output_dir: &Path,
) -> Result<PdfRenderResult, PdfRenderError> {
    let output_prefix = output_dir.join("page");

    // First, get page count
    let total_pages = get_page_count(pdf_path).await.unwrap_or(1);
    let pages_to_render = total_pages.min(MAX_PAGES);

    let output = tokio::process::Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            &RENDER_DPI.to_string(),
            "-l",
            &pages_to_render.to_string(), // last page to render
        ])
        .arg(pdf_path)
        .arg(&output_prefix)
        .output()
        .await
        .map_err(|e| PdfRenderError::Io(format!("failed to run pdftoppm: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PdfRenderError::Io(format!("pdftoppm failed: {stderr}")));
    }

    // Collect rendered PNG files (pdftoppm names them page-01.png, page-02.png, etc.)
    let mut page_images = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(output_dir)
        .map_err(|e| PdfRenderError::Io(e.to_string()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "png")
                .unwrap_or(false)
        })
        .collect();

    // Sort by filename to preserve page order
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let png_bytes = std::fs::read(entry.path())
            .map_err(|e| PdfRenderError::Io(e.to_string()))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        page_images.push(format!("data:image/png;base64,{b64}"));
    }

    Ok(PdfRenderResult {
        page_images,
        fallback_text: None,
        total_pages,
    })
}

/// Get the number of pages in a PDF using pdfinfo (poppler-utils).
async fn get_page_count(pdf_path: &Path) -> Option<usize> {
    let output = tokio::process::Command::new("pdfinfo")
        .arg(pdf_path)
        .output()
        .await
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            return rest.trim().parse().ok();
        }
    }
    None
}

/// Fallback: extract text from PDF using the pdf-extract crate.
fn extract_text_fallback(pdf_bytes: &[u8]) -> String {
    match pdf_extract::extract_text_from_mem(pdf_bytes) {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                "[PDF text extraction returned no readable text — this PDF may contain only images or use non-standard font encoding]".to_string()
            } else {
                // Truncate very long extractions
                if trimmed.len() > 30_000 {
                    format!(
                        "{}...\n\n[Text truncated at 30KB, {} total characters]",
                        &trimmed[..30_000],
                        trimmed.len()
                    )
                } else {
                    trimmed.to_string()
                }
            }
        }
        Err(e) => {
            format!("[Failed to extract text from PDF: {e}]")
        }
    }
}

/// Check whether pdftoppm is available on the system.
/// Call this at server startup to log a warning if missing.
pub async fn check_poppler_available() -> bool {
    tokio::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}
