//! Fitur berbasis PDFium: render halaman, ekstraksi teks terstruktur, PDF→gambar,
//! redaksi permanen, OCR (via Tesseract) dan perbandingan dokumen.
//!
//! `pdfium.dll` harus berada di folder yang sama dengan `pdf-studio.exe`
//! (jalankan `setup-pdfium.ps1` untuk mengunduhnya).

use crate::pdfops;
use anyhow::{anyhow, bail, Result};
use image::DynamicImage;
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

struct Shared(Pdfium);
// PDFium tidak thread-safe; semua akses diserialkan lewat LOCK di bawah.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

static PDFIUM: OnceLock<std::result::Result<Shared, String>> = OnceLock::new();
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn bind() -> std::result::Result<Shared, String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
        }
    }
    dirs.push(PathBuf::from("."));
    for d in &dirs {
        if let Ok(b) = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(d)) {
            return Ok(Shared(Pdfium::new(b)));
        }
    }
    Pdfium::bind_to_system_library()
        .map(|b| Shared(Pdfium::new(b)))
        .map_err(|_| "pdfium tidak ditemukan. Jalankan setup-pdfium.ps1 atau salin pdfium.dll ke folder aplikasi.".to_string())
}

/// Mengembalikan handle PDFium beserta kunci global (tahan selama dokumen dipakai).
pub fn pdfium() -> Result<(&'static Pdfium, std::sync::MutexGuard<'static, ()>)> {
    let p = PDFIUM.get_or_init(bind).as_ref().map_err(|e| anyhow!("{e}"))?;
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok((&p.0, g))
}

pub fn pdfium_status() -> std::result::Result<(), String> {
    PDFIUM.get_or_init(bind).as_ref().map(|_| ()).map_err(|e| e.clone())
}

fn pw(p: &str) -> Option<&str> {
    if p.is_empty() { None } else { Some(p) }
}

/// Render satu halaman (indeks 0). `scale` = piksel per point. Mengembalikan (gambar, lebar_pt, tinggi_pt).
pub fn render_page(path: &Path, password: &str, idx: u16, scale: f32) -> Result<(DynamicImage, f32, f32)> {
    let (pdfium, _guard) = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, pw(password)).map_err(|e| anyhow!("{e}"))?;
    let page = doc.pages().get(idx).map_err(|e| anyhow!("{e}"))?;
    let (w, h) = (page.width().value, page.height().value);
    let bmp = page
        .render_with_config(&PdfRenderConfig::new().scale_page_by_factor(scale).render_form_data(true))
        .map_err(|e| anyhow!("{e}"))?;
    Ok((bmp.as_image(), w, h))
}

pub fn page_total(path: &Path, password: &str) -> Result<u16> {
    let (pdfium, _guard) = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, pw(password)).map_err(|e| anyhow!("{e}"))?;
    Ok(doc.pages().len())
}

/// Teks polos per halaman.
pub fn page_texts(path: &Path, password: &str) -> Result<Vec<String>> {
    let (pdfium, _guard) = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, pw(password)).map_err(|e| anyhow!("{e}"))?;
    let mut out = Vec::new();
    for page in doc.pages().iter() {
        out.push(page.text().map(|t| t.all()).unwrap_or_default());
    }
    Ok(out)
}

// ───────────────────────── PDF → gambar ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ImgFormat {
    Jpg,
    Png,
}

pub fn pdf_to_images(
    path: &Path,
    password: &str,
    pages: &[u32],
    dpi: f32,
    fmt: ImgFormat,
    quality: u8,
    out_dir: &Path,
    progress: &dyn Fn(&str),
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    let mut outs = Vec::new();
    for &n in pages {
        progress(&format!("Merender halaman {n}…"));
        let (img, _, _) = render_page(path, password, (n - 1) as u16, dpi / 72.0)?;
        let ext = if fmt == ImgFormat::Jpg { "jpg" } else { "png" };
        let out = out_dir.join(format!("{stem}_page{n:03}.{ext}"));
        match fmt {
            ImgFormat::Jpg => {
                let mut f = std::fs::File::create(&out)?;
                let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality.clamp(1, 100));
                img.to_rgb8().write_with_encoder(enc)?;
            }
            ImgFormat::Png => img.save_with_format(&out, image::ImageFormat::Png)?,
        }
        outs.push(out);
    }
    Ok(outs)
}

