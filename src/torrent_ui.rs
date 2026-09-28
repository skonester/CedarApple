//! The player-side half of torrent streaming: which files of a torrent are
//! videos and in what order, which subtitles belong to which episode, how a
//! stall is explained, and how sizes and rates are written.
//!
//! Ported from Frame Player's `torrent.svelte.ts` (`torrentVideos`,
//! `torrentSubtitles`, `torrentFailureText`), `open.svelte.ts` (`torrentLabel`,
//! `swarmAdvice`), `source.ts` (`parseTorrentUrl`, `magnetFor`, `torrentId`),
//! `format.ts` (`formatTime`, `displayName`) and `units.ts` (`fmtSize`,
//! `fmtSpeed`), GPL-3.0-or-later, Copyright (c) Evgenii Zakharov. English
//! strings are Frame Player's own.

use crate::torrent::{TorrentFile, TorrentInfo, TorrentStatus};
use std::cmp::Ordering;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "ts", "m2ts", "mts", "mpg", "mpeg", "vob",
    "ogv", "m4v", "3gp", "rm", "rmvb", "y4m",
];

pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "sub", "idx", "vtt", "sup", "smi", "mks"];

/// How long "live, no peers" lasts before the overlay says what that usually
/// means. Long enough for a healthy swarm to have answered.
pub const PEERLESS_ADVICE_MS: u128 = 20_000;

pub fn extension_of(path: &str) -> String {
    let base = base_name(path);
    match base.rsplit_once('.') {
        Some((_, ext)) => ext.to_ascii_lowercase(),
        None => String::new(),
    }
}

/// Last path component. Both separators, because paths arrive from either OS.
pub fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().filter(|s| !s.is_empty()).unwrap_or(path)
}

fn decode_maybe(s: &str) -> String {
    urlencoding::decode(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string())
}

/// What a source is called on screen: a magnet's `dn`, or a file name without
/// its extension and with the dots release names use for spaces.
pub fn display_name(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.to_ascii_lowercase().starts_with("magnet:") {
        let dn = trimmed
            .split(['?', '&'])
            .find_map(|kv| kv.strip_prefix("dn=").or_else(|| kv.strip_prefix("DN=")));
        if let Some(dn) = dn {
            let name = decode_maybe(&dn.replace('+', " ")).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
        return trimmed.to_string();
    }
    let clean = trimmed.split(['?', '#']).next().unwrap_or(trimmed);
    let base = decode_maybe(base_name(clean));
    let name = match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => base.clone(),
    };
    // A dot between two non-spaces is a release-name space.
    let chars: Vec<char> = name.chars().collect();
    let spaced: String = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let between = i > 0
                && i + 1 < chars.len()
                && !chars[i - 1].is_whitespace()
                && !chars[i + 1].is_whitespace();
            if c == '.' && between { ' ' } else { c }
        })
        .collect();
    let spaced = spaced.trim().to_string();
    if spaced.is_empty() { base } else { spaced }
}

/// `m:ss`, or `h:mm:ss` from an hour up.
pub fn format_time(seconds: f64) -> String {
    let s = if seconds.is_finite() && seconds >= 0.0 { seconds } else { 0.0 };
    let total = s as u64;
    let (h, m, sec) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

pub fn fmt_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1024f64.powi(3);
    if gb >= 1.0 {
        format!("{gb:.2} GB")
    } else {
        format!("{} MB", (bytes as f64 / 1024f64.powi(2)).round() as u64)
    }
}

/// A download rate, in bytes — the unit a torrent client is read in.
pub fn fmt_speed(bytes_per_second: f64) -> String {
    if bytes_per_second >= 1024f64.powi(2) {
        format!("{:.1} MB/s", bytes_per_second / 1024f64.powi(2))
    } else {
        format!("{} KB/s", (bytes_per_second / 1024.0).round() as u64)
    }
}

/// `http://127.0.0.1:<port>/t/<infohash>/<index>/…` → `(infohash, index)`.
/// The queue carries this pair already; this reads it back from a bare URL.
/// The port is not part of the identity: it changes every run.
#[allow(dead_code)]
pub fn parse_torrent_url(src: &str) -> Option<(String, usize)> {
    let rest = src
        .trim()
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| src.trim().strip_prefix("https://127.0.0.1:"))?;
    let (port, rest) = rest.split_once('/')?;
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut parts = rest.splitn(4, '/');
    if parts.next()? != "t" {
        return None;
    }
    let hash = parts.next()?;
    let index = parts.next()?.parse().ok()?;
    (hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| (hash.to_ascii_lowercase(), index))
}

/// `torrent:<infohash>/<index>` — the key positions are remembered under.
pub fn torrent_id(info_hash: &str, index: usize) -> String {
    format!("torrent:{}/{index}", info_hash.to_ascii_lowercase())
}

pub fn magnet_for(info_hash: &str, name: Option<&str>, trackers: &[String]) -> String {
    let dn = name
        .map(|n| format!("&dn={}", urlencoding::encode(n)))
        .unwrap_or_default();
    let tr: String = trackers
        .iter()
        .map(|t| format!("&tr={}", urlencoding::encode(t)))
        .collect();
    format!("magnet:?xt=urn:btih:{info_hash}{dn}{tr}")
}

