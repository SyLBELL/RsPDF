//! Operasi PDF berbasis `lopdf`: organize, optimize, edit, security, forms.
//!
//! Semua koordinat overlay (watermark, nomor halaman, teks, kotak, tanda tangan)
//! memakai sistem koordinat *visual*: titik (0,0) di kiri-bawah halaman seperti yang
//! tampil di layar (sudah memperhitungkan /Rotate dan CropBox), satuan point (1/72 inci).

use anyhow::{anyhow, bail, Result};
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{
    dictionary, Dictionary, Document, EncryptionState, EncryptionVersion, Object, ObjectId,
    Permissions, Stream,
};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type Rgb = [u8; 3];

// ───────────────────────── util dasar ─────────────────────────

pub fn load(path: &Path, password: &str) -> Result<Document> {
    let res = if password.is_empty() {
        Document::load(path)
    } else {
        Document::load_with_password(path, password)
    };
    let doc = res.map_err(|e| {
        anyhow!("Gagal membuka '{}': {e}. Jika PDF berkata sandi, isi kolom password.", path.display())
    })?;
    if doc.is_encrypted() {
        bail!("'{}' terenkripsi. Gunakan menu Unlock PDF atau isi password yang benar.", path.display());
    }
    Ok(doc)
}

pub fn save(doc: &mut Document, out: &Path) -> Result<()> {
    if let Some(dir) = out.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    doc.save(out).map_err(|e| anyhow!("Gagal menyimpan '{}': {e}", out.display()))?;
    Ok(())
}

/// Bersihkan objek yatim, nomori ulang, kompres stream, lalu simpan.
pub fn finalize(doc: &mut Document, out: &Path) -> Result<()> {
    doc.prune_objects();
    doc.delete_zero_length_streams();
    doc.renumber_objects();
    doc.compress();
    save(doc, out)
}

pub fn page_count(path: &Path, password: &str) -> Result<u32> {
    Ok(load(path, password)?.get_pages().len() as u32)
}

/// "1-3,5,7-" → [1,2,3,5,7,...]. Urutan dan duplikat dipertahankan. Kosong/"all" = semua.
pub fn parse_ranges(spec: &str, total: u32) -> Result<Vec<u32>> {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("all") || spec == "*" {
        return Ok((1..=total).collect());
    }
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let num = |s: &str| -> Result<u32> {
            let s = s.trim();
            if s.eq_ignore_ascii_case("end") {
                return Ok(total);
            }
            s.parse::<u32>().map_err(|_| anyhow!("Rentang halaman tidak valid: '{part}'"))
        };
        let (a, b) = if let Some((l, r)) = part.split_once('-') {
            let a = if l.trim().is_empty() { 1 } else { num(l)? };
            let b = if r.trim().is_empty() { total } else { num(r)? };
            (a, b)
        } else {
            let n = num(part)?;
            (n, n)
        };
        if a == 0 || b == 0 || a > total || b > total {
            bail!("Halaman di luar jangkauan (1-{total}): '{part}'");
        }
        if a <= b {
            out.extend(a..=b);
        } else {
            out.extend((b..=a).rev());
        }
    }
    if out.is_empty() {
        bail!("Tidak ada halaman yang dipilih");
    }
    Ok(out)
}

fn obj_f32(doc: &Document, o: &Object) -> Option<f32> {
    let (_, o) = doc.dereference(o).ok()?;
    o.as_float().ok()
}

fn rect_of(doc: &Document, o: &Object) -> Option<[f32; 4]> {
    let (_, o) = doc.dereference(o).ok()?;
    let a = o.as_array().ok()?;
    if a.len() < 4 {
        return None;
    }
    let v: Vec<f32> = a.iter().take(4).filter_map(|x| obj_f32(doc, x)).collect();
    if v.len() < 4 {
        return None;
    }
    Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

/// Atribut halaman yang bisa diwariskan dari node /Pages induk.
pub fn inherited(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut cur = page_id;
    for _ in 0..64 {
        let d = doc.get_dictionary(cur).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(v.clone());
        }
        cur = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn flatten_inheritance(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for id in ids {
        for key in [&b"Resources"[..], b"MediaBox", b"CropBox", b"Rotate"] {
            let has = doc.get_dictionary(id).map(|d| d.has(key)).unwrap_or(true);
            if !has {
                if let Some(v) = inherited(doc, id, key) {
                    if let Ok(d) = doc.get_dictionary_mut(id) {
                        d.set(key.to_vec(), v);
                    }
                }
            }
        }
    }
}

// ───────────────────────── geometri halaman ─────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub rot: i64,
}

impl Geom {
    /// Lebar/tinggi halaman seperti yang terlihat (setelah rotasi).
    pub fn vw(&self) -> f32 {
        if self.rot % 180 == 0 { self.x1 - self.x0 } else { self.y1 - self.y0 }
    }
    pub fn vh(&self) -> f32 {
        if self.rot % 180 == 0 { self.y1 - self.y0 } else { self.x1 - self.x0 }
    }
    /// Matriks [a b c d e f]: koordinat visual → ruang pengguna halaman.
    pub fn matrix(&self) -> [f32; 6] {
        match self.rot {
            90 => [0.0, 1.0, -1.0, 0.0, self.x1, self.y0],
            180 => [-1.0, 0.0, 0.0, -1.0, self.x1, self.y1],
            270 => [0.0, -1.0, 1.0, 0.0, self.x0, self.y1],
            _ => [1.0, 0.0, 0.0, 1.0, self.x0, self.y0],
        }
    }
    pub fn to_user(&self, x: f32, y: f32) -> (f32, f32) {
        let m = self.matrix();
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
}

pub fn geom(doc: &Document, page_id: ObjectId) -> Geom {
    let media = inherited(doc, page_id, b"MediaBox")
        .and_then(|o| rect_of(doc, &o))
        .unwrap_or([0.0, 0.0, 595.0, 842.0]);
    let mut b = media;
    if let Some(c) = inherited(doc, page_id, b"CropBox").and_then(|o| rect_of(doc, &o)) {
        b = [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])];
        if b[2] <= b[0] || b[3] <= b[1] {
            b = media;
        }
    }
    let rot = inherited(doc, page_id, b"Rotate")
        .and_then(|o| doc.dereference(&o).ok().and_then(|(_, o)| o.as_i64().ok()))
        .unwrap_or(0);
    Geom { x0: b[0], y0: b[1], x1: b[2], y1: b[3], rot: ((rot % 360) + 360) % 360 }
}

