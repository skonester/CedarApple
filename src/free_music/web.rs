//! The HTTP the Free Music browser needs: JSON with a timeout, size-capped
//! downloads and file names safe on every system.
//!
//! Vivi's `plugins/web.rs` (MIT, Copyright (c) 2026 Ruslan Popelyshyn) runs on
//! ureq; CedarApple already links reqwest, so the same helpers sit on its
//! blocking client. Every call here blocks: they run on the browser's worker
//! threads, never on the UI thread (which is inside the tokio runtime, where
//! reqwest's blocking client refuses to run).

use std::sync::OnceLock;
use std::time::Duration;

/// `CedarApple/<version> ( https://github.com/skonester/CedarApple )`.
pub fn user_agent() -> String {
    format!(
        "CedarApple/{} ( https://github.com/skonester/CedarApple )",
        env!("CARGO_PKG_VERSION")
    )
}

/// One client for every request (it owns a connection pool and a runtime
/// thread); timeouts are set per request.
pub fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(user_agent())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("HTTP client")
    })
}

/// GET a JSON document within `secs`; non-2xx answers become errors naming `what`.
pub fn get_json_timeout(
    url: &str,
    query: &[(&str, &str)],
    what: &str,
    secs: u64,
) -> Result<serde_json::Value, String> {
    let url = url::Url::parse_with_params(url, query).map_err(|e| format!("{what}: {e}"))?;
    let r = client()
        .get(url)
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(secs))
        .send()
        .map_err(|e| format!("{what}: {}", without_url(&e)))?;
    let status = r.status().as_u16();
    let body = read_capped(r, 8 * 1024 * 1024).map_err(|e| format!("{what}: {e}"))?;
    let body = String::from_utf8_lossy(&body);
    match status {
        200..=299 => serde_json::from_str(&body).map_err(|e| format!("{what}: {e}")),
        503 | 429 => Err(format!("{what}: rate limited, try again in a moment")),
        _ => {
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v["error"]["message"]
                        .as_str()
                        .or(v["error"].as_str())
                        .or(v["headers"]["error_message"].as_str())
                        .map(String::from)
                })
                .unwrap_or_default();
            Err(format!("{what} error {status} {msg}").trim().to_string())
        }
    }
}

/// Download a URL into memory (redirects followed), at most `limit` bytes.
/// `Ok(None)` for 404.
pub fn get_bytes(url: &str, limit: u64) -> Result<Option<Vec<u8>>, String> {
    let r = client()
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|e| without_url(&e))?;
    if r.status().as_u16() == 404 {
        return Ok(None);
    }
    if !r.status().is_success() {
        return Err(format!("download failed ({})", r.status()));
    }
    read_capped(r, limit).map(Some)
}

fn read_capped(r: reqwest::blocking::Response, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    r.take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    if out.len() as u64 > limit {
        return Err(format!("larger than {} MB", limit / (1024 * 1024)));
    }
    Ok(out)
}

/// reqwest puts the whole URL in its errors; Jamendo's carries the client ID,
/// and the error ends up in the status line.
pub fn without_url(e: &reqwest::Error) -> String {
    let s = e.to_string();
    match e.url() {
        Some(u) => s.replace(u.as_str(), "…"),
        None => s,
    }
}

/// A file name without path separators or characters Windows rejects.
pub fn safe_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() {
        "untitled".into()
    } else {
        t.chars().take(120).collect()
    }
}

/// Open a web page in the system browser.
pub fn open_url(url: &str) {
    if !url.starts_with("http") {
        return;
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(safe_name("AC/DC: Live?"), "AC_DC_ Live_");
        assert_eq!(safe_name(" .. "), "untitled");
    }
}
