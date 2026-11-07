//! Fitur "PDF Intelligence": ringkasan & terjemahan lewat Claude API (Anthropic Messages API).
//! Butuh API key (isi di Pengaturan atau env `ANTHROPIC_API_KEY`). Teks PDF dikirim ke API Anthropic.

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";

pub fn claude(api_key: &str, model: &str, system: &str, user: &str, max_tokens: u32) -> Result<String> {
    if api_key.trim().is_empty() {
        bail!("API key Anthropic belum diisi (Pengaturan → API key, atau variabel lingkungan ANTHROPIC_API_KEY).");
    }
    let body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        "messages": [{ "role": "user", "content": user }],
    });
    let resp = ureq::post("https://api.anthropic.com/v1/messages")
        .set("x-api-key", api_key.trim())
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .timeout(std::time::Duration::from_secs(300))
        .send_json(body);
    let v: Value = match resp {
        Ok(r) => r.into_json()?,
        Err(ureq::Error::Status(code, r)) => {
            let t = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&t)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(String::from))
                .unwrap_or(t);
            bail!("API Claude error {code}: {msg}");
        }
        Err(e) => bail!("Tidak bisa terhubung ke API Claude: {e}"),
    };
    let text: String = v["content"]
        .as_array()
        .ok_or_else(|| anyhow!("Respons API tidak terduga"))?
        .iter()
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("");
    Ok(text)
}

fn chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in text.split("\n\n") {
        if cur.len() + para.len() + 2 > max_chars && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if para.len() > max_chars {
            let chars: Vec<char> = para.chars().collect();
            for c in chars.chunks(max_chars) {
                out.push(c.iter().collect());
            }
            continue;
        }
        cur.push_str(para);
        cur.push_str("\n\n");
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

pub fn summarize(api_key: &str, model: &str, text: &str, lang: &str, style: &str, progress: &dyn Fn(&str)) -> Result<String> {
    if text.trim().is_empty() {
        bail!("PDF tidak memiliki teks yang bisa dibaca (hasil scan? jalankan OCR dulu).");
    }
    let sys = format!(
        "Kamu adalah asisten yang meringkas dokumen secara akurat. Jangan menambahkan fakta yang tidak ada di dokumen. Tulis dalam bahasa: {lang}. Format keluaran: Markdown."
    );
    let parts = chunks(text, 60_000);
    let mut partials = Vec::new();
    if parts.len() > 1 {
        for (i, p) in parts.iter().enumerate() {
            progress(&format!("Meringkas bagian {}/{}…", i + 1, parts.len()));
            partials.push(claude(api_key, model, &sys, &format!("Ringkas bagian dokumen berikut dengan padat tetapi lengkap (poin-poin penting, angka, nama):\n\n{p}"), 2000)?);
        }
    }
    progress("Menyusun ringkasan akhir…");
    let src = if partials.is_empty() { text.to_string() } else { partials.join("\n\n---\n\n") };
    claude(api_key, model, &sys, &format!("Buat ringkasan dokumen ini. Gaya: {style}.\n\n{src}"), 3000)
}

pub fn translate(api_key: &str, model: &str, text: &str, target: &str, progress: &dyn Fn(&str)) -> Result<String> {
    if text.trim().is_empty() {
        bail!("PDF tidak memiliki teks yang bisa diterjemahkan (hasil scan? jalankan OCR dulu).");
    }
    let sys = format!(
        "Kamu penerjemah profesional. Terjemahkan teks ke {target}. Pertahankan struktur paragraf dan penomoran, jangan menambah komentar, nama diri dan kode dibiarkan. Keluarkan hanya hasil terjemahan."
    );
    let parts = chunks(text, 5000);
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        progress(&format!("Menerjemahkan bagian {}/{}…", i + 1, parts.len()));
        out.push(claude(api_key, model, &sys, p, 4096)?.trim().to_string());
    }
    Ok(out.join("\n\n"))
}