/// Whether a link box entry is something the torrent engine opens rather than
/// something mpv opens directly.
pub fn is_torrent_source(source: &str) -> bool {
    let s = source.trim();
    let lower = s.to_ascii_lowercase();
    lower.starts_with("magnet:")
        || (s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        || lower.split(['?', '#']).next().unwrap_or("").ends_with(".torrent")
}

/// Natural order, the way a file manager sorts: `E2` before `E10`, case
/// ignored. Stands in for `Intl.Collator({ numeric: true, sensitivity: 'base' })`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = a.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    a.next();
                }
                let mut nb = String::new();
                while let Some(c) = b.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    b.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Video files of a torrent, in the order a queue should play them.
pub fn torrent_videos(info: &TorrentInfo) -> Vec<TorrentFile> {
    let mut videos: Vec<TorrentFile> = info
        .files
        .iter()
        .filter(|f| VIDEO_EXTENSIONS.contains(&extension_of(&f.path).as_str()))
        .cloned()
        .collect();
    videos.sort_by(|a, b| natural_cmp(&a.path, &b.path));
    videos
}

/// The subtitle files that belong to one video of a torrent.
///
/// A single-video torrent owns all of its subtitles. In a season, a subtitle
/// belongs to an episode when its path carries that episode's file stem, which
/// is how release groups name them (`Show.S01E03.en.srt` beside
/// `Show.S01E03.mkv`, or `Subs/Show.S01E03/2_English.srt`).
pub fn torrent_subtitles(info: &TorrentInfo, video_path: &str) -> Vec<TorrentFile> {
    let subs: Vec<TorrentFile> = info
        .files
        .iter()
        .filter(|f| SUBTITLE_EXTENSIONS.contains(&extension_of(&f.path).as_str()))
        .cloned()
        .collect();
    if subs.is_empty() {
        return subs;
    }
    let videos = info
        .files
        .iter()
        .filter(|f| VIDEO_EXTENSIONS.contains(&extension_of(&f.path).as_str()))
        .count();
    if videos <= 1 {
        return subs;
    }
    let base = base_name(video_path);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base).to_lowercase();
    if stem.is_empty() {
        return Vec::new();
    }
    subs.into_iter()
        .filter(|f| f.path.to_lowercase().contains(&stem))
        .collect()
}

/// The torrent line under a stall, and in the top-right readout: *why* it is
/// waiting. No peers at all and a slow swarm are different problems.
pub fn torrent_label(s: &TorrentStatus) -> Option<String> {
    match s.state.as_str() {
        "gone" => None,
        "error" => Some(s.error.clone().unwrap_or_else(|| "Torrent error".into())),
        // librqbit hashing what is already on disk: peers are legitimately 0.
        "initializing" => Some("Checking what is downloaded…".into()),
        _ if s.peers == 0 => Some(if s.peers_seen > 0 {
            "Connecting to the swarm…".into()
        } else {
            "No peers".into()
        }),
        _ => Some(format!("{} peers · {}", s.peers, fmt_speed(s.down_bps))),
    }
}

/// After a while live with no peers, say what that usually means: addresses
/// arriving while no connection is made is the signature of BitTorrent being
/// filtered, the one cause a viewer can act on.
pub fn swarm_advice(s: &TorrentStatus, peerless_ms: u128) -> Option<String> {
    if s.state != "live" || s.peers > 0 || peerless_ms < PEERLESS_ADVICE_MS {
        return None;
    }
    Some(if s.peers_seen > 0 {
        "Peers are being found but no connection is being made — usually a VPN or an ISP blocking BitTorrent.".into()
    } else {
        "Nobody is answering. A VPN or an ISP that blocks BitTorrent looks exactly like this.".into()
    })
}

pub fn torrent_failure_text(reason: &str) -> String {
    match reason {
        "resolve_timeout" => "Nobody answered. This torrent looks like it has no seeders.".into(),
        "route_no_direct" => "There is no way out past the VPN right now.".into(),
        r => match r.strip_prefix("route_missing:") {
            Some(name) => format!("The network interface {name} is not connected."),
            None => format!("Could not open the torrent: {r}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_sorts_episodes() {
        let mut v = vec!["Show E10.mkv", "show e2.mkv", "Show E1.mkv"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["Show E1.mkv", "show e2.mkv", "Show E10.mkv"]);
    }

    #[test]
    fn torrent_url_round_trip() {
        let hash = "0123456789abcdef0123456789abcdef01234567";
        let url = format!("http://127.0.0.1:51234/t/{hash}/3/Show.S01E03.mkv");
        assert_eq!(parse_torrent_url(&url), Some((hash.to_string(), 3)));
        assert_eq!(parse_torrent_url("http://example.com/t/x/1/a"), None);
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name("C:\\films\\The.Movie.2024.mkv"), "The Movie 2024");
        assert_eq!(display_name("magnet:?xt=urn:btih:abc&dn=Some+Show"), "Some Show");
        assert_eq!(format_time(3725.0), "1:02:05");
        assert_eq!(format_time(65.0), "1:05");
    }
}
