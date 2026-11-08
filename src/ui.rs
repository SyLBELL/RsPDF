use anyhow::{anyhow, Result};
use crate::icons;
use crate::theme::{self, Accent, BannerKind, CardKind, Kind};
use eframe::egui::{self, load::SizedTexture, Align, Align2, Color32, Id, Layout, Margin, Order, Pos2, Rect, RichText, Rounding, Sense, Stroke, TextureHandle, Vec2};
use pdf_studio::ai;
use pdf_studio::convert::{self, ImgPdfOpts, OfficeKind, PageMode, PptMode};
use pdf_studio::pdfops::{self, CompressOpts, EditItem, FieldKind, FormField, PageNumOpts, Pos, ProtectOpts, SignOpts, SplitMode, WatermarkOpts, WmKind};
use pdf_studio::render::{self, CompareResult, ImgFormat};
use pdf_studio::util;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Merge, Split, RemovePages, ExtractPages, Organize, ScanToPdf,
    Compress, Repair, Ocr,
    JpgToPdf, WordToPdf, PowerpointToPdf, ExcelToPdf, HtmlToPdf,
    PdfToJpg, PdfToWord, PdfToPowerpoint, PdfToExcel, PdfToPdfa,
    Rotate, AddPageNumbers, AddWatermark, Crop, EditPdf, PdfForms,
    Unlock, Protect, Sign, Redact, Compare,
    Summarizer, Translate, PdfToMarkdown,
}

const CAT_ICONS: [char; 7] = [icons::FOLDER, icons::TUNE, icons::FILE_UPLOAD, icons::FILE_DOWNLOAD, icons::EDIT_NOTE, icons::SHIELD, icons::AUTO_AWESOME];

fn cat_of(t: Tool) -> usize {
    CATS.iter().position(|(_, ts)| ts.contains(&t)).unwrap_or(0)
}

const CATS: &[(&str, &[Tool])] = &[
    ("ORGANIZE PDF", &[Tool::Merge, Tool::Split, Tool::RemovePages, Tool::ExtractPages, Tool::Organize, Tool::ScanToPdf]),
    ("OPTIMIZE PDF", &[Tool::Compress, Tool::Repair, Tool::Ocr]),
    ("CONVERT TO PDF", &[Tool::JpgToPdf, Tool::WordToPdf, Tool::PowerpointToPdf, Tool::ExcelToPdf, Tool::HtmlToPdf]),
    ("CONVERT FROM PDF", &[Tool::PdfToJpg, Tool::PdfToWord, Tool::PdfToPowerpoint, Tool::PdfToExcel, Tool::PdfToPdfa]),
    ("EDIT PDF", &[Tool::Rotate, Tool::AddPageNumbers, Tool::AddWatermark, Tool::Crop, Tool::EditPdf, Tool::PdfForms]),
    ("PDF SECURITY", &[Tool::Unlock, Tool::Protect, Tool::Sign, Tool::Redact, Tool::Compare]),
    ("PDF INTELLIGENCE", &[Tool::Summarizer, Tool::Translate, Tool::PdfToMarkdown]),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Input {
    PdfOne,
    PdfMany,
    Images,
    Word,
    Ppt,
    Excel,
    Html,
    PdfTwo,
}

impl Tool {
    fn label(self) -> &'static str {
        use Tool::*;
        match self {
            Merge => "Merge PDF", Split => "Split PDF", RemovePages => "Remove pages", ExtractPages => "Extract pages",
            Organize => "Organize PDF", ScanToPdf => "Scan to PDF", Compress => "Compress PDF", Repair => "Repair PDF",
            Ocr => "OCR PDF", JpgToPdf => "JPG to PDF", WordToPdf => "WORD to PDF", PowerpointToPdf => "POWERPOINT to PDF",
            ExcelToPdf => "EXCEL to PDF", HtmlToPdf => "HTML to PDF", PdfToJpg => "PDF to JPG", PdfToWord => "PDF to WORD",
            PdfToPowerpoint => "PDF to POWERPOINT", PdfToExcel => "PDF to EXCEL", PdfToPdfa => "PDF to PDF/A",
            Rotate => "Rotate PDF", AddPageNumbers => "Add page numbers", AddWatermark => "Add watermark", Crop => "Crop PDF",
            EditPdf => "Edit PDF", PdfForms => "PDF Forms", Unlock => "Unlock PDF", Protect => "Protect PDF",
            Sign => "Sign PDF", Redact => "Redact PDF", Compare => "Compare PDF", Summarizer => "AI Summarizer",
            Translate => "Translate PDF", PdfToMarkdown => "PDF to Markdown",
        }
    }
    fn desc(self) -> &'static str {
        use Tool::*;
        match self {
            Merge => "Gabungkan beberapa PDF menjadi satu. Atur urutan dengan tombol ▲▼.",
            Split => "Pecah PDF menjadi beberapa file berdasarkan rentang, setiap N halaman, atau per halaman.",
            RemovePages => "Hapus halaman tertentu, misalnya 2,5-7.",
            ExtractPages => "Ambil halaman tertentu menjadi PDF baru, misalnya 1-3,8.",
            Organize => "Susun ulang, duplikasi atau buang halaman dengan menulis urutan baru, misalnya 3,1,2,2.",
            ScanToPdf => "Pindai dari scanner (WIA) atau pakai foto dokumen; opsi perbaikan kontras gaya hasil scan.",
            Compress => "Perkecil ukuran file: kompres stream, hapus objek tak terpakai, dan kompres ulang gambar JPEG.",
            Repair => "Coba pulihkan PDF rusak: bangun ulang tabel xref dan pohon halaman.",
            Ocr => "Ubah PDF hasil scan menjadi PDF yang bisa dicari/disalin (butuh Tesseract OCR).",
            JpgToPdf => "Ubah gambar JPG/PNG menjadi PDF, satu gambar per halaman.",
            WordToPdf => "Konversi .doc/.docx ke PDF (butuh Microsoft Word atau LibreOffice).",
            PowerpointToPdf => "Konversi .ppt/.pptx ke PDF (butuh Microsoft PowerPoint atau LibreOffice).",
            ExcelToPdf => "Konversi .xls/.xlsx ke PDF (butuh Microsoft Excel atau LibreOffice).",
            HtmlToPdf => "Ubah file HTML atau URL web menjadi PDF memakai Microsoft Edge/Chrome.",
            PdfToJpg => "Simpan halaman PDF sebagai gambar JPG atau PNG.",
            PdfToWord => "Ekstrak teks, judul, dan paragraf ke .docx (tata letak asli tidak dipertahankan).",
            PdfToPowerpoint => "Ubah tiap halaman menjadi slide (gambar identik atau teks yang bisa diedit).",
            PdfToExcel => "Ekstrak teks berbentuk tabel ke .xlsx, satu sheet per halaman.",
            PdfToPdfa => "Konversi ke PDF/A untuk pengarsipan jangka panjang (butuh Ghostscript).",
            Rotate => "Putar halaman 90°, 180° atau 270°.",
            AddPageNumbers => "Tambahkan nomor halaman dengan posisi dan format pilihan.",
            AddWatermark => "Tambahkan watermark teks atau gambar dengan transparansi dan sudut.",
            Crop => "Potong margin halaman. Seret di preview untuk memilih area yang dipertahankan.",
            EditPdf => "Tambah teks, gambar, kotak penutup (whiteout) dan stabilo. Klik/seret di preview untuk menentukan posisi.",
            PdfForms => "Baca dan isi kolom formulir PDF (AcroForm).",
            Unlock => "Hapus password/proteksi dari PDF (kamu harus tahu password-nya).",
            Protect => "Beri password (AES-128) dan batasi cetak/salin/ubah.",
            Sign => "Bubuhkan tanda tangan visual (gambar/teks + tanggal). Bukan tanda tangan digital bersertifikat.",
            Redact => "Sensor permanen: area/teks yang dipilih dihapus dari halaman (halaman dirender ulang sebagai gambar).",
            Compare => "Bandingkan dua PDF: perbedaan teks per baris dan persentase perbedaan visual per halaman.",
            Summarizer => "Ringkas isi PDF dengan Claude (butuh API key Anthropic).",
            Translate => "Terjemahkan teks PDF dengan Claude ke .docx/.md/.txt (butuh API key Anthropic).",
            PdfToMarkdown => "Konversi PDF ke Markdown (judul dari ukuran huruf, tabel sederhana).",
        }
    }
    fn input(self) -> Input {
        use Tool::*;
        match self {
            Merge => Input::PdfMany,
            ScanToPdf | JpgToPdf => Input::Images,
            WordToPdf => Input::Word,
            PowerpointToPdf => Input::Ppt,
            ExcelToPdf => Input::Excel,
            HtmlToPdf => Input::Html,
            Compare => Input::PdfTwo,
            _ => Input::PdfOne,
        }
    }
    /// (akhiran, ekstensi)
    fn out_spec(self) -> (&'static str, &'static str) {
        use Tool::*;
        match self {
            Merge => ("merged", "pdf"), RemovePages => ("removed", "pdf"), ExtractPages => ("extracted", "pdf"),
            Organize => ("organized", "pdf"), ScanToPdf => ("scan", "pdf"), Compress => ("compressed", "pdf"),
            Repair => ("repaired", "pdf"), Ocr => ("ocr", "pdf"), JpgToPdf => ("images", "pdf"),
            WordToPdf | PowerpointToPdf | ExcelToPdf | HtmlToPdf => ("converted", "pdf"),
            PdfToWord => ("converted", "docx"), PdfToPowerpoint => ("converted", "pptx"), PdfToExcel => ("converted", "xlsx"),
            PdfToPdfa => ("pdfa", "pdf"), Rotate => ("rotated", "pdf"), AddPageNumbers => ("numbered", "pdf"),
            AddWatermark => ("watermarked", "pdf"), Crop => ("cropped", "pdf"), EditPdf => ("edited", "pdf"),
            PdfForms => ("filled", "pdf"), Unlock => ("unlocked", "pdf"), Protect => ("protected", "pdf"),
            Sign => ("signed", "pdf"), Redact => ("redacted", "pdf"), Summarizer => ("summary", "md"),
            Translate => ("translated", "docx"), PdfToMarkdown => ("converted", "md"),
            Split => ("split", ""), PdfToJpg => ("images", ""), Compare => ("", ""),
        }
    }
    fn icon(self) -> char {
        use Tool::*;
        match self {
            Merge => icons::MERGE_TYPE, Split => icons::CALL_SPLIT, RemovePages => icons::DELETE, ExtractPages => icons::OUTPUT,
            Organize => icons::REORDER, ScanToPdf => icons::DOCUMENT_SCANNER, Compress => icons::COMPRESS, Repair => icons::BUILD,
            Ocr => icons::FIND_IN_PAGE, JpgToPdf => icons::IMAGE, WordToPdf => icons::DESCRIPTION, PowerpointToPdf => icons::SLIDESHOW,
            ExcelToPdf => icons::TABLE_CHART, HtmlToPdf => icons::HTML, PdfToJpg => icons::PHOTO_LIBRARY, PdfToWord => icons::ARTICLE,
            PdfToPowerpoint => icons::PRESENT_TO_ALL, PdfToExcel => icons::GRID_ON, PdfToPdfa => icons::INVENTORY_2,
            Rotate => icons::ROTATE_RIGHT, AddPageNumbers => icons::FORMAT_LIST_NUMBERED, AddWatermark => icons::BRANDING_WATERMARK,
            Crop => icons::CROP, EditPdf => icons::EDIT, PdfForms => icons::DYNAMIC_FORM, Unlock => icons::LOCK_OPEN, Protect => icons::LOCK,
            Sign => icons::DRAW, Redact => icons::VISIBILITY_OFF, Compare => icons::COMPARE, Summarizer => icons::AUTO_AWESOME,
            Translate => icons::TRANSLATE, PdfToMarkdown => icons::CODE,
        }
    }
    fn short(self) -> &'static str {
        use Tool::*;
        match self {
            Merge => "Gabungkan beberapa PDF jadi satu",
            Split => "Pecah PDF per rentang, per N halaman, atau per halaman",
            RemovePages => "Hapus halaman yang tidak diperlukan",
            ExtractPages => "Ambil halaman tertentu jadi PDF baru",
            Organize => "Urutkan ulang, duplikasi, atau buang halaman",
            ScanToPdf => "Pindai dari scanner atau foto dokumen",
            Compress => "Perkecil ukuran file PDF",
            Repair => "Pulihkan PDF yang rusak",
            Ocr => "Jadikan PDF hasil scan bisa dicari dan disalin",
            JpgToPdf => "Ubah gambar JPG/PNG menjadi PDF",
            WordToPdf => "Konversi dokumen Word ke PDF",
            PowerpointToPdf => "Konversi presentasi PowerPoint ke PDF",
            ExcelToPdf => "Konversi spreadsheet Excel ke PDF",
            HtmlToPdf => "Ubah halaman web atau file HTML ke PDF",
            PdfToJpg => "Simpan halaman PDF sebagai gambar",
            PdfToWord => "Ekstrak teks PDF ke dokumen Word",
            PdfToPowerpoint => "Ubah halaman PDF menjadi slide",
            PdfToExcel => "Ekstrak tabel PDF ke Excel",
            PdfToPdfa => "Arsipkan sebagai PDF/A jangka panjang",
            Rotate => "Putar halaman 90°, 180°, atau 270°",
            AddPageNumbers => "Tambahkan nomor halaman",
            AddWatermark => "Tambahkan watermark teks atau gambar",
            Crop => "Potong margin halaman",
            EditPdf => "Tambah teks, gambar, kotak, dan stabilo",
            PdfForms => "Isi formulir PDF interaktif",
            Unlock => "Hapus password dari PDF",
            Protect => "Kunci PDF dengan password",
            Sign => "Bubuhkan tanda tangan visual",
            Redact => "Sensor teks atau area secara permanen",
            Compare => "Bandingkan dua PDF: teks dan tampilan",
            Summarizer => "Ringkas isi PDF dengan AI",
            Translate => "Terjemahkan teks PDF dengan AI",
            PdfToMarkdown => "Konversi PDF ke Markdown",
        }
    }
    fn has_options(self) -> bool {
        !matches!(
            self,
            Tool::Merge | Tool::Repair | Tool::Unlock | Tool::PdfToWord | Tool::PdfToExcel | Tool::PdfToMarkdown | Tool::WordToPdf | Tool::PowerpointToPdf | Tool::ExcelToPdf | Tool::HtmlToPdf
        )
    }
    fn is_folder_output(self) -> bool {
        matches!(self, Tool::Split | Tool::PdfToJpg)
    }
    fn uses_preview(self) -> bool {
        !matches!(self, Tool::Unlock | Tool::Summarizer | Tool::Translate | Tool::PdfToMarkdown | Tool::Compare | Tool::JpgToPdf | Tool::ScanToPdf | Tool::HtmlToPdf | Tool::WordToPdf | Tool::PowerpointToPdf | Tool::ExcelToPdf)
    }
}