// ───────────────────────── ekstraksi terstruktur ─────────────────────────

#[derive(Clone, Debug)]
pub struct Cell {
    pub x0: f32,
    pub x1: f32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub top: f32,
    pub size: f32,
    pub cells: Vec<Cell>,
}

impl Line {
    pub fn text(&self) -> String {
        let mut s = String::new();
        let mut prev_x1: Option<f32> = None;
        for c in &self.cells {
            if let Some(p) = prev_x1 {
                if c.x0 - p > self.size * 0.15 {
                    s.push(' ');
                }
            }
            s.push_str(c.text.trim());
            prev_x1 = Some(c.x1);
        }
        s.trim().to_string()
    }
}

/// Garis-garis teks (urut atas→bawah) untuk setiap halaman.
pub fn page_lines(path: &Path, password: &str) -> Result<Vec<Vec<Line>>> {
    let (pdfium, _guard) = pdfium()?;
    let doc = pdfium.load_pdf_from_file(path, pw(password)).map_err(|e| anyhow!("{e}"))?;
    let mut pages = Vec::new();
    for page in doc.pages().iter() {
        struct Item {
            x0: f32,
            x1: f32,
            top: f32,
            size: f32,
            text: String,
        }
        let mut items: Vec<Item> = Vec::new();
        for obj in page.objects().iter() {
            if let Some(t) = obj.as_text_object() {
                let text = t.text();
                if text.trim().is_empty() {
                    continue;
                }
                let Ok(b) = obj.bounds() else { continue };
                items.push(Item {
                    x0: b.left().value,
                    x1: b.right().value,
                    top: b.top().value,
                    size: t.scaled_font_size().value.max(1.0),
                    text: text.replace(['\r', '\n'], " "),
                });
            }
        }
        items.sort_by(|a, b| b.top.partial_cmp(&a.top).unwrap_or(std::cmp::Ordering::Equal));
        let mut lines: Vec<(Vec<Item>, f32)> = Vec::new(); // (items, top)
        for it in items {
            match lines.last_mut() {
                Some((v, top)) if (*top - it.top).abs() < it.size * 0.5 => v.push(it),
                _ => {
                    let t = it.top;
                    lines.push((vec![it], t));
                }
            }
        }
        let mut out = Vec::new();
        for (mut v, top) in lines {
            v.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal));
            let size = v.iter().map(|i| i.size).fold(0.0, f32::max);
            let mut cells: Vec<Cell> = Vec::new();
            for it in v {
                match cells.last_mut() {
                    Some(c) if it.x0 - c.x1 < it.size * 0.45 => {
                        if it.x0 - c.x1 > it.size * 0.12 {
                            c.text.push(' ');
                        }
                        c.text.push_str(&it.text);
                        c.x1 = it.x1.max(c.x1);
                    }
                    _ => cells.push(Cell { x0: it.x0, x1: it.x1, text: it.text }),
                }
            }
            out.push(Line { top, size, cells });
        }
        pages.push(out);
    }
    Ok(pages)
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockKind {
    Heading(u8),
    Paragraph,
    /// Baris dengan beberapa kolom (kemungkinan tabel)
    TableRow(Vec<String>),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub page: u32,
    pub kind: BlockKind,
    pub text: String,
}