// ───────────────────────── Organize ─────────────────────────

pub fn merge(inputs: &[PathBuf], out: &Path) -> Result<()> {
    if inputs.len() < 2 {
        bail!("Pilih minimal 2 file PDF untuk digabung");
    }
    let mut max_id = 1;
    let mut page_ids: Vec<ObjectId> = Vec::new();
    let mut objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    for p in inputs {
        let mut d = load(p, "")?;
        flatten_inheritance(&mut d);
        d.renumber_objects_with(max_id);
        max_id = d.max_id + 1;
        page_ids.extend(d.get_pages().values().copied());
        objects.extend(d.objects);
    }
    let mut doc = Document::with_version("1.7");
    for (id, obj) in objects {
        let ty = match &obj {
            Object::Dictionary(d) => d.get(b"Type").ok().and_then(|t| t.as_name().ok()).map(|n| n.to_vec()),
            _ => None,
        };
        // Katalog & node /Pages lama dibuang, dibangun ulang di bawah.
        if matches!(ty.as_deref(), Some(b"Catalog") | Some(b"Pages") | Some(b"Outlines")) {
            continue;
        }
        doc.objects.insert(id, obj);
    }
    doc.max_id = max_id;
    let pages_id = doc.new_object_id();
    for pid in &page_ids {
        if let Ok(d) = doc.get_dictionary_mut(*pid) {
            d.set("Parent", pages_id);
        }
    }
    let kids: Vec<Object> = page_ids.iter().map(|i| Object::Reference(*i)).collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids,
        }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    finalize(&mut doc, out)
}

/// Simpan hanya halaman `keep` (urutan asli).
pub fn extract_pages(input: &Path, keep: &[u32], out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let total = doc.get_pages().len() as u32;
    let del: Vec<u32> = (1..=total).filter(|n| !keep.contains(n)).collect();
    if del.len() as u32 == total {
        bail!("Tidak ada halaman tersisa");
    }
    doc.delete_pages(&del);
    finalize(&mut doc, out)
}

pub fn remove_pages(input: &Path, remove: &[u32], out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let total = doc.get_pages().len() as u32;
    if remove.iter().collect::<std::collections::BTreeSet<_>>().len() as u32 >= total {
        bail!("Tidak boleh menghapus semua halaman");
    }
    doc.delete_pages(remove);
    finalize(&mut doc, out)
}

pub enum SplitMode {
    /// Setiap rentang (dipisah koma) jadi satu file: "1-3,4-6"
    Ranges(String),
    EveryN(u32),
    EachPage,
}

pub fn split(input: &Path, mode: &SplitMode, out_dir: &Path, password: &str) -> Result<Vec<PathBuf>> {
    let total = page_count(input, password)?;
    let parts: Vec<Vec<u32>> = match mode {
        SplitMode::EachPage => (1..=total).map(|n| vec![n]).collect(),
        SplitMode::EveryN(n) => {
            let n = (*n).max(1);
            (1..=total).collect::<Vec<_>>().chunks(n as usize).map(|c| c.to_vec()).collect()
        }
        SplitMode::Ranges(spec) => spec
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| parse_ranges(s, total))
            .collect::<Result<_>>()?,
    };
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("split");
    std::fs::create_dir_all(out_dir)?;
    let width = parts.len().to_string().len().max(2);
    let mut outs = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let out = out_dir.join(format!("{stem}_part{:0w$}.pdf", i + 1, w = width));
        let mut sorted = part.clone();
        sorted.sort_unstable();
        extract_pages(input, &sorted, &out, password)?;
        outs.push(out);
    }
    Ok(outs)
}

/// Susun ulang / duplikasi / buang halaman sesuai `order` (nomor halaman asli).
pub fn organize(input: &Path, order: &[u32], out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    flatten_inheritance(&mut doc);
    let pages = doc.get_pages();
    let root = doc
        .catalog()
        .map_err(|e| anyhow!("{e}"))?
        .get(b"Pages")
        .map_err(|e| anyhow!("{e}"))?
        .as_reference()
        .map_err(|e| anyhow!("{e}"))?;
    let mut seen: HashMap<u32, usize> = HashMap::new();
    let mut kids = Vec::new();
    for &n in order {
        let id = *pages.get(&n).ok_or_else(|| anyhow!("Halaman {n} tidak ada"))?;
        let count = seen.entry(n).or_insert(0);
        let use_id = if *count == 0 {
            id
        } else {
            let copy = doc.get_object(id).map_err(|e| anyhow!("{e}"))?.clone();
            doc.add_object(copy)
        };
        *count += 1;
        doc.get_dictionary_mut(use_id).map_err(|e| anyhow!("{e}"))?.set("Parent", root);
        kids.push(Object::Reference(use_id));
    }
    let r = doc.get_dictionary_mut(root).map_err(|e| anyhow!("{e}"))?;
    r.set("Count", kids.len() as i64);
    r.set("Kids", kids);
    finalize(&mut doc, out)
}

// ───────────────────────── Optimize ─────────────────────────

pub struct CompressOpts {
    /// Kualitas JPEG ulang (1-100); None = jangan sentuh gambar.
    pub jpeg_quality: Option<u8>,
    /// Skala dimensi gambar (0.1-1.0).
    pub image_scale: f32,
}

