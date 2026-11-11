//! Resolve YouTube with yt-dlp, as Lumtub does, then play its streams in mpv.
//! Tools live in CedarApple's cache; release downloads are SHA-256 verified.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct Resolved {
    pub url: String,
    pub title: String,
    pub channel: String,
    pub channel_id: String,
    pub duration: u64,
    pub user_agent: String,
}

fn cache() -> Result<PathBuf, String> {
    directories::ProjectDirs::from("", "", "CedarApple")
        .map(|p| p.cache_dir().join("lumtub-tools"))
        .ok_or_else(|| "Could not find CedarApple's cache folder".into())
}

fn executable(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn find(name: &str, root: &Path) -> Option<PathBuf> {
    let file = executable(name);
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join(&file)))
        .filter(|p| p.is_file())
        .or_else(|| Some(root.join(&file)).filter(|p| p.is_file()))
        .or_else(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join(&file))
                    .find(|p| p.is_file())
            })
        })
}

fn assets() -> Result<(&'static str, &'static str), String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok(("yt-dlp.exe", "deno-x86_64-pc-windows-msvc.zip")),
        ("windows", "aarch64") => Ok(("yt-dlp_arm64.exe", "deno-aarch64-pc-windows-msvc.zip")),
        ("linux", "x86_64") => Ok(("yt-dlp_linux", "deno-x86_64-unknown-linux-gnu.zip")),
        ("linux", "aarch64") => Ok(("yt-dlp_linux_aarch64", "deno-aarch64-unknown-linux-gnu.zip")),
        ("macos", "x86_64") => Ok(("yt-dlp_macos", "deno-x86_64-apple-darwin.zip")),
        ("macos", "aarch64") => Ok(("yt-dlp_macos", "deno-aarch64-apple-darwin.zip")),
        _ => Err("Install yt-dlp and Deno on PATH for this platform.".into()),
    }
}

async fn download(repo: &str, asset: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .user_agent("CedarApple-Lumtub")
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;
    let release: Value = client
        .get(format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let info = release["assets"]
        .as_array()
        .and_then(|list| list.iter().find(|v| v["name"] == asset))
        .ok_or_else(|| {
            format!("No {asset} in the latest {repo} release. Install the tool on PATH.")
        })?;
    let digest = info["digest"]
        .as_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| format!("No SHA-256 checksum for {asset}"))?;
    let url = info["browser_download_url"]
        .as_str()
        .ok_or("Missing release download URL")?;
    if !url.starts_with(&format!("https://github.com/{repo}/releases/download/")) {
        return Err("Unexpected tool download address".into());
    }
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    verify(&bytes, digest)?;
    Ok(bytes.to_vec())
}

fn verify(bytes: &[u8], digest: &str) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(digest) {
        Ok(())
    } else {
        Err("Tool download failed SHA-256 verification".into())
    }
}

async fn save(root: &Path, name: &str, bytes: Vec<u8>) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(root)
        .await
        .map_err(|e| e.to_string())?;
    let path = root.join(executable(name));
    let temp = root.join(format!("{name}.download"));
    tokio::fs::write(&temp, bytes)
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755))
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::rename(&temp, &path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

pub(crate) async fn ready() -> Result<(PathBuf, PathBuf), String> {
    let _guard = INSTALL.lock().await;
    let root = cache()?;
    let yt = find("yt-dlp", &root);
    let deno = find("deno", &root);
    if let (Some(yt), Some(deno)) = (&yt, &deno) {
        return Ok((yt.clone(), deno.clone()));
    }
    let (yt_asset, deno_asset) = assets()?;
    let yt = match yt {
        Some(path) => path,
        None => save(&root, "yt-dlp", download("yt-dlp/yt-dlp", yt_asset).await?).await?,
    };
    let deno = match deno {
        Some(path) => path,
        None => {
            let bytes = download("denoland/deno", deno_asset).await?;
            let unpacked = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
                use std::io::Read;
                let mut zip =
                    zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
                let mut entry = zip
                    .by_name(&executable("deno"))
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
                Ok(out)
            })
            .await
            .map_err(|e| e.to_string())??;
            save(&root, "deno", unpacked).await?
        }
    };
    Ok((yt, deno))
}

pub async fn resolve(id: &str) -> Result<Resolved, String> {
    if !super::catalog::video_id(id) {
        return Err("Invalid YouTube video ID".into());
    }
    let (yt, deno) = ready().await?;
    // Lumtub's StreamResolver.quick starts with the visionOS client, whose
    // VOD URLs need neither a JavaScript signature nor a PO token. Ask the
    // same client through yt-dlp, retaining its full extractor as fallback.
    match resolve_using(&yt, &deno, id, Some("visionos")).await {
        Ok(video) => Ok(video),
        Err(_) => resolve_using(&yt, &deno, id, None).await,
    }
}