/// Blok dokumen (judul / paragraf) dengan heuristik ukuran huruf.
pub fn blocks(path: &Path, password: &str) -> Result<Vec<Block>> {
    let pages = page_lines(path, password)?;
    // ukuran huruf badan = ukuran paling banyak muncul (berbobot panjang teks)
    let mut hist: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    for p in &pages {
        for l in p {
            *hist.entry((l.size * 2.0).round() as i32).or_default() += l.text().len();
        }
    }
    let body = hist.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s as f32 / 2.0).unwrap_or(11.0);
    // Peringkat ukuran huruf judul: terbesar = tingkat 1, berikutnya tingkat 2, dst (maks 4).
    let mut hsizes: Vec<f32> = Vec::new();
    for p in &pages {
        for l in p {
            if l.cells.len() < 2 && l.size / body >= 1.13 && !l.text().is_empty() {
                let r = (l.size * 2.0).round() / 2.0;
                if !hsizes.iter().any(|s| (s - r).abs() < 0.26) {
                    hsizes.push(r);
                }
            }
        }
    }
    hsizes.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<Block> = Vec::new();
    for (pi, lines) in pages.iter().enumerate() {
        let mut prev: Option<(f32, f32, bool)> = None; // (top, size, was_para)
        for l in lines {
            let text = l.text();
            if text.is_empty() {
                continue;
            }
            if l.cells.len() >= 2 && l.size / body < 1.13 {
                out.push(Block {
                    page: pi as u32 + 1,
                    kind: BlockKind::TableRow(l.cells.iter().map(|c| c.text.trim().to_string()).collect()),
                    text,
                });
                prev = None;
                continue;
            }
            let ratio = l.size / body;
            let level = if ratio >= 1.13 {
                let r = (l.size * 2.0).round() / 2.0;
                hsizes.iter().position(|s| (s - r).abs() < 0.26).map(|i| ((i + 1).min(4)) as u8)
            } else {
                None
            };
            match level {
                Some(lv) => {
                    out.push(Block { page: pi as u32 + 1, kind: BlockKind::Heading(lv), text });
                    prev = None;
                }
                None => {
                    let join = matches!(prev, Some((pt, ps, true)) if (pt - l.top) < ps * 1.9 && (pt - l.top) > 0.0);
                    if join {
                        if let Some(last) = out.last_mut() {
                            if last.text.ends_with('-') {
                                last.text.pop();
                            } else {
                                last.text.push(' ');
                            }
                            last.text.push_str(&text);
                        }
                    } else {
                        out.push(Block { page: pi as u32 + 1, kind: BlockKind::Paragraph, text });
                    }
                    prev = Some((l.top, l.size, true));
                }
            }
        }
    }
    Ok(out)
}

pub fn to_markdown(path: &Path, password: &str) -> Result<String> {
    let bl = blocks(path, password)?;
    let mut md = String::new();
    let mut i = 0;
    while i < bl.len() {
        match &bl[i].kind {
            BlockKind::Heading(l) => md.push_str(&format!("{} {}\n\n", "#".repeat(*l as usize), bl[i].text)),
            BlockKind::Paragraph => md.push_str(&format!("{}\n\n", bl[i].text)),
            BlockKind::TableRow(_) => {
                let mut rows: Vec<&Vec<String>> = Vec::new();
                while i < bl.len() {
                    if let BlockKind::TableRow(r) = &bl[i].kind {
                        rows.push(r);
                        i += 1;
                    } else {
                        break;
                    }
                }
                let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1);
                for (k, r) in rows.iter().enumerate() {
                    let mut cells: Vec<String> = r.iter().map(|c| c.replace('|', "\\|")).collect();
                    cells.resize(cols, String::new());
                    md.push_str(&format!("| {} |\n", cells.join(" | ")));
                    if k == 0 {
                        md.push_str(&format!("|{}\n", " --- |".repeat(cols)));
                    }
                }
                md.push('\n');
                continue;
            }
        }
        i += 1;
    }
    Ok(md.trim_end().to_string() + "\n")
}

