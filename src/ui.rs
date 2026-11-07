use anyhow::{anyhow, Result};
use eframe::egui::{self, load::SizedTexture, Color32, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Vec2};
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

pub struct App {
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

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for p in [r"C:\Windows\Fonts\segoeui.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"] {
        if let Ok(bytes) = std::fs::read(p) {
            fonts.font_data.insert("ui".into(), egui::FontData::from_owned(bytes));
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "ui".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Vec<String>) -> Self {
        setup_fonts(&cc.egui_ctx);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(8.0, 7.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        cc.egui_ctx.set_style(style);
        let (tx, rx) = channel();
        let mut app = Self {
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
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            if a == "--tool" {
                if let Some(n) = it.next() {
                    let n = n.to_lowercase().replace(['_', '-', ' '], "");
                    if let Some(t) = CATS.iter().flat_map(|(_, ts)| ts.iter()).find(|t| format!("{t:?}").to_lowercase() == n || t.label().to_lowercase().replace([' ', '/'], "") == n) {
                        app.tool = *t;
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
            app.add_files(files);
        }
        app
    }

    // ───────── state helpers ─────────

    fn select_tool(&mut self, t: Tool) {
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
        self.status = t.desc().into();
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
                    self.status = format!("✔ {}", o.msg);
                    self.log.push(self.status.clone());
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
                    self.status = format!("✖ {e}");
                    self.log.push(self.status.clone());
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
        let input = self.tool.input();
        if input == Input::Html {
            ui.label("Sumber HTML (URL atau file):");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.html_src).desired_width(380.0).hint_text("https://contoh.com  atau  C:\\path\\file.html"));
                if ui.button("Pilih file…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("HTML", &["html", "htm"]).pick_file() {
                        self.html_src = p.display().to_string();
                        self.files = vec![p];
                    }
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            let multi = matches!(input, Input::PdfMany | Input::Images);
            if ui.button(if multi { "➕ Tambah file…" } else { "📂 Pilih file…" }).clicked() {
                self.pick_input("");
            }
            if input == Input::Images && ui.button("🖨 Pindai (scanner)").clicked() && !self.busy {
                match convert::scan_with_wia() {
                    Ok(p) => self.files.push(p),
                    Err(e) => self.status = format!("✖ {e}"),
                }
            }
            if !self.files.is_empty() && ui.button("Kosongkan").clicked() {
                self.files.clear();
                self.on_files_changed();
            }
            if matches!(input, Input::PdfOne | Input::PdfMany | Input::PdfTwo) {
                ui.label("Password:");
                let r = ui.add(egui::TextEdit::singleline(&mut self.password).password(true).desired_width(110.0).hint_text("jika ada"));
                if r.lost_focus() && r.changed() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.refresh_total();
                }
            }
        });
        if self.files.is_empty() {
            ui.weak("Belum ada file. Klik tombol di atas atau seret file ke jendela ini.");
        }
        let mut remove = None;
        let mut swap = None;
        let multi = matches!(input, Input::PdfMany | Input::Images);
        let n = self.files.len();
        for (i, f) in self.files.iter().enumerate() {
            ui.horizontal(|ui| {
                let name = f.file_name().and_then(|s| s.to_str()).unwrap_or("?");
                let is_active = i == self.active;
                if ui.selectable_label(is_active, format!("{}. {name}", i + 1)).on_hover_text(f.display().to_string()).clicked() {
                    self.active = i;
                    self.pv.key = None;
                }
                if multi {
                    if ui.add_enabled(i > 0, egui::Button::new("▲").small()).clicked() {
                        swap = Some((i, i - 1));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("▼").small()).clicked() {
                        swap = Some((i, i + 1));
                    }
                }
                if ui.add(egui::Button::new("✕").small()).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some((a, b)) = swap {
            self.files.swap(a, b);
            self.active = b;
        }
        if let Some(i) = remove {
            self.files.remove(i);
            self.on_files_changed();
        }
        if self.total > 0 {
            ui.weak(format!("{} halaman", self.total));
        }
        if let Some(e) = &self.pv.err {
            if self.total == 0 {
                ui.colored_label(Color32::LIGHT_RED, e);
            }
        }
    }

    fn output_section(&mut self, ui: &mut egui::Ui) {
        if self.tool == Tool::Compare {
            return;
        }
        let Some(p) = self.out_path() else { return };
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(if self.tool.is_folder_output() { "Folder hasil:" } else { "Simpan ke:" });
            ui.monospace(p.display().to_string());
            if ui.button("Ubah…").clicked() {
                if self.tool.is_folder_output() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.out_override = Some(d);
                    }
                } else {
                    let (_, ext) = self.tool.out_spec();
                    let ext = if self.tool == Tool::Translate { ["docx", "md", "txt"][self.tr_fmt.min(2)] } else { ext };
                    let mut d = rfd::FileDialog::new().add_filter(ext, &[ext]);
                    if let Some(n) = p.file_name().and_then(|s| s.to_str()) {
                        d = d.set_file_name(n);
                    }
                    if let Some(f) = d.save_file() {
                        self.out_override = Some(f);
                    }
                }
            }
        });
    }