enum Msg {
    Progress(String),
    Done(std::result::Result<TaskOutput, String>),
}

#[derive(Default)]
struct TaskOutput {
    msg: String,
    outputs: Vec<PathBuf>,
    text: Option<String>,
    compare: Option<CompareResult>,
}

#[derive(Default)]
struct Preview {
    key: Option<(PathBuf, u32, u32)>,
    tex: Option<TextureHandle>,
    size_pt: (f32, f32),
    err: Option<String>,
}

struct Snack {
    text: String,
    kind: BannerKind,
    at: std::time::Instant,
}

pub struct App {
    // tampilan
    dark: bool,
    follow_system: bool,
    accent: Accent,
    home: bool,
    search: String,
    nav_open: [bool; 7],
    accent_open: bool,
    accent_btn: Rect,
    snack: Option<Snack>,
    sizes: std::collections::HashMap<PathBuf, u64>,
    tool: Tool,
    files: Vec<PathBuf>,
    active: usize,
    password: String,
    out_override: Option<PathBuf>,
    busy: bool,
    status: String,
    log: Vec<String>,
    outputs: Vec<PathBuf>,
    result_text: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    // preview
    pv: Preview,
    pv_gen: u32,
    page: u32,
    total: u32,
    zoom: f32,
    sel: Option<[f32; 4]>,
    pt: Option<(f32, f32)>,
    drag_start: Option<(f32, f32)>,
    // parameter
    pages_spec: String,
    split_mode: usize,
    split_spec: String,
    split_n: u32,
    organize_order: String,
    compress_level: usize,
    ocr_lang: String,
    ocr_dpi: f32,
    img_mode: usize,
    img_margin: f32,
    img_enhance: bool,
    html_src: String,
    jpg_dpi: f32,
    jpg_png: bool,
    jpg_quality: u8,
    pdfa_level: u8,
    ppt_editable: bool,
    ppt_dpi: f32,
    rotate_deg: i64,
    pn_format: String,
    pn_pos: Pos,
    pn_size: f32,
    pn_margin: f32,
    pn_start: u32,
    wm_image: bool,
    wm_text: String,
    wm_size: f32,
    wm_color: [u8; 3],
    wm_opacity: f32,
    wm_angle: f32,
    wm_img: Option<PathBuf>,
    wm_scale: f32,
    crop: [f32; 4],
    edit_action: usize,
    edit_text: String,
    edit_size: f32,
    edit_color: [u8; 3],
    edit_img: Option<PathBuf>,
    edit_items: Vec<EditItem>,
    forms: Vec<FormField>,
    form_vals: Vec<String>,
    form_lock: bool,
    prot_user: String,
    prot_owner: String,
    prot_print: bool,
    prot_copy: bool,
    prot_modify: bool,
    prot_annot: bool,
    sign_img: Option<PathBuf>,
    sign_name: String,
    sign_reason: String,
    sign_date: bool,
    sign_border: bool,
    redact_rects: Vec<(u32, [f32; 4])>,
    redact_terms: String,
    redact_dpi: f32,
    cmp_b: Option<PathBuf>,
    cmp_pw: String,
    cmp_result: Option<CompareResult>,
    cmp_show_equal: bool,
    // pengaturan & AI
    show_settings: bool,
    tesseract_path: String,
    api_key: String,
    model: String,
    ai_lang: String,
    ai_style: usize,
    tr_target: String,
    tr_fmt: usize,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Vec<String>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let (accent, saved_dark) = load_cfg();
        let dark = saved_dark.unwrap_or(true);
        theme::apply(&cc.egui_ctx, accent, dark);
        let (tx, rx) = channel();
        let mut app = Self {
            dark,
            follow_system: saved_dark.is_none(),
            accent,
            home: true,
            search: String::new(),
            nav_open: [false; 7],
            accent_open: false,
            accent_btn: Rect::NOTHING,
            snack: None,
            sizes: Default::default(),
            tool: Tool::Merge,
            files: vec![],
            active: 0,
            password: String::new(),
            out_override: None,
            busy: false,
            status: "Siap. Pilih alat di kiri, lalu tambahkan file (atau seret file ke jendela).".into(),
            log: vec![],
            outputs: vec![],
            result_text: String::new(),
            tx,
            rx,
            pv: Preview::default(),
            pv_gen: 0,
            page: 1,
            total: 0,
            zoom: 0.9,
            sel: None,
            pt: None,
            drag_start: None,
            pages_spec: String::new(),
            split_mode: 0,
            split_spec: "1-3,4-6".into(),
            split_n: 1,
            organize_order: String::new(),
            compress_level: 1,
            ocr_lang: "eng".into(),
            ocr_dpi: 300.0,
            img_mode: 1,
            img_margin: 20.0,
            img_enhance: false,
            html_src: String::new(),
            jpg_dpi: 150.0,
            jpg_png: false,
            jpg_quality: 90,
            pdfa_level: 2,
            ppt_editable: false,
            ppt_dpi: 150.0,
            rotate_deg: 90,
            pn_format: "{n}".into(),
            pn_pos: Pos::BottomCenter,
            pn_size: 11.0,
            pn_margin: 28.0,
            pn_start: 1,
            wm_image: false,
            wm_text: "CONFIDENTIAL".into(),
            wm_size: 64.0,
            wm_color: [200, 0, 0],
            wm_opacity: 0.25,
            wm_angle: 45.0,
            wm_img: None,
            wm_scale: 0.5,
            crop: [0.0; 4],
            edit_action: 0,
            edit_text: String::new(),
            edit_size: 14.0,
            edit_color: [0, 0, 0],
            edit_img: None,
            edit_items: vec![],
            forms: vec![],
            form_vals: vec![],
            form_lock: false,
            prot_user: String::new(),
            prot_owner: String::new(),
            prot_print: true,
            prot_copy: true,
            prot_modify: false,
            prot_annot: false,
            sign_img: None,
            sign_name: String::new(),
            sign_reason: String::new(),
            sign_date: true,
            sign_border: true,
            redact_rects: vec![],
            redact_terms: String::new(),
            redact_dpi: 150.0,
            cmp_b: None,
            cmp_pw: String::new(),
            cmp_result: None,
            cmp_show_equal: false,
            show_settings: false,
            tesseract_path: String::new(),
            api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            model: ai::DEFAULT_MODEL.into(),
            ai_lang: "Bahasa Indonesia".into(),
            ai_style: 0,
            tr_target: "English".into(),
            tr_fmt: 0,
        };
        // Argumen: [--tool NamaAlat] [file ...]  (mis. dari "Open with" di Windows)
        let mut files = Vec::new();
        let mut args_tool = false;
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            if a == "--tool" {
                if let Some(n) = it.next() {
                    let n = n.to_lowercase().replace(['_', '-', ' '], "");
                    if let Some(t) = CATS.iter().flat_map(|(_, ts)| ts.iter()).find(|t| format!("{t:?}").to_lowercase() == n || t.label().to_lowercase().replace([' ', '/'], "") == n) {
                        app.tool = *t;
                        args_tool = true;
                    }
                }
            } else {
                files.push(PathBuf::from(a));
            }
        }
        if !files.is_empty() {
            if app.tool == Tool::Merge && files.len() == 1 {
                app.tool = Tool::EditPdf;
            }
            app.home = false;
            app.nav_open[cat_of(app.tool)] = true;
            app.add_files(files);
        } else if args_tool {
            app.home = false;
            app.nav_open[cat_of(app.tool)] = true;
        }
        app
    }

    // ───────── state helpers ─────────

    fn select_tool(&mut self, t: Tool) {
        self.home = false;
        self.nav_open[cat_of(t)] = true;
        if self.tool == t {
            return;
        }
        let was = self.tool.input();
        self.tool = t;
        self.out_override = None;
        self.sel = None;
        self.pt = None;
        self.result_text.clear();
        self.outputs.clear();
        self.pages_spec.clear();
        if was != t.input() && !(matches!(was, Input::PdfOne | Input::PdfMany | Input::PdfTwo) && matches!(t.input(), Input::PdfOne | Input::PdfMany | Input::PdfTwo)) {
            self.files.clear();
            self.active = 0;
            self.total = 0;
        }
        if matches!(t.input(), Input::PdfOne | Input::PdfTwo) && self.files.len() > 1 {
            self.files.truncate(1);
        }
    }

    fn notify(&mut self, s: String) {
        let kind = if s.starts_with('✖') {
            BannerKind::Error
        } else if s.starts_with('✔') {
            BannerKind::Success
        } else {
            BannerKind::Info
        };
        let text = s.trim_start_matches(['✖', '✔', ' ']).to_string();
        self.snack = Some(Snack { text, kind, at: std::time::Instant::now() });
    }

    fn ext_ok(&self, p: &Path) -> bool {
        let e = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        match self.tool.input() {
            Input::PdfOne | Input::PdfMany | Input::PdfTwo => e == "pdf",
            Input::Images => matches!(e.as_str(), "jpg" | "jpeg" | "png"),
            Input::Word => matches!(e.as_str(), "doc" | "docx" | "rtf" | "odt"),
            Input::Ppt => matches!(e.as_str(), "ppt" | "pptx" | "odp"),
            Input::Excel => matches!(e.as_str(), "xls" | "xlsx" | "csv" | "ods"),
            Input::Html => matches!(e.as_str(), "html" | "htm"),
        }
    }

    fn filters(&self) -> (&'static str, Vec<&'static str>) {
        match self.tool.input() {
            Input::PdfOne | Input::PdfMany | Input::PdfTwo => ("PDF", vec!["pdf"]),
            Input::Images => ("Gambar", vec!["jpg", "jpeg", "png"]),
            Input::Word => ("Word", vec!["doc", "docx", "rtf", "odt"]),
            Input::Ppt => ("PowerPoint", vec!["ppt", "pptx", "odp"]),
            Input::Excel => ("Excel", vec!["xls", "xlsx", "csv", "ods"]),
            Input::Html => ("HTML", vec!["html", "htm"]),
        }
    }

    fn add_files(&mut self, ps: Vec<PathBuf>) {
        let ps: Vec<PathBuf> = ps.into_iter().filter(|p| self.ext_ok(p)).collect();
        if ps.is_empty() {
            return;
        }
        for p in &ps {
            if let Ok(m) = std::fs::metadata(p) {
                self.sizes.insert(p.clone(), m.len());
            }
        }
        match self.tool.input() {
            Input::PdfMany | Input::Images => self.files.extend(ps),
            _ => self.files = vec![ps[0].clone()],
        }
        self.out_override = None;
        self.on_files_changed();
    }

    fn on_files_changed(&mut self) {
        if self.active >= self.files.len() {
            self.active = 0;
        }
        self.page = 1;
        self.sel = None;
        self.pt = None;
        self.edit_items.clear();
        self.redact_rects.clear();
        self.forms.clear();
        self.cmp_result = None;
        self.refresh_total();
        if matches!(self.tool, Tool::Organize) {
            self.organize_order = (1..=self.total).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        }
    }

    fn refresh_total(&mut self) {
        self.total = 0;
        self.pv.err = None;
        if matches!(self.tool.input(), Input::PdfOne | Input::PdfMany | Input::PdfTwo) {
            if let Some(f) = self.files.get(self.active) {
                match pdfops::page_count(f, &self.password) {
                    Ok(n) => self.total = n,
                    Err(e) => self.pv.err = Some(e.to_string()),
                }
            }
        }
        self.pv.key = None;
    }

    fn out_path(&self) -> Option<PathBuf> {
        if let Some(o) = &self.out_override {
            return Some(o.clone());
        }
        let (suffix, ext) = self.tool.out_spec();
        if self.tool == Tool::HtmlToPdf && self.files.is_empty() {
            let base = std::env::var("USERPROFILE").map(|h| PathBuf::from(h).join("Documents")).unwrap_or_else(|_| PathBuf::from("."));
            return Some(base.join("halaman_web.pdf"));
        }
        let f = self.files.first()?;
        if self.tool.is_folder_output() {
            let stem = f.file_stem()?.to_str()?;
            return Some(f.parent()?.join(format!("{stem}_{suffix}")));
        }
        if self.tool == Tool::Merge || self.tool == Tool::JpgToPdf || self.tool == Tool::ScanToPdf {
            return Some(f.parent()?.join(format!("{suffix}.{ext}")));
        }
        let mut ext = ext;
        if self.tool == Tool::Translate {
            ext = ["docx", "md", "txt"][self.tr_fmt.min(2)];
        }
        Some(util::default_out(f, suffix, ext))
    }

    fn pick_input(&mut self, ui_label: &str) {
        let (name, exts) = self.filters();
        let _ = ui_label;
        let d = rfd::FileDialog::new().add_filter(name, &exts);
        let picked = if matches!(self.tool.input(), Input::PdfMany | Input::Images) { d.pick_files() } else { d.pick_file().map(|p| vec![p]) };
        if let Some(ps) = picked {
            self.add_files(ps);
        }
    }

    fn spawn<F>(&mut self, ctx: &egui::Context, f: F)
    where
        F: FnOnce(&dyn Fn(&str)) -> Result<TaskOutput> + Send + 'static,
    {
        self.busy = true;
        self.status = "Memproses…".into();
        self.outputs.clear();
        let (tx, ctx2) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let (txp, ctxp) = (tx.clone(), ctx2.clone());
            let progress = move |s: &str| {
                let _ = txp.send(Msg::Progress(s.to_string()));
                ctxp.request_repaint();
            };
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&progress)))
                .unwrap_or_else(|_| Err(anyhow!("Terjadi kesalahan internal (panic) saat memproses")));
            let _ = tx.send(Msg::Done(res.map_err(|e| e.to_string())));
            ctx2.request_repaint();
        });
    }

    fn poll(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Progress(s) => self.status = s,
                Msg::Done(Ok(o)) => {
                    self.busy = false;
                    self.notify(format!("✔ {}", o.msg));
                    self.outputs = o.outputs;
                    if let Some(t) = o.text {
                        self.result_text = t;
                    }
                    if o.compare.is_some() {
                        self.cmp_result = o.compare;
                    }
                    self.pv_gen += 1;
                }
                Msg::Done(Err(e)) => {
                    self.busy = false;
                    self.notify(format!("✖ {e}"));
                }
            }
        }
    }

    fn refresh_preview(&mut self, ctx: &egui::Context) {
        if self.busy || !self.tool.uses_preview() || self.total == 0 {
            return;
        }
        let Some(f) = self.files.get(self.active).cloned() else { return };
        let key = (f.clone(), self.page, self.pv_gen);
        if self.pv.key.as_ref() == Some(&key) {
            return;
        }
        self.pv.key = Some(key);
        match render::render_page(&f, &self.password, (self.page - 1) as u16, 2.0) {
            Ok((img, w, h)) => {
                let rgb = img.to_rgb8();
                let ci = egui::ColorImage::from_rgb([rgb.width() as usize, rgb.height() as usize], rgb.as_raw());
                self.pv.tex = Some(ctx.load_texture("page", ci, egui::TextureOptions::LINEAR));
                self.pv.size_pt = (w, h);
                self.pv.err = None;
            }
            Err(e) => {
                self.pv.tex = None;
                self.pv.err = Some(e.to_string());
            }
        }
    }

    // ───────── UI pieces ─────────

    fn file_section(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui.ctx());
        let input = self.tool.input();
        if input == Input::Html {
            ui.label("Alamat web (URL) atau file HTML lokal");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.html_src).desired_width((ui.available_width() - 170.0).max(200.0)).hint_text(theme::hint(ui.ctx(), "https://contoh.com  atau  C:\\path\\file.html")));
                if theme::button(ui, Kind::Tonal, Some(icons::FOLDER_OPEN), "Pilih file").clicked() {
                    if let Some(f) = rfd::FileDialog::new().add_filter("HTML", &["html", "htm"]).pick_file() {
                        self.html_src = f.display().to_string();
                        self.files = vec![f];
                    }
                }
            });
            return;
        }
        let multi = matches!(input, Input::PdfMany | Input::Images);
        let is_pdf = matches!(input, Input::PdfOne | Input::PdfMany | Input::PdfTwo);
        let (kind_label, kind_icon) = match input {
            Input::Images => ("JPG, PNG", icons::IMAGE),
            Input::Word => ("DOC, DOCX, RTF, ODT", icons::DESCRIPTION),
            Input::Ppt => ("PPT, PPTX, ODP", icons::SLIDESHOW),
            Input::Excel => ("XLS, XLSX, CSV, ODS", icons::TABLE_CHART),
            _ => ("PDF", icons::PICTURE_AS_PDF),
        };
        if self.files.is_empty() {
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 150.0), Sense::click());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let painter = ui.painter();
            painter.rect_filled(rect, Rounding::same(14.0), if resp.hovered() { theme::alpha(p.primary, 28) } else { p.sc_lowest });
            let r = rect.shrink(1.0);
            let st = Stroke::new(1.5, if resp.hovered() { p.primary } else { p.outline });
            for seg in [[r.left_top(), r.right_top()], [r.right_top(), r.right_bottom()], [r.right_bottom(), r.left_bottom()], [r.left_bottom(), r.left_top()]] {
                painter.extend(egui::Shape::dashed_line(&seg, st, 8.0, 6.0));
            }
            theme::paint_icon(painter, Pos2::new(rect.center().x, rect.top() + 48.0), icons::UPLOAD_FILE, 44.0, p.primary);
            painter.text(Pos2::new(rect.center().x, rect.top() + 88.0), Align2::CENTER_CENTER, if multi { "Seret file ke sini, atau klik untuk memilih" } else { "Seret file ke sini, atau klik untuk memilih" }, egui::FontId::proportional(15.5), p.on_surface);
            painter.text(Pos2::new(rect.center().x, rect.top() + 112.0), Align2::CENTER_CENTER, format!("Format: {kind_label}{}", if multi { " · bisa banyak file" } else { "" }), egui::FontId::proportional(13.0), p.on_surface_variant);
            if resp.clicked() {
                self.pick_input("");
            }
            if input == Input::Images {
                ui.add_space(4.0);
                if theme::button(ui, Kind::Outlined, Some(icons::DOCUMENT_SCANNER), "Pindai dari scanner").clicked() && !self.busy {
                    match convert::scan_with_wia() {
                        Ok(f) => {
                            self.files.push(f);
                            self.on_files_changed();
                        }
                        Err(e) => self.notify(format!("✖ {e}")),
                    }
                }
            }
        } else {
            let n = self.files.len();
            let mut remove = None;
            let mut swap = None;
            let mut select = None;
            egui::ScrollArea::vertical().max_height(236.0).auto_shrink([false, true]).id_salt("filelist").show(ui, |ui| {
                for (i, f) in self.files.iter().enumerate() {
                    let sel = i == self.active && n > 1;
                    let w = ui.available_width();
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 56.0), Sense::click());
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    {
                        let painter = ui.painter();
                        painter.rect_filled(rect, Rounding::same(12.0), if sel { p.secondary_container } else { p.sc_lowest });
                        if resp.hovered() {
                            painter.rect_filled(rect, Rounding::same(12.0), theme::alpha(p.on_surface, 14));
                        }
                        painter.circle_filled(Pos2::new(rect.left() + 32.0, rect.center().y), 20.0, if sel { p.primary } else { p.primary_container });
                        theme::paint_icon(painter, Pos2::new(rect.left() + 32.0, rect.center().y), kind_icon, 22.0, if sel { p.on_primary } else { p.on_primary_container });
                        let name = f.file_name().and_then(|s| s.to_str()).unwrap_or("?");
                        let sub = format!("{}{}", fmt_size(self.sizes.get(f).copied().unwrap_or(0)), f.parent().and_then(|d| d.file_name()).and_then(|d| d.to_str()).map(|d| format!("  ·  {d}")).unwrap_or_default());
                        let tr = Rect::from_min_max(Pos2::new(rect.left() + 64.0, rect.top()), Pos2::new(rect.right() - if multi { 124.0 } else { 52.0 }, rect.bottom()));
                        let pc = painter.with_clip_rect(tr);
                        pc.text(Pos2::new(tr.left(), rect.center().y - 9.0), Align2::LEFT_CENTER, format!("{}. {name}", i + 1), egui::FontId::proportional(14.5), p.on_surface);
                        pc.text(Pos2::new(tr.left(), rect.center().y + 10.0), Align2::LEFT_CENTER, sub, egui::FontId::proportional(12.0), p.on_surface_variant);
                    }
                    if resp.clicked() {
                        select = Some(i);
                    }
                    let by = rect.center().y - 16.0;
                    let mut x = rect.right() - 40.0;
                    let del = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(x, by), Vec2::splat(32.0))), |ui| theme::icon_button_sm(ui, icons::CLOSE, "Hapus dari daftar", true)).inner;
                    if del.clicked() {
                        remove = Some(i);
                    }
                    if multi {
                        x -= 36.0;
                        let dn = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(x, by), Vec2::splat(32.0))), |ui| theme::icon_button_sm(ui, icons::ARROW_DOWNWARD, "Geser ke bawah", i + 1 < n)).inner;
                        if dn.clicked() {
                            swap = Some((i, i + 1));
                        }
                        x -= 36.0;
                        let up = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(x, by), Vec2::splat(32.0))), |ui| theme::icon_button_sm(ui, icons::ARROW_UPWARD, "Geser ke atas", i > 0)).inner;
                        if up.clicked() {
                            swap = Some((i, i - 1));
                        }
                    }
                    ui.add_space(2.0);
                }
            });
            if let Some(i) = select {
                self.active = i;
                self.pv.key = None;
            }
            if let Some((a, b)) = swap {
                self.files.swap(a, b);
                self.active = b;
            }
            if let Some(i) = remove {
                self.files.remove(i);
                self.on_files_changed();
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if theme::button(ui, Kind::Tonal, Some(if multi { icons::ADD } else { icons::FOLDER_OPEN }), if multi { "Tambah file" } else { "Ganti file" }).clicked() {
                    self.pick_input("");
                }
                if input == Input::Images && theme::button(ui, Kind::Outlined, Some(icons::DOCUMENT_SCANNER), "Pindai").clicked() && !self.busy {
                    match convert::scan_with_wia() {
                        Ok(f) => {
                            self.files.push(f);
                            self.on_files_changed();
                        }
                        Err(e) => self.notify(format!("✖ {e}")),
                    }
                }
                if !self.files.is_empty() && theme::button(ui, Kind::Text, Some(icons::DELETE), "Kosongkan").clicked() {
                    self.files.clear();
                    self.on_files_changed();
                }
                if self.total > 0 {
                    ui.label(RichText::new(format!("{} file · {} halaman", n, self.total)).color(p.on_surface_variant));
                } else if n > 1 {
                    ui.label(RichText::new(format!("{n} file")).color(p.on_surface_variant));
                }
            });
        }
        if is_pdf {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                theme::icon(ui, icons::LOCK, 20.0, p.on_surface_variant);
                let r = ui.add(egui::TextEdit::singleline(&mut self.password).password(true).desired_width(220.0).hint_text(theme::hint(ui.ctx(), "Password PDF (jika ada)")));
                if r.lost_focus() && (r.changed() || ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.refresh_total();
                }
            });
        }
        if let Some(e) = self.pv.err.clone() {
            if self.total == 0 && !self.files.is_empty() {
                ui.add_space(4.0);
                theme::banner(ui, icons::ERROR, &e, BannerKind::Error);
            }
        }
    }





    fn pages_row(&mut self, ui: &mut egui::Ui, hint: &str) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Halaman:");
            ui.add(egui::TextEdit::singleline(&mut self.pages_spec).desired_width(200.0).hint_text(theme::hint(ui.ctx(), hint)));
            ui.weak("(kosong = semua) contoh: 1-3,5,8-");
        });
    }

    fn pick_image_row(ui: &mut egui::Ui, label: &str, slot: &mut Option<PathBuf>) {
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            if theme::button_sm(ui, Kind::Tonal, Some(icons::IMAGE), "Pilih gambar").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Gambar", &["png", "jpg", "jpeg"]).pick_file() {
                    *slot = Some(p);
                }
            }
            match slot {
                Some(p) => {
                    ui.monospace(p.file_name().and_then(|s| s.to_str()).unwrap_or("?"));
                    if theme::icon_button_sm(ui, icons::CLOSE, "Hapus", true).clicked() {
                        *slot = None;
                    }
                }
                None => {
                    ui.weak("belum dipilih");
                }
            }
        });
    }

    fn sel_info(&self, ui: &mut egui::Ui) {
        match (self.sel, self.pt) {
            (Some(r), _) => {
                ui.weak(format!("Seleksi di preview (hal {}): x {:.0}–{:.0}, y {:.0}–{:.0} pt", self.page, r[0], r[2], r[1], r[3]));
            }
            (None, Some(p)) => {
                ui.weak(format!("Titik terpilih (hal {}): x {:.0}, y {:.0} pt", self.page, p.0, p.1));
            }
            _ => {
                ui.weak("Klik atau seret di preview halaman (kanan) untuk memilih posisi/area.");
            }
        }
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t = self.tool;
        match t {
            Tool::Merge | Tool::Repair | Tool::Unlock | Tool::PdfToWord | Tool::PdfToExcel | Tool::PdfToMarkdown => {}
            Tool::Split => {
                ui.horizontal_wrapped(|ui| {
                    theme::choice(ui, &mut self.split_mode, 0, "Rentang");
                    theme::choice(ui, &mut self.split_mode, 1, "Setiap N halaman");
                    theme::choice(ui, &mut self.split_mode, 2, "Satu halaman per file");
                });
                match self.split_mode {
                    0 => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Rentang (pisahkan dengan koma, tiap rentang = 1 file):");
                            ui.add(egui::TextEdit::singleline(&mut self.split_spec).desired_width(180.0));
                        });
                    }
                    1 => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("N =");
                            ui.add(egui::DragValue::new(&mut self.split_n).range(1..=10_000));
                        });
                    }
                    _ => {}
                }
            }
            Tool::RemovePages => self.pages_row(ui, "2,5-7"),
            Tool::ExtractPages => self.pages_row(ui, "1-3,8"),
            Tool::Organize => {
                ui.label("Urutan halaman baru (boleh duplikat / dikurangi):");
                ui.add(egui::TextEdit::singleline(&mut self.organize_order).desired_width(420.0));
                ui.horizontal_wrapped(|ui| {
                    if theme::button_sm(ui, Kind::Tonal, Some(icons::REFRESH), "Reset").clicked() {
                        self.organize_order = (1..=self.total).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
                    }
                    if theme::button_sm(ui, Kind::Tonal, Some(icons::ARROW_DOWNWARD), "Balik urutan").clicked() {
                        self.organize_order = (1..=self.total).rev().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
                    }
                });
            }
            Tool::ScanToPdf | Tool::JpgToPdf => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Ukuran halaman:");
                    theme::choice(ui, &mut self.img_mode, 0, "Sesuai gambar");
                    theme::choice(ui, &mut self.img_mode, 1, "A4 tegak");
                    theme::choice(ui, &mut self.img_mode, 2, "A4 mendatar");
                });
                if self.img_mode != 0 {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Margin (pt):");
                        ui.add(egui::DragValue::new(&mut self.img_margin).range(0.0..=100.0));
                    });
                }
                if t == Tool::ScanToPdf {
                    self.img_enhance = true;
                    ui.weak("Mode pindaian aktif: abu-abu + kontras otomatis.");
                } else {
                    theme::switch(ui, &mut self.img_enhance, "Perbaiki kontras gaya scan (abu-abu)");
                }
            }
            Tool::Compress => {
                ui.horizontal_wrapped(|ui| {
                    theme::choice(ui, &mut self.compress_level, 0, "Ringan (kualitas terjaga)");
                    theme::choice(ui, &mut self.compress_level, 1, "Sedang");
                    theme::choice(ui, &mut self.compress_level, 2, "Maksimal (gambar diperkecil)");
                });
            }
            Tool::Ocr => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bahasa Tesseract:");
                    ui.add(egui::TextEdit::singleline(&mut self.ocr_lang).desired_width(110.0));
                    ui.weak("mis. eng, ind, eng+ind");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("DPI render:");
                    ui.add(egui::DragValue::new(&mut self.ocr_dpi).range(100.0..=600.0));
                });
                if render::find_tesseract(&self.tesseract_path).is_none() {
                    theme::banner(ui, icons::WARNING, "Tesseract belum ditemukan. Pasang Tesseract OCR (installer UB-Mannheim) atau isi path-nya di Pengaturan.", BannerKind::Warn);
                }
            }
            Tool::WordToPdf | Tool::PowerpointToPdf | Tool::ExcelToPdf => {
                ui.weak("Memakai Microsoft Office jika terpasang, jika tidak LibreOffice.");
            }
            Tool::HtmlToPdf => {}
            Tool::PdfToJpg => {
                self.pages_row(ui, "semua");
                ui.horizontal_wrapped(|ui| {
                    theme::choice(ui, &mut self.jpg_png, false, "JPG");
                    theme::choice(ui, &mut self.jpg_png, true, "PNG");
                    ui.label("DPI:");
                    ui.add(egui::DragValue::new(&mut self.jpg_dpi).range(36.0..=600.0));
                    if !self.jpg_png {
                        ui.label("Kualitas:");
                        ui.add(egui::DragValue::new(&mut self.jpg_quality).range(10..=100));
                    }
                });
            }
            Tool::PdfToPowerpoint => {
                ui.horizontal_wrapped(|ui| {
                    theme::choice(ui, &mut self.ppt_editable, false, "Gambar per halaman (tampilan identik)");
                    theme::choice(ui, &mut self.ppt_editable, true, "Teks yang bisa diedit");
                });
                if !self.ppt_editable {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("DPI gambar:");
                        ui.add(egui::DragValue::new(&mut self.ppt_dpi).range(72.0..=300.0));
                    });
                }
            }
            Tool::PdfToPdfa => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Level PDF/A:");
                    theme::choice(ui, &mut self.pdfa_level, 1, "1");
                    theme::choice(ui, &mut self.pdfa_level, 2, "2");
                    theme::choice(ui, &mut self.pdfa_level, 3, "3");
                });
                ui.weak("Memakai Ghostscript (winget install ArtifexSoftware.GhostScript). Validasi hasilnya dengan veraPDF bila untuk arsip resmi.");
            }
            Tool::Rotate => {
                self.pages_row(ui, "semua");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Putar:");
                    theme::choice(ui, &mut self.rotate_deg, 90, "90° ⟳");
                    theme::choice(ui, &mut self.rotate_deg, 180, "180°");
                    theme::choice(ui, &mut self.rotate_deg, 270, "90° ⟲");
                });
            }
            Tool::AddPageNumbers => {
                self.pages_row(ui, "semua");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Format:");
                    ui.add(egui::TextEdit::singleline(&mut self.pn_format).desired_width(200.0));
                    ui.weak("{n} = nomor, {total} = jumlah");
                });
                ui.label("Posisi nomor:");
                ui.horizontal_wrapped(|ui| {
                    for p in [Pos::TopLeft, Pos::TopCenter, Pos::TopRight, Pos::BottomLeft, Pos::BottomCenter, Pos::BottomRight] {
                        theme::choice(ui, &mut self.pn_pos, p, pos_label(p));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Ukuran:");
                    ui.add(egui::DragValue::new(&mut self.pn_size).range(6.0..=48.0));
                    ui.label("Margin:");
                    ui.add(egui::DragValue::new(&mut self.pn_margin).range(0.0..=200.0));
                    ui.label("Mulai dari:");
                    ui.add(egui::DragValue::new(&mut self.pn_start).range(0..=9999));
                });
            }
            Tool::AddWatermark => {
                self.pages_row(ui, "semua");
                ui.horizontal_wrapped(|ui| {
                    theme::choice(ui, &mut self.wm_image, false, "Teks");
                    theme::choice(ui, &mut self.wm_image, true, "Gambar");
                });
                if self.wm_image {
                    Self::pick_image_row(ui, "Gambar:", &mut self.wm_img);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Lebar relatif:");
                        ui.add(egui::Slider::new(&mut self.wm_scale, 0.1..=1.0));
                    });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Teks:");
                        ui.add(egui::TextEdit::singleline(&mut self.wm_text).desired_width(220.0));
                        ui.label("Ukuran:");
                        ui.add(egui::DragValue::new(&mut self.wm_size).range(8.0..=300.0));
                        ui.color_edit_button_srgb(&mut self.wm_color);
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Transparansi:");
                    ui.add(egui::Slider::new(&mut self.wm_opacity, 0.05..=1.0));
                    ui.label("Sudut:");
                    ui.add(egui::DragValue::new(&mut self.wm_angle).range(-180.0..=180.0));
                });
            }
            Tool::Crop => {
                self.pages_row(ui, "semua");
                ui.label("Margin yang dibuang (pt, dihitung dari tampilan):");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Kiri");
                    ui.add(egui::DragValue::new(&mut self.crop[0]).range(0.0..=2000.0));
                    ui.label("Bawah");
                    ui.add(egui::DragValue::new(&mut self.crop[1]).range(0.0..=2000.0));
                    ui.label("Kanan");
                    ui.add(egui::DragValue::new(&mut self.crop[2]).range(0.0..=2000.0));
                    ui.label("Atas");
                    ui.add(egui::DragValue::new(&mut self.crop[3]).range(0.0..=2000.0));
                });
                self.sel_info(ui);
                if theme::button_en(ui, Kind::Tonal, Some(icons::CROP), "Pakai seleksi sebagai area yang dipertahankan", self.sel.is_some()).clicked() {
                    if let Some(r) = self.sel {
                        let (w, h) = self.pv.size_pt;
                        self.crop = [r[0], r[1], (w - r[2]).max(0.0), (h - r[3]).max(0.0)];
                    }
                }
            }
            Tool::EditPdf => self.edit_ui(ui),
            Tool::PdfForms => self.forms_ui(ui),
            Tool::Protect => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Password buka:");
                    ui.add(egui::TextEdit::singleline(&mut self.prot_user).password(true).desired_width(160.0));
                    ui.label("Password pemilik:");
                    ui.add(egui::TextEdit::singleline(&mut self.prot_owner).password(true).desired_width(160.0));
                });
                ui.horizontal_wrapped(|ui| {
                    theme::switch(ui, &mut self.prot_print, "Boleh cetak");
                    theme::switch(ui, &mut self.prot_copy, "Boleh salin teks");
                    theme::switch(ui, &mut self.prot_modify, "Boleh ubah");
                    theme::switch(ui, &mut self.prot_annot, "Boleh anotasi/isi form");
                });
                ui.weak("Enkripsi AES-128. Jangan lupa password — tidak ada cara memulihkannya.");
            }
            Tool::Sign => {
                Self::pick_image_row(ui, "Gambar tanda tangan (opsional, PNG transparan terbaik):", &mut self.sign_img);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Nama:");
                    ui.add(egui::TextEdit::singleline(&mut self.sign_name).desired_width(180.0));
                    ui.label("Alasan:");
                    ui.add(egui::TextEdit::singleline(&mut self.sign_reason).desired_width(180.0));
                });
                ui.horizontal_wrapped(|ui| {
                    theme::switch(ui, &mut self.sign_date, "Sertakan tanggal");
                    theme::switch(ui, &mut self.sign_border, "Bingkai");
                });
                self.sel_info(ui);
                ui.weak("Seret kotak di preview pada halaman tujuan sebagai tempat tanda tangan.");
            }
            Tool::Redact => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Sensor teks (pisahkan dengan ;):");
                    ui.add(egui::TextEdit::singleline(&mut self.redact_terms).desired_width(260.0).hint_text(theme::hint(ui.ctx(), "mis. 12345; rahasia")));
                });
                self.sel_info(ui);
                ui.horizontal_wrapped(|ui| {
                    if theme::button_en(ui, Kind::Tonal, Some(icons::ADD), "Tambah area dari seleksi", self.sel.is_some()).clicked() {
                        if let Some(r) = self.sel.take() {
                            self.redact_rects.push((self.page, r));
                        }
                    }
                    ui.label("DPI:");
                    ui.add(egui::DragValue::new(&mut self.redact_dpi).range(100.0..=300.0));
                });
                let mut rm = None;
                for (i, (p, r)) in self.redact_rects.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Hal {p}: {:.0},{:.0} – {:.0},{:.0}", r[0], r[1], r[2], r[3]));
                        if theme::icon_button_sm(ui, icons::CLOSE, "Hapus", true).clicked() {
                            rm = Some(i);
                        }
                    });
                }
                if let Some(i) = rm {
                    self.redact_rects.remove(i);
                }
                theme::banner(ui, icons::WARNING, "Halaman yang disensor menjadi gambar (teks asli dihapus permanen). Simpan sebagai file baru.", BannerKind::Warn);
            }
            Tool::Compare => self.compare_ui(ui, ctx),
            Tool::Summarizer => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bahasa ringkasan:");
                    ui.add(egui::TextEdit::singleline(&mut self.ai_lang).desired_width(160.0));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Gaya:");
                    theme::choice(ui, &mut self.ai_style, 0, "Singkat");
                    theme::choice(ui, &mut self.ai_style, 1, "Poin-poin");
                    theme::choice(ui, &mut self.ai_style, 2, "Detail per bagian");
                });
                self.api_note(ui);
            }
            Tool::Translate => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Terjemahkan ke:");
                    ui.add(egui::TextEdit::singleline(&mut self.tr_target).desired_width(160.0));
                    ui.label("Format:");
                    for (i, n) in ["DOCX", "Markdown", "TXT"].iter().enumerate() {
                        if theme::choice(ui, &mut self.tr_fmt, i, *n).clicked() {
                            self.out_override = None;
                        }
                    }
                });
                self.api_note(ui);
            }
        }
    }

    fn tool_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = theme::pal(ctx);
        let t = self.tool;
        ui.horizontal_top(|ui| {
            theme::icon_badge(ui, t.icon(), 56.0, p.primary_container, p.on_primary_container);
            ui.add_space(4.0);
            let w = (ui.available_width() - 64.0).max(180.0);
            ui.allocate_ui_with_layout(Vec2::new(w, 0.0), Layout::top_down(Align::LEFT), |ui| {
                ui.label(theme::headline(t.label()));
                ui.add(egui::Label::new(RichText::new(t.desc()).color(p.on_surface_variant)).wrap());
            });
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                if theme::icon_button_ex(ui, icons::CLOSE, "Tutup dan kembali ke Beranda (Esc)", true, 44.0, Some(p.on_surface)).clicked() {
                    self.home = true;
                }
            });
        });
        ui.add_space(10.0);
        theme::step_header(ui, 1, if t == Tool::Compare { "Dokumen pertama" } else { "Pilih file" });
        theme::card(ui, CardKind::Filled, |ui| self.file_section(ui));
        ui.add_space(10.0);
        if t.has_options() {
            theme::step_header(ui, 2, match t {
                Tool::Compare => "Bandingkan dengan dokumen kedua",
                Tool::Summarizer | Tool::Translate => "Pengaturan AI",
                _ => "Atur opsi",
            });
            theme::card(ui, CardKind::Filled, |ui| self.options_ui(ui, ctx));
        } else {
            match t {
                Tool::WordToPdf | Tool::PowerpointToPdf | Tool::ExcelToPdf => theme::banner(ui, icons::INFO, "Memakai Microsoft Office jika terpasang, jika tidak memakai LibreOffice.", BannerKind::Info),
                Tool::Unlock => theme::banner(ui, icons::INFO, "Isi password PDF pada kolom password di atas, lalu tekan Proses.", BannerKind::Info),
                Tool::PdfToWord | Tool::PdfToExcel | Tool::PdfToMarkdown => theme::banner(ui, icons::INFO, "PDF hasil scan (berupa gambar) perlu di-OCR dulu agar teksnya terbaca.", BannerKind::Info),
                _ => {}
            }
        }
        self.result_ui(ui);
        ui.add_space(24.0);
    }

    /// Bilah bawah tetap: lokasi simpan + tombol aksi utama (selalu terlihat).
    fn run_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = theme::pal(ctx);
        if self.busy {
            theme::progress_indeterminate(ui, ui.available_width());
        } else {
            ui.add_space(4.0);
        }
        let label = match self.tool {
            Tool::Summarizer => "Ringkas",
            Tool::Translate => "Terjemahkan",
            Tool::PdfForms => "Isi form & simpan",
            _ => "Proses",
        };
        let out = self.out_path();
        ui.horizontal(|ui| {
            let fab_w = 190.0;
            let left_w = (ui.available_width() - fab_w - 12.0).max(120.0);
            ui.allocate_ui_with_layout(Vec2::new(left_w, 56.0), Layout::left_to_right(Align::Center), |ui| {
                if let Some(path) = &out {
                    theme::icon_badge(ui, if self.tool.is_folder_output() { icons::FOLDER_OPEN } else { icons::SAVE }, 40.0, p.secondary_container, p.on_secondary_container);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(theme::label_small(if self.tool.is_folder_output() { "Folder hasil" } else { "Simpan ke" }).color(p.on_surface_variant));
                        ui.add(egui::Label::new(RichText::new(path.display().to_string()).size(13.0)).truncate());
                    });
                    if theme::button_sm(ui, Kind::Text, None, "Ubah").clicked() {
                        if self.tool.is_folder_output() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                self.out_override = Some(d);
                            }
                        } else {
                            let (_, ext) = self.tool.out_spec();
                            let ext = if self.tool == Tool::Translate { ["docx", "md", "txt"][self.tr_fmt.min(2)] } else { ext };
                            let mut d = rfd::FileDialog::new().add_filter(ext, &[ext]);
                            if let Some(n) = path.file_name().and_then(|s| s.to_str()) {
                                d = d.set_file_name(n);
                            }
                            if let Some(f) = d.save_file() {
                                self.out_override = Some(f);
                            }
                        }
                    }
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::fab(ui, if self.busy { icons::REFRESH } else { icons::PLAY_ARROW }, if self.busy { "Memproses…" } else { label }, !self.busy).clicked() {
                    self.run(ctx);
                }
            });
        });
        ui.add_space(6.0);
    }

    fn api_note(&self, ui: &mut egui::Ui) {
        if self.api_key.trim().is_empty() {
            theme::banner(ui, icons::WARNING, "Isi API key Anthropic di Pengaturan (ikon roda gigi) atau lewat variabel lingkungan ANTHROPIC_API_KEY.", BannerKind::Warn);
        } else {
            ui.weak("Teks dokumen dikirim ke API Anthropic untuk diproses.");
        }
    }

    fn edit_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (i, n) in ["Teks", "Kotak putih (whiteout)", "Stabilo", "Gambar"].iter().enumerate() {
                theme::choice(ui, &mut self.edit_action, i, *n);
            }
        });
        if self.edit_action == 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label("Teks:");
                ui.add(egui::TextEdit::multiline(&mut self.edit_text).desired_rows(2).desired_width(300.0));
                ui.label("Ukuran:");
                ui.add(egui::DragValue::new(&mut self.edit_size).range(5.0..=120.0));
                ui.color_edit_button_srgb(&mut self.edit_color);
            });
        }
        if self.edit_action == 3 {
            Self::pick_image_row(ui, "Gambar:", &mut self.edit_img);
        }
        self.sel_info(ui);
        let page = self.page;
        let can = match self.edit_action {
            0 => self.pt.is_some() && !self.edit_text.trim().is_empty(),
            3 => self.sel.is_some() && self.edit_img.is_some(),
            _ => self.sel.is_some(),
        };
        if theme::button_en(ui, Kind::Tonal, Some(icons::ADD), &format!("Tambahkan ke halaman {page}"), can).clicked() {
            let item = match self.edit_action {
                0 => self.pt.map(|(x, y)| EditItem::Text { page, x, y, size: self.edit_size, color: self.edit_color, text: self.edit_text.clone() }),
                1 => self.sel.map(|r| EditItem::Rect { page, rect: r, color: [255, 255, 255], opacity: 1.0, multiply: false }),
                2 => self.sel.map(|r| EditItem::Rect { page, rect: r, color: [255, 235, 59], opacity: 1.0, multiply: true }),
                _ => self.sel.zip(self.edit_img.clone()).map(|(r, p)| EditItem::Image { page, rect: r, path: p }),
            };
            if let Some(i) = item {
                self.edit_items.push(i);
                self.sel = None;
            }
        }
        let mut rm = None;
        for (i, it) in self.edit_items.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(it.describe());
                if theme::icon_button_sm(ui, icons::CLOSE, "Hapus", true).clicked() {
                    rm = Some(i);
                }
            });
        }
        if let Some(i) = rm {
            self.edit_items.remove(i);
        }
        ui.weak("Mengubah teks yang sudah ada: tutup dengan kotak putih lalu tulis teks baru di atasnya.");
    }

    fn forms_ui(&mut self, ui: &mut egui::Ui) {
        if theme::button(ui, Kind::Tonal, Some(icons::SEARCH), "Baca kolom form").clicked() {
            if let Some(f) = self.files.get(self.active) {
                match pdfops::read_fields(f, &self.password) {
                    Ok(v) => {
                        self.form_vals = v
                            .iter()
                            .map(|f| match f.kind {
                                FieldKind::Checkbox | FieldKind::Radio => (!matches!(f.value.as_str(), "" | "Off")).to_string(),
                                _ => f.value.clone(),
                            })
                            .collect();
                        self.notify(format!("{} kolom form ditemukan", v.len()));
                        self.forms = v;
                    }
                    Err(e) => self.notify(format!("✖ {e}")),
                }
            }
        }
        if self.forms.is_empty() {
            ui.weak("Belum ada kolom yang dimuat (atau PDF tidak memiliki formulir interaktif).");
            return;
        }
        egui::Grid::new("forms").num_columns(3).striped(true).show(ui, |ui| {
            for (i, f) in self.forms.iter().enumerate() {
                ui.label(&f.name);
                ui.weak(format!("{:?}", f.kind));
                match f.kind {
                    FieldKind::Checkbox | FieldKind::Radio => {
                        let mut b = self.form_vals[i] == "true";
                        if theme::switch(ui, &mut b, "").changed() {
                            self.form_vals[i] = b.to_string();
                        }
                    }
                    FieldKind::Choice if !f.options.is_empty() => {
                        egui::ComboBox::from_id_salt(("ch", i)).selected_text(self.form_vals[i].clone()).show_ui(ui, |ui| {
                            for o in &f.options {
                                ui.selectable_value(&mut self.form_vals[i], o.clone(), o);
                            }
                        });
                    }
                    FieldKind::Text | FieldKind::Choice => {
                        ui.add(egui::TextEdit::singleline(&mut self.form_vals[i]).desired_width(260.0));
                    }
                    _ => {
                        ui.weak("(tidak dapat diisi)");
                    }
                }
                ui.end_row();
            }
        });
        theme::switch(ui, &mut self.form_lock, "Kunci kolom setelah diisi (read-only)");
    }

    fn compare_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let pp = theme::pal(ctx);
        ui.horizontal_wrapped(|ui| {
            ui.label("Dokumen B:");
            if theme::button(ui, Kind::Tonal, Some(icons::FOLDER_OPEN), "Pilih PDF B").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                    self.cmp_b = Some(p);
                }
            }
            match &self.cmp_b {
                Some(p) => {
                    ui.monospace(p.file_name().and_then(|s| s.to_str()).unwrap_or("?"));
                }
                None => {
                    ui.weak("belum dipilih");
                }
            }
            ui.label("Password B:");
            ui.add(egui::TextEdit::singleline(&mut self.cmp_pw).password(true).desired_width(100.0));
        });
        ui.label("Dokumen A = file yang dipilih di atas.");
        let ready = !self.busy && !self.files.is_empty() && self.cmp_b.is_some();
        ui.horizontal_wrapped(|ui| {
            if theme::button_en(ui, Kind::Filled, Some(icons::COMPARE), "Bandingkan", ready).clicked() {
                let (a, b, pa, pb) = (self.files[0].clone(), self.cmp_b.clone().unwrap(), self.password.clone(), self.cmp_pw.clone());
                self.spawn(ctx, move |progress| {
                    let r = render::compare(&a, &b, &pa, &pb, progress)?;
                    let changed = r.lines.iter().filter(|l| l.tag != '=').count();
                    Ok(TaskOutput { msg: format!("Selesai: {changed} baris berbeda"), compare: Some(r), ..Default::default() })
                });
            }
            if self.busy {
                ui.spinner();
            }
        });
        if let Some(r) = &self.cmp_result {
            ui.separator();
            ui.label(format!("A: {} halaman · B: {} halaman", r.pages_a, r.pages_b));
            if !r.visual.is_empty() {
                ui.label("Perbedaan visual per halaman:");
                ui.horizontal_wrapped(|ui| {
                    for (p, pct) in &r.visual {
                        let c = if *pct < 0.05 { pp.success } else { pp.error };
                        ui.colored_label(c, format!("Hal {p}: {pct:.2}%"));
                    }
                });
            }
            theme::switch(ui, &mut self.cmp_show_equal, "Tampilkan baris yang sama");
            let show_eq = self.cmp_show_equal;
            let mut report = String::new();
            for l in &r.lines {
                report.push_str(&format!("{} [hal {}] {}\n", l.tag, l.page, l.text));
            }
            egui::ScrollArea::vertical().max_height(320.0).id_salt("cmpscroll").show(ui, |ui| {
                for l in &r.lines {
                    if l.tag == '=' && !show_eq {
                        continue;
                    }
                    let (c, pre) = match l.tag {
                        '+' => (pp.success, "+ "),
                        '-' => (pp.error, "− "),
                        _ => (pp.on_surface_variant, "  "),
                    };
                    ui.label(RichText::new(format!("{pre}[{}] {}", l.page, l.text)).color(c).monospace());
                }
            });
            ui.horizontal_wrapped(|ui| {
                if theme::button_sm(ui, Kind::Tonal, Some(icons::CONTENT_COPY), "Salin laporan").clicked() {
                    ctx.copy_text(report.clone());
                }
                if theme::button_sm(ui, Kind::Tonal, Some(icons::SAVE), "Simpan laporan .txt").clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_file_name("perbandingan.txt").save_file() {
                        let _ = std::fs::write(p, report);
                    }
                }
            });
            ui.weak("＋ hijau = hanya ada di B · − merah = hanya ada di A");
        }
    }

    fn result_ui(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui.ctx());
        if !self.outputs.is_empty() {
            ui.add_space(6.0);
            let outs = self.outputs.clone();
            theme::card(ui, CardKind::Success, |ui| {
                ui.horizontal(|ui| {
                    theme::icon(ui, icons::CHECK_CIRCLE, 26.0, p.on_success_container);
                    ui.label(RichText::new("Selesai").size(17.0).strong().color(p.on_success_container));
                });
                for o in &outs {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.add(egui::Label::new(RichText::new(o.display().to_string()).monospace().color(p.on_success_container)).truncate());
                    });
                    ui.horizontal_wrapped(|ui| {
                        if theme::button_sm(ui, Kind::Filled, Some(icons::OPEN_IN_NEW), "Buka").clicked() {
                            util::open_in_shell(o);
                        }
                        if theme::button_sm(ui, Kind::Outlined, Some(icons::FOLDER_OPEN), "Tampilkan di folder").clicked() {
                            util::reveal_in_shell(o);
                        }
                        if o.extension().and_then(|e| e.to_str()) == Some("pdf") && theme::button_sm(ui, Kind::Outlined, Some(icons::REFRESH), "Pakai sebagai input").clicked() {
                            self.files = vec![o.clone()];
                            self.out_override = None;
                            self.on_files_changed();
                        }
                    });
                }
            });
        }
        if !self.result_text.is_empty() {
            ui.add_space(10.0);
            theme::card(ui, CardKind::Filled, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::title("Hasil teks"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::icon_button_sm(ui, icons::CONTENT_COPY, "Salin", true).clicked() {
                            ui.ctx().copy_text(self.result_text.clone());
                        }
                    });
                });
                egui::ScrollArea::vertical().max_height(320.0).id_salt("resscroll").show(ui, |ui| {
                    let mut s: &str = &self.result_text;
                    ui.add(egui::TextEdit::multiline(&mut s).desired_width(f32::INFINITY).desired_rows(8));
                });
            });
        }
    }

    // ───────── preview ─────────

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui.ctx());
        ui.horizontal(|ui| {
            if theme::icon_button_ex(ui, icons::CHEVRON_LEFT, "Halaman sebelumnya", self.page > 1, 36.0, None).clicked() {
                self.page -= 1;
                self.sel = None;
            }
            let mut pg = self.page;
            ui.add(egui::DragValue::new(&mut pg).range(1..=self.total.max(1)));
            if pg != self.page {
                self.page = pg;
                self.sel = None;
            }
            ui.label(format!("dari {}", self.total));
            if theme::icon_button_ex(ui, icons::CHEVRON_RIGHT, "Halaman berikutnya", self.page < self.total, 36.0, None).clicked() {
                self.page += 1;
                self.sel = None;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button_ex(ui, icons::ZOOM_IN, "Perbesar", true, 36.0, None).clicked() {
                    self.zoom = (self.zoom * 1.15).min(2.5);
                }
                ui.label(format!("{:.0}%", self.zoom * 100.0));
                if theme::icon_button_ex(ui, icons::ZOOM_OUT, "Perkecil", true, 36.0, None).clicked() {
                    self.zoom = (self.zoom / 1.15).max(0.3);
                }
            });
        });
        if let Some(e) = self.pv.err.clone() {
            theme::banner(ui, icons::ERROR, &e, BannerKind::Error);
            return;
        }
        let Some(tex) = self.pv.tex.clone() else {
            ui.weak("Pratinjau tidak tersedia");
            return;
        };
        let (wpt, hpt) = self.pv.size_pt;
        let z = self.zoom * 1.1;
        egui::Frame::none().fill(p.sc_high).rounding(Rounding::same(16.0)).inner_margin(Margin::same(8.0)).show(ui, |ui| {
        egui::ScrollArea::both().drag_to_scroll(false).auto_shrink([false, false]).id_salt("pvscroll").show(ui, |ui| {
            let size = Vec2::new(wpt * z, hpt * z);
            let resp = ui.add(egui::Image::new(SizedTexture::new(tex.id(), size)).sense(Sense::click_and_drag()));
            let rect = resp.rect;
            let to_pt = |p: Pos2| ((p.x - rect.left()) / z, hpt - (p.y - rect.top()) / z);
            let to_px = |x: f32, y: f32| Pos2::new(rect.left() + x * z, rect.top() + (hpt - y) * z);
            if resp.drag_started() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.drag_start = Some(to_pt(p));
                    self.pt = self.drag_start;
                }
            }
            if resp.dragged() {
                if let (Some(s), Some(p)) = (self.drag_start, resp.interact_pointer_pos()) {
                    let e = to_pt(p);
                    self.sel = Some([s.0.min(e.0).clamp(0.0, wpt), s.1.min(e.1).clamp(0.0, hpt), s.0.max(e.0).clamp(0.0, wpt), s.1.max(e.1).clamp(0.0, hpt)]);
                }
            }
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.pt = Some(to_pt(p));
                    self.sel = None;
                }
            }
            let painter = ui.painter_at(rect);
            let draw_rect = |r: [f32; 4], fill: Color32, stroke: Color32| {
                let rr = Rect::from_two_pos(to_px(r[0], r[1]), to_px(r[2], r[3]));
                painter.rect_filled(rr, 0.0, fill);
                painter.rect_stroke(rr, 0.0, Stroke::new(1.0, stroke));
            };
            if let Some(r) = self.sel {
                draw_rect(r, theme::alpha(p.primary, 50), p.primary);
            }
            if let Some((x, y)) = self.pt {
                if self.sel.is_none() {
                    painter.circle_filled(to_px(x, y), 5.0, p.error);
                }
            }
            for (p, r) in &self.redact_rects {
                if *p == self.page {
                    draw_rect(*r, Color32::from_rgba_unmultiplied(0, 0, 0, 160), Color32::BLACK);
                }
            }
            for it in &self.edit_items {
                if it.page() != self.page {
                    continue;
                }
                match it {
                    EditItem::Rect { rect, .. } | EditItem::Image { rect, .. } => {
                        draw_rect(*rect, Color32::from_rgba_unmultiplied(255, 200, 0, 60), Color32::from_rgb(255, 160, 0))
                    }
                    EditItem::Text { x, y, .. } => {
                        painter.circle_stroke(to_px(*x, *y), 5.0, Stroke::new(1.5, Color32::from_rgb(255, 160, 0)));
                    }
                }
            }
        });
        });
    }
    // ───────── tugas ─────────

    fn run(&mut self, ctx: &egui::Context) {
        let tool = self.tool;
        let pw = self.password.clone();
        let input = self.files.first().cloned();
        let all = self.files.clone();
        let out = self.out_path();
        let total = self.total;
        let spec = self.pages_spec.clone();
        macro_rules! need {
            ($e:expr, $m:expr) => {
                match $e {
                    Some(v) => v,
                    None => {
                        self.notify(format!("✖ {}", $m));
                        return;
                    }
                }
            };
        }
        if tool != Tool::HtmlToPdf {
            need!(input.as_ref(), "Pilih file terlebih dahulu");
        }
        let out = need!(out, "Tentukan lokasi keluaran");
        let one = |m: &str, p: &Path| TaskOutput { msg: m.to_string(), outputs: vec![p.to_path_buf()], ..Default::default() };
        let pages = |s: &str, t: u32| pdfops::parse_ranges(s, t);
        use Tool::*;
        match tool {
            Merge => {
                if all.len() < 2 {
                    self.notify("✖ Tambahkan minimal 2 PDF".into());
                    return;
                }
                self.spawn(ctx, move |_| {
                    pdfops::merge(&all, &out)?;
                    Ok(one(&format!("{} file digabung", all.len()), &out))
                });
            }
            Split => {
                let mode = match self.split_mode {
                    0 => SplitMode::Ranges(self.split_spec.clone()),
                    1 => SplitMode::EveryN(self.split_n),
                    _ => SplitMode::EachPage,
                };
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let v = pdfops::split(&input, &mode, &out, &pw)?;
                    Ok(TaskOutput { msg: format!("{} file dibuat di {}", v.len(), out.display()), outputs: vec![out.clone()], ..Default::default() })
                });
            }
            RemovePages | ExtractPages => {
                if spec.trim().is_empty() {
                    self.notify("✖ Isi nomor halaman".into());
                    return;
                }
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let p = pages(&spec, total)?;
                    if tool == RemovePages {
                        pdfops::remove_pages(&input, &p, &out, &pw)?;
                    } else {
                        let mut s = p.clone();
                        s.sort_unstable();
                        pdfops::extract_pages(&input, &s, &out, &pw)?;
                    }
                    Ok(one("Selesai", &out))
                });
            }
            Organize => {
                let order = self.organize_order.clone();
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let o = pages(&order, total)?;
                    pdfops::organize(&input, &o, &out, &pw)?;
                    Ok(one(&format!("PDF baru berisi {} halaman", o.len()), &out))
                });
            }
            ScanToPdf | JpgToPdf => {
                let mode = [PageMode::Fit, PageMode::A4Portrait, PageMode::A4Landscape][self.img_mode.min(2)];
                let o = ImgPdfOpts { mode, margin: self.img_margin, enhance: self.img_enhance || tool == ScanToPdf };
                self.spawn(ctx, move |_| {
                    convert::images_to_pdf(&all, &o, &out)?;
                    Ok(one(&format!("{} gambar dikonversi", all.len()), &out))
                });
            }
            Compress => {
                let (q, s) = [(Some(85u8), 1.0f32), (Some(70), 0.85), (Some(55), 0.6)][self.compress_level.min(2)];
                let o = CompressOpts { jpeg_quality: q, image_scale: s };
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let (a, b) = pdfops::compress(&input, &out, &o, &pw)?;
                    let pct = if a > 0 { 100.0 - b as f64 / a as f64 * 100.0 } else { 0.0 };
                    Ok(one(&format!("{} → {} (hemat {pct:.0}%)", fmt_size(a), fmt_size(b)), &out))
                });
            }
            Repair => {
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let m = pdfops::repair(&input, &out)?;
                    Ok(one(&m, &out))
                });
            }
            Ocr => {
                let (lang, tess, dpi) = (self.ocr_lang.clone(), self.tesseract_path.clone(), self.ocr_dpi);
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    let n = render::ocr_pdf(&input, &out, &pw, &lang, &tess, dpi, progress)?;
                    Ok(one(&format!("OCR selesai ({n} halaman)"), &out))
                });
            }
            WordToPdf | PowerpointToPdf | ExcelToPdf => {
                let kind = match tool {
                    WordToPdf => OfficeKind::Word,
                    PowerpointToPdf => OfficeKind::PowerPoint,
                    _ => OfficeKind::Excel,
                };
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    progress("Mengonversi (membuka aplikasi Office/LibreOffice)…");
                    let via = convert::office_to_pdf(&input, &out, kind)?;
                    Ok(one(&format!("Selesai via {via}"), &out))
                });
            }
            HtmlToPdf => {
                let src = if self.html_src.trim().is_empty() { input.map(|p| p.display().to_string()).unwrap_or_default() } else { self.html_src.trim().to_string() };
                if src.is_empty() {
                    self.notify("✖ Isi URL atau pilih file HTML".into());
                    return;
                }
                self.spawn(ctx, move |progress| {
                    progress("Merender halaman web…");
                    convert::html_to_pdf(&src, &out)?;
                    Ok(one("Selesai", &out))
                });
            }
            PdfToJpg => {
                let (dpi, png, q) = (self.jpg_dpi, self.jpg_png, self.jpg_quality);
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    let p = pages(&spec, total)?;
                    let v = render::pdf_to_images(&input, &pw, &p, dpi, if png { ImgFormat::Png } else { ImgFormat::Jpg }, q, &out, progress)?;
                    Ok(TaskOutput { msg: format!("{} gambar disimpan", v.len()), outputs: vec![out.clone()], ..Default::default() })
                });
            }
            PdfToWord => {
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let n = convert::pdf_to_docx(&input, &out, &pw)?;
                    Ok(one(&format!("{n} blok teks dikonversi"), &out))
                });
            }
            PdfToExcel => {
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let n = convert::pdf_to_xlsx(&input, &out, &pw)?;
                    Ok(one(&format!("{n} baris diekstrak"), &out))
                });
            }
            PdfToPowerpoint => {
                let (mode, dpi) = (if self.ppt_editable { PptMode::EditableText } else { PptMode::Image }, self.ppt_dpi);
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    let n = convert::pdf_to_pptx(&input, &out, &pw, mode, dpi, progress)?;
                    Ok(one(&format!("{n} slide dibuat"), &out))
                });
            }
            PdfToPdfa => {
                let lv = self.pdfa_level;
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    progress("Menjalankan Ghostscript…");
                    convert::pdf_to_pdfa(&input, &out, lv)?;
                    Ok(one(&format!("Dikonversi ke PDF/A-{lv}"), &out))
                });
            }
            Rotate => {
                let deg = self.rotate_deg;
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let p = pages(&spec, total)?;
                    pdfops::rotate(&input, &p, deg, &out, &pw)?;
                    Ok(one(&format!("{} halaman diputar", p.len()), &out))
                });
            }
            AddPageNumbers => {
                let (fmt, pos, size, margin, start) = (self.pn_format.clone(), self.pn_pos, self.pn_size, self.pn_margin, self.pn_start);
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let o = PageNumOpts { format: fmt, pos, size, margin, start, pages: pages(&spec, total)?, color: [0, 0, 0] };
                    pdfops::add_page_numbers(&input, &o, &out, &pw)?;
                    Ok(one("Nomor halaman ditambahkan", &out))
                });
            }
            AddWatermark => {
                let kind = if self.wm_image {
                    WmKind::Image { path: need!(self.wm_img.clone(), "Pilih gambar watermark"), scale: self.wm_scale }
                } else {
                    if self.wm_text.trim().is_empty() {
                        self.notify("✖ Teks watermark kosong".into());
                        return;
                    }
                    WmKind::Text { text: self.wm_text.clone(), size: self.wm_size, color: self.wm_color }
                };
                let (opacity, angle) = (self.wm_opacity, self.wm_angle);
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let o = WatermarkOpts { kind, opacity, angle, pages: pages(&spec, total)? };
                    pdfops::add_watermark(&input, &o, &out, &pw)?;
                    Ok(one("Watermark ditambahkan", &out))
                });
            }
            Crop => {
                let c = self.crop;
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let p = pages(&spec, total)?;
                    pdfops::crop(&input, &p, c[0], c[1], c[2], c[3], &out, &pw)?;
                    Ok(one(&format!("{} halaman dipotong", p.len()), &out))
                });
            }
            EditPdf => {
                let items = self.edit_items.clone();
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    pdfops::apply_edit_items(&input, &items, &out, &pw)?;
                    Ok(one(&format!("{} perubahan diterapkan", items.len()), &out))
                });
            }
            PdfForms => {
                if self.forms.is_empty() {
                    self.notify("✖ Klik “Baca kolom form” dulu".into());
                    return;
                }
                let vals: Vec<(lopdf::ObjectId, String)> = self.forms.iter().zip(self.form_vals.iter()).map(|(f, v)| (f.id, v.clone())).collect();
                let lock = self.form_lock;
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    pdfops::fill_form(&input, &vals, lock, &out, &pw)?;
                    Ok(one("Form diisi", &out))
                });
            }
            Unlock => {
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let m = pdfops::unlock(&input, &pw, &out)?;
                    Ok(one(&m, &out))
                });
            }
            Protect => {
                let o = ProtectOpts {
                    user_password: self.prot_user.clone(),
                    owner_password: self.prot_owner.clone(),
                    allow_print: self.prot_print,
                    allow_copy: self.prot_copy,
                    allow_modify: self.prot_modify,
                    allow_annotate: self.prot_annot,
                };
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    pdfops::protect(&input, &o, &out, &pw)?;
                    Ok(one("PDF dilindungi password (AES-128)", &out))
                });
            }
            Sign => {
                let rect = need!(self.sel, "Seret kotak di preview untuk menentukan area tanda tangan");
                if self.sign_img.is_none() && self.sign_name.trim().is_empty() {
                    self.notify("✖ Pilih gambar tanda tangan atau isi nama".into());
                    return;
                }
                let o = SignOpts {
                    page: self.page,
                    rect,
                    image: self.sign_img.clone(),
                    name: self.sign_name.clone(),
                    reason: self.sign_reason.clone(),
                    with_date: self.sign_date,
                    border: self.sign_border,
                };
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    pdfops::sign_visual(&input, &o, &out, &pw)?;
                    Ok(one("Tanda tangan visual ditambahkan", &out))
                });
            }
            Redact => {
                let rects = self.redact_rects.clone();
                let terms: Vec<String> = self.redact_terms.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let dpi = self.redact_dpi;
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    let r = render::redact(&input, &out, &pw, &rects, &terms, dpi, progress)?;
                    Ok(one(&format!("{} area disensor di {} halaman", r.boxes, r.pages), &out))
                });
            }
            Summarizer => {
                let (key, model, lang) = (self.api_key.clone(), self.model.clone(), self.ai_lang.clone());
                let style = ["ringkasan singkat satu paragraf", "poin-poin utama (bullet)", "ringkasan detail per bagian dengan sub-judul"][self.ai_style.min(2)];
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    progress("Membaca teks PDF…");
                    let text = render::page_texts(&input, &pw)?.join("\n\n");
                    let s = ai::summarize(&key, &model, &text, &lang, style, progress)?;
                    std::fs::write(&out, &s)?;
                    Ok(TaskOutput { msg: "Ringkasan selesai".into(), outputs: vec![out.clone()], text: Some(s), ..Default::default() })
                });
            }
            Translate => {
                let (key, model, target) = (self.api_key.clone(), self.model.clone(), self.tr_target.clone());
                let input = input.unwrap();
                self.spawn(ctx, move |progress| {
                    progress("Membaca teks PDF…");
                    let text = render::page_texts(&input, &pw)?.join("\n\n");
                    let t = ai::translate(&key, &model, &text, &target, progress)?;
                    match out.extension().and_then(|e| e.to_str()) {
                        Some("docx") => {
                            let paras: Vec<(bool, String)> = t.split("\n\n").map(|p| (false, p.trim().to_string())).filter(|(_, p)| !p.is_empty()).collect();
                            convert::text_to_docx(&paras, &out)?;
                        }
                        _ => std::fs::write(&out, &t)?,
                    }
                    Ok(TaskOutput { msg: "Terjemahan selesai".into(), outputs: vec![out.clone()], text: Some(t), ..Default::default() })
                });
            }
            PdfToMarkdown => {
                let input = input.unwrap();
                self.spawn(ctx, move |_| {
                    let md = render::to_markdown(&input, &pw)?;
                    std::fs::write(&out, &md)?;
                    Ok(TaskOutput { msg: "Markdown disimpan".into(), outputs: vec![out.clone()], text: Some(md), ..Default::default() })
                });
            }
            Compare => {}
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        // scrim
        let screen = ctx.screen_rect();
        egui::Area::new(Id::new("scrim")).order(Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(110));
            if resp.clicked() {
                self.show_settings = false;
            }
        });
        let mut close = false;
        egui::Window::new("Pengaturan")
            .title_bar(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .fixed_size(Vec2::new(460.0, 0.0))
            .frame(egui::Frame::window(&ctx.style()).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                ui.label(theme::headline("Pengaturan"));
                ui.add_space(8.0);
                ui.label(theme::title("Tampilan"));
                let mut dark = self.dark;
                theme::switch(ui, &mut dark, "Mode gelap");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Aksen:");
                    let mut acc = self.accent;
                    for a in Accent::ALL {
                        theme::choice(ui, &mut acc, a, a.name());
                    }
                    if acc != self.accent || dark != self.dark {
                        self.set_theme(ctx, acc, dark);
                    }
                });
                if dark != self.dark {
                    self.set_theme(ctx, self.accent, dark);
                }
                ui.add_space(10.0);
                ui.label(theme::title("Mesin PDF"));
                match render::pdfium_status() {
                    Ok(()) => theme::banner(ui, icons::CHECK_CIRCLE, "PDFium aktif: pratinjau, OCR, konversi gambar, dan redaksi tersedia.", BannerKind::Success),
                    Err(e) => theme::banner(ui, icons::ERROR, &e, BannerKind::Error),
                }
                ui.add_space(10.0);
                ui.label(theme::title("Program pendukung"));
                ui.label("Path tesseract.exe (kosong = otomatis)");
                ui.add(egui::TextEdit::singleline(&mut self.tesseract_path).desired_width(f32::INFINITY));
                ui.add_space(10.0);
                ui.label(theme::title("Claude API"));
                ui.label("API key Anthropic (hanya di memori, tidak disimpan ke disk)");
                ui.add(egui::TextEdit::singleline(&mut self.api_key).password(true).desired_width(f32::INFINITY));
                ui.label("Model");
                ui.add(egui::TextEdit::singleline(&mut self.model).desired_width(f32::INFINITY));
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::button(ui, Kind::Filled, None, "Selesai").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.show_settings = false;
        }
    }

    fn set_theme(&mut self, ctx: &egui::Context, accent: Accent, dark: bool) {
        self.accent = accent;
        self.dark = dark;
        self.follow_system = false;
        theme::apply(ctx, accent, dark);
        save_cfg(accent, dark);
    }

    fn matching_tools(&self) -> Vec<Tool> {
        let q = self.search.trim().to_lowercase();
        CATS.iter()
            .flat_map(|(_, ts)| ts.iter().copied())
            .filter(|t| q.is_empty() || t.label().to_lowercase().contains(&q) || t.short().to_lowercase().contains(&q) || t.desc().to_lowercase().contains(&q))
            .collect()
    }

    // ───────── kerangka M3 ─────────

    fn appbar(&mut self, ctx: &egui::Context) {
        let p = theme::pal(ctx);
        egui::TopBottomPanel::top("appbar")
            .exact_height(72.0)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::symmetric(16.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::click());
                    if resp.hovered() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.painter().rect_filled(rect, Rounding::same(15.0), p.primary);
                    theme::paint_icon(ui.painter(), rect.center(), icons::PICTURE_AS_PDF, 28.0, p.on_primary);
                    if resp.clicked() {
                        self.home = true;
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add_space(3.0);
                        ui.label(RichText::new("PDF Studio").size(20.0).strong());
                        ui.label(theme::label_small("Editor PDF all-in-one").color(p.on_surface_variant));
                    });
                    ui.add_space(20.0);
                    let sid = Id::new("search_field");
                    egui::Frame::none().fill(p.sc_high).rounding(Rounding::same(26.0)).inner_margin(Margin::symmetric(16.0, 7.0)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            theme::icon(ui, icons::SEARCH, 22.0, p.on_surface_variant);
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .id(sid)
                                    .frame(false)
                                    .hint_text(theme::hint(ui.ctx(), "Cari alat PDF…  (Ctrl+K)"))
                                    .desired_width(300.0),
                            );
                            if r.changed() && !self.search.trim().is_empty() {
                                self.home = true;
                            }
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                if let Some(t) = self.matching_tools().first().copied() {
                                    self.search.clear();
                                    self.select_tool(t);
                                }
                            }
                            if !self.search.is_empty() && theme::icon_button_sm(ui, icons::CLOSE, "Hapus pencarian", true).clicked() {
                                self.search.clear();
                            }
                        });
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::icon_button(ui, icons::SETTINGS, "Pengaturan").clicked() {
                            self.show_settings = !self.show_settings;
                        }
                        if theme::icon_button(ui, if self.dark { icons::LIGHT_MODE } else { icons::DARK_MODE }, if self.dark { "Mode terang" } else { "Mode gelap" }).clicked() {
                            let (a, d) = (self.accent, !self.dark);
                            self.set_theme(ctx, a, d);
                        }
                        let r = theme::icon_button(ui, icons::PALETTE, "Warna aksen");
                        self.accent_btn = r.rect;
                        if r.clicked() {
                            self.accent_open = !self.accent_open;
                        }
                    });
                });
            });
        if self.accent_open {
            let mut pick = None;
            let ar = egui::Area::new(Id::new("accent_pop"))
                .order(Order::Foreground)
                .pivot(Align2::RIGHT_TOP)
                .fixed_pos(self.accent_btn.right_bottom() + Vec2::new(0.0, 6.0))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).fill(p.sc_high).rounding(Rounding::same(16.0)).inner_margin(Margin::same(14.0)).show(ui, |ui| {
                        ui.label(theme::title("Warna aksen"));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            for a in Accent::ALL {
                                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
                                if resp.hovered() {
                                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                let col = theme::palette(a, self.dark).primary;
                                ui.painter().circle_filled(rect.center(), 15.0, col);
                                if a == self.accent {
                                    ui.painter().circle_stroke(rect.center(), 17.5, Stroke::new(2.5, p.on_surface));
                                    theme::paint_icon(ui.painter(), rect.center(), icons::CHECK, 18.0, p.sc_high);
                                }
                                if resp.on_hover_text(a.name()).clicked() {
                                    pick = Some(a);
                                }
                            }
                        });
                    });
                });
            if let Some(a) = pick {
                let d = self.dark;
                self.set_theme(ctx, a, d);
                self.accent_open = false;
            }
            if ctx.input(|i| i.pointer.any_pressed()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !ar.response.rect.contains(pos) && !self.accent_btn.contains(pos) {
                        self.accent_open = false;
                    }
                }
            }
        }
    }

    fn nav(&mut self, ctx: &egui::Context) {
        let p = theme::pal(ctx);
        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(272.0)
            .frame(egui::Frame::none().fill(p.sc_low).inner_margin(Margin::symmetric(12.0, 12.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if nav_item(ui, &p, icons::HOME, "Beranda", self.home) {
                        self.home = true;
                    }
                    ui.add_space(4.0);
                    for (ci, (cat, tools)) in CATS.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
                        if resp.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                            ui.painter().rect_filled(rect, Rounding::same(20.0), theme::alpha(p.on_surface, 14));
                        }
                        theme::paint_icon(ui.painter(), Pos2::new(rect.left() + 28.0, rect.center().y), CAT_ICONS[ci], 20.0, p.primary);
                        ui.painter().text(Pos2::new(rect.left() + 52.0, rect.center().y), Align2::LEFT_CENTER, pretty(cat), egui::FontId::proportional(14.0), p.on_surface);
                        theme::paint_icon(ui.painter(), Pos2::new(rect.right() - 24.0, rect.center().y), if self.nav_open[ci] { icons::EXPAND_LESS } else { icons::EXPAND_MORE }, 22.0, p.on_surface_variant);
                        if resp.clicked() {
                            self.nav_open[ci] = !self.nav_open[ci];
                        }
                        if self.nav_open[ci] {
                            for t in tools.iter() {
                                if nav_item(ui, &p, t.icon(), t.label(), !self.home && self.tool == *t) {
                                    self.select_tool(*t);
                                }
                            }
                            ui.add_space(4.0);
                        }
                    }
                });
            });
    }

    fn home_ui(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui.ctx());
        let q = self.search.trim().to_string();
        let mut open: Option<Tool> = None;
        if q.is_empty() {
            let fr = egui::Frame::none().fill(p.primary_container).rounding(Rounding::same(28.0)).inner_margin(Margin::same(28.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Semua alat PDF, dalam satu aplikasi.").size(30.0).strong().color(p.on_primary_container));
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - 170.0).max(260.0));
                    ui.add(egui::Label::new(RichText::new("Gabung, pisah, kompres, konversi, edit, amankan, sampai meringkas dengan AI. Cepat dan langsung di komputermu.").size(15.0).color(p.on_primary_container)).wrap());
                });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    if theme::button(ui, Kind::Filled, Some(icons::MERGE_TYPE), "Gabungkan PDF").clicked() {
                        open = Some(Tool::Merge);
                    }
                    if theme::button(ui, Kind::Outlined, Some(icons::COMPRESS), "Kompres").clicked() {
                        open = Some(Tool::Compress);
                    }
                    if theme::button(ui, Kind::Outlined, Some(icons::EDIT), "Edit PDF").clicked() {
                        open = Some(Tool::EditPdf);
                    }
                    if theme::button(ui, Kind::Outlined, Some(icons::AUTO_AWESOME), "Ringkas dengan AI").clicked() {
                        open = Some(Tool::Summarizer);
                    }
                });
            });
            ui.painter().text(fr.response.rect.right_center() - Vec2::new(90.0, 0.0), Align2::CENTER_CENTER, icons::PICTURE_AS_PDF.to_string(), theme::icon_font(120.0), theme::alpha(p.on_primary_container, 40));
            ui.add_space(14.0);
            for (ci, (cat, tools)) in CATS.iter().enumerate() {
                ui.horizontal(|ui| {
                    theme::icon_badge(ui, CAT_ICONS[ci], 34.0, p.secondary_container, p.on_secondary_container);
                    ui.label(theme::title(&pretty(cat)));
                    ui.label(RichText::new(format!("{} alat", tools.len())).color(p.on_surface_variant));
                });
                if let Some(t) = tool_grid(ui, &p, tools) {
                    open = Some(t);
                }
                ui.add_space(12.0);
            }
        } else {
            ui.label(theme::headline(&format!("Hasil untuk “{q}”")));
            let res = self.matching_tools();
            if res.is_empty() {
                theme::banner(ui, icons::INFO, "Tidak ada alat yang cocok. Coba kata lain, misalnya: gabung, kompres, putar, sensor, terjemah.", BannerKind::Info);
            } else {
                ui.label(RichText::new("Tekan Enter untuk membuka hasil pertama.").color(p.on_surface_variant));
                ui.add_space(6.0);
                if let Some(t) = tool_grid(ui, &p, &res) {
                    open = Some(t);
                }
            }
        }
        if let Some(t) = open {
            self.search.clear();
            self.select_tool(t);
        }
    }

    fn snackbar(&mut self, ctx: &egui::Context) {
        let Some(s) = &self.snack else { return };
        let limit = if s.kind == BannerKind::Error { 14.0 } else { 7.0 };
        if s.at.elapsed().as_secs_f32() > limit {
            self.snack = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
        let p = theme::pal(ctx);
        let (text, kind) = (s.text.clone(), s.kind);
        let (ic, icc) = match kind {
            BannerKind::Error => (icons::ERROR, Color32::from_rgb(0xEF, 0x77, 0x6F)),
            BannerKind::Success => (icons::CHECK_CIRCLE, Color32::from_rgb(0x4C, 0xAF, 0x6A)),
            _ => (icons::INFO, p.inverse_primary),
        };
        let has_out = kind == BannerKind::Success && !self.outputs.is_empty();
        let mut dismiss = false;
        let mut open_out = false;
        egui::Area::new(Id::new("snackbar")).order(Order::Foreground).anchor(Align2::CENTER_BOTTOM, Vec2::new(136.0, -104.0)).show(ctx, |ui| {
            egui::Frame::none()
                .fill(p.inverse_surface)
                .rounding(Rounding::same(10.0))
                .inner_margin(Margin::symmetric(18.0, 14.0))
                .shadow(egui::epaint::Shadow { offset: Vec2::new(0.0, 4.0), blur: 16.0, spread: 0.0, color: Color32::from_black_alpha(90) })
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.horizontal(|ui| {
                        theme::icon(ui, ic, 22.0, icc);
                        ui.add(egui::Label::new(RichText::new(&text).color(p.inverse_on_surface).size(14.5)).wrap());
                        ui.add_space(8.0);
                        if has_out && ui.add(egui::Label::new(RichText::new("BUKA").color(p.inverse_primary).strong()).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open_out = true;
                        }
                        if ui.add(egui::Label::new(RichText::new("TUTUP").color(p.inverse_primary).strong()).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            dismiss = true;
                        }
                    });
                });
        });
        if open_out {
            if let Some(o) = self.outputs.first() {
                util::open_in_shell(o);
            }
        }
        if dismiss || open_out {
            self.snack = None;
        }
    }

}