pub fn compress(input: &Path, out: &Path, opts: &CompressOpts, password: &str) -> Result<(u64, u64)> {
    let before = std::fs::metadata(input)?.len();
    let mut doc = load(input, password)?;
    doc.trailer.remove(b"Info");
    if let Some(q) = opts.jpeg_quality {
        for (_, obj) in doc.objects.iter_mut() {
            let Object::Stream(s) = obj else { continue };
            if s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Image") {
                continue;
            }
            let single = match s.dict.get(b"Filter") {
                Ok(Object::Name(n)) => Some(n.clone()),
                Ok(Object::Array(a)) if a.len() == 1 => a[0].as_name().ok().map(|n| n.to_vec()),
                _ => None,
            };
            if single.as_deref() != Some(b"DCTDecode") {
                continue;
            }
            let Ok(img) = image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg) else {
                continue;
            };
            let gray = matches!(img.color(), image::ColorType::L8 | image::ColorType::L16);
            let (w0, h0) = (img.width(), img.height());
            let scale = opts.image_scale.clamp(0.1, 1.0);
            let img = if scale < 0.999 {
                img.resize(
                    ((w0 as f32 * scale) as u32).max(1),
                    ((h0 as f32 * scale) as u32).max(1),
                    image::imageops::FilterType::Lanczos3,
                )
            } else {
                img
            };
            let mut buf = Vec::new();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q.clamp(1, 100));
            let ok = if gray { img.to_luma8().write_with_encoder(enc) } else { img.to_rgb8().write_with_encoder(enc) };
            if ok.is_err() || buf.len() >= s.content.len() {
                continue;
            }
            s.dict.set("Width", img.width() as i64);
            s.dict.set("Height", img.height() as i64);
            s.dict.set("ColorSpace", if gray { "DeviceGray" } else { "DeviceRGB" });
            s.dict.set("BitsPerComponent", 8);
            s.dict.remove(b"DecodeParms");
            s.dict.remove(b"Decode");
            s.set_content(buf);
        }
    }
    finalize(&mut doc, out)?;
    Ok((before, std::fs::metadata(out)?.len()))
}

/// Perbaiki PDF rusak: buang sampah sebelum %PDF-, bangun ulang xref & pohon halaman.
pub fn repair(input: &Path, out: &Path) -> Result<String> {
    let mut bytes = std::fs::read(input)?;
    if let Some(pos) = bytes.windows(5).position(|w| w == b"%PDF-") {
        bytes.drain(..pos);
    }
    let mut doc = Document::load_mem(&bytes).map_err(|e| anyhow!("PDF tidak dapat dipulihkan: {e}"))?;
    let mut notes = Vec::new();
    if doc.get_pages().is_empty() {
        // Bangun ulang pohon halaman dari semua objek /Type /Page.
        let pages: Vec<ObjectId> = doc
            .objects
            .iter()
            .filter(|(_, o)| {
                matches!(o, Object::Dictionary(d) if d.get(b"Type").ok().and_then(|t| t.as_name().ok()) == Some(b"Page"))
            })
            .map(|(id, _)| *id)
            .collect();
        if pages.is_empty() {
            bail!("Tidak ditemukan halaman yang dapat dipulihkan");
        }
        let pages_id = doc.new_object_id();
        for p in &pages {
            if let Ok(d) = doc.get_dictionary_mut(*p) {
                d.set("Parent", pages_id);
            }
        }
        let kids: Vec<Object> = pages.iter().map(|i| Object::Reference(*i)).collect();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids }),
        );
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", cat);
        notes.push(format!("pohon halaman dibangun ulang ({} halaman)", pages.len()));
    }
    let n = doc.get_pages().len();
    finalize(&mut doc, out)?;
    notes.push("tabel xref ditulis ulang".into());
    Ok(format!("{n} halaman dipulihkan; {}", notes.join(", ")))
}

// ───────────────────────── Edit: rotate / crop / overlay ─────────────────────────

pub fn rotate(input: &Path, pages: &[u32], delta: i64, out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let map = doc.get_pages();
    for n in pages {
        let id = *map.get(n).ok_or_else(|| anyhow!("Halaman {n} tidak ada"))?;
        let cur = geom(&doc, id).rot;
        let new = (((cur + delta) % 360) + 360) % 360;
        doc.get_dictionary_mut(id).map_err(|e| anyhow!("{e}"))?.set("Rotate", new);
    }
    finalize(&mut doc, out)
}

/// Margin dalam point (sisi sesuai tampilan visual). Mengubah CropBox.
pub fn crop(input: &Path, pages: &[u32], l: f32, b: f32, r: f32, t: f32, out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let map = doc.get_pages();
    for n in pages {
        let id = *map.get(n).ok_or_else(|| anyhow!("Halaman {n} tidak ada"))?;
        let g = geom(&doc, id);
        let (vx0, vy0, vx1, vy1) = (l, b, g.vw() - r, g.vh() - t);
        if vx1 - vx0 < 10.0 || vy1 - vy0 < 10.0 {
            bail!("Margin terlalu besar untuk halaman {n}");
        }
        let (ax, ay) = g.to_user(vx0, vy0);
        let (bx, by) = g.to_user(vx1, vy1);
        let cb = vec![
            Object::Real(ax.min(bx)),
            Object::Real(ay.min(by)),
            Object::Real(ax.max(bx)),
            Object::Real(ay.max(by)),
        ];
        doc.get_dictionary_mut(id).map_err(|e| anyhow!("{e}"))?.set("CropBox", cb);
    }
    finalize(&mut doc, out)
}

#[derive(Default)]
pub struct OverlayRes {
    pub fonts: Vec<(String, ObjectId)>,
    pub xobjs: Vec<(String, ObjectId)>,
    pub gstates: Vec<(String, ObjectId)>,
}