    fn run_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, label: &str) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let ready = !self.busy;
            if ui.add_enabled(ready, egui::Button::new(RichText::new(label).strong().size(15.0)).min_size(Vec2::new(160.0, 34.0))).clicked() {
                self.run(ctx);
            }
            if self.busy {
                ui.spinner();
            }
        });
    }

    fn pages_row(&mut self, ui: &mut egui::Ui, hint: &str) {
        ui.horizontal(|ui| {
            ui.label("Halaman:");
            ui.add(egui::TextEdit::singleline(&mut self.pages_spec).desired_width(200.0).hint_text(hint));
            ui.weak("(kosong = semua) contoh: 1-3,5,8-");
        });
    }

    fn pick_image_row(ui: &mut egui::Ui, label: &str, slot: &mut Option<PathBuf>) {
        ui.horizontal(|ui| {
            ui.label(label);
            if ui.button("Pilih gambar…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Gambar", &["png", "jpg", "jpeg"]).pick_file() {
                    *slot = Some(p);
                }
            }
            match slot {
                Some(p) => {
                    ui.monospace(p.file_name().and_then(|s| s.to_str()).unwrap_or("?"));
                    if ui.small_button("✕").clicked() {
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

    fn tool_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading(self.tool.label());
        ui.label(self.tool.desc());
        ui.separator();
        self.file_section(ui);
        ui.add_space(4.0);
        let t = self.tool;
        match t {
            Tool::Merge | Tool::Repair | Tool::Unlock | Tool::PdfToWord | Tool::PdfToExcel | Tool::PdfToMarkdown => {}
            Tool::Split => {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.split_mode, 0, "Rentang");
                    ui.radio_value(&mut self.split_mode, 1, "Setiap N halaman");
                    ui.radio_value(&mut self.split_mode, 2, "Satu halaman per file");
                });
                match self.split_mode {
                    0 => {
                        ui.horizontal(|ui| {
                            ui.label("Rentang (pisahkan dengan koma, tiap rentang = 1 file):");
                            ui.add(egui::TextEdit::singleline(&mut self.split_spec).desired_width(180.0));
                        });
                    }
                    1 => {
                        ui.horizontal(|ui| {
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
                ui.horizontal(|ui| {
                    if ui.button("Reset").clicked() {
                        self.organize_order = (1..=self.total).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
                    }
                    if ui.button("Balik urutan").clicked() {
                        self.organize_order = (1..=self.total).rev().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
                    }
                });
            }
            Tool::ScanToPdf | Tool::JpgToPdf => {
                ui.horizontal(|ui| {
                    ui.label("Ukuran halaman:");
                    ui.radio_value(&mut self.img_mode, 0, "Sesuai gambar");
                    ui.radio_value(&mut self.img_mode, 1, "A4 tegak");
                    ui.radio_value(&mut self.img_mode, 2, "A4 mendatar");
                });
                if self.img_mode != 0 {
                    ui.horizontal(|ui| {
                        ui.label("Margin (pt):");
                        ui.add(egui::DragValue::new(&mut self.img_margin).range(0.0..=100.0));
                    });
                }
                if t == Tool::ScanToPdf {
                    self.img_enhance = true;
                    ui.weak("Mode pindaian aktif: abu-abu + kontras otomatis.");
                } else {
                    ui.checkbox(&mut self.img_enhance, "Perbaiki kontras gaya scan (abu-abu)");
                }
            }
            Tool::Compress => {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.compress_level, 0, "Ringan (kualitas terjaga)");
                    ui.radio_value(&mut self.compress_level, 1, "Sedang");
                    ui.radio_value(&mut self.compress_level, 2, "Maksimal (gambar diperkecil)");
                });
            }
            Tool::Ocr => {
                ui.horizontal(|ui| {
                    ui.label("Bahasa Tesseract:");
                    ui.add(egui::TextEdit::singleline(&mut self.ocr_lang).desired_width(110.0));
                    ui.weak("mis. eng, ind, eng+ind");
                });
                ui.horizontal(|ui| {
                    ui.label("DPI render:");
                    ui.add(egui::DragValue::new(&mut self.ocr_dpi).range(100.0..=600.0));
                });
                if render::find_tesseract(&self.tesseract_path).is_none() {
                    ui.colored_label(Color32::YELLOW, "Tesseract belum ditemukan. Pasang: winget install UB-Mannheim.TesseractOCR (atau isi path di ⚙ Pengaturan).");
                }
            }
            Tool::WordToPdf | Tool::PowerpointToPdf | Tool::ExcelToPdf => {
                ui.weak("Memakai Microsoft Office jika terpasang, jika tidak LibreOffice.");
            }
            Tool::HtmlToPdf => {}
            Tool::PdfToJpg => {
                self.pages_row(ui, "semua");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.jpg_png, false, "JPG");
                    ui.radio_value(&mut self.jpg_png, true, "PNG");
                    ui.label("DPI:");
                    ui.add(egui::DragValue::new(&mut self.jpg_dpi).range(36.0..=600.0));
                    if !self.jpg_png {
                        ui.label("Kualitas:");
                        ui.add(egui::DragValue::new(&mut self.jpg_quality).range(10..=100));
                    }
                });
            }
            Tool::PdfToPowerpoint => {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.ppt_editable, false, "Gambar per halaman (tampilan identik)");
                    ui.radio_value(&mut self.ppt_editable, true, "Teks yang bisa diedit");
                });
                if !self.ppt_editable {
                    ui.horizontal(|ui| {
                        ui.label("DPI gambar:");
                        ui.add(egui::DragValue::new(&mut self.ppt_dpi).range(72.0..=300.0));
                    });
                }
            }
            Tool::PdfToPdfa => {
                ui.horizontal(|ui| {
                    ui.label("Level PDF/A:");
                    ui.radio_value(&mut self.pdfa_level, 1, "1");
                    ui.radio_value(&mut self.pdfa_level, 2, "2");
                    ui.radio_value(&mut self.pdfa_level, 3, "3");
                });
                ui.weak("Memakai Ghostscript (winget install ArtifexSoftware.GhostScript). Validasi hasilnya dengan veraPDF bila untuk arsip resmi.");
            }
            Tool::Rotate => {
                self.pages_row(ui, "semua");
                ui.horizontal(|ui| {
                    ui.label("Putar:");
                    ui.radio_value(&mut self.rotate_deg, 90, "90° ⟳");
                    ui.radio_value(&mut self.rotate_deg, 180, "180°");
                    ui.radio_value(&mut self.rotate_deg, 270, "90° ⟲");
                });
            }
            Tool::AddPageNumbers => {
                self.pages_row(ui, "semua");
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    ui.add(egui::TextEdit::singleline(&mut self.pn_format).desired_width(200.0));
                    ui.weak("{n} = nomor, {total} = jumlah");
                });
                ui.horizontal(|ui| {
                    ui.label("Posisi:");
                    egui::ComboBox::from_id_salt("pnpos")
                        .selected_text(pos_label(self.pn_pos))
                        .show_ui(ui, |ui| {
                            for p in [Pos::TopLeft, Pos::TopCenter, Pos::TopRight, Pos::BottomLeft, Pos::BottomCenter, Pos::BottomRight] {
                                ui.selectable_value(&mut self.pn_pos, p, pos_label(p));
                            }
                        });
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
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.wm_image, false, "Teks");
                    ui.radio_value(&mut self.wm_image, true, "Gambar");
                });
                if self.wm_image {
                    Self::pick_image_row(ui, "Gambar:", &mut self.wm_img);
                    ui.horizontal(|ui| {
                        ui.label("Lebar relatif:");
                        ui.add(egui::Slider::new(&mut self.wm_scale, 0.1..=1.0));
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Teks:");
                        ui.add(egui::TextEdit::singleline(&mut self.wm_text).desired_width(220.0));
                        ui.label("Ukuran:");
                        ui.add(egui::DragValue::new(&mut self.wm_size).range(8.0..=300.0));
                        ui.color_edit_button_srgb(&mut self.wm_color);
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Transparansi:");
                    ui.add(egui::Slider::new(&mut self.wm_opacity, 0.05..=1.0));
                    ui.label("Sudut:");
                    ui.add(egui::DragValue::new(&mut self.wm_angle).range(-180.0..=180.0));
                });
            }
            Tool::Crop => {
                self.pages_row(ui, "semua");
                ui.label("Margin yang dibuang (pt, dihitung dari tampilan):");
                ui.horizontal(|ui| {
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
                if ui.add_enabled(self.sel.is_some(), egui::Button::new("Pakai seleksi sebagai area yang dipertahankan")).clicked() {
                    if let Some(r) = self.sel {
                        let (w, h) = self.pv.size_pt;
                        self.crop = [r[0], r[1], (w - r[2]).max(0.0), (h - r[3]).max(0.0)];
                    }
                }
            }
            Tool::EditPdf => self.edit_ui(ui),
            Tool::PdfForms => self.forms_ui(ui),
            Tool::Protect => {
                ui.horizontal(|ui| {
                    ui.label("Password buka:");
                    ui.add(egui::TextEdit::singleline(&mut self.prot_user).password(true).desired_width(160.0));
                    ui.label("Password pemilik:");
                    ui.add(egui::TextEdit::singleline(&mut self.prot_owner).password(true).desired_width(160.0));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.prot_print, "Boleh cetak");
                    ui.checkbox(&mut self.prot_copy, "Boleh salin teks");
                    ui.checkbox(&mut self.prot_modify, "Boleh ubah");
                    ui.checkbox(&mut self.prot_annot, "Boleh anotasi/isi form");
                });
                ui.weak("Enkripsi AES-128. Jangan lupa password — tidak ada cara memulihkannya.");
            }
            Tool::Sign => {
                Self::pick_image_row(ui, "Gambar tanda tangan (opsional, PNG transparan terbaik):", &mut self.sign_img);
                ui.horizontal(|ui| {
                    ui.label("Nama:");
                    ui.add(egui::TextEdit::singleline(&mut self.sign_name).desired_width(180.0));
                    ui.label("Alasan:");
                    ui.add(egui::TextEdit::singleline(&mut self.sign_reason).desired_width(180.0));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.sign_date, "Sertakan tanggal");
                    ui.checkbox(&mut self.sign_border, "Bingkai");
                });
                self.sel_info(ui);
                ui.weak("Seret kotak di preview pada halaman tujuan sebagai tempat tanda tangan.");
            }
            Tool::Redact => {
                ui.horizontal(|ui| {
                    ui.label("Sensor teks (pisahkan dengan ;):");
                    ui.add(egui::TextEdit::singleline(&mut self.redact_terms).desired_width(260.0).hint_text("mis. 12345; rahasia"));
                });
                self.sel_info(ui);
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.sel.is_some(), egui::Button::new("＋ Tambah area dari seleksi")).clicked() {
                        if let Some(r) = self.sel.take() {
                            self.redact_rects.push((self.page, r));
                        }
                    }
                    ui.label("DPI:");
                    ui.add(egui::DragValue::new(&mut self.redact_dpi).range(100.0..=300.0));
                });
                let mut rm = None;
                for (i, (p, r)) in self.redact_rects.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("Hal {p}: {:.0},{:.0} – {:.0},{:.0}", r[0], r[1], r[2], r[3]));
                        if ui.small_button("✕").clicked() {
                            rm = Some(i);
                        }
                    });
                }
                if let Some(i) = rm {
                    self.redact_rects.remove(i);
                }
                ui.colored_label(Color32::YELLOW, "Halaman yang disensor menjadi gambar (teks asli dihapus permanen). Simpan sebagai file baru.");
            }
            Tool::Compare => self.compare_ui(ui, ctx),
            Tool::Summarizer => {
                ui.horizontal(|ui| {
                    ui.label("Bahasa ringkasan:");
                    ui.add(egui::TextEdit::singleline(&mut self.ai_lang).desired_width(160.0));
                    ui.label("Gaya:");
                    egui::ComboBox::from_id_salt("aistyle")
                        .selected_text(["Singkat (1 paragraf)", "Poin-poin", "Detail per bagian"][self.ai_style.min(2)])
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.ai_style, 0, "Singkat (1 paragraf)");
                            ui.selectable_value(&mut self.ai_style, 1, "Poin-poin");
                            ui.selectable_value(&mut self.ai_style, 2, "Detail per bagian");
                        });
                });
                self.api_note(ui);
            }
            Tool::Translate => {
                ui.horizontal(|ui| {
                    ui.label("Terjemahkan ke:");
                    ui.add(egui::TextEdit::singleline(&mut self.tr_target).desired_width(160.0));
                    ui.label("Format:");
                    for (i, n) in ["DOCX", "Markdown", "TXT"].iter().enumerate() {
                        if ui.radio_value(&mut self.tr_fmt, i, *n).clicked() {
                            self.out_override = None;
                        }
                    }
                });
                self.api_note(ui);
            }
        }
        if t != Tool::Compare {
            self.output_section(ui);
            let label = match t {
                Tool::Summarizer => "Ringkas",
                Tool::Translate => "Terjemahkan",
                Tool::PdfForms => "Isi form & simpan",
                _ => "Proses",
            };
            self.run_button(ui, ctx, label);
        }
        self.result_ui(ui);
    }

    fn api_note(&self, ui: &mut egui::Ui) {
        if self.api_key.trim().is_empty() {
            ui.colored_label(Color32::YELLOW, "Isi API key Anthropic di ⚙ Pengaturan (atau env ANTHROPIC_API_KEY).");
        } else {
            ui.weak("Teks dokumen dikirim ke API Anthropic untuk diproses.");
        }
    }

    fn edit_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, n) in ["Teks", "Kotak putih (whiteout)", "Stabilo", "Gambar"].iter().enumerate() {
                ui.radio_value(&mut self.edit_action, i, *n);
            }
        });
        if self.edit_action == 0 {
            ui.horizontal(|ui| {
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
        if ui.add_enabled(can, egui::Button::new(format!("＋ Tambahkan ke halaman {page}"))).clicked() {
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
            ui.horizontal(|ui| {
                ui.label(it.describe());
                if ui.small_button("✕").clicked() {
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
        if ui.button("🔍 Baca kolom form").clicked() {
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
                        self.status = format!("{} kolom form ditemukan", v.len());
                        self.forms = v;
                    }
                    Err(e) => self.status = format!("✖ {e}"),
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
                        if ui.checkbox(&mut b, "").changed() {
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
        ui.checkbox(&mut self.form_lock, "Kunci kolom setelah diisi (read-only)");
    }

    fn compare_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label("Dokumen B:");
            if ui.button("Pilih PDF B…").clicked() {
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
        ui.horizontal(|ui| {
            if ui.add_enabled(ready, egui::Button::new(RichText::new("Bandingkan").strong()).min_size(Vec2::new(140.0, 32.0))).clicked() {
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
                        let c = if *pct < 0.05 { Color32::LIGHT_GREEN } else { Color32::from_rgb(255, 170, 80) };
                        ui.colored_label(c, format!("Hal {p}: {pct:.2}%"));
                    }
                });
            }
            ui.checkbox(&mut self.cmp_show_equal, "Tampilkan baris yang sama");
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
                        '+' => (Color32::LIGHT_GREEN, "+ "),
                        '-' => (Color32::LIGHT_RED, "− "),
                        _ => (Color32::GRAY, "  "),
                    };
                    ui.label(RichText::new(format!("{pre}[{}] {}", l.page, l.text)).color(c).monospace());
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Salin laporan").clicked() {
                    ctx.copy_text(report.clone());
                }
                if ui.button("Simpan laporan .txt…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_file_name("perbandingan.txt").save_file() {
                        let _ = std::fs::write(p, report);
                    }
                }
            });
            ui.weak("＋ hijau = hanya ada di B · − merah = hanya ada di A");
        }
    }

    fn result_ui(&mut self, ui: &mut egui::Ui) {
        if !self.outputs.is_empty() {
            ui.separator();
            let outs = self.outputs.clone();
            for o in &outs {
                ui.horizontal(|ui| {
                    ui.monospace(o.display().to_string());
                    if ui.small_button("Buka").clicked() {
                        util::open_in_shell(o);
                    }
                    if ui.small_button("Tampilkan di folder").clicked() {
                        util::reveal_in_shell(o);
                    }
                    if o.extension().and_then(|e| e.to_str()) == Some("pdf") && ui.small_button("Pakai sebagai input").clicked() {
                        self.files = vec![o.clone()];
                        self.out_override = None;
                        self.on_files_changed();
                    }
                });
            }
        }
        if !self.result_text.is_empty() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Hasil:");
                if ui.small_button("Salin").clicked() {
                    ui.ctx().copy_text(self.result_text.clone());
                }
            });
            egui::ScrollArea::vertical().max_height(300.0).id_salt("resscroll").show(ui, |ui| {
                let mut s: &str = &self.result_text;
                ui.add(egui::TextEdit::multiline(&mut s).desired_width(f32::INFINITY).desired_rows(8));
            });
        }
    }

    // ───────── preview ─────────

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(self.page > 1, egui::Button::new("◀")).clicked() {
                self.page -= 1;
                self.sel = None;
            }
            let mut p = self.page;
            ui.add(egui::DragValue::new(&mut p).range(1..=self.total.max(1)));
            if p != self.page {
                self.page = p;
                self.sel = None;
            }
            ui.label(format!("/ {}", self.total));
            if ui.add_enabled(self.page < self.total, egui::Button::new("▶")).clicked() {
                self.page += 1;
                self.sel = None;
            }
            ui.separator();
            ui.add(egui::Slider::new(&mut self.zoom, 0.3..=2.5).text("zoom"));
        });
        if let Some(e) = &self.pv.err {
            ui.colored_label(Color32::LIGHT_RED, e);
            return;
        }
        let Some(tex) = self.pv.tex.clone() else {
            ui.weak("Preview tidak tersedia");
            return;
        };
        let (wpt, hpt) = self.pv.size_pt;
        let z = self.zoom * 1.1;
        egui::ScrollArea::both().drag_to_scroll(false).id_salt("pvscroll").show(ui, |ui| {
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
                draw_rect(r, Color32::from_rgba_unmultiplied(40, 120, 255, 50), Color32::from_rgb(40, 120, 255));
            }
            if let Some((x, y)) = self.pt {
                if self.sel.is_none() {
                    painter.circle_filled(to_px(x, y), 4.0, Color32::from_rgb(255, 60, 60));
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
                        self.status = format!("✖ {}", $m);
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
                    self.status = "✖ Tambahkan minimal 2 PDF".into();
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
                    self.status = "✖ Isi nomor halaman".into();
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
                    self.status = "✖ Isi URL atau pilih file HTML".into();
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
                        self.status = "✖ Teks watermark kosong".into();
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
                    self.status = "✖ Klik “Baca kolom form” dulu".into();
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
                    self.status = "✖ Pilih gambar tanda tangan atau isi nama".into();
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
        let mut open = self.show_settings;
        egui::Window::new("⚙ Pengaturan").open(&mut open).collapsible(false).show(ctx, |ui| {
            match render::pdfium_status() {
                Ok(()) => ui.colored_label(Color32::LIGHT_GREEN, "PDFium: aktif (preview, OCR, konversi gambar, redaksi tersedia)"),
                Err(e) => ui.colored_label(Color32::LIGHT_RED, format!("PDFium: {e}")),
            };
            ui.separator();
            ui.label("Path tesseract.exe (kosong = otomatis):");
            ui.text_edit_singleline(&mut self.tesseract_path);
            ui.label("API key Anthropic (hanya di memori, tidak disimpan ke disk):");
            ui.add(egui::TextEdit::singleline(&mut self.api_key).password(true).desired_width(320.0));
            ui.label("Model Claude:");
            ui.text_edit_singleline(&mut self.model);
        });
        self.show_settings = open;
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
        self.poll();
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() && !self.busy {
            if self.tool == Tool::HtmlToPdf {
                self.html_src = dropped[0].display().to_string();
            } else {
                self.add_files(dropped);
            }
        }
        self.refresh_preview(ctx);

        egui::SidePanel::left("nav").resizable(false).exact_width(215.0).show(ctx, |ui| {
            ui.add_space(6.0);
            ui.heading("PDF Studio");
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (cat, tools) in CATS {
                    ui.add_space(6.0);
                    ui.label(RichText::new(*cat).small().strong().color(Color32::GRAY));
                    for t in *tools {
                        if ui.selectable_label(self.tool == *t, t.label()).clicked() {
                            self.select_tool(*t);
                        }
                    }
                }
                ui.add_space(12.0);
                ui.separator();
                if ui.button("⚙ Pengaturan").clicked() {
                    self.show_settings = !self.show_settings;
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(&self.status);
            });
        });

        let show_pv = self.tool.uses_preview() && self.total > 0;
        if show_pv {
            egui::SidePanel::right("preview").resizable(true).default_width(520.0).min_width(280.0).show(ctx, |ui| {
                self.preview_ui(ui);
            });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.tool_ui(ui, ctx);
            });
        });
        if self.show_settings {
            self.settings_window(ctx);
        }
    }
}
