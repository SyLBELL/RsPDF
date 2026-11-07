//! Konversi ke/dari PDF.
//!
//! * Gambar → PDF, PDF → Word/Excel/PowerPoint: diimplementasikan langsung (OOXML ditulis sendiri).
//! * Word/Excel/PowerPoint → PDF: memakai Microsoft Office (COM) atau LibreOffice yang terpasang.
//! * HTML → PDF: memakai Microsoft Edge/Chrome headless (Edge sudah ada di Windows 11).
//! * PDF → PDF/A: memakai Ghostscript.

use crate::pdfops;
use crate::render::{self, BlockKind};
use crate::util::{run_hidden, xml_escape};
use anyhow::{anyhow, bail, Result};
use lopdf::{dictionary, Document, Object, Stream};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

// ───────────────────────── Gambar → PDF ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageMode {
    /// Ukuran halaman mengikuti ukuran gambar
    Fit,
    A4Portrait,
    A4Landscape,
}

pub struct ImgPdfOpts {
    pub mode: PageMode,
    pub margin: f32,
    /// Mode pindaian: abu-abu + kontras otomatis agar tampak seperti dokumen scan.
    pub enhance: bool,
}

pub fn images_to_pdf(images: &[PathBuf], o: &ImgPdfOpts, out: &Path) -> Result<()> {
    if images.is_empty() {
        bail!("Tambahkan minimal satu gambar");
    }
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for p in images {
        let (iid, w, h) = if o.enhance {
            let img = image::open(p).map_err(|e| anyhow!("{}: {e}", p.display()))?;
            let mut g = img.to_luma8();
            // regangkan histogram (1%-99%)
            let mut hist = [0u32; 256];
            g.pixels().for_each(|px| hist[px.0[0] as usize] += 1);
            let total: u32 = hist.iter().sum();
            let (mut lo, mut hi, mut acc) = (0usize, 255usize, 0u32);
            for (i, c) in hist.iter().enumerate() {
                acc += c;
                if acc * 100 >= total {
                    lo = i;
                    break;
                }
            }
            acc = 0;
            for (i, c) in hist.iter().enumerate().rev() {
                acc += c;
                if acc * 100 >= total {
                    hi = i;
                    break;
                }
            }
            if hi > lo + 10 {
                for px in g.pixels_mut() {
                    let v = (px.0[0] as f32 - lo as f32) / (hi - lo) as f32;
                    px.0[0] = (v.clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
            let (w, h) = g.dimensions();
            let mut jpg = Vec::new();
            g.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 85))?;
            let id = doc.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                    "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
                },
                jpg,
            ));
            (id, w, h)
        } else {
            pdfops::add_image(&mut doc, p)?
        };
        let (pw, ph) = match o.mode {
            PageMode::Fit => (w as f32 * 0.75, h as f32 * 0.75),
            PageMode::A4Portrait => (595.28, 841.89),
            PageMode::A4Landscape => (841.89, 595.28),
        };
        let m = if o.mode == PageMode::Fit { 0.0 } else { o.margin };
        let (aw, ah) = ((pw - 2.0 * m).max(10.0), (ph - 2.0 * m).max(10.0));
        let sc = if o.mode == PageMode::Fit { pw / w as f32 } else { (aw / w as f32).min(ah / h as f32) };
        let (dw, dh) = (w as f32 * sc, h as f32 * sc);
        let (x, y) = ((pw - dw) / 2.0, (ph - dh) / 2.0);
        let c = doc.add_object(Stream::new(
            lopdf::Dictionary::new(),
            format!("q {dw:.3} 0 0 {dh:.3} {x:.3} {y:.3} cm /Im0 Do Q").into_bytes(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(pw), Object::Real(ph)],
            "Contents" => c,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => iid } },
        });
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    pdfops::finalize(&mut doc, out)
}