/// Tambahkan konten (dalam koordinat visual) di atas halaman.
pub fn apply_overlay(doc: &mut Document, page_id: ObjectId, content: Vec<u8>, res: OverlayRes) -> Result<()> {
    let g = geom(doc, page_id);
    let m = g.matrix();
    let mut resources = match inherited(doc, page_id, b"Resources") {
        Some(o) => match doc.dereference(&o) {
            Ok((_, Object::Dictionary(d))) => d.clone(),
            _ => Dictionary::new(),
        },
        None => Dictionary::new(),
    };
    let groups: [(&str, &Vec<(String, ObjectId)>); 3] =
        [("Font", &res.fonts), ("XObject", &res.xobjs), ("ExtGState", &res.gstates)];
    for (key, items) in groups {
        if items.is_empty() {
            continue;
        }
        let mut sub = match resources.get(key.as_bytes()) {
            Ok(o) => match doc.dereference(o) {
                Ok((_, Object::Dictionary(d))) => d.clone(),
                _ => Dictionary::new(),
            },
            Err(_) => Dictionary::new(),
        };
        for (name, id) in items {
            sub.set(name.as_bytes().to_vec(), Object::Reference(*id));
        }
        resources.set(key, Object::Dictionary(sub));
    }
    let existing: Vec<Object> = {
        let page = doc.get_dictionary(page_id).map_err(|e| anyhow!("{e}"))?;
        match page.get(b"Contents") {
            Ok(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Array(a)) => a.clone(),
                _ => vec![Object::Reference(*r)],
            },
            Ok(Object::Array(a)) => a.clone(),
            _ => vec![],
        }
    };
    let q_open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut body = Vec::new();
    body.extend_from_slice(b"Q\nq\n");
    body.extend_from_slice(
        format!("{:.5} {:.5} {:.5} {:.5} {:.4} {:.4} cm\n", m[0], m[1], m[2], m[3], m[4], m[5]).as_bytes(),
    );
    body.extend_from_slice(&content);
    body.extend_from_slice(b"\nQ\n");
    let mut s = Stream::new(Dictionary::new(), body);
    let _ = s.compress();
    let ov = doc.add_object(s);
    let mut arr = vec![Object::Reference(q_open)];
    arr.extend(existing);
    arr.push(Object::Reference(ov));
    let page = doc.get_dictionary_mut(page_id).map_err(|e| anyhow!("{e}"))?;
    page.set("Resources", resources);
    page.set("Contents", Object::Array(arr));
    Ok(())
}

const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667,
    556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556,
    556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722,
    500, 500, 500, 334, 260, 334, 584,
];

pub fn text_width(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if (32..=126).contains(&u) { HELV[(u - 32) as usize] as f32 } else { 556.0 }
        })
        .sum::<f32>()
        * size
        / 1000.0
}

fn pdf_string(s: &str) -> Vec<u8> {
    let mut o = vec![b'('];
    for ch in s.chars() {
        let b = if (ch as u32) < 256 { ch as u32 as u8 } else { b'?' };
        match b {
            b'(' | b')' | b'\\' => {
                o.push(b'\\');
                o.push(b);
            }
            b'\r' | b'\n' => o.push(b' '),
            _ => o.push(b),
        }
    }
    o.push(b')');
    o
}

fn col(c: Rgb) -> String {
    format!("{:.3} {:.3} {:.3}", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)
}

/// Pembangun konten overlay untuk satu halaman.
#[derive(Default)]
pub struct Overlay {
    pub content: Vec<u8>,
    res: OverlayRes,
    gs: HashMap<(u32, bool), String>,
    font: Option<String>,
    n_img: usize,
}

impl Overlay {
    pub fn new() -> Self {
        Self::default()
    }
    fn ensure_font(&mut self, doc: &mut Document, font_obj: &mut Option<ObjectId>) -> String {
        if let Some(n) = &self.font {
            return n.clone();
        }
        let id = *font_obj.get_or_insert_with(|| {
            doc.add_object(dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
                "Encoding" => "WinAnsiEncoding",
            })
        });
        self.res.fonts.push(("FPS".into(), id));
        self.font = Some("FPS".into());
        "FPS".into()
    }
    fn ensure_gs(&mut self, doc: &mut Document, opacity: f32, multiply: bool) -> String {
        let key = ((opacity.clamp(0.0, 1.0) * 1000.0) as u32, multiply);
        if let Some(n) = self.gs.get(&key) {
            return n.clone();
        }
        let mut d = dictionary! { "Type" => "ExtGState", "ca" => opacity.clamp(0.0, 1.0), "CA" => opacity.clamp(0.0, 1.0) };
        if multiply {
            d.set("BM", "Multiply");
        }
        let id = doc.add_object(d);
        let name = format!("GPS{}", self.gs.len());
        self.res.gstates.push((name.clone(), id));
        self.gs.insert(key, name.clone());
        name
    }

    /// Teks (boleh multi-baris) dengan sudut `angle` derajat, titik awal (x,y) visual.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, doc: &mut Document, font_obj: &mut Option<ObjectId>, text: &str, x: f32, y: f32, size: f32, color: Rgb, opacity: f32, angle: f32) {
        let f = self.ensure_font(doc, font_obj);
        let gs = self.ensure_gs(doc, opacity, false);
        let (s, c) = angle.to_radians().sin_cos();
        let mut out = format!("q /{gs} gs {} rg BT /{f} {size:.2} Tf {:.1} TL\n", col(color), size * 1.2).into_bytes();
        for (i, line) in text.lines().enumerate() {
            let off = -(i as f32) * size * 1.2;
            // titik awal baris, diputar sesuai sudut
            let (lx, ly) = (x - off * s, y + off * c);
            out.extend_from_slice(format!("{c:.5} {s:.5} {:.5} {c:.5} {lx:.3} {ly:.3} Tm ", -s).as_bytes());
            out.extend_from_slice(&pdf_string(line));
            out.extend_from_slice(b" Tj\n");
        }
        out.extend_from_slice(b"ET Q\n");
        self.content.extend(out);
    }

    /// Kotak [x0,y0,x1,y1] terisi atau hanya garis tepi.
    pub fn rect(&mut self, doc: &mut Document, r: [f32; 4], color: Rgb, opacity: f32, multiply: bool, fill: bool, line_w: f32) {
        let gs = self.ensure_gs(doc, opacity, multiply);
        let (x0, y0, x1, y1) = (r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3]));
        let op = if fill { format!("{} rg f", col(color)) } else { format!("{} RG {line_w:.2} w S", col(color)) };
        self.content.extend_from_slice(
            format!("q /{gs} gs {x0:.3} {y0:.3} {:.3} {:.3} re {op} Q\n", x1 - x0, y1 - y0).as_bytes(),
        );
    }

    pub fn image(&mut self, doc: &mut Document, img: ObjectId, r: [f32; 4], opacity: f32) {
        let gs = self.ensure_gs(doc, opacity, false);
        let name = format!("IMS{}", self.n_img);
        self.n_img += 1;
        self.res.xobjs.push((name.clone(), img));
        let (x0, y0, x1, y1) = (r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3]));
        self.content.extend_from_slice(
            format!("q /{gs} gs {:.3} 0 0 {:.3} {x0:.3} {y0:.3} cm /{name} Do Q\n", x1 - x0, y1 - y0).as_bytes(),
        );
    }

    pub fn apply(self, doc: &mut Document, page_id: ObjectId) -> Result<()> {
        if self.content.is_empty() {
            return Ok(());
        }
        apply_overlay(doc, page_id, self.content, self.res)
    }
}