async fn resolve_using(
    yt: &Path,
    deno: &Path,
    id: &str,
    client: Option<&str>,
) -> Result<Resolved, String> {
    let mut cmd = Command::new(yt);
    cmd.kill_on_drop(true).args([
        "--ignore-config",
        "--no-playlist",
        "--no-warnings",
        "--skip-download",
        "--dump-single-json",
        "--socket-timeout",
        "15",
        "--retries",
        "1",
        "--extractor-retries",
        "1",
        "--js-runtimes",
        &format!("deno:{}", deno.display()),
        "--format",
        "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best",
    ]);
    // Resolve and fetch through the same route; signed URLs are IP-bound.
    cmd.args(["--proxy", crate::settings::get().network.http_proxy.trim()]);
    if let Some(client) = client {
        cmd.args([
            "--extractor-args",
            &format!("youtube:player_client={client}"),
        ]);
    }
    cmd.args(["--", &format!("https://www.youtube.com/watch?v={id}")]);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(Duration::from_secs(120), cmd.output())
        .await
        .map_err(|_| "YouTube took too long to resolve this video. Try again.".to_string())?
        .map_err(|e| format!("Could not start yt-dlp: {e}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        // Never include signed media URLs or cookies in the UI.
        let hint = if error.contains("Sign in") || error.contains("sign in") {
            "YouTube requires sign-in for this video."
        } else if error.contains("unavailable") || error.contains("not available") {
            "This video is unavailable in your region or has been removed."
        } else if error.contains("Requested format") {
            "YouTube did not offer a playable format."
        } else {
            "YouTube could not resolve this video. Check your connection and try again."
        };
        return Err(hint.into());
    }
    let mut info: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Could not read the video information: {e}"))?;
    // Lumtub's playback_open.dart / ResolvedVideo.directAddress registers
    // each VOD stream with NativeRelay before building the EDL. Keep this
    // shared by the native browser and the Lua YouTube Search Slice.
    tokio::task::spawn_blocking(move || {
        relay_streams(&mut info)?;
        from_info(&info)
    })
    .await
    .map_err(|_| "Could not prepare video playback".to_string())?
}

fn relay_streams(info: &mut Value) -> Result<(), String> {
    if info["is_live"] == true || info["live_status"] == "is_live" {
        return Ok(());
    }
    let agent = from_info(info)?.user_agent;
    let proxy = crate::settings::get().network.http_proxy.trim().to_owned();
    let proxy = (!proxy.is_empty()).then_some(proxy);
    let tracks = match info.get_mut("requested_formats") {
        Some(Value::Array(list)) if !list.is_empty() => list.iter_mut().collect::<Vec<_>>(),
        _ => vec![],
    };
    if tracks.is_empty() {
        relay_track(info, &agent, proxy)?;
    } else {
        for track in tracks {
            relay_track(track, &agent, proxy.clone())?;
        }
    }
    Ok(())
}

fn relay_track(track: &mut Value, agent: &str, proxy: Option<String>) -> Result<(), String> {
    // Manifest/fragment URLs stay with mpv; the relay serves complete files.
    if track.get("fragments").is_some_and(|v| !v.is_null())
        || track["protocol"]
            .as_str()
            .is_some_and(|p| !matches!(p, "http" | "https"))
    {
        return Ok(());
    }
    let url = track["url"]
        .as_str()
        .ok_or("Missing video stream address")?;
    super::relay::check(url, agent, proxy.clone())?;
    let local = super::relay::register(url, track["filesize"].as_u64(), agent, proxy)?;
    track["url"] = local.into();
    Ok(())
}

pub fn from_info(info: &Value) -> Result<Resolved, String> {
    let tracks: Vec<&Value> = match info["requested_formats"].as_array() {
        Some(list) if !list.is_empty() => list.iter().collect(),
        _ => vec![info],
    };
    let mut addresses = Vec::new();
    for track in &tracks {
        let url = track["url"]
            .as_str()
            .ok_or("YouTube did not return a stream address")?;
        let parsed = url::Url::parse(url).map_err(|_| "Invalid stream address")?;
        if !matches!(parsed.scheme(), "https" | "http") {
            return Err("Unsupported video stream".into());
        }
        addresses.push(url.to_owned());
    }
    // Lumtub's resolved.rs::edl: byte lengths protect URLs containing EDL punctuation.
    let url = if addresses.len() == 1 {
        addresses.remove(0)
    } else {
        edl(&addresses)
    };
    let string = |key| info[key].as_str().unwrap_or("").to_owned();
    let user_agent = tracks
        .iter()
        .find_map(|t| t["http_headers"]["User-Agent"].as_str())
        .or_else(|| info["http_headers"]["User-Agent"].as_str())
        .unwrap_or(super::catalog::USER_AGENT)
        .to_owned();
    Ok(Resolved {
        url,
        title: string("title"),
        channel: string("channel"),
        channel_id: string("channel_id"),
        duration: info["duration"].as_f64().unwrap_or(0.0).max(0.0) as u64,
        user_agent,
    })
}

fn edl(addresses: &[String]) -> String {
    let parts: Vec<String> = addresses
        .iter()
        .map(|a| format!("!new_stream;!no_clip;!no_chapters;%{}%{a}", a.len()))
        .collect();
    format!("edl://{}", parts.join(";"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "downloads playback tools and contacts YouTube; run explicitly"]
    async fn live_resolution_smoke() {
        let video = resolve("aqz-KE-bpKQ").await.unwrap();
        assert!(!video.title.is_empty());
        assert!(video.url.starts_with("https://") || video.url.starts_with("edl://"));
    }
    #[test]
    fn verifies_downloads_before_installing() {
        assert!(
            verify(
                b"abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            )
            .is_ok()
        );
        assert!(
            verify(
                b"tampered",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            )
            .is_err()
        );
    }
    #[test]
    fn keeps_audio_and_video_together_and_escapes_edl_urls() {
        let result = from_info(&serde_json::json!({"title": "Test", "requested_formats": [
            {"url": "https://a/é;one"}, {"url": "https://b/audio"}
        ]}))
        .unwrap();
        assert_eq!(
            result.url,
            "edl://!new_stream;!no_clip;!no_chapters;%16%https://a/é;one;!new_stream;!no_clip;!no_chapters;%15%https://b/audio"
        );
        assert!(from_info(&serde_json::json!({"url": "file:///tmp/test"})).is_err());
    }
}