/// Pindai lewat scanner (WIA, Windows). Mengembalikan path PNG hasil pindai.
pub fn scan_with_wia() -> Result<PathBuf> {
    if !cfg!(windows) {
        bail!("Pemindaian scanner hanya tersedia di Windows (WIA)");
    }
    let out = std::env::temp_dir().join(format!(
        "scan_{}.png",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
    ));
    let script = format!(
        "$ErrorActionPreference='Stop'; $d = New-Object -ComObject WIA.CommonDialog; \
         $img = $d.ShowAcquireImage(1,1,0,'{{B96B3CAF-0728-11D3-9D7B-0000F81EF32E}}',$false,$true,$true); \
         if ($img -ne $null) {{ $img.SaveFile('{}') }}",
        ps_quote(&out)
    );
    let o = run_hidden(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]))?;
    if !o.status.success() || !out.exists() {
        bail!("Pemindaian dibatalkan atau gagal: {}", String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(out)
}

// ───────────────────────── Office / HTML → PDF ─────────────────────────

fn ps_quote(p: &Path) -> String {
    p.display().to_string().replace('\'', "''")
}

fn find_soffice() -> Option<PathBuf> {
    for c in [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
    ] {
        if Path::new(c).exists() {
            return Some(c.into());
        }
    }
    if Command::new("soffice").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Some("soffice".into());
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OfficeKind {
    Word,
    Excel,
    PowerPoint,
}

pub fn office_to_pdf(input: &Path, out: &Path, kind: OfficeKind) -> Result<String> {
    let _ = std::fs::remove_file(out);
    let (inp, outp) = (ps_quote(&std::fs::canonicalize(input).unwrap_or(input.into())), ps_quote(out));
    let inp = inp.trim_start_matches(r"\\?\").to_string();
    // 1) Microsoft Office via COM
    if cfg!(windows) {
        let script = match kind {
            OfficeKind::Word => format!(
                "$ErrorActionPreference='Stop'; $a=New-Object -ComObject Word.Application; $a.Visible=$false; \
                 try {{ $d=$a.Documents.Open('{inp}',$false,$true); $d.ExportAsFixedFormat('{outp}',17); $d.Close(0) }} finally {{ $a.Quit() }}"
            ),
            OfficeKind::Excel => format!(
                "$ErrorActionPreference='Stop'; $a=New-Object -ComObject Excel.Application; $a.Visible=$false; $a.DisplayAlerts=$false; \
                 try {{ $w=$a.Workbooks.Open('{inp}'); $w.ExportAsFixedFormat(0,'{outp}'); $w.Close($false) }} finally {{ $a.Quit() }}"
            ),
            OfficeKind::PowerPoint => format!(
                "$ErrorActionPreference='Stop'; $a=New-Object -ComObject PowerPoint.Application; \
                 try {{ $p=$a.Presentations.Open('{inp}',$true,$false,$false); $p.SaveAs('{outp}',32); $p.Close() }} finally {{ $a.Quit() }}"
            ),
        };
        if let Ok(o) = run_hidden(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script])) {
            if o.status.success() && out.exists() {
                return Ok("Microsoft Office".into());
            }
        }
    }
    // 2) LibreOffice
    if let Some(so) = find_soffice() {
        let dir = out.parent().unwrap_or(Path::new("."));
        let o = run_hidden(Command::new(&so).args(["--headless", "--convert-to", "pdf", "--outdir"]).arg(dir).arg(input))?;
        let produced = dir.join(format!("{}.pdf", input.file_stem().and_then(|s| s.to_str()).unwrap_or("out")));
        if o.status.success() && produced.exists() {
            if produced != out {
                std::fs::rename(&produced, out)?;
            }
            return Ok("LibreOffice".into());
        }
        bail!("LibreOffice gagal mengonversi: {}", String::from_utf8_lossy(&o.stderr));
    }
    bail!("Butuh Microsoft Office (Word/Excel/PowerPoint) atau LibreOffice yang terpasang untuk mengonversi file Office ke PDF.")
}

fn find_browser() -> Option<PathBuf> {
    for c in [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ] {
        if Path::new(c).exists() {
            return Some(c.into());
        }
    }
    for n in ["chromium", "google-chrome", "chromium-browser"] {
        if Command::new(n).arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
            return Some(n.into());
        }
    }
    None
}