/// Buat XObject gambar dari file JPG/PNG. Mengembalikan (id, lebar_px, tinggi_px).
pub fn add_image(doc: &mut Document, path: &Path) -> Result<(ObjectId, u32, u32)> {
    use image::ImageDecoder;
    let bytes = std::fs::read(path).map_err(|e| anyhow!("Tidak bisa membaca {}: {e}", path.display()))?;
    let is_jpeg = bytes.starts_with(&[0xFF, 0xD8]);
    if is_jpeg {
        if let Ok(dec) = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&bytes)) {
            let (w, h) = dec.dimensions();
            let cs = match dec.color_type() {
                image::ColorType::L8 | image::ColorType::L16 => Some("DeviceGray"),
                image::ColorType::Rgb8 | image::ColorType::Rgb16 => Some("DeviceRGB"),
                _ => None,
            };
            if let Some(cs) = cs {
                let st = Stream::new(
                    dictionary! {
                        "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                        "ColorSpace" => cs, "BitsPerComponent" => 8, "Filter" => "DCTDecode",
                    },
                    bytes,
                );
                return Ok((doc.add_object(st), w, h));
            }
        }
    }
    let img = image::load_from_memory(&bytes).map_err(|e| anyhow!("Format gambar tidak didukung ({}): {e}", path.display()))?;
    add_dynamic_image(doc, &img)
}

