//! What the player remembers between runs: the torrents that were opened (so a
//! season can be reopened from the start screen without its magnet) and the
//! position reached in each file.
//!
//! Ported from Frame Player's `torrent.svelte.ts` (`rememberTorrent`,
//! `markWatched`, `torrentResume`, `torrentPositions`) and the position half of
//! `history.svelte.ts`, GPL-3.0-or-later, Copyright (c) Evgenii Zakharov. Frame
//! Player keeps these in the webview's localStorage; CedarApple has no webview,
//! so they live in one JSON file in the data directory instead. The shape and
//! the rules are the same.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Past this fraction a file counts as watched: the credits are not the film.
pub const FINISHED_FRACTION: f64 = 0.97;

/// How many torrents are remembered. Oldest first out.
const STORE_LIMIT: usize = 100;
/// How many file positions are remembered.
const POSITIONS_LIMIT: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RememberedTorrent {
    pub info_hash: String,
    /// What reopens it. For a `.torrent` file this is a magnet built from the
    /// hash: the metadata is cached on disk, so it opens without a lookup.
    pub magnet: String,
    pub name: Option<String>,
    pub videos: usize,
    pub at: i64,
    /// File names (not indices) that were watched to the end, so "delete the
    /// watched episodes" survives an uploader inserting one.
    #[serde(default)]
    pub watched: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    pub pos: f64,
    pub dur: f64,
    pub title: String,
    pub ts: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    #[serde(default)]
    torrents: HashMap<String, RememberedTorrent>,
    #[serde(default)]
    positions: HashMap<String, Position>,
}

fn path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "CedarApple")?;
    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir).ok()?;
    Some(dir.join("history.json"))
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| {
        let loaded = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Mutex::new(loaded)
    })
}

/// Written through a temp file and a rename, so a crash mid-write leaves the
/// previous history rather than half of one.
fn save(s: &Store) {
    let Some(p) = path() else { return };
    let Ok(bytes) = serde_json::to_vec(s) else { return };
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn remember_torrent(info_hash: &str, magnet: &str, name: Option<String>, videos: usize) {
    let mut s = store().lock().unwrap();
    let hash = info_hash.to_ascii_lowercase();
    let watched = s.torrents.get(&hash).map(|t| t.watched.clone()).unwrap_or_default();
    s.torrents.insert(
        hash.clone(),
        RememberedTorrent {
            info_hash: hash,
            magnet: magnet.to_string(),
            name,
            videos,
            at: now(),
            watched,
        },
    );
    if s.torrents.len() > STORE_LIMIT {
        let mut all: Vec<_> = s.torrents.values().map(|t| (t.at, t.info_hash.clone())).collect();
        all.sort();
        for (_, h) in all.iter().take(s.torrents.len() - STORE_LIMIT) {
            s.torrents.remove(h);
        }
    }
    save(&s);
}

pub fn remembered_torrent(info_hash: &str) -> Option<RememberedTorrent> {
    store()
        .lock()
        .unwrap()
        .torrents
        .get(&info_hash.to_ascii_lowercase())
        .cloned()
}

pub fn forget_torrent(info_hash: &str) {
    let mut s = store().lock().unwrap();
    s.torrents.remove(&info_hash.to_ascii_lowercase());
    let prefix = format!("torrent:{}/", info_hash.to_ascii_lowercase());
    s.positions.retain(|id, _| !id.starts_with(&prefix));
    save(&s);
}

pub fn mark_watched(info_hash: &str, name: &str) {
    if name.is_empty() {
        return;
    }
    let mut s = store().lock().unwrap();
    let Some(entry) = s.torrents.get_mut(&info_hash.to_ascii_lowercase()) else {
        return;
    };
    if entry.watched.iter().any(|w| w == name) {
        return;
    }
    entry.watched.push(name.to_string());
    save(&s);
}

pub fn watched_files(info_hash: &str) -> Vec<String> {
    remembered_torrent(info_hash).map(|t| t.watched).unwrap_or_default()
}

/// Forget that these were watched, once their files are gone.
pub fn clear_watched(info_hash: &str) {
    let mut s = store().lock().unwrap();
    if let Some(entry) = s.torrents.get_mut(&info_hash.to_ascii_lowercase()) {
        entry.watched.clear();
        save(&s);
    }
}

pub fn save_position(id: &str, pos: f64, dur: f64, title: &str) {
    if id.is_empty() || !(pos.is_finite() && dur.is_finite()) || dur <= 0.0 {
        return;
    }
    let mut s = store().lock().unwrap();
    s.positions.insert(
        id.to_string(),
        Position {
            pos,
            dur,
            title: title.to_string(),
            ts: now(),
        },
    );
    if s.positions.len() > POSITIONS_LIMIT {
        let mut all: Vec<_> = s.positions.iter().map(|(k, v)| (v.ts, k.clone())).collect();
        all.sort();
        for (_, k) in all.iter().take(s.positions.len() - POSITIONS_LIMIT) {
            s.positions.remove(k);
        }
    }
    save(&s);
}

pub fn position(id: &str) -> Option<Position> {
    store().lock().unwrap().positions.get(id).cloned()
}

/// Every position saved for one torrent, by file index.
pub fn torrent_positions(info_hash: &str) -> HashMap<usize, Position> {
    let prefix = format!("torrent:{}/", info_hash.to_ascii_lowercase());
    store()
        .lock()
        .unwrap()
        .positions
        .iter()
        .filter_map(|(id, p)| {
            let index = id.strip_prefix(&prefix)?.parse().ok()?;
            Some((index, p.clone()))
        })
        .collect()
}

/// The file of a torrent that was watched most recently, for "continue".
pub fn torrent_resume(info_hash: &str) -> Option<(usize, Position)> {
    torrent_positions(info_hash)
        .into_iter()
        .max_by_key(|(_, p)| p.ts)
}
