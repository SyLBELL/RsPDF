# PDF Studio

Editor PDF all-in-one untuk **Windows 11** yang ditulis dengan **Rust** (GUI: egui/eframe).
Menu dan fitur mengikuti daftar alat pada screenshot: Organize, Optimize, Convert to/from PDF, Edit, Security, dan Intelligence.

## Build

1. Pasang Rust: <https://rustup.rs> (toolchain `stable-x86_64-pc-windows-msvc`) dan
   *Visual Studio Build Tools* dengan workload **Desktop development with C++**.
2. Di folder proyek:

```powershell
cargo build --release
.\setup-pdfium.ps1          # mengunduh pdfium.dll ke target\release
.\target\release\pdf-studio.exe
```

`pdfium.dll` harus berada satu folder dengan `pdf-studio.exe` (dipakai untuk preview, OCR, PDF→JPG, redaksi, dll).
Tanpa PDFium, fitur berbasis struktur PDF (merge, split, rotate, watermark, protect, …) tetap berjalan.

Argumen opsional: `pdf-studio.exe [--tool CropPdf] file.pdf` (juga cocok untuk "Open with").
Seret-lepas file ke jendela juga didukung.

## Status fitur

| Alat | Implementasi | Catatan |
|---|---|---|
| Merge / Split / Remove / Extract / Organize | lopdf | Urutan, rentang `1-3,5,8-`, duplikasi halaman |
| Scan to PDF | WIA + olah gambar | Scanner via dialog WIA Windows; mode abu-abu + kontras otomatis |
| Compress | lopdf + re-encode JPEG | Tiga tingkat; gambar non-JPEG tidak dikompres ulang |
| Repair | lopdf | Membangun ulang xref & pohon halaman; tidak bisa memulihkan konten yang benar-benar hilang |
| OCR | PDFium + **Tesseract** | Pasang Tesseract + data bahasa (`ind`, `eng`) |
| JPG to PDF | langsung | JPEG disematkan tanpa re-encode |
| Word/PowerPoint/Excel to PDF | **Microsoft Office (COM)** atau **LibreOffice** | Salah satu harus terpasang |
| HTML to PDF | **Edge/Chrome** headless | File lokal atau URL |
| PDF to JPG | PDFium | DPI & kualitas dapat diatur |
| PDF to Word / Excel | ekstraksi teks PDFium + penulis OOXML | Tidak mempertahankan tata letak; PDF hasil scan perlu OCR dulu |
| PDF to PowerPoint | PDFium + penulis OOXML | Mode gambar (identik) atau teks (bisa diedit) |
| PDF to PDF/A | **Ghostscript** | Validasi dengan veraPDF untuk keperluan arsip resmi |
| Rotate / Page numbers / Watermark / Crop | lopdf | Memperhitungkan /Rotate & CropBox |
| Edit PDF | lopdf | Tambah teks, gambar, whiteout, stabilo. Teks lama tidak diedit di tempat (tutup lalu tulis ulang) |
| PDF Forms | lopdf | Baca & isi AcroForm; opsi kunci field |
| Unlock / Protect | lopdf | Protect memakai AES-128 |
| Sign | lopdf | **Tanda tangan visual** (gambar/teks + tanggal). Bukan tanda tangan digital berbasis sertifikat |
| Redact | PDFium + lopdf | Halaman terkena dirender ulang sebagai gambar sehingga teks asli benar-benar hilang |
| Compare | PDFium + similar | Diff teks per baris + persentase beda visual per halaman |
| AI Summarizer / Translate | **Claude API** | Butuh API key; teks dokumen dikirim ke Anthropic |
| PDF to Markdown | PDFium | Judul dari ukuran huruf; tabel sederhana |

## Struktur kode

- `src/pdfops.rs` — operasi PDF murni (lopdf)
- `src/render.rs` — PDFium: render, ekstraksi, redaksi, OCR, compare
- `src/convert.rs` — konversi ke/dari PDF
- `src/ai.rs` — Claude API
- `src/ui.rs`, `src/main.rs` — antarmuka egui