pub fn add_dynamic_image(doc: &mut Document, img: &image::DynamicImage) -> Result<(ObjectId, u32, u32)> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for px in rgba.pixels() {
        rgb.extend_from_slice(&px.0[..3]);
        alpha.push(px.0[3]);
    }
    let has_alpha = alpha.iter().any(|a| *a < 255);
    let mut dict = dictionary! {
        "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
        "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
    };
    if has_alpha {
        let mut sm = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
            },
            alpha,
        );
        let _ = sm.compress();
        let sid = doc.add_object(sm);
        dict.set("SMask", sid);
    }
    let mut st = Stream::new(dict, rgb);
    let _ = st.compress();
    Ok((doc.add_object(st), w, h))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pos {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

pub struct PageNumOpts {
    /// Contoh: "{n}" atau "Halaman {n} dari {total}"
    pub format: String,
    pub pos: Pos,
    pub size: f32,
    pub margin: f32,
    pub start: u32,
    pub pages: Vec<u32>,
    pub color: Rgb,
}

pub fn add_page_numbers(input: &Path, o: &PageNumOpts, out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let map = doc.get_pages();
    let total = map.len() as u32;
    let mut font = None;
    for &n in &o.pages {
        let id = *map.get(&n).ok_or_else(|| anyhow!("Halaman {n} tidak ada"))?;
        let g = geom(&doc, id);
        let label = o
            .format
            .replace("{n}", &(o.start + n - 1).to_string())
            .replace("{total}", &(o.start + total - 1).to_string());
        let w = text_width(&label, o.size);
        let x = match o.pos {
            Pos::TopLeft | Pos::BottomLeft => o.margin,
            Pos::TopCenter | Pos::BottomCenter => (g.vw() - w) / 2.0,
            Pos::TopRight | Pos::BottomRight => g.vw() - o.margin - w,
        };
        let y = match o.pos {
            Pos::TopLeft | Pos::TopCenter | Pos::TopRight => g.vh() - o.margin - o.size * 0.8,
            _ => o.margin,
        };
        let mut ov = Overlay::new();
        ov.text(&mut doc, &mut font, &label, x, y, o.size, o.color, 1.0, 0.0);
        ov.apply(&mut doc, id)?;
    }
    finalize(&mut doc, out)
}

pub enum WmKind {
    Text { text: String, size: f32, color: Rgb },
    Image { path: PathBuf, scale: f32 },
}

pub struct WatermarkOpts {
    pub kind: WmKind,
    pub opacity: f32,
    pub angle: f32,
    pub pages: Vec<u32>,
}

pub fn add_watermark(input: &Path, o: &WatermarkOpts, out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let map = doc.get_pages();
    let mut font = None;
    let img = if let WmKind::Image { path, .. } = &o.kind { Some(add_image(&mut doc, path)?) } else { None };
    for &n in &o.pages {
        let id = *map.get(&n).ok_or_else(|| anyhow!("Halaman {n} tidak ada"))?;
        let g = geom(&doc, id);
        let (cx, cy) = (g.vw() / 2.0, g.vh() / 2.0);
        let mut ov = Overlay::new();
        match &o.kind {
            WmKind::Text { text, size, color } => {
                let w = text_width(text, *size);
                let (s, c) = o.angle.to_radians().sin_cos();
                let (dx, dy) = (-w / 2.0, -size * 0.3);
                let (x, y) = (cx + dx * c - dy * s, cy + dx * s + dy * c);
                ov.text(&mut doc, &mut font, text, x, y, *size, *color, o.opacity, o.angle);
            }
            WmKind::Image { scale, .. } => {
                let (iid, pw, ph) = img.unwrap();
                let w = g.vw() * scale;
                let h = w * ph as f32 / pw as f32;
                ov.image(&mut doc, iid, [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0], o.opacity);
            }
        }
        ov.apply(&mut doc, id)?;
    }
    finalize(&mut doc, out)
}

/// Item yang ditambahkan oleh alat "Edit PDF".
#[derive(Clone, Debug)]
pub enum EditItem {
    Text { page: u32, x: f32, y: f32, size: f32, color: Rgb, text: String },
    Rect { page: u32, rect: [f32; 4], color: Rgb, opacity: f32, multiply: bool },
    Image { page: u32, rect: [f32; 4], path: PathBuf },
}

impl EditItem {
    pub fn page(&self) -> u32 {
        match self {
            EditItem::Text { page, .. } | EditItem::Rect { page, .. } | EditItem::Image { page, .. } => *page,
        }
    }
    pub fn describe(&self) -> String {
        match self {
            EditItem::Text { page, text, .. } => format!("Hal {page}: teks \"{}\"", text.chars().take(24).collect::<String>()),
            EditItem::Rect { page, multiply, .. } => format!("Hal {page}: {}", if *multiply { "stabilo" } else { "kotak" }),
            EditItem::Image { page, path, .. } => format!(
                "Hal {page}: gambar {}",
                path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
            ),
        }
    }
}

pub fn apply_edit_items(input: &Path, items: &[EditItem], out: &Path, password: &str) -> Result<()> {
    if items.is_empty() {
        bail!("Belum ada perubahan yang ditambahkan");
    }
    let mut doc = load(input, password)?;
    let map = doc.get_pages();
    let mut font = None;
    let mut by_page: BTreeMap<u32, Vec<&EditItem>> = BTreeMap::new();
    for it in items {
        by_page.entry(it.page()).or_default().push(it);
    }
    let mut imgs: HashMap<PathBuf, (ObjectId, u32, u32)> = HashMap::new();
    for (pg, its) in by_page {
        let id = *map.get(&pg).ok_or_else(|| anyhow!("Halaman {pg} tidak ada"))?;
        let mut ov = Overlay::new();
        for it in its {
            match it {
                EditItem::Text { x, y, size, color, text, .. } => {
                    ov.text(&mut doc, &mut font, text, *x, *y, *size, *color, 1.0, 0.0)
                }
                EditItem::Rect { rect, color, opacity, multiply, .. } => {
                    ov.rect(&mut doc, *rect, *color, *opacity, *multiply, true, 1.0)
                }
                EditItem::Image { rect, path, .. } => {
                    let (iid, _, _) = match imgs.get(path) {
                        Some(v) => *v,
                        None => {
                            let v = add_image(&mut doc, path)?;
                            imgs.insert(path.clone(), v);
                            v
                        }
                    };
                    ov.image(&mut doc, iid, *rect, 1.0);
                }
            }
        }
        ov.apply(&mut doc, id)?;
    }
    finalize(&mut doc, out)
}

// ───────────────────────── Security ─────────────────────────

pub fn unlock(input: &Path, password: &str, out: &Path) -> Result<String> {
    let mut doc = Document::load_with_password(input, password)
        .map_err(|e| anyhow!("Tidak bisa membuka PDF (password salah?): {e}"))?;
    let was = doc.is_encrypted()
        || std::fs::read(input).map(|b| b.windows(8).any(|w| w == b"/Encrypt")).unwrap_or(false);
    if doc.is_encrypted() {
        doc.decrypt(password).map_err(|e| anyhow!("Dekripsi gagal: {e}"))?;
    }
    doc.trailer.remove(b"Encrypt");
    doc.encryption_state = None;
    doc.prune_objects();
    doc.renumber_objects();
    save(&mut doc, out)?;
    Ok(if was { "Proteksi dihapus.".into() } else { "PDF ini memang tidak terenkripsi; salinan disimpan.".into() })
}

pub struct ProtectOpts {
    pub user_password: String,
    pub owner_password: String,
    pub allow_print: bool,
    pub allow_copy: bool,
    pub allow_modify: bool,
    pub allow_annotate: bool,
}

pub fn protect(input: &Path, o: &ProtectOpts, out: &Path, password: &str) -> Result<()> {
    if o.user_password.is_empty() && o.owner_password.is_empty() {
        bail!("Isi minimal satu password");
    }
    let mut doc = load(input, password)?;
    doc.prune_objects();
    doc.renumber_objects();
    doc.compress();
    if doc.trailer.get(b"ID").is_err() {
        let seed = format!("{:?}{}", std::time::SystemTime::now(), input.display());
        let mut id = vec![0u8; 16];
        for (i, b) in seed.bytes().enumerate() {
            id[i % 16] = id[i % 16].wrapping_mul(31).wrapping_add(b);
        }
        doc.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
                Object::String(id, lopdf::StringFormat::Hexadecimal),
            ]),
        );
    }
    let mut perms = Permissions::COPYABLE_FOR_ACCESSIBILITY | Permissions::ASSEMBLABLE;
    if o.allow_print {
        perms |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
    }
    if o.allow_copy {
        perms |= Permissions::COPYABLE;
    }
    if o.allow_modify {
        perms |= Permissions::MODIFIABLE;
    }
    if o.allow_annotate {
        perms |= Permissions::ANNOTABLE | Permissions::FILLABLE;
    }
    let owner = if o.owner_password.is_empty() { format!("{}-owner", o.user_password) } else { o.owner_password.clone() };
    let filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> =
        BTreeMap::from([(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter) as Arc<dyn CryptFilter>)]);
    let state = {
        let v = EncryptionVersion::V4 {
            document: &doc,
            encrypt_metadata: true,
            crypt_filters: filters,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: &owner,
            user_password: &o.user_password,
            permissions: perms,
        };
        EncryptionState::try_from(v).map_err(|e| anyhow!("Gagal menyiapkan enkripsi: {e}"))?
    };
    doc.encrypt(&state).map_err(|e| anyhow!("Enkripsi gagal: {e}"))?;
    save(&mut doc, out)
}