fn pretty(cat: &str) -> String {
    cat.split(' ')
        .map(|w| {
            if w == "PDF" {
                w.to_string()
            } else if w == "TO" || w == "FROM" {
                w.to_lowercase()
            } else {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn nav_item(ui: &mut egui::Ui, p: &theme::Palette, ic: char, label: &str, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, Rounding::same(22.0), p.secondary_container);
    } else if resp.hovered() {
        painter.rect_filled(rect, Rounding::same(22.0), theme::alpha(p.on_surface, 16));
    }
    let fg = if selected { p.on_secondary_container } else { p.on_surface_variant };
    theme::paint_icon(painter, Pos2::new(rect.left() + 28.0, rect.center().y), ic, 22.0, fg);
    painter.text(Pos2::new(rect.left() + 56.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(14.5), fg);
    resp.clicked()
}

fn tool_grid(ui: &mut egui::Ui, p: &theme::Palette, tools: &[Tool]) -> Option<Tool> {
    let gap = 12.0;
    let avail = ui.available_width();
    let n = (((avail + gap) / (280.0 + gap)).floor() as usize).max(1);
    let w = ((avail - gap * (n as f32 - 1.0)) / n as f32).floor() - 1.0;
    let mut out = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(gap);
        ui.horizontal_wrapped(|ui| {
            for t in tools {
                if tool_card(ui, p, *t, w) {
                    out = Some(*t);
                }
            }
        });
    });
    out
}

fn tool_card(ui: &mut egui::Ui, p: &theme::Palette, t: Tool, w: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 100.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let painter = ui.painter();
    let rd = Rounding::same(16.0);
    painter.rect_filled(rect, rd, if resp.hovered() { p.sc_high } else { p.sc_low });
    if resp.is_pointer_button_down_on() {
        painter.rect_filled(rect, rd, theme::alpha(p.on_surface, 20));
    }
    let c = Pos2::new(rect.left() + 16.0 + 23.0, rect.top() + 16.0 + 23.0);
    painter.circle_filled(c, 23.0, p.secondary_container);
    theme::paint_icon(painter, c, t.icon(), 26.0, p.on_secondary_container);
    let tx = rect.left() + 16.0 + 46.0 + 14.0;
    painter.text(Pos2::new(tx, rect.top() + 24.0), Align2::LEFT_CENTER, t.label(), egui::FontId::proportional(15.5), p.on_surface);
    let first = t.short().to_string();
    let mut job = egui::text::LayoutJob::single_section(first, egui::TextFormat { font_id: egui::FontId::proportional(12.5), color: p.on_surface_variant, ..Default::default() });
    job.wrap.max_width = (rect.right() - tx - 14.0).max(40.0);
    job.wrap.max_rows = 3;
    let g = painter.layout_job(job);
    painter.galley(Pos2::new(tx, rect.top() + 40.0), g, p.on_surface_variant);
    resp.clicked()
}

fn cfg_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("pdf-studio").join("theme.txt"))
}