/// `source` boleh file .html atau URL http(s).
pub fn html_to_pdf(source: &str, out: &Path) -> Result<()> {
    let b = find_browser().ok_or_else(|| anyhow!("Microsoft Edge atau Google Chrome tidak ditemukan"))?;
    let url = if source.starts_with("http://") || source.starts_with("https://") || source.starts_with("file:") {
        source.to_string()
    } else {
        let p = std::fs::canonicalize(source).map_err(|e| anyhow!("File HTML tidak ditemukan: {e}"))?;
        let s = p.display().to_string();
        let s = s.trim_start_matches(r"\\?\").replace('\\', "/");
        format!("file:///{}", s.trim_start_matches('/'))
    };
    let profile = std::env::temp_dir().join(format!("pdfstudio_browser_{}", std::process::id()));
    let _ = std::fs::remove_file(out);
    let o = run_hidden(
        Command::new(&b)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--no-pdf-header-footer")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--print-to-pdf={}", out.display()))
            .arg(&url),
    )?;
    let _ = std::fs::remove_dir_all(&profile);
    if !out.exists() {
        bail!("Browser gagal membuat PDF: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or(""));
    }
    Ok(())
}

// ───────────────────────── PDF → PDF/A ─────────────────────────

fn find_gs() -> Option<PathBuf> {
    if let Ok(rd) = std::fs::read_dir(r"C:\Program Files\gs") {
        let mut dirs: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs.into_iter().rev() {
            for exe in ["gswin64c.exe", "gswin32c.exe"] {
                let p = d.join("bin").join(exe);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    for n in ["gswin64c", "gs"] {
        if Command::new(n).arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
            return Some(n.into());
        }
    }
    None
}

/// Konversi ke PDF/A (level 1, 2 atau 3) memakai Ghostscript.
pub fn pdf_to_pdfa(input: &Path, out: &Path, level: u8) -> Result<()> {
    let gs = find_gs().ok_or_else(|| anyhow!("Ghostscript tidak ditemukan. Pasang dengan: winget install ArtifexSoftware.GhostScript"))?;
    // cari profil ICC bawaan Ghostscript
    let mut icc = None;
    if let Some(root) = gs.parent().and_then(|b| b.parent()) {
        for sub in ["iccprofiles", "lib", "../iccprofiles", "../lib"] {
            let p = root.join(sub).join("default_rgb.icc");
            if p.exists() {
                icc = Some(p);
                break;
            }
        }
    }
    let icc = icc.ok_or_else(|| anyhow!("default_rgb.icc milik Ghostscript tidak ditemukan"))?;
    let icc_s = icc.display().to_string().replace('\\', "/").replace('(', "\\(").replace(')', "\\)");
    let def = std::env::temp_dir().join(format!("pdfstudio_pdfa_{}.ps", std::process::id()));
    std::fs::write(
        &def,
        format!(
            "%!\n/ICCProfile ({icc_s}) def\n\
             [/_objdef {{icc_PDFA}} /type /stream /OBJ pdfmark\n\
             [{{icc_PDFA}} <</N 3>> /PUT pdfmark\n\
             [{{icc_PDFA}} ICCProfile (r) file /PUT pdfmark\n\
             [/_objdef {{OutputIntent_PDFA}} /type /dict /OBJ pdfmark\n\
             [{{OutputIntent_PDFA}} <</Type /OutputIntent /S /GTS_PDFA1 /DestOutputProfile {{icc_PDFA}} /OutputConditionIdentifier (sRGB)>> /PUT pdfmark\n\
             [{{Catalog}} <</OutputIntents [ {{OutputIntent_PDFA}} ]>> /PUT pdfmark\n"
        ),
    )?;
    let o = run_hidden(
        Command::new(&gs)
            .args(["-dBATCH", "-dNOPAUSE", "-dNOOUTERSAVE", "-dQUIET", "-sDEVICE=pdfwrite"])
            .arg(format!("-dPDFA={}", level.clamp(1, 3)))
            .args(["-dPDFACompatibilityPolicy=1", "-sColorConversionStrategy=RGB", "-sProcessColorModel=DeviceRGB"])
            .arg(format!("-sOutputFile={}", out.display()))
            .arg(&def)
            .arg(input),
    )?;
    let _ = std::fs::remove_file(&def);
    if !o.status.success() || !out.exists() {
        bail!("Ghostscript gagal: {}", String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(())
}

// ───────────────────────── Penulis OOXML ─────────────────────────

fn zip_write(out: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    let f = std::fs::File::create(out).map_err(|e| anyhow!("Tidak bisa membuat {}: {e}", out.display()))?;
    let mut z = zip::ZipWriter::new(f);
    let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        z.start_file(name.as_str(), opt)?;
        z.write_all(data)?;
    }
    z.finish()?;
    Ok(())
}

const XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// PDF → DOCX (teks, judul, paragraf; tata letak asli tidak dipertahankan).
pub fn pdf_to_docx(input: &Path, out: &Path, password: &str) -> Result<usize> {
    let blocks = render::blocks(input, password)?;
    let mut body = String::new();
    let mut last_page = 1;
    for b in &blocks {
        if b.page != last_page {
            body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
            last_page = b.page;
        }
        let (sz, bold) = match b.kind {
            BlockKind::Heading(1) => (36, true),
            BlockKind::Heading(2) => (30, true),
            BlockKind::Heading(_) => (26, true),
            BlockKind::Paragraph | BlockKind::TableRow(_) => (22, false),
        };
        let text = match &b.kind {
            BlockKind::TableRow(c) => c.join("\t"),
            _ => b.text.clone(),
        };
        let runs = text
            .split('\t')
            .map(|t| format!("<w:t xml:space=\"preserve\">{}</w:t>", xml_escape(t)))
            .collect::<Vec<_>>()
            .join("<w:tab/>");
        body.push_str(&format!(
            "<w:p><w:pPr><w:spacing w:after=\"{}\"/></w:pPr><w:r><w:rPr>{}<w:sz w:val=\"{sz}\"/></w:rPr>{runs}</w:r></w:p>",
            if bold { 120 } else { 160 },
            if bold { "<w:b/>" } else { "" },
        ));
    }
    if body.is_empty() {
        bail!("Tidak ada teks yang dapat diekstrak (PDF hasil scan? jalankan OCR dulu)");
    }
    let doc = format!(
        "{XML}<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}\
         <w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let ct = format!(
        "{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>"
    );
    let rels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>"
    );
    zip_write(
        out,
        &[("[Content_Types].xml".into(), ct.into()), ("_rels/.rels".into(), rels.into()), ("word/document.xml".into(), doc.into())],
    )?;
    Ok(blocks.len())
}

/// Teks polos → DOCX (dipakai oleh Translate PDF).
pub fn text_to_docx(paragraphs: &[(bool, String)], out: &Path) -> Result<()> {
    let mut body = String::new();
    for (heading, t) in paragraphs {
        body.push_str(&format!(
            "<w:p><w:r><w:rPr>{}<w:sz w:val=\"{}\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            if *heading { "<w:b/>" } else { "" },
            if *heading { 28 } else { 22 },
            xml_escape(t)
        ));
    }
    let doc = format!(
        "{XML}<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let ct = format!(
        "{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>"
    );
    let rels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>"
    );
    zip_write(out, &[("[Content_Types].xml".into(), ct.into()), ("_rels/.rels".into(), rels.into()), ("word/document.xml".into(), doc.into())])
}

fn col_name(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s
}

/// PDF → XLSX: satu sheet per halaman, kolom ditebak dari posisi teks.
pub fn pdf_to_xlsx(input: &Path, out: &Path, password: &str) -> Result<usize> {
    let pages = render::tables(input, password)?;
    let n = pages.len().max(1);
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut ct = format!(
        "{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>"
    );
    let mut wb = format!(
        "{XML}<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>"
    );
    let mut wrels = format!("{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    let mut total_rows = 0;
    for i in 0..n {
        let rows = pages.get(i).cloned().unwrap_or_default();
        total_rows += rows.len();
        let mut sd = format!("{XML}<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>");
        for (r, row) in rows.iter().enumerate() {
            sd.push_str(&format!("<row r=\"{}\">", r + 1));
            for (c, cell) in row.iter().enumerate() {
                if cell.is_empty() {
                    continue;
                }
                let t = cell.trim();
                let is_num = t.parse::<f64>().is_ok() && !t.starts_with('+') && t.len() < 16;
                if is_num {
                    sd.push_str(&format!("<c r=\"{}{}\"><v>{}</v></c>", col_name(c), r + 1, t));
                } else {
                    sd.push_str(&format!(
                        "<c r=\"{}{}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                        col_name(c),
                        r + 1,
                        xml_escape(t)
                    ));
                }
            }
            sd.push_str("</row>");
        }
        sd.push_str("</sheetData></worksheet>");
        files.push((format!("xl/worksheets/sheet{}.xml", i + 1), sd.into()));
        ct.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            i + 1
        ));
        wb.push_str(&format!("<sheet name=\"Halaman {}\" sheetId=\"{}\" r:id=\"rId{}\"/>", i + 1, i + 1, i + 1));
        wrels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{}.xml\"/>",
            i + 1,
            i + 1
        ));
    }
    if total_rows == 0 {
        bail!("Tidak ada teks/tabel yang dapat diekstrak (PDF hasil scan? jalankan OCR dulu)");
    }
    ct.push_str("</Types>");
    wb.push_str("</sheets></workbook>");
    wrels.push_str("</Relationships>");
    let rels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"
    );
    files.push(("[Content_Types].xml".into(), ct.into()));
    files.push(("_rels/.rels".into(), rels.into()));
    files.push(("xl/workbook.xml".into(), wb.into()));
    files.push(("xl/_rels/workbook.xml.rels".into(), wrels.into()));
    zip_write(out, &files)?;
    Ok(total_rows)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PptMode {
    /// Tiap halaman menjadi gambar satu slide (tampilan identik)
    Image,
    /// Teks dipindahkan ke kotak teks yang bisa diedit
    EditableText,
}

const THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;

const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
const GRP: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

fn textbox(id: u32, name: &str, x: i64, y: i64, w: i64, h: i64, paras: &[(String, u32, bool)]) -> String {
    let mut ps = String::new();
    for (t, sz, bold) in paras {
        ps.push_str(&format!(
            "<a:p><a:r><a:rPr lang=\"en-US\" sz=\"{}\" b=\"{}\" dirty=\"0\"/><a:t>{}</a:t></a:r></a:p>",
            sz * 100,
            if *bold { 1 } else { 0 },
            xml_escape(t)
        ));
    }
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"square\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{ps}</p:txBody></p:sp>"
    )
}

pub fn pdf_to_pptx(input: &Path, out: &Path, password: &str, mode: PptMode, dpi: f32, progress: &dyn Fn(&str)) -> Result<usize> {
    let total = render::page_total(input, password)? as usize;
    if total == 0 {
        bail!("PDF kosong");
    }
    let (_, w0, h0) = render::render_page(input, password, 0, 0.1)?;
    let (cx, cy): (i64, i64) = if w0 >= h0 {
        (12_192_000, (12_192_000.0 * h0 / w0) as i64)
    } else {
        ((6_858_000.0 * w0 / h0) as i64, 6_858_000)
    };
    let blocks = if mode == PptMode::EditableText { render::blocks(input, password)? } else { vec![] };
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut ct = format!(
        "{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>"
    );
    let mut sld_ids = String::new();
    let mut prels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>"
    );
    for i in 1..=total {
        progress(&format!("Membuat slide {i}/{total}…"));
        let mut slide_rels = format!(
            "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>"
        );
        let shapes = match mode {
            PptMode::Image => {
                let (img, _, _) = render::render_page(input, password, (i - 1) as u16, dpi / 72.0)?;
                let mut jpg = Vec::new();
                img.to_rgb8().write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 88))?;
                files.push((format!("ppt/media/image{i}.jpg"), jpg));
                slide_rels.push_str(&format!(
                    "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image{i}.jpg\"/>"
                ));
                format!(
                    "<p:pic><p:nvPicPr><p:cNvPr id=\"2\" name=\"Halaman {i}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>"
                )
            }
            PptMode::EditableText => {
                let pb: Vec<_> = blocks.iter().filter(|b| b.page as usize == i).collect();
                let mut title = String::new();
                let mut body: Vec<(String, u32, bool)> = Vec::new();
                for (k, b) in pb.iter().enumerate() {
                    if k == 0 && matches!(b.kind, BlockKind::Heading(_)) {
                        title = b.text.clone();
                    } else {
                        body.push((b.text.clone(), 16, matches!(b.kind, BlockKind::Heading(_))));
                    }
                }
                let m = cx / 20;
                let mut s = String::new();
                let mut y = m / 2;
                if !title.is_empty() {
                    s.push_str(&textbox(2, "Judul", m, y, cx - 2 * m, cy / 6, &[(title, 32, true)]));
                    y += cy / 6 + m / 4;
                }
                if !body.is_empty() {
                    s.push_str(&textbox(3, "Isi", m, y, cx - 2 * m, cy - y - m / 2, &body));
                }
                s
            }
        };
        slide_rels.push_str("</Relationships>");
        let slide = format!("{XML}<p:sld {NS}><p:cSld><p:spTree>{GRP}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>");
        files.push((format!("ppt/slides/slide{i}.xml"), slide.into()));
        files.push((format!("ppt/slides/_rels/slide{i}.xml.rels"), slide_rels.into()));
        ct.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
        sld_ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + i, 10 + i));
        prels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{i}.xml\"/>",
            10 + i
        ));
    }
    ct.push_str("</Types>");
    prels.push_str("<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/></Relationships>");
    let pres = format!(
        "{XML}<p:presentation {NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{sld_ids}</p:sldIdLst><p:sldSz cx=\"{cx}\" cy=\"{cy}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
    );
    let master = format!(
        "{XML}<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>{GRP}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>"
    );
    let master_rels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"../theme/theme1.xml\"/></Relationships>"
    );
    let layout = format!("{XML}<p:sldLayout {NS} type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree>{GRP}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>");
    let layout_rels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>"
    );
    let rels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>"
    );
    files.push(("[Content_Types].xml".into(), ct.into()));
    files.push(("_rels/.rels".into(), rels.into()));
    files.push(("ppt/presentation.xml".into(), pres.into()));
    files.push(("ppt/_rels/presentation.xml.rels".into(), prels.into()));
    files.push(("ppt/slideMasters/slideMaster1.xml".into(), master.into()));
    files.push(("ppt/slideMasters/_rels/slideMaster1.xml.rels".into(), master_rels.into()));
    files.push(("ppt/slideLayouts/slideLayout1.xml".into(), layout.into()));
    files.push(("ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(), layout_rels.into()));
    files.push(("ppt/theme/theme1.xml".into(), format!("{XML}{THEME}").into()));
    zip_write(out, &files)?;
    Ok(total)
}