// ───────────────────────── Sign (tanda tangan visual) ─────────────────────────

pub struct SignOpts {
    pub page: u32,
    pub rect: [f32; 4],
    pub image: Option<PathBuf>,
    pub name: String,
    pub reason: String,
    pub with_date: bool,
    pub border: bool,
}

pub fn sign_visual(input: &Path, o: &SignOpts, out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let map = doc.get_pages();
    let id = *map.get(&o.page).ok_or_else(|| anyhow!("Halaman {} tidak ada", o.page))?;
    let mut font = None;
    let mut ov = Overlay::new();
    let [x0, y0, x1, y1] = [o.rect[0].min(o.rect[2]), o.rect[1].min(o.rect[3]), o.rect[0].max(o.rect[2]), o.rect[1].max(o.rect[3])];
    let h = y1 - y0;
    let mut lines = Vec::new();
    if !o.name.trim().is_empty() {
        lines.push(format!("Ditandatangani oleh: {}", o.name.trim()));
    }
    if !o.reason.trim().is_empty() {
        lines.push(format!("Alasan: {}", o.reason.trim()));
    }
    if o.with_date {
        lines.push(format!("Tanggal: {}", crate::util::now_utc_string()));
    }
    let text_h = if lines.is_empty() { 0.0 } else { (h * 0.32).min(lines.len() as f32 * 11.0 + 4.0) };
    if let Some(p) = &o.image {
        let (iid, pw, ph) = add_image(&mut doc, p)?;
        let (aw, ah) = (x1 - x0 - 4.0, (h - text_h - 4.0).max(8.0));
        let sc = (aw / pw as f32).min(ah / ph as f32);
        let (w, hh) = (pw as f32 * sc, ph as f32 * sc);
        let cx = (x0 + x1) / 2.0;
        let by = y0 + text_h + 2.0 + (ah - hh) / 2.0;
        ov.image(&mut doc, iid, [cx - w / 2.0, by, cx + w / 2.0, by + hh], 1.0);
    } else if !o.name.trim().is_empty() {
        let size = ((x1 - x0) / (o.name.trim().len().max(4) as f32 * 0.55)).clamp(10.0, 26.0);
        ov.text(&mut doc, &mut font, o.name.trim(), x0 + 6.0, y0 + text_h + (h - text_h) / 2.0 - size / 3.0, size, [20, 40, 120], 1.0, 0.0);
    }
    let size = 7.5;
    for (i, l) in lines.iter().enumerate() {
        let y = y0 + text_h - 3.0 - size - i as f32 * 10.0;
        ov.text(&mut doc, &mut font, l, x0 + 4.0, y.max(y0 + 2.0), size, [50, 50, 50], 1.0, 0.0);
    }
    if o.border {
        ov.rect(&mut doc, [x0, y0, x1, y1], [60, 60, 60], 1.0, false, false, 0.8);
    }
    ov.apply(&mut doc, id)?;
    finalize(&mut doc, out)
}

// ───────────────────────── Redact: ganti halaman dengan gambar ─────────────────────────

/// Ganti seluruh isi halaman dengan satu gambar JPEG (teks/vektor asli dihapus permanen).
pub fn replace_page_with_jpeg(doc: &mut Document, page_id: ObjectId, jpeg: Vec<u8>, px_w: u32, px_h: u32, pt_w: f32, pt_h: f32) -> Result<()> {
    let img = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => px_w as i64, "Height" => px_h as i64,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
        },
        jpeg,
    ));
    let mut c = Stream::new(Dictionary::new(), format!("q {pt_w:.3} 0 0 {pt_h:.3} 0 0 cm /Im0 Do Q").into_bytes());
    let _ = c.compress();
    let cid = doc.add_object(c);
    let page = doc.get_dictionary_mut(page_id).map_err(|e| anyhow!("{e}"))?;
    page.set("Contents", cid);
    page.set("Resources", dictionary! { "XObject" => dictionary! { "Im0" => img } });
    page.set("MediaBox", vec![Object::Real(0.0), Object::Real(0.0), Object::Real(pt_w), Object::Real(pt_h)]);
    page.remove(b"CropBox");
    page.remove(b"Rotate");
    page.remove(b"Annots");
    page.remove(b"B");
    Ok(())
}

// ───────────────────────── Forms ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldKind {
    Text,
    Checkbox,
    Radio,
    Choice,
    Button,
    Signature,
    Other,
}

#[derive(Clone, Debug)]
pub struct FormField {
    pub id: ObjectId,
    pub name: String,
    pub kind: FieldKind,
    pub value: String,
    pub options: Vec<String>,
}