/// Tabel per halaman: baris → kolom (dikelompokkan berdasarkan posisi x kiri).
pub fn tables(path: &Path, password: &str) -> Result<Vec<Vec<Vec<String>>>> {
    let pages = page_lines(path, password)?;
    let mut out = Vec::new();
    for lines in pages {
        let mut xs: Vec<f32> = lines.iter().flat_map(|l| l.cells.iter().map(|c| c.x0)).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut cols: Vec<f32> = Vec::new();
        for x in xs {
            if cols.last().map(|c| x - c > 8.0).unwrap_or(true) {
                cols.push(x);
            }
        }
        let mut rows = Vec::new();
        for l in &lines {
            let mut row = vec![String::new(); cols.len().max(1)];
            for c in &l.cells {
                let idx = cols
                    .iter()
                    .enumerate()
                    .min_by(|a, b| (a.1 - c.x0).abs().partial_cmp(&(b.1 - c.x0).abs()).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                if !row[idx].is_empty() {
                    row[idx].push(' ');
                }
                row[idx].push_str(c.text.trim());
            }
            rows.push(row);
        }
        out.push(rows);
    }
    Ok(out)
}

// ───────────────────────── Redact ─────────────────────────

pub struct RedactReport {
    pub pages: usize,
    pub boxes: usize,
}

/// Redaksi permanen: halaman yang terkena dirender ulang menjadi gambar dengan kotak hitam,
/// sehingga teks di bawahnya benar-benar hilang (tidak bisa disalin/dicari lagi).
pub fn redact(
    input: &Path,
    out: &Path,
    password: &str,
    rects: &[(u32, [f32; 4])],
    terms: &[String],
    dpi: f32,
    progress: &dyn Fn(&str),
) -> Result<RedactReport> {
    let (pdfium, _guard) = pdfium()?;
    let pdoc = pdfium.load_pdf_from_file(input, pw(password)).map_err(|e| anyhow!("{e}"))?;
    let mut per_page: std::collections::BTreeMap<u32, Vec<[f32; 4]>> = std::collections::BTreeMap::new();
    for (p, r) in rects {
        per_page.entry(*p).or_default().push(*r);
    }
    for term in terms.iter().filter(|t| !t.trim().is_empty()) {
        for (i, page) in pdoc.pages().iter().enumerate() {
            let Ok(text) = page.text() else { continue };
            let Ok(search) = text.search(term.trim(), &PdfSearchOptions::new()) else { continue };
            for segs in search.iter(PdfSearchDirection::SearchForward) {
                for seg in segs.iter() {
                    let b = seg.bounds();
                    per_page.entry(i as u32 + 1).or_default().push([
                        b.left().value - 1.0,
                        b.bottom().value - 1.0,
                        b.right().value + 1.0,
                        b.top().value + 1.0,
                    ]);
                }
            }
        }
    }
    if per_page.is_empty() {
        bail!("Tidak ada area atau teks yang akan disensor");
    }
    let mut doc = pdfops::load(input, password)?;
    let map = doc.get_pages();
    let scale = dpi / 72.0;
    let mut boxes = 0;
    for (n, rs) in &per_page {
        progress(&format!("Menyensor halaman {n}…"));
        let id = *map.get(n).ok_or_else(|| anyhow!("Halaman {n} tidak ada"))?;
        let page = pdoc.pages().get((*n - 1) as u16).map_err(|e| anyhow!("{e}"))?;
        let (w, h) = (page.width().value, page.height().value);
        let bmp = page
            .render_with_config(&PdfRenderConfig::new().scale_page_by_factor(scale))
            .map_err(|e| anyhow!("{e}"))?;
        let mut img = bmp.as_image().to_rgb8();
        let (iw, ih) = img.dimensions();
        for r in rs {
            let x0 = (r[0].min(r[2]) * scale).floor().max(0.0) as u32;
            let x1 = ((r[0].max(r[2]) * scale).ceil() as u32).min(iw);
            let y0 = (((h - r[1].max(r[3])) * scale).floor().max(0.0)) as u32;
            let y1 = (((h - r[1].min(r[3])) * scale).ceil() as u32).min(ih);
            for y in y0..y1 {
                for x in x0..x1 {
                    img.put_pixel(x, y, image::Rgb([0, 0, 0]));
                }
            }
            boxes += 1;
        }
        let mut jpg = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 90);
        img.write_with_encoder(enc)?;
        pdfops::replace_page_with_jpeg(&mut doc, id, jpg, iw, ih, w, h)?;
    }
    // hapus metadata & struktur yang mungkin membawa teks asli
    doc.trailer.remove(b"Info");
    if let Ok(cat) = doc.catalog().map(|c| c.clone()) {
        let root = doc.trailer.get(b"Root").ok().and_then(|o| o.as_reference().ok());
        if let (Some(root), true) = (root, cat.has(b"Metadata") || cat.has(b"StructTreeRoot")) {
            if let Ok(c) = doc.get_dictionary_mut(root) {
                c.remove(b"Metadata");
                c.remove(b"StructTreeRoot");
            }
        }
    }
    pdfops::finalize(&mut doc, out)?;
    Ok(RedactReport { pages: per_page.len(), boxes })
}

// ───────────────────────── OCR (Tesseract) ─────────────────────────

pub fn find_tesseract(custom: &str) -> Option<PathBuf> {
    if !custom.trim().is_empty() && Path::new(custom.trim()).exists() {
        return Some(PathBuf::from(custom.trim()));
    }
    let cands = [
        r"C:\Program Files\Tesseract-OCR\tesseract.exe",
        r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
    ];
    for c in cands {
        if Path::new(c).exists() {
            return Some(PathBuf::from(c));
        }
    }
    let probe = std::process::Command::new("tesseract").arg("--version").output();
    if probe.map(|o| o.status.success()).unwrap_or(false) {
        return Some(PathBuf::from("tesseract"));
    }
    None
}

