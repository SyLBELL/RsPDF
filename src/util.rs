use std::path::{Path, PathBuf};
use std::process::Command;

/// Jalankan proses tanpa membuka jendela konsol (Windows).
pub fn run_hidden(cmd: &mut Command) -> anyhow::Result<std::process::Output> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.output().map_err(|e| anyhow::anyhow!("Gagal menjalankan program: {e}"))
}

/// Nama file keluaran bawaan: <folder input>/<nama>_<suffix>.<ext>
pub fn default_out(input: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!("{stem}_{suffix}.{ext}"))
}

pub fn open_in_shell(path: &Path) {
    #[cfg(windows)]
    {
        let _ = Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}

pub fn reveal_in_shell(path: &Path) {
    #[cfg(windows)]
    {
        let _ = Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        if let Some(p) = path.parent() {
            let _ = Command::new("xdg-open").arg(p).spawn();
        }
    }
}

/// Tanggal/jam UTC "YYYY-MM-DD HH:MM" tanpa dependensi tambahan.
pub fn now_utc_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (h, m) = (rem / 3600, (rem % 3600) / 60);
    // civil_from_days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

pub fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}