fn decode_pdf_text(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

fn encode_pdf_text(s: &str) -> Object {
    if s.chars().all(|c| (c as u32) < 256) {
        Object::String(s.chars().map(|c| c as u32 as u8).collect(), lopdf::StringFormat::Literal)
    } else {
        let mut v = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(v, lopdf::StringFormat::Hexadecimal)
    }
}

fn obj_text(doc: &Document, o: &Object) -> String {
    match doc.dereference(o).map(|(_, o)| o) {
        Ok(Object::String(b, _)) => decode_pdf_text(b),
        Ok(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        _ => String::new(),
    }
}

fn walk_fields(doc: &Document, id: ObjectId, prefix: &str, ft: Option<Vec<u8>>, ff: i64, depth: usize, out: &mut Vec<FormField>) {
    if depth > 24 {
        return;
    }
    let Ok(d) = doc.get_dictionary(id) else { return };
    let name = match d.get(b"T") {
        Ok(t) => {
            let t = obj_text(doc, t);
            if prefix.is_empty() { t } else { format!("{prefix}.{t}") }
        }
        Err(_) => prefix.to_string(),
    };
    let ft = d.get(b"FT").ok().and_then(|o| o.as_name().ok()).map(|n| n.to_vec()).or(ft);
    let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(ff);
    let kids: Vec<ObjectId> = d
        .get(b"Kids")
        .ok()
        .and_then(|k| k.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default();
    let field_kids: Vec<ObjectId> = kids
        .iter()
        .copied()
        .filter(|k| doc.get_dictionary(*k).map(|kd| kd.has(b"T")).unwrap_or(false))
        .collect();
    if !field_kids.is_empty() {
        for k in field_kids {
            walk_fields(doc, k, &name, ft.clone(), ff, depth + 1, out);
        }
        return;
    }
    let kind = match ft.as_deref() {
        Some(b"Tx") => FieldKind::Text,
        Some(b"Ch") => FieldKind::Choice,
        Some(b"Sig") => FieldKind::Signature,
        Some(b"Btn") => {
            if ff & (1 << 16) != 0 { FieldKind::Button } else if ff & (1 << 15) != 0 { FieldKind::Radio } else { FieldKind::Checkbox }
        }
        _ => FieldKind::Other,
    };
    let value = d.get(b"V").map(|v| obj_text(doc, v)).unwrap_or_default();
    let mut options = Vec::new();
    if kind == FieldKind::Choice {
        if let Ok(Object::Array(a)) = d.get(b"Opt").and_then(|o| doc.dereference(o).map(|(_, o)| o)) {
            for o in a {
                match doc.dereference(o).map(|(_, o)| o) {
                    Ok(Object::Array(pair)) if pair.len() == 2 => options.push(obj_text(doc, &pair[1])),
                    _ => options.push(obj_text(doc, o)),
                }
            }
        }
    }
    out.push(FormField { id, name, kind, value, options });
}

pub fn list_fields(doc: &Document) -> Vec<FormField> {
    let mut out = Vec::new();
    let Ok(cat) = doc.catalog() else { return out };
    let Ok(af) = cat.get(b"AcroForm").and_then(|o| doc.dereference(o).map(|(_, o)| o)) else { return out };
    let Ok(af) = af.as_dict() else { return out };
    if let Ok(Object::Array(fields)) = af.get(b"Fields").and_then(|o| doc.dereference(o).map(|(_, o)| o)) {
        for f in fields {
            if let Ok(id) = f.as_reference() {
                walk_fields(doc, id, "", None, 0, 0, &mut out);
            }
        }
    }
    out
}

pub fn read_fields(input: &Path, password: &str) -> Result<Vec<FormField>> {
    Ok(list_fields(&load(input, password)?))
}

fn on_state(doc: &Document, field: ObjectId) -> Vec<u8> {
    let mut cands = vec![field];
    if let Ok(d) = doc.get_dictionary(field) {
        if let Ok(Object::Array(k)) = d.get(b"Kids") {
            cands.extend(k.iter().filter_map(|o| o.as_reference().ok()));
        }
    }
    for c in cands {
        if let Ok(d) = doc.get_dictionary(c) {
            if let Ok(ap) = d.get(b"AP").and_then(|o| doc.dereference(o).map(|(_, o)| o)) {
                if let Ok(n) = ap.as_dict().and_then(|a| a.get(b"N")).and_then(|o| doc.dereference(o).map(|(_, o)| o)) {
                    if let Ok(nd) = n.as_dict() {
                        for (k, _) in nd.iter() {
                            if k.as_slice() != b"Off" {
                                return k.clone();
                            }
                        }
                    }
                }
            }
        }
    }
    b"Yes".to_vec()
}

/// Isi nilai form. Checkbox/radio: "true"/"yes"/"1"/"on" = tercentang.
pub fn fill_form(input: &Path, values: &[(ObjectId, String)], lock: bool, out: &Path, password: &str) -> Result<()> {
    let mut doc = load(input, password)?;
    let fields = list_fields(&doc);
    for (id, val) in values {
        let Some(f) = fields.iter().find(|f| f.id == *id) else { continue };
        match f.kind {
            FieldKind::Checkbox | FieldKind::Radio => {
                let checked = matches!(val.trim().to_lowercase().as_str(), "true" | "yes" | "ya" | "1" | "on" | "x");
                let st = if checked { on_state(&doc, *id) } else { b"Off".to_vec() };
                let mut targets = vec![*id];
                if let Ok(d) = doc.get_dictionary(*id) {
                    if let Ok(Object::Array(k)) = d.get(b"Kids") {
                        targets.extend(k.iter().filter_map(|o| o.as_reference().ok()));
                    }
                }
                for t in targets {
                    if let Ok(d) = doc.get_dictionary_mut(t) {
                        if t == *id {
                            d.set("V", Object::Name(st.clone()));
                        }
                        d.set("AS", Object::Name(st.clone()));
                    }
                }
            }
            FieldKind::Button | FieldKind::Signature | FieldKind::Other => {}
            _ => {
                let d = doc.get_dictionary_mut(*id).map_err(|e| anyhow!("{e}"))?;
                d.set("V", encode_pdf_text(val));
                d.remove(b"AP"); // paksa penampil membuat ulang tampilan
            }
        }
    }
    if lock {
        for f in &fields {
            if let Ok(d) = doc.get_dictionary_mut(f.id) {
                let ff = d.get(b"Ff").ok().and_then(|o| o.as_i64().ok()).unwrap_or(0);
                d.set("Ff", ff | 1);
            }
        }
    }
    // NeedAppearances agar nilai tampil di semua penampil
    let af = doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok().cloned());
    match af {
        Some(Object::Reference(r)) => {
            if let Ok(d) = doc.get_dictionary_mut(r) {
                d.set("NeedAppearances", true);
            }
        }
        Some(Object::Dictionary(_)) => {
            let root = doc.trailer.get(b"Root").ok().and_then(|o| o.as_reference().ok());
            if let Some(root) = root {
                if let Ok(c) = doc.get_dictionary_mut(root) {
                    if let Ok(Object::Dictionary(d)) = c.get_mut(b"AcroForm") {
                        d.set("NeedAppearances", true);
                    }
                }
            }
        }
        _ => {}
    }
    finalize(&mut doc, out)
}