/// Buat PDF yang dapat dicari (teks tak kasat mata di atas gambar halaman).
pub fn ocr_pdf(
    input: &Path,
    out: &Path,
    password: &str,
    lang: &str,
    tesseract: &str,
    dpi: f32,
    progress: &dyn Fn(&str),
) -> Result<u32> {
    let tess = find_tesseract(tesseract)
        .ok_or_else(|| anyhow!("Tesseract tidak ditemukan. Pasang Tesseract OCR (winget install UB-Mannheim.TesseractOCR) atau isi path-nya di Pengaturan."))?;
    let total = page_total(input, password)?;
    let tmp = std::env::temp_dir().join(format!("pdfstudio_ocr_{}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    let mut parts = Vec::new();
    for n in 1..=total as u32 {
        progress(&format!("OCR halaman {n}/{total}…"));
        let (img, _, _) = render_page(input, password, (n - 1) as u16, dpi / 72.0)?;
        let png = tmp.join(format!("p{n:04}.png"));
        img.save_with_format(&png, image::ImageFormat::Png)?;
        let base = tmp.join(format!("o{n:04}"));
        let o = crate::util::run_hidden(
            std::process::Command::new(&tess).arg(&png).arg(&base).args(["-l", lang, "pdf"]),
        )?;
        if !o.status.success() {
            let _ = std::fs::remove_dir_all(&tmp);
            bail!("Tesseract gagal: {}", String::from_utf8_lossy(&o.stderr));
        }
        parts.push(base.with_extension("pdf"));
    }
    let res = if parts.len() == 1 {
        std::fs::copy(&parts[0], out).map(|_| ()).map_err(|e| anyhow!("{e}"))
    } else {
        pdfops::merge(&parts, out)
    };
    let _ = std::fs::remove_dir_all(&tmp);
    res?;
    Ok(total as u32)
}

// ───────────────────────── Compare ─────────────────────────

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub page: u32,
    /// '=' sama, '+' hanya di dokumen B, '-' hanya di dokumen A
    pub tag: char,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct CompareResult {
    pub lines: Vec<DiffLine>,
    /// (halaman, persen piksel berbeda)
    pub visual: Vec<(u32, f32)>,
    pub pages_a: usize,
    pub pages_b: usize,
}

pub fn compare(a: &Path, b: &Path, pass_a: &str, pass_b: &str, progress: &dyn Fn(&str)) -> Result<CompareResult> {
    progress("Mengekstrak teks…");
    let ta = page_texts(a, pass_a)?;
    let tb = page_texts(b, pass_b)?;
    let mut res = CompareResult { pages_a: ta.len(), pages_b: tb.len(), ..Default::default() };
    let n = ta.len().max(tb.len());
    for i in 0..n {
        let (sa, sb) = (ta.get(i).map(String::as_str).unwrap_or(""), tb.get(i).map(String::as_str).unwrap_or(""));
        let diff = similar::TextDiff::from_lines(sa, sb);
        for ch in diff.iter_all_changes() {
            let tag = match ch.tag() {
                similar::ChangeTag::Equal => '=',
                similar::ChangeTag::Insert => '+',
                similar::ChangeTag::Delete => '-',
            };
            let t = ch.value().trim_end().to_string();
            if t.is_empty() && tag == '=' {
                continue;
            }
            res.lines.push(DiffLine { page: i as u32 + 1, tag, text: t });
        }
    }
    for i in 0..ta.len().min(tb.len()) {
        progress(&format!("Membandingkan visual halaman {}/{}…", i + 1, ta.len().min(tb.len())));
        let (ia, _, _) = render_page(a, pass_a, i as u16, 0.6)?;
        let (ib, _, _) = render_page(b, pass_b, i as u16, 0.6)?;
        let (ia, ib) = (ia.to_rgb8(), ib.to_rgb8());
        if ia.dimensions() != ib.dimensions() {
            res.visual.push((i as u32 + 1, 100.0));
            continue;
        }
        let total = (ia.width() * ia.height()) as f32;
        let diff = ia
            .pixels()
            .zip(ib.pixels())
            .filter(|(p, q)| p.0.iter().zip(q.0.iter()).any(|(x, y)| x.abs_diff(*y) > 40))
            .count() as f32;
        res.visual.push((i as u32 + 1, diff / total * 100.0));
    }
    Ok(res)
}