fn load_cfg() -> (Accent, Option<bool>) {
    let (mut accent, mut dark) = (Accent::Purple, None);
    if let Some(t) = cfg_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("dark=") {
                dark = Some(v.trim() == "1");
            }
            if let Some(v) = l.strip_prefix("accent=") {
                if let Some(a) = Accent::ALL.iter().find(|a| a.name() == v.trim()) {
                    accent = *a;
                }
            }
        }
    }
    (accent, dark)
}

fn save_cfg(accent: Accent, dark: bool) {
    if let Some(p) = cfg_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, format!("dark={}\naccent={}\n", if dark { 1 } else { 0 }, accent.name()));
    }
}

fn pos_label(p: Pos) -> &'static str {
    match p {
        Pos::TopLeft => "Kiri atas",
        Pos::TopCenter => "Tengah atas",
        Pos::TopRight => "Kanan atas",
        Pos::BottomLeft => "Kiri bawah",
        Pos::BottomCenter => "Tengah bawah",
        Pos::BottomRight => "Kanan bawah",
    }
}

fn fmt_size(b: u64) -> String {
    if b >= 1 << 20 { format!("{:.2} MB", b as f64 / (1 << 20) as f64) } else { format!("{:.1} KB", b as f64 / 1024.0) }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Sebelum pengguna memilih tema sendiri, ikuti mode terang/gelap Windows.
        if self.follow_system {
            if let Some(t) = ctx.system_theme() {
                let sys_dark = t == egui::Theme::Dark;
                if sys_dark != self.dark {
                    self.dark = sys_dark;
                    theme::apply(ctx, self.accent, sys_dark);
                }
            }
        }
        // Esc: tutup dialog/popup, lepas fokus kolom isian, lalu kembali ke Beranda.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.show_settings {
                self.show_settings = false;
            } else if self.accent_open {
                self.accent_open = false;
            } else if let Some(id) = ctx.memory(|m| m.focused()) {
                if id == Id::new("search_field") {
                    self.search.clear();
                }
                ctx.memory_mut(|m| m.surrender_focus(id));
            } else if !self.search.is_empty() {
                self.search.clear();
            } else if !self.home {
                self.home = true;
            }
        }
        self.poll();
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() && !self.busy {
            if self.home {
                // dari beranda: pilih alat yang cocok dengan jenis file
                let e = dropped[0].extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                let t = match e.as_str() {
                    "pdf" => Some(if dropped.len() > 1 { Tool::Merge } else { Tool::EditPdf }),
                    "jpg" | "jpeg" | "png" => Some(Tool::JpgToPdf),
                    "doc" | "docx" | "rtf" | "odt" => Some(Tool::WordToPdf),
                    "ppt" | "pptx" | "odp" => Some(Tool::PowerpointToPdf),
                    "xls" | "xlsx" | "csv" | "ods" => Some(Tool::ExcelToPdf),
                    "html" | "htm" => Some(Tool::HtmlToPdf),
                    _ => None,
                };
                if let Some(t) = t {
                    self.select_tool(t);
                }
            }
            if self.tool == Tool::HtmlToPdf {
                self.html_src = dropped[0].display().to_string();
            } else {
                self.add_files(dropped);
            }
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            ctx.memory_mut(|m| m.request_focus(Id::new("search_field")));
        }
        self.refresh_preview(ctx);
        let p = theme::pal(ctx);

        self.appbar(ctx);
        self.nav(ctx);
        let tool_screen = !self.home;
        if tool_screen && self.tool != Tool::Compare {
            egui::TopBottomPanel::bottom("runbar")
                .frame(egui::Frame::none().fill(p.sc).inner_margin(Margin::symmetric(20.0, 6.0)))
                .show(ctx, |ui| {
                    self.run_bar(ui, ctx);
                });
        }
        if tool_screen && self.tool.uses_preview() && self.total > 0 {
            egui::SidePanel::right("preview")
                .resizable(true)
                .default_width(500.0)
                .min_width(300.0)
                .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin { left: 4.0, right: 20.0, top: 12.0, bottom: 8.0 }))
                .show(ctx, |ui| {
                    ui.label(theme::title("Pratinjau"));
                    self.preview_ui(ui);
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin { left: 28.0, right: 24.0, top: 12.0, bottom: 8.0 }))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if self.home {
                        self.home_ui(ui);
                    } else {
                        ui.set_max_width(ui.available_width().min(1000.0));
                        self.tool_ui(ui, ctx);
                    }
                });
            });
        self.snackbar(ctx);
        if self.show_settings {
            self.settings_window(ctx);
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let screen = ctx.screen_rect();
            egui::Area::new(Id::new("dropov")).order(Order::Foreground).fixed_pos(screen.min).interactable(false).show(ctx, |ui| {
                ui.painter().rect_filled(screen, 0.0, theme::alpha(p.primary, 60));
                let c = screen.center();
                let r = Rect::from_center_size(c, Vec2::new(380.0, 170.0));
                ui.painter().rect_filled(r, Rounding::same(28.0), p.sc_high);
                theme::paint_icon(ui.painter(), c - Vec2::new(0.0, 28.0), icons::UPLOAD_FILE, 54.0, p.primary);
                ui.painter().text(c + Vec2::new(0.0, 36.0), Align2::CENTER_CENTER, "Lepaskan file di sini", egui::FontId::proportional(20.0), p.on_surface);
            });
        }
    }

}
