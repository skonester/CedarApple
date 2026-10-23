//! Free Music: search and browse Creative Commons / public-domain music on
//! Jamendo (API v3, free `client_id` from <https://devportal.jamendo.com>) and the
//! Internet Archive (advancedsearch + metadata APIs, no key; its `freemusicarchive`
//! collection stands in for the Free Music Archive, whose API is gone). Both sources
//! are searched in parallel and merged with source badges; filters (genre, mood,
//! licence, length), sort, infinite scroll, album / artist pages and "play similar".
//! Downloads land in a folder with tags, cover, licence and attribution (in the
//! tags and in a `<file>.licence.txt`); downloaded tracks are marked and play from
//! the file. The licence is shown on every row, in the detail pane and on the
//! now-playing card.
//!
//! Ported from Vivi's Free Music plugin (`src/plugins/free_music/mod.rs`, MIT,
//! Copyright (c) 2026 Ruslan Popelyshyn; see licenses/MIT-vivi.txt). Vivi runs it
//! as a plugin inside a shared music dialog and a settings page; here it is one
//! dialog of its own (ui/music.slint) opened from the start screen, the top bar or
//! the context menu. Its settings (Jamendo client ID, download folder) are the Free
//! Music category of Settings (src/settings.rs), kept in `free-music.json` beside
//! history.json.
//! Playback goes through `playback::play_music`, so a song is a queue entry like
//! any file.

mod api;
mod catalog;
mod tags;
mod web;

pub use api::*;
pub use catalog::*;
use tags::Tags;

use crate::{AppWindow, FmFilter, FmRow, FreeMusicState};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

pub const JAMENDO: &str = "Jamendo";
pub const IA: &str = "Internet Archive";
pub const FMA: &str = "Free Music Archive";
/// Source dropdown; FMA is searched through its Internet Archive mirror.
pub const SOURCES: [&str; 4] = ["All sources", JAMENDO, IA, FMA];
const NO_KEY: &str = "Jamendo needs a client ID (free at devportal.jamendo.com; paste it in Free Music settings). Showing the Internet Archive only.";
const NO_KEY_NOTE: &str = "Jamendo is off (no client ID in the settings).";
pub const PAGE: usize = 24;

/// Browse chips: three lists, then genres (label, Jamendo tag / IA subject).
pub const CHIPS: [(&str, &str); 14] = [
    ("Popular", ""),
    ("New", ""),
    ("Featured", ""),
    ("Electronic", "electronic"),
    ("Rock", "rock"),
    ("Pop", "pop"),
    ("Hip-hop", "hiphop"),
    ("Jazz", "jazz"),
    ("Classical", "classical"),
    ("Ambient", "ambient"),
    ("Lo-fi", "lofi"),
    ("Folk", "folk"),
    ("Metal", "metal"),
    ("Soundtrack", "soundtrack"),
];
pub const MOODS: [&str; 8] = [
    "Any mood",
    "Relaxing",
    "Happy",
    "Energetic",
    "Sad",
    "Dark",
    "Romantic",
    "Epic",
];
pub const LICENCES: [&str; 6] = [
    "Any licence",
    "Commercial use OK",
    "Remix allowed",
    "CC BY",
    "CC BY-SA",
    "CC0 / public domain",
];
pub const LENGTHS: [&str; 4] = ["Any length", "Under 3 min", "3–6 min", "Over 6 min"];
pub const SORTS: [&str; 5] = ["Best match", "Popular", "Newest", "Shortest", "Longest"];

const RADIO: usize = 12;
const DEBOUNCE: Duration = Duration::from_millis(450);
/// Longest side of a row thumbnail and of the large covers (detail pane, card).
const ROW_PX: u32 = 96;
const LARGE_PX: u32 = 400;

type Pixels = SharedPixelBuffer<Rgba8Pixel>;

enum Msg {
    /// Tracks of one source for search `epoch`; `done` = that source finished.
    Page {
        epoch: u64,
        src: &'static str,
        pos: usize,
        tracks: Vec<Track>,
        more: bool,
        done: bool,
    },
    Fail {
        epoch: u64,
        src: &'static str,
        err: String,
    },
    /// Row thumbnail decoded: cover URL, pixels (`None`: no usable cover).
    Cover(String, Option<Pixels>),
    /// Large cover of the detail pane: cover URL, pixels.
    Detail(String, Option<Pixels>),
    /// Track id, title, result.
    Saved(String, String, Result<PathBuf, String>),
}

// ---- Settings ----------------------------------------------------------------

/// What the settings button keeps, in `free-music.json` beside history.json.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
struct Settings {
    #[serde(default)]
    jamendo_client_id: String,
    /// Empty: [`default_folder`].
    #[serde(default)]
    folder: String,
}

fn settings_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "CedarApple")?;
    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir).ok()?;
    Some(dir.join("free-music.json"))
}

impl Settings {
    fn load() -> Self {
        settings_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Through a temp file and a rename, like history.json.
    fn save(&self) {
        let Some(p) = settings_path() else { return };
        let Ok(bytes) = serde_json::to_vec_pretty(self) else { return };
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }

    /// The setting, else `JAMENDO_CLIENT_ID` from the environment, else one baked
    /// in at build time (`CEDARAPPLE_JAMENDO_CLIENT_ID`). Never a client secret.
    fn jamendo_key(&self) -> String {
        let k = self.jamendo_client_id.trim().to_string();
        if !k.is_empty() {
            return k;
        }
        std::env::var("JAMENDO_CLIENT_ID")
            .ok()
            .filter(|k| !k.trim().is_empty())
            .or(option_env!("CEDARAPPLE_JAMENDO_CLIENT_ID").map(String::from))
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    fn folder(&self) -> PathBuf {
        let f = self.folder.trim();
        if f.is_empty() {
            default_folder()
        } else if let Some(r) = f.strip_prefix("~/") {
            home().join(r)
        } else {
            PathBuf::from(f)
        }
    }
}

fn home() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_default()
}

/// `<Music>/Free Music` (one folder per artist inside).
fn default_folder() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|u| u.audio_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| home().join("Music"))
        .join("Free Music")
}

// ---- Covers ------------------------------------------------------------------

fn covers_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "CedarApple")
        .map(|d| d.cache_dir().join("free-music-covers"))
        .unwrap_or_else(|| std::env::temp_dir().join("cedarapple-free-music-covers"))
}

/// FNV-1a of the URL: the cache file name (stable across builds, unlike
/// `DefaultHasher`).
fn hash_name(url: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

const COVER_EXTS: [&str; 3] = ["jpg", "png", "webp"];

/// Cover already in the cache (no download).
fn cached_cover(url: &str) -> Option<PathBuf> {
    let base = covers_dir().join(hash_name(url));
    COVER_EXTS
        .iter()
        .map(|e| base.with_extension(e))
        .find(|p| p.is_file())
}

fn fetch_cover(url: &str) -> Option<PathBuf> {
    if let Some(p) = cached_cover(url) {
        return Some(p);
    }
    let base = covers_dir().join(hash_name(url));
    // A cover that failed once is not asked for again.
    if base.with_extension("none").is_file() {
        return None;
    }
    std::fs::create_dir_all(covers_dir()).ok()?;
    let b = web::get_bytes(url, 4 * 1024 * 1024).ok().flatten();
    let ext = match b.as_deref() {
        Some([0x89, b'P', b'N', b'G', ..]) => "png",
        Some([0xFF, 0xD8, ..]) => "jpg",
        Some([b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..]) => "webp",
        _ => {
            let _ = std::fs::write(base.with_extension("none"), b"");
            return None;
        }
    };
    let p = base.with_extension(ext);
    std::fs::write(&p, b?).ok()?;
    Some(p)
}

/// Decode a cached cover to at most `max` px a side. `None` when it does not
/// decode or is no cover: the Internet Archive's "no image" thumbnail is a wide
/// banner, album art is about square.
fn decode(path: &Path, max: u32) -> Option<Pixels> {
    let img = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 || w >= 2 * h {
        return None;
    }
    let img = img.thumbnail(max, max).to_rgba8();
    Some(Pixels::clone_from_slice(img.as_raw(), img.width(), img.height()))
}

// ---- What is playing ---------------------------------------------------------

/// A Free Music track as the player shows it: the now-playing card and the
/// presence line.
#[derive(Clone)]
struct Meta {
    title: String,
    artist: String,
    album: String,
    cover: String,
    /// Source and licence.
    info: String,
}

thread_local! {
    static UI: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
    static BROWSER: RefCell<Option<Browser>> = const { RefCell::new(None) };
    /// Stream URL or downloaded file → the track it plays.
    static META: RefCell<HashMap<String, Meta>> = RefCell::new(HashMap::new());
    /// The URL whose card is up, so a late cover does not land on the next song.
    static NP_URL: RefCell<String> = const { RefCell::new(String::new()) };
}

fn window() -> Option<AppWindow> {
    UI.with(|u| u.borrow().as_ref().and_then(|w| w.upgrade()))
}

/// Remember what `url` plays, so it shows as a song rather than a file name.
fn describe(t: &Track, url: &str) {
    let meta = Meta {
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        cover: [&t.cover_full, &t.cover]
            .into_iter()
            .find(|u| u.starts_with("http"))
            .cloned()
            .unwrap_or_default(),
        // The card's info line names the source and the licence while it plays.
        info: format!("{} · {}", t.source, t.license()),
    };
    META.with(|m| m.borrow_mut().insert(url.to_string(), meta));
}

/// What plays changed (`url`, empty = nothing): show or hide the music card.
/// Returns (title, artist) of a Free Music track, for the presence line.
pub fn now_playing(url: &str) -> Option<(String, String)> {
    let meta = META.with(|m| m.borrow().get(url).cloned());
    NP_URL.with(|n| *n.borrow_mut() = url.to_string());
    let ui = window()?;
    let st = ui.global::<FreeMusicState>();
    let Some(m) = meta else {
        st.set_np_visible(false);
        return None;
    };
    st.set_np_title(m.title.clone().into());
    st.set_np_artist(
        [m.artist.as_str(), m.album.as_str()]
            .iter()
            .filter(|x| !x.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ")
            .into(),
    );
    st.set_np_info(m.info.clone().into());
    st.set_np_has_cover(false);
    st.set_np_visible(true);
    if !m.cover.is_empty() {
        let (cover, url) = (m.cover.clone(), url.to_string());
        std::thread::spawn(move || {
            let Some(px) = fetch_cover(&cover).and_then(|p| decode(&p, LARGE_PX)) else {
                return;
            };
            let _ = slint::invoke_from_event_loop(move || {
                if NP_URL.with(|n| *n.borrow() != url) {
                    return;
                }
                if let Some(ui) = window() {
                    let st = ui.global::<FreeMusicState>();
                    st.set_np_cover(slint::Image::from_rgba8(px));
                    st.set_np_has_cover(true);
                }
            });
        });
    }
    Some((m.title, m.artist))
}

// ---- The browser -------------------------------------------------------------

/// Label of the action that opens the track's page on its source.
fn page_label(t: &Track) -> String {
    match t.source {
        JAMENDO => "Open on Jamendo".into(),
        _ => "Open on Internet Archive".into(),
    }
}

struct Browser {
    settings: Settings,
    q: Query,
    epoch: u64,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Tracks of earlier pages (fixed order, infinite scroll appends below).
    base: Vec<Track>,
    /// Current page per source: (position in the API answer, tracks).
    page: HashMap<&'static str, std::collections::BTreeMap<usize, Vec<Track>>>,
    pending: HashSet<&'static str>,
    /// "Play similar" in progress: (search epoch, keys already queued).
    radio: Option<(u64, HashSet<String>)>,
    more: HashMap<&'static str, bool>,
    errors: Vec<String>,
    tracks: Vec<Track>,
    sel: Option<String>,
    covers: HashMap<String, slint::Image>,
    cover_asked: HashSet<String>,
    covers_dirty: bool,
    typed: Option<Instant>,
    /// Queries to go back to from album / artist pages.
    back: Vec<Query>,
    title: String,
    dl_queue: VecDeque<Track>,
    dl_active: Option<(Track, Arc<DlProgress>)>,
    /// The last thing that happened outside the list: a download saved or
    /// failed, a track sent to the player.
    last: String,
    note: String,
    /// The dialog's row model, updated in place (a new model rebuilds every row).
    model: Option<Rc<VecModel<FmRow>>>,
    /// Track id → downloaded already (file in the download folder), checked once.
    saved: HashMap<String, bool>,
    /// Cover URL wanted in the detail pane.
    detail_url: String,
}

impl Browser {
    fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            settings: Settings::load(),
            q: Query::default(),
            epoch: 0,
            tx,
            rx,
            base: Vec::new(),
            page: HashMap::new(),
            pending: HashSet::new(),
            radio: None,
            more: HashMap::new(),
            errors: Vec::new(),
            tracks: Vec::new(),
            sel: None,
            covers: HashMap::new(),
            cover_asked: HashSet::new(),
            covers_dirty: false,
            typed: None,
            back: Vec::new(),
            title: String::new(),
            dl_queue: VecDeque::new(),
            dl_active: None,
            last: String::new(),
            note: String::new(),
            model: None,
            saved: HashMap::new(),
            detail_url: String::new(),
        }
    }

    fn set_status(ui: &AppWindow, status: &str, busy: bool) {
        let st = ui.global::<FreeMusicState>();
        st.set_status(status.into());
        st.set_busy(busy);
    }

    fn selected(&self) -> Option<&Track> {
        let id = self.sel.as_ref()?;
        self.tracks.iter().find(|t| &t.id == id)
    }

    /// The track at row `i`, else the selected one.
    fn at(&self, i: i32) -> Option<Track> {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.tracks.get(i).cloned())
            .or_else(|| self.selected().cloned())
    }

    /// Start (or continue, for page > 0) the search for `self.q`.
    fn run(&mut self, ui: &AppWindow) {
        self.epoch += 1;
        self.typed = None;
        let epoch = self.epoch;
        if self.q.page == 0 {
            self.base.clear();
            self.tracks.clear();
            self.sel = None;
            let st = ui.global::<FreeMusicState>();
            st.set_detail("".into());
            st.set_copy_text("".into());
            st.set_has_detail_cover(false);
            st.set_generation(st.get_generation().wrapping_add(1));
            self.detail_url.clear();
        } else {
            self.base = self.tracks.clone();
        }
        self.page.clear();
        self.pending.clear();
        self.errors.clear();
        self.note.clear();
        let key = self.settings.jamendo_key();
        let q = self.q.clone();
        if q.uses(JAMENDO) {
            if key.is_empty() {
                if q.source == 1 || q.album.is_some() || q.artist.is_some() {
                    self.show(ui);
                    Self::set_status(ui, NO_KEY, false);
                    return;
                }
                // Short: it follows the result count on one status line.
                self.note = NO_KEY_NOTE.into();
            } else if self.more.get(JAMENDO).copied().unwrap_or(true) || q.page == 0 {
                self.pending.insert(JAMENDO);
                let (tx, q, key) = (self.tx.clone(), q.clone(), key.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(match jamendo_search(&key, &q) {
                        Ok((tracks, more)) => Msg::Page {
                            epoch,
                            src: JAMENDO,
                            pos: 0,
                            tracks,
                            more,
                            done: true,
                        },
                        Err(err) => Msg::Fail {
                            epoch,
                            src: JAMENDO,
                            err,
                        },
                    });
                });
            }
        }
        if q.uses(IA) && (self.more.get(IA).copied().unwrap_or(true) || q.page == 0) {
            self.pending.insert(IA);
            let (tx, q) = (self.tx.clone(), q.clone());
            std::thread::spawn(move || ia_job(epoch, &q, &tx));
        }
        if q.page == 0 {
            self.more.clear();
        }
        self.show(ui);
        self.status(ui);
    }

    fn status(&self, ui: &AppWindow) {
        if let Some((t, p)) = &self.dl_active {
            let pct = p.pm.load(Ordering::Relaxed) / 10;
            let left = self.dl_queue.len();
            let s = format!(
                "Downloading “{}” {pct}%{}",
                t.title,
                if left > 0 {
                    format!(" · {left} more queued")
                } else {
                    String::new()
                }
            );
            Self::set_status(ui, &s, !self.pending.is_empty());
            return;
        }
        let loading: Vec<&str> = [JAMENDO, IA]
            .into_iter()
            .filter(|s| self.pending.contains(s))
            .collect();
        let n = self.tracks.len();
        let mut s = if !loading.is_empty() {
            format!(
                "{}Searching {}…",
                if n > 0 {
                    format!("{n} tracks · ")
                } else {
                    String::new()
                },
                loading.join(" and ")
            )
        } else if n == 0 && !self.errors.is_empty() {
            self.errors.join(" · ")
        } else if n == 0 {
            "Nothing found. Try other words, another genre or fewer filters.".into()
        } else {
            let more = self.more.values().any(|m| *m);
            format!(
                "{n} tracks{}. Double-click plays; Download saves with tags and the licence.",
                if more { " (scroll for more)" } else { "" }
            )
        };
        if n > 0 && !self.errors.is_empty() && loading.is_empty() {
            s = format!("{s}  ({})", self.errors.join(" · "));
        }
        if !self.note.is_empty() && loading.is_empty() {
            s = format!("{s}  {}", self.note);
        }
        if !self.last.is_empty() && loading.is_empty() {
            s = format!("{} · {s}", self.last);
        }
        Self::set_status(ui, &s, !loading.is_empty());
    }

    /// Something happened that the list does not show: say it on the status line.
    fn flash(&mut self, ui: &AppWindow, msg: String) {
        self.last = msg;
        self.status(ui);
    }

    fn remerge(&mut self) {
        let order = [JAMENDO, IA];
        let lists: Vec<Vec<Track>> = order
            .iter()
            .filter_map(|s| self.page.get(s))
            .map(|m| m.values().flatten().cloned().collect())
            .collect();
        let fresh = merge(&lists, &self.q);
        let seen: HashSet<String> = self.base.iter().map(|t| t.key()).collect();
        self.tracks = self.base.clone();
        self.tracks
            .extend(fresh.into_iter().filter(|t| !seen.contains(&t.key())));
    }

    /// Downloaded already: the file is in the download folder (checked once per track).
    fn is_saved(&mut self, t: &Track) -> bool {
        if t.download.is_empty() {
            return false;
        }
        if let Some(s) = self.saved.get(&t.id) {
            return *s;
        }
        let s = target(t, &self.settings.folder()).is_file();
        self.saved.insert(t.id.clone(), s);
        s
    }

    fn show(&mut self, ui: &AppWindow) {
        let sel = self.sel.clone();
        let tracks = std::mem::take(&mut self.tracks);
        let saved: Vec<bool> = tracks.iter().map(|t| self.is_saved(t)).collect();
        self.tracks = tracks;
        let rows: Vec<FmRow> = self
            .tracks
            .iter()
            .zip(saved)
            .map(|(t, saved)| {
                let img = self.covers.get(&t.cover).cloned();
                FmRow {
                    title: t.title.clone().into(),
                    sub: [t.artist.as_str(), t.album.as_str()]
                        .iter()
                        .filter(|x| !x.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" · ")
                        .into(),
                    badge: match t.source {
                        JAMENDO => "Jamendo",
                        FMA => "FMA",
                        _ => "Archive",
                    }
                    .into(),
                    lic: t.license().replace("Unknown licence", "?").into(),
                    free: commercial_ok(&t.license_url),
                    dur: t.length().into(),
                    has_cover: img.is_some(),
                    cover: img.unwrap_or_default(),
                    dl: !t.download.is_empty(),
                    saved,
                    selected: sel.as_ref() == Some(&t.id),
                }
            })
            .collect();
        let st = ui.global::<FreeMusicState>();
        // The highlight follows the selected track when rows are re-sorted or replaced.
        let idx = sel
            .as_ref()
            .and_then(|id| self.tracks.iter().position(|t| &t.id == id));
        st.set_selected(idx.map_or(-1, |i| i as i32));
        match &self.model {
            // Change only the rows that differ.
            Some(m) => {
                for (i, row) in rows.into_iter().enumerate() {
                    if i < m.row_count() {
                        if m.row_data(i).as_ref() != Some(&row) {
                            m.set_row_data(i, row);
                        }
                    } else {
                        m.push(row);
                    }
                }
                while m.row_count() > self.tracks.len() {
                    m.remove(m.row_count() - 1);
                }
            }
            None => {
                let m = Rc::new(VecModel::from(rows));
                st.set_tracks(ModelRc::from(m.clone()));
                self.model = Some(m);
            }
        }
        st.set_has_more(
            self.pending.is_empty() && self.more.values().any(|m| *m) && !self.tracks.is_empty(),
        );
        self.want_covers();
        self.buttons(ui);
    }

    fn want_covers(&mut self) {
        let want: Vec<String> = self
            .tracks
            .iter()
            .map(|t| t.cover.clone())
            .filter(|c| c.starts_with("http") && self.cover_asked.insert(c.clone()))
            .collect();
        if want.is_empty() {
            return;
        }
        // A few parallel fetchers per batch; results trickle in through the channel.
        let chunk = want.len().div_ceil(4).max(1);
        for part in want.chunks(chunk) {
            let (tx, part) = (self.tx.clone(), part.to_vec());
            std::thread::spawn(move || {
                for u in part {
                    let px = fetch_cover(&u).and_then(|p| decode(&p, ROW_PX));
                    if tx.send(Msg::Cover(u, px)).is_err() {
                        break;
                    }
                }
            });
        }
    }

    fn buttons(&self, ui: &AppWindow) {
        let mut acts: Vec<SharedString> = Vec::new();
        if !self.back.is_empty() {
            acts.push("‹ Back".into());
        }
        if let Some(t) = self.selected() {
            acts.push("Queue".into());
            acts.push("Similar".into());
            if !t.album_id.is_empty() {
                acts.push("Album".into());
            }
            if !t.artist_id.is_empty() {
                acts.push("Artist".into());
            }
            acts.push("Copy credit".into());
            // A direct link to the track's page (Jamendo's API terms ask for one).
            acts.push(page_label(t).into());
        }
        if self.dl_active.is_some() {
            acts.push("Cancel download".into());
        }
        let st = ui.global::<FreeMusicState>();
        st.set_actions(ModelRc::new(VecModel::from(acts)));
        st.set_secondary_text(
            match self.selected() {
                Some(t) if t.download.is_empty() => "Preview only",
                _ => "Download",
            }
            .into(),
        );
    }

    fn filters(&self, ui: &AppWindow) {
        let f = |name: &str, opts: &[&str], i: usize| FmFilter {
            name: name.into(),
            options: ModelRc::new(VecModel::from(
                opts.iter()
                    .map(|o| SharedString::from(*o))
                    .collect::<Vec<_>>(),
            )),
            value: opts[i.min(opts.len() - 1)].into(),
        };
        let v = vec![
            f("mood", &MOODS, self.q.mood),
            f("licence", &LICENCES, self.q.lic),
            f("length", &LENGTHS, self.q.len),
            f("sort", &SORTS, self.q.sort),
        ];
        let st = ui.global::<FreeMusicState>();
        st.set_filters(ModelRc::new(VecModel::from(v)));
        let chips: Vec<SharedString> = CHIPS.iter().map(|c| c.0.into()).collect();
        st.set_chips(ModelRc::new(VecModel::from(chips)));
        st.set_chip(
            self.q
                .chip
                .filter(|_| self.q.album.is_none() && self.q.artist.is_none())
                .map(|c| CHIPS[c].0)
                .unwrap_or("")
                .into(),
        );
        st.set_title(self.title_text().into());
    }

    fn title_text(&self) -> String {
        if self.title.is_empty() {
            "Free Music".into()
        } else {
            format!("Free Music  ›  {}", self.title)
        }
    }

    fn detail(&self, t: &Track) -> String {
        let mut d = format!("{}\n{}", t.title, t.artist);
        if !t.album.is_empty() {
            d += &format!("\nAlbum: {}", t.album);
        }
        let mut meta = Vec::new();
        if !t.date.is_empty() {
            meta.push(t.date.clone());
        }
        if t.duration > 0 {
            meta.push(t.length());
        }
        meta.push(t.source.to_string());
        d += &format!("\n{}", meta.join(" · "));
        let tags: Vec<&str> = t
            .genres
            .iter()
            .chain(&t.tags)
            .map(String::as_str)
            .take(8)
            .collect();
        if !tags.is_empty() {
            d += &format!("\nTags: {}", tags.join(", "));
        }
        d += &format!(
            "\n\nLicence: {}\n{}\n{}",
            t.license(),
            lic_summary(&t.license_url),
            t.license_url
        );
        d += &format!(
            "\n\nAttribution (Copy credit; also saved in the tags and a .licence.txt):\n{}",
            t.attribution()
        );
        d += &if t.download.is_empty() {
            "\n\nThe artist allows streaming only, no download.".to_string()
        } else {
            format!("\n\nDownloads to {}", self.settings.folder().display())
        };
        d
    }

    fn select(&mut self, ui: &AppWindow, t: &Track) {
        self.sel = Some(t.id.clone());
        let st = ui.global::<FreeMusicState>();
        st.set_detail(self.detail(t).into());
        st.set_copy_text(t.attribution().into());
        self.want_detail_cover(ui, t);
        self.buttons(ui);
    }

    /// Large cover above the detail text: the full-size cover (cached like the row
    /// covers), the row thumbnail when that fails.
    fn want_detail_cover(&mut self, ui: &AppWindow, t: &Track) {
        let url = [&t.cover_full, &t.cover]
            .into_iter()
            .find(|u| u.starts_with("http"))
            .cloned()
            .unwrap_or_default();
        if url == self.detail_url {
            return;
        }
        ui.global::<FreeMusicState>().set_has_detail_cover(false);
        self.detail_url = url.clone();
        if url.is_empty() {
            return;
        }
        let (tx, small) = (self.tx.clone(), t.cover.clone());
        std::thread::spawn(move || {
            let px = fetch_cover(&url)
                .and_then(|p| decode(&p, LARGE_PX))
                .or_else(|| {
                    (small != url && small.starts_with("http"))
                        .then(|| fetch_cover(&small).and_then(|p| decode(&p, LARGE_PX)))
                        .flatten()
                });
            let _ = tx.send(Msg::Detail(url, px));
        });
    }

    fn open(&mut self, ui: &AppWindow) {
        let st = ui.global::<FreeMusicState>();
        let src: Vec<SharedString> = SOURCES.iter().map(|s| (*s).into()).collect();
        st.set_sources(ModelRc::new(VecModel::from(src)));
        st.set_source(SOURCES[self.q.source].into());
        st.set_detail("".into());
        st.set_has_detail_cover(false);
        self.detail_url.clear();
        st.set_open(true);
        // Downloads may have been moved or deleted since the last time.
        self.saved.clear();
        self.back.clear();
        self.title.clear();
        self.q.album = None;
        self.q.artist = None;
        if self.q.text.trim().is_empty() && self.q.chip.is_none() {
            self.q.chip = Some(0);
        }
        st.set_query(self.q.text.clone().into());
        self.q.page = 0;
        self.filters(ui);
        self.run(ui);
    }

    /// Read the query box and the source dropdown into `self.q` and search page 0.
    fn search(&mut self, ui: &AppWindow) {
        let st = ui.global::<FreeMusicState>();
        let text = st.get_query().to_string();
        let src = st.get_source();
        self.q.source = SOURCES.iter().position(|s| *s == src.as_str()).unwrap_or(0);
        self.q.text = text;
        self.q.page = 0;
        self.q.album = None;
        self.q.artist = None;
        self.back.clear();
        self.title.clear();
        if !self.q.text.trim().is_empty() && self.q.chip.is_some_and(|c| c < 3) {
            // A text search replaces the Popular / New / Featured lists.
            self.q.chip = None;
        }
        if self.q.text.trim().is_empty() && self.q.chip.is_none() {
            self.q.chip = Some(0);
        }
        self.filters(ui);
        self.run(ui);
    }

    fn page_view(&mut self, ui: &AppWindow, title: String, q: Query) {
        self.back.push(self.q.clone());
        self.title = title;
        self.q = q;
        self.sync_query_box(ui);
        self.filters(ui);
        self.run(ui);
    }

    fn play(&mut self, ui: &AppWindow, t: &Track, queue: bool) {
        // Downloaded already: play the file (offline, with its tags and cover).
        let local = target(t, &self.settings.folder());
        let (url, copy) = if !t.download.is_empty() && local.is_file() {
            (local.to_string_lossy().into_owned(), " (downloaded copy)")
        } else {
            (t.stream.clone(), "")
        };
        describe(t, &url);
        crate::playback::play_music(&url, &format!("{} — {}", t.artist, t.title), queue);
        self.flash(
            ui,
            if queue {
                format!("Added {} — {} to the queue{copy}", t.artist, t.title)
            } else {
                format!("Playing {} — {}{copy}", t.artist, t.title)
            },
        );
    }

    /// Show the current page's search text in the query box (Similar, album and artist
    /// pages, and Back from them), so it never shows a search that is not running.
    fn sync_query_box(&mut self, ui: &AppWindow) {
        // A half-typed edit belongs to the page that was left.
        self.typed = None;
        ui.global::<FreeMusicState>()
            .set_query(query_box_text(&self.q).into());
    }

    /// "Play similar": play this track, then queue similar ones as they arrive.
    fn similar(&mut self, ui: &AppWindow, t: &Track) {
        self.play(ui, t, false);
        let mut q = similar_query(t);
        if q.source == 3 {
            q.source = 2;
        }
        let title = format!("Similar to {}", t.title);
        self.page_view(ui, title, q);
        // Radio: the first results that arrive are queued after this track.
        self.radio = Some((self.epoch, HashSet::from([t.key()])));
    }

    /// Queue up to [`RADIO`] tracks of the radio search as results come in.
    fn feed_radio(&mut self) {
        let Some((epoch, seen)) = &mut self.radio else {
            return;
        };
        if *epoch != self.epoch {
            self.radio = None;
            return;
        }
        let mut n = 0;
        for t in &self.tracks {
            if seen.len() > RADIO {
                break;
            }
            if seen.insert(t.key()) {
                describe(t, &t.stream);
                crate::playback::play_music(&t.stream, &format!("{} — {}", t.artist, t.title), true);
                n += 1;
            }
        }
        if n > 0 {
            self.note = format!("Queued {} similar tracks.", seen.len() - 1);
        }
    }

    fn download(&mut self, ui: &AppWindow, t: Track) {
        if t.download.is_empty() {
            Self::set_status(
                ui,
                "The artist does not allow downloads of this track (preview only)",
                false,
            );
            return;
        }
        if target(&t, &self.settings.folder()).is_file() {
            self.flash(ui, format!("“{}” is already downloaded", t.title));
            return;
        }
        if self.dl_queue.iter().any(|x| x.id == t.id)
            || self.dl_active.as_ref().is_some_and(|(x, _)| x.id == t.id)
        {
            self.flash(ui, "Already in the download queue".into());
            return;
        }
        self.dl_queue.push_back(t);
        self.next_download();
        self.buttons(ui);
        self.status(ui);
    }

    fn next_download(&mut self) {
        if self.dl_active.is_some() {
            return;
        }
        let Some(t) = self.dl_queue.pop_front() else {
            return;
        };
        let p = Arc::new(DlProgress::default());
        let (tx, dir, t2, p2) = (self.tx.clone(), self.settings.folder(), t.clone(), p.clone());
        std::thread::spawn(move || {
            let r = download(&t2, &dir, &p2);
            let _ = tx.send(Msg::Saved(t2.id.clone(), t2.title.clone(), r));
        });
        self.dl_active = Some((t, p));
    }

    /// Settings → Free Music changed `name` ("jamendo" or "folder").
    fn set_setting(&mut self, ui: &AppWindow, name: &str, v: &str) {
        let v = v.trim().to_string();
        match name {
            "jamendo" if v != self.settings.jamendo_client_id => {
                self.settings.jamendo_client_id = v;
                self.settings.save();
                // Another key, other results: search again if the browser is up.
                if ui.global::<FreeMusicState>().get_open() {
                    self.q.page = 0;
                    self.run(ui);
                }
            }
            "folder" if v != self.settings.folder => {
                self.settings.folder = v;
                self.settings.save();
                // Another folder holds other downloads.
                self.saved.clear();
                if ui.global::<FreeMusicState>().get_open() {
                    self.show(ui);
                }
            }
            _ => {}
        }
    }

    fn action(&mut self, ui: &AppWindow, name: &str, i: i32) {
        match name {
            "‹ Back" => {
                if let Some(q) = self.back.pop() {
                    self.q = q;
                    if self.back.is_empty() {
                        self.title.clear();
                    }
                    self.sync_query_box(ui);
                    self.filters(ui);
                    self.run(ui);
                }
                return;
            }
            "Cancel download" => {
                if let Some((_, p)) = &self.dl_active {
                    p.cancel.store(true, Ordering::Relaxed);
                }
                self.dl_queue.clear();
                return;
            }
            _ => {}
        }
        let Some(t) = self.at(i) else {
            return;
        };
        let src = SOURCES.iter().position(|s| *s == t.source).unwrap_or(0);
        match name {
            "Queue" => self.play(ui, &t, true),
            "Similar" => self.similar(ui, &t),
            "Album" => {
                let q = Query {
                    source: src,
                    album: Some((t.source, t.album_id.clone())),
                    ..Default::default()
                };
                self.page_view(ui, format!("Album: {}", t.album), q);
            }
            "Artist" => {
                let q = Query {
                    source: src,
                    sort: self.q.sort,
                    artist: Some((t.source, t.artist_id.clone())),
                    ..Default::default()
                };
                self.page_view(ui, format!("Artist: {}", t.artist), q);
            }
            // The dialog copied the text to the clipboard itself.
            "Copy credit" => self.flash(ui, "Attribution copied to the clipboard".into()),
            a if a == page_label(&t) => web::open_url(&t.page),
            _ => {}
        }
    }

    fn tick(&mut self, ui: &AppWindow) {
        let st = ui.global::<FreeMusicState>();
        let open = st.get_open();
        if self.typed.is_some_and(|t| t.elapsed() >= DEBOUNCE) {
            self.typed = None;
            let q = st.get_query();
            if open && q.trim().chars().count() >= 2 && q.trim() != self.q.text.trim() {
                self.search(ui);
            }
        }
        let mut changed = false;
        let mut status = false;
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Page {
                    epoch,
                    src,
                    pos,
                    tracks,
                    more,
                    done,
                } if epoch == self.epoch => {
                    if !tracks.is_empty() {
                        self.page.entry(src).or_default().insert(pos, tracks);
                    }
                    self.more.insert(src, more);
                    if done {
                        self.pending.remove(src);
                    }
                    changed = true;
                }
                Msg::Fail { epoch, src, err } if epoch == self.epoch => {
                    eprintln!("[CedarApple] free music: {err}");
                    self.pending.remove(src);
                    self.errors.push(err);
                    changed = true;
                }
                Msg::Cover(url, Some(px)) => {
                    self.covers.insert(url, slint::Image::from_rgba8(px));
                    self.covers_dirty = true;
                }
                Msg::Detail(url, Some(px)) if url == self.detail_url => {
                    st.set_detail_cover(slint::Image::from_rgba8(px));
                    st.set_has_detail_cover(true);
                }
                Msg::Saved(id, title, r) => {
                    // Cancel download: "Cancelled", not a failure.
                    let cancelled = self
                        .dl_active
                        .take()
                        .is_some_and(|(_, p)| p.cancel.load(Ordering::Relaxed));
                    // The row gets its check mark (or loses it) on the next refresh.
                    self.saved.remove(&id);
                    self.covers_dirty = true;
                    self.last = match r {
                        Ok(_) => format!("Saved “{title}”"),
                        Err(_) if cancelled => format!("Cancelled “{title}”"),
                        Err(e) => format!("“{title}” failed: {e}"),
                    };
                    self.next_download();
                    self.buttons(ui);
                    status = true;
                }
                _ => {}
            }
        }
        if changed {
            self.remerge();
            self.feed_radio();
            self.covers_dirty = false;
            if open {
                self.show(ui);
            }
            status = true;
        } else if self.covers_dirty && open {
            self.covers_dirty = false;
            self.show(ui);
        }
        if self.dl_active.is_some() {
            status = true;
        }
        if status && open {
            self.status(ui);
        }
    }
}

/// Internet Archive search page: item ids, then item metadata in parallel; each item
/// is sent as soon as it arrives so the list fills progressively.
fn ia_job(epoch: u64, q: &Query, tx: &Sender<Msg>) {
    let (ids, more) = if let Some((_, id)) = &q.album {
        (vec![(id.clone(), 0)], false)
    } else {
        match ia_search_ids(q) {
            Ok(x) => x,
            Err(err) => {
                let _ = tx.send(Msg::Fail { epoch, src: IA, err });
                return;
            }
        }
    };
    let max = if q.album.is_some() {
        IA_ALBUM_TRACKS
    } else if q.text.trim().is_empty() && q.artist.is_none() {
        // Browse lists: a few tracks per album so that one album does not fill the page.
        3
    } else {
        IA_TRACKS_PER_ITEM
    };
    let hs: Vec<_> = ids
        .into_iter()
        .enumerate()
        .map(|(pos, (id, dls))| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let tracks = ia_item(&id, max, dls);
                let _ = tx.send(Msg::Page {
                    epoch,
                    src: IA,
                    pos,
                    tracks,
                    more,
                    done: false,
                });
            })
        })
        .collect();
    for h in hs {
        let _ = h.join();
    }
    let _ = tx.send(Msg::Page {
        epoch,
        src: IA,
        pos: usize::MAX,
        tracks: Vec::new(),
        more,
        done: true,
    });
}

/// Text the search box shows for `q`: an album or artist page lists that album or
/// artist, not a text search, so the box is empty there (the title names the page).
pub(crate) fn query_box_text(q: &Query) -> String {
    if q.album.is_some() || q.artist.is_some() {
        String::new()
    } else {
        q.text.clone()
    }
}

/// Settings → Free Music: (Jamendo client ID, download folder, the folder used
/// while that is empty).
pub fn settings() -> (String, String, String) {
    let s = BROWSER.with(|b| b.borrow().as_ref().map(|b| b.settings.clone()));
    let s = s.unwrap_or_else(Settings::load);
    (
        s.jamendo_client_id,
        s.folder,
        default_folder().to_string_lossy().into_owned(),
    )
}

/// Settings → Free Music changed `name` ("jamendo" or "folder").
pub fn set_setting(name: &str, v: &str) {
    with(|b, ui| b.set_setting(ui, name, v));
}

// ---- Wiring ------------------------------------------------------------------

/// Run `f` with the browser and the window.
fn with(f: impl FnOnce(&mut Browser, &AppWindow)) {
    let Some(ui) = window() else { return };
    BROWSER.with(|b| {
        if let Some(b) = b.borrow_mut().as_mut() {
            f(b, &ui);
        }
    });
}

/// Install the dialog's callbacks and its timer. The timer must live as long as
/// the event loop.
pub fn install(ui: &AppWindow) -> slint::Timer {
    UI.with(|u| *u.borrow_mut() = Some(ui.as_weak()));
    BROWSER.with(|b| *b.borrow_mut() = Some(Browser::new()));

    let st = ui.global::<FreeMusicState>();
    st.on_opened(|| with(|b, ui| b.open(ui)));
    st.on_closed(|| {
        with(|b, ui| {
            b.epoch += 1;
            b.pending.clear();
            b.typed = None;
            ui.global::<FreeMusicState>().set_open(false);
        })
    });
    st.on_search(|| with(|b, ui| b.search(ui)));
    // Typing: search after a short pause (see `tick`).
    st.on_edited(|| with(|b, _| b.typed = Some(Instant::now())));
    st.on_more(|| {
        with(|b, ui| {
            if b.pending.is_empty() && b.more.values().any(|m| *m) {
                b.q.page += 1;
                b.run(ui);
            }
        })
    });
    st.on_chip_clicked(|c| {
        with(|b, ui| {
            let i = CHIPS.iter().position(|x| x.0 == c.as_str());
            b.q.chip = if b.q.chip == i && i.is_some_and(|i| i >= 3) {
                None
            } else {
                i
            };
            if b.q.chip.is_some_and(|c| c < 3) {
                // Popular / New / Featured are lists, not text searches.
                b.q.text.clear();
                ui.global::<FreeMusicState>().set_query("".into());
            }
            b.search(ui);
        })
    });
    st.on_filter(|name, value| {
        with(|b, ui| {
            let pos = |l: &[&str]| l.iter().position(|x| *x == value.as_str()).unwrap_or(0);
            match name.as_str() {
                "mood" => b.q.mood = pos(&MOODS),
                "licence" => b.q.lic = pos(&LICENCES),
                "length" => b.q.len = pos(&LENGTHS),
                "sort" => b.q.sort = pos(&SORTS),
                _ => return,
            }
            b.q.page = 0;
            b.filters(ui);
            b.run(ui);
        })
    });
    st.on_select(|i| {
        with(|b, ui| {
            if let Some(t) = usize::try_from(i).ok().and_then(|i| b.tracks.get(i).cloned()) {
                b.select(ui, &t);
                b.show(ui);
            }
        })
    });
    st.on_primary(|i| {
        with(|b, ui| match b.at(i) {
            Some(t) => b.play(ui, &t, false),
            None => Browser::set_status(ui, "Select a track first", false),
        })
    });
    st.on_secondary(|i| {
        with(|b, ui| match b.at(i) {
            Some(t) => b.download(ui, t),
            None => Browser::set_status(ui, "Select a track first", false),
        })
    });
    st.on_action(|name, i| with(|b, ui| b.action(ui, name.as_str(), i)));
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), || {
        with(|b, ui| b.tick(ui))
    });
    timer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> serde_json::Value {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/free_music")
            .join(name);
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
    }

    fn t(ti: &str, ar: &str) -> Track {
        Track {
            id: format!("{ar}/{ti}"),
            title: ti.into(),
            artist: ar.into(),
            ..Default::default()
        }
    }

    #[test]
    fn page_labels() {
        let mut tr = t("Air", "Bach");
        tr.source = FMA;
        tr.license_url = "https://creativecommons.org/licenses/by-sa/4.0/".into();
        assert_eq!(tr.license(), "CC BY-SA 4.0");
        assert_eq!(page_label(&tr), "Open on Internet Archive");
        tr.source = JAMENDO;
        assert_eq!(page_label(&tr), "Open on Jamendo");
    }

    #[test]
    fn client_id_is_not_kept_in_urls() {
        assert_eq!(
            strip_app("https://prod-1.storage.jamendo.com/?trackid=7&format=mp31&from=app-abc123"),
            "https://prod-1.storage.jamendo.com/?trackid=7&format=mp31"
        );
        assert_eq!(
            strip_app("https://x/?from=app-abc&client_id=abc"),
            "https://x/"
        );
        assert_eq!(strip_app("https://x/a.mp3"), "https://x/a.mp3");
        assert_eq!(
            strip_app("https://x/?a=1&fromage=2"),
            "https://x/?a=1&fromage=2"
        );
    }

    #[test]
    fn licences() {
        assert_eq!(
            license_name("http://creativecommons.org/licenses/by-nc-sa/4.0/"),
            "CC BY-NC-SA 4.0"
        );
        assert_eq!(
            license_name("https://creativecommons.org/licenses/by/3.0/us/"),
            "CC BY 3.0"
        );
        assert_eq!(
            license_name("http://creativecommons.org/publicdomain/zero/1.0/"),
            "CC0 1.0"
        );
        assert_eq!(
            license_name("https://creativecommons.org/publicdomain/mark/1.0/"),
            "Public Domain Mark"
        );
        let by = "http://creativecommons.org/licenses/by/3.0/";
        let bysa = "http://creativecommons.org/licenses/by-sa/3.0/";
        let nc = "http://creativecommons.org/licenses/by-nc-nd/3.0/";
        let pd = "http://creativecommons.org/publicdomain/zero/1.0/";
        assert!(commercial_ok(by) && commercial_ok(pd) && !commercial_ok(nc));
        assert!(!commercial_ok("") && !commercial_ok("http://example.com/x"));
        assert!(lic_match(by, 3) && !lic_match(bysa, 3) && lic_match(bysa, 4));
        assert!(lic_match(pd, 5) && !lic_match(by, 5));
        assert!(lic_match(bysa, 2) && !lic_match(nc, 2) && !lic_match(nc, 1));
        assert!(lic_match("", 0) && !lic_match("", 1));
        assert!(lic_summary(nc).contains("non-commercial"));
        assert!(lic_summary(bysa).contains("same licence"));
    }

    #[test]
    fn parse_jamendo_fixture() {
        let v = jamendo_tracks(&fixture("jamendo_tracks.json"));
        assert_eq!(v.len(), 3);
        let a = &v[0];
        assert_eq!(a.title, "That's Life");
        assert_eq!(a.artist, "Yellow Faces");
        assert_eq!(
            (a.artist_id.as_str(), a.album_id.as_str()),
            ("7123", "104336")
        );
        assert_eq!(a.duration, 187);
        assert_eq!(a.genres, ["rock", "pop"]);
        assert_eq!(a.tags, ["happy", "energetic"]);
        assert!(a.cover.contains("width=300") && a.cover_full.contains("width=600"));
        assert_eq!(a.license(), "CC BY-SA 3.0");
        assert!(!a.download.is_empty());
        // The client ID Jamendo puts in stream URLs (`from=app-…`) is dropped.
        assert_eq!(
            a.stream,
            "https://prod-1.storage.jamendo.com/?trackid=1532771&format=mp31"
        );
        assert_eq!(v[1].download, "", "download not allowed");
        let dir = std::env::temp_dir().join("cedarapple-fm-nodl");
        assert!(download(&v[1], &dir, &DlProgress::default()).is_err());
        // Entries without a stream are dropped.
        assert!(v.iter().all(|t| !t.stream.is_empty()));
    }

    #[test]
    fn parse_ia_fixture() {
        let v = ia_tracks(&fixture("ia_metadata.json"), 10);
        // The FLAC original, the 64 kbps duplicate and the hour-long mix are skipped.
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title, "Opening");
        assert_eq!(v[1].title, "Night Drive");
        assert_eq!(v[0].artist, "Some Netlabel Artist");
        assert_eq!(v[0].album, "Night Sessions");
        assert_eq!(v[0].date, "2012-12-30");
        assert_eq!(v[0].duration, 245);
        assert_eq!(v[0].source, FMA);
        assert!(v[0].tags.contains(&"electronic".to_string()));
        assert_eq!(
            v[1].download,
            "https://archive.org/download/night_sessions/disc%201/02%20Night%20Drive.mp3"
        );
        assert!(v[0].cover_full.ends_with("/cover.jpg"));
        assert!(v[0].attribution().contains("CC BY 4.0"));
        let s = fixture("ia_search.json");
        let (ids, more) = ia_ids(&s, 0);
        assert_eq!(ids[0], ("night_sessions".to_string(), 1234));
        assert!(more);
        assert!(!ia_ids(&s, 10).1);
    }

    #[test]
    fn parse_ia_item_inline() {
        let m = serde_json::json!({
            "metadata": {"identifier": "my item", "creator": ["Band"], "title": "Live",
                "date": "2012-12-30T00:00:00Z",
                "licenseurl": "http://creativecommons.org/licenses/by/4.0/"},
            "files": [
                {"name": "a.flac", "format": "Flac"},
                {"name": "dir/b c.mp3", "format": "VBR MP3", "title": "Song B"},
                {"name": "c_64kb.mp3", "format": "64Kbps MP3"}
            ]
        });
        let t = ia_tracks(&m, 12);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].title, "Song B");
        assert_eq!(t[0].source, IA);
        assert_eq!(
            t[0].download,
            "https://archive.org/download/my%20item/dir/b%20c.mp3"
        );
        // The waveform PNG derived from an audio file is no cover: the item thumbnail is.
        let m = serde_json::json!({
            "metadata": {"identifier": "w", "creator": "A", "title": "T",
                "licenseurl": "http://creativecommons.org/licenses/by/4.0/"},
            "files": [
                {"name": "T.mp3", "format": "VBR MP3", "source": "original"},
                {"name": "T.png", "format": "PNG", "source": "derivative", "original": "T.mp3"},
                {"name": "T_spectrogram.png", "format": "Spectrogram", "source": "derivative", "original": "T.mp3"}
            ]
        });
        assert_eq!(
            ia_tracks(&m, 12)[0].cover_full,
            "https://archive.org/services/img/w"
        );
        let mut m = m;
        m["files"].as_array_mut().unwrap().push(serde_json::json!(
            {"name": "art_thumb.jpg", "format": "JPEG", "source": "derivative", "original": "art.png"}
        ));
        assert_eq!(
            ia_tracks(&m, 12)[0].cover_full,
            "https://archive.org/download/w/art_thumb.jpg"
        );
        // No licence: nothing.
        let m = serde_json::json!({"metadata": {"identifier": "x"}, "files": []});
        assert!(ia_tracks(&m, 12).is_empty());
    }

    #[test]
    fn query_box_follows_the_page() {
        // Album and artist pages: no stale search text, even if the query carries some.
        let album = Query {
            text: "old search".into(),
            album: Some((JAMENDO, "123".into())),
            ..Default::default()
        };
        assert_eq!(query_box_text(&album), "");
        let artist = Query {
            artist: Some((JAMENDO, "Kevin MacLeod".into())),
            ..Default::default()
        };
        assert_eq!(query_box_text(&artist), "");
        // Similar: the box shows what the similar search really looks for.
        let mut t = t("Song", "Some Artist");
        t.source = JAMENDO;
        let q = similar_query(&t);
        assert_eq!(query_box_text(&q), q.text);
        assert_eq!(q.text, "Some Artist");
        // A plain search keeps its text (Back restores it).
        let plain = Query {
            text: "kevin macleod".into(),
            ..Default::default()
        };
        assert_eq!(query_box_text(&plain), "kevin macleod");
    }

    #[test]
    fn relevance_and_merge() {
        let q = Query {
            text: "kevin macleod".into(),
            ..Default::default()
        };
        let ia = vec![
            t("Lecture 3", "Prof"),
            t("Other", "Kevin MacLeod"),
            t("Kevin MacLeod Medley", "X"),
        ];
        let jam = vec![
            t("Kevin's Song", "Somebody"),
            t("Monkeys Spinning Monkeys", "Kevin MacLeod"),
        ];
        let v = merge(&[jam, ia], &q);
        assert_eq!(v[0].artist, "Kevin MacLeod");
        assert_eq!(v[1].artist, "Kevin MacLeod", "both exact artist hits first");
        // Fewer than five hits: non-matching ones stay, at the end.
        assert_eq!(v.last().unwrap().title, "Lecture 3");
        // Duplicates across sources collapse.
        let a = vec![t("Song", "Band")];
        let mut b = t("song!", "band");
        b.id = "other".into();
        assert_eq!(merge(&[a, vec![b]], &Query::default()).len(), 1);
        // Runs of one artist are broken up.
        let run = vec![
            t("1", "A"),
            t("2", "A"),
            t("3", "A"),
            t("4", "B"),
            t("5", "C"),
        ];
        let names: Vec<String> = spread(run).into_iter().map(|t| t.title).collect();
        assert_eq!(names, ["1", "2", "4", "3", "5"]);
        // Tag matches count.
        let mut g = t("Untitled", "Y");
        g.genres = vec!["jazz".into()];
        assert!(score(&g, "jazz") > score(&t("Untitled", "Z"), "jazz"));
    }

    #[test]
    fn filters_and_sort() {
        let mk = |ti: &str, d: u32, lic: &str, date: &str| Track {
            id: ti.into(),
            title: ti.into(),
            duration: d,
            license_url: lic.into(),
            date: date.into(),
            ..Default::default()
        };
        let by = "http://creativecommons.org/licenses/by/3.0/";
        let nc = "http://creativecommons.org/licenses/by-nc/3.0/";
        let l = vec![
            mk("a", 120, by, "2020-01-01"),
            mk("b", 400, nc, "2022-01-01"),
            mk("c", 200, by, "2021-01-01"),
        ];
        let q = |f: fn(&mut Query)| {
            let mut q = Query::default();
            f(&mut q);
            merge(std::slice::from_ref(&l), &q)
                .iter()
                .map(|t| t.title.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(q(|q| q.lic = 1), ["a", "c"]);
        let mut j = mk("j", 100, by, "");
        j.source = JAMENDO;
        j.genres = vec!["ambient".into()];
        let jazz = Query {
            chip: Some(7),
            ..Default::default()
        };
        assert!(!keep(&j, &jazz) && keep(&j, &Query::default()));
        j.genres.push("jazzfusion".into());
        assert!(keep(&j, &jazz));
        assert_eq!(q(|q| q.len = 1), ["a"]);
        assert_eq!(q(|q| q.len = 3), ["b"]);
        assert_eq!(q(|q| q.sort = 2), ["b", "c", "a"]);
        assert_eq!(q(|q| q.sort = 3), ["a", "c", "b"]);
        assert_eq!(q(|q| q.sort = 4), ["b", "c", "a"]);
    }

    #[test]
    fn query_building() {
        let mut q = Query {
            text: "ac/dc: (live)".into(),
            chip: Some(7),
            mood: 2,
            lic: 1,
            len: 2,
            ..Default::default()
        };
        let s = ia_query(&q);
        assert!(s.contains("title:(ac dc live)"), "{s}");
        assert!(s.contains("subject:(jazz)") && s.contains("subject:(happy)"));
        assert!(s.contains("NOT licenseurl:*nc*") && s.contains("NOT subject:(podcast"));
        assert!(
            s.contains("opensource_audio"),
            "text searches include community uploads"
        );
        assert_eq!(ia_sort(&q), "", "text search keeps IA relevance");
        let p = jamendo_params(&q, JMode::Name);
        let has = |k: &str, v: &str| p.iter().any(|(a, b)| *a == k && b == v);
        assert!(
            has("namesearch", "ac/dc: (live)")
                && has("fuzzytags", "jazz")
                && has("vartags", "happy")
        );
        assert!(has("ccnc", "false") && has("durationbetween", "180_360"));
        assert!(!p.iter().any(|(k, _)| *k == "client_id"));
        q.text.clear();
        q.chip = Some(1);
        q.page = 2;
        let p = jamendo_params(&q, JMode::Browse);
        let has = |k: &str, v: &str| p.iter().any(|(a, b)| *a == k && b == v);
        assert!(has("order", "releasedate_desc") && has("offset", "48"));
        assert_eq!(ia_sort(&q), "publicdate desc");
        assert!(
            !ia_query(&q).contains("opensource_audio"),
            "browse: curated only"
        );
        q.source = 3;
        assert!(ia_query(&q).contains("collection:(freemusicarchive)"));
        let a = Query {
            album: Some((JAMENDO, "9".into())),
            ..Default::default()
        };
        let p = jamendo_params(&a, JMode::Browse);
        assert!(p.iter().any(|(k, v)| *k == "album_id" && v == "9"));
        assert!(a.uses(JAMENDO) && !a.uses(IA));
        assert_eq!(tidy_stem("03_-_My_Song", ""), "My Song");
        assert_eq!(
            tidy_stem("deep-and-dirty-by-kevin-macleod", "Kevin MacLeod"),
            "deep and dirty"
        );
        assert_eq!(ia_len(&serde_json::json!("4:05")), Some(245.0));
        assert_eq!(fmt_len(245), "4:05");
        assert_eq!(
            clean_title("{dj-N} Club Step (ID: 396093)"),
            "{dj-N} Club Step"
        );
        assert_eq!(clean_title("Song (Live)"), "Song (Live)");
        assert_eq!(fmt_len(3725), "1:02:05");
        assert_eq!(unescape("That&#039;s"), "That's");
        assert_eq!(
            scrub("url?client_id=abc1".into(), "abc1"),
            "url?client_id=…"
        );
    }

    #[test]
    fn similar_and_tags() {
        let mut x = t("Song", "Band");
        x.source = JAMENDO;
        x.genres = vec!["jazz".into()];
        x.tags = vec!["relaxing".into()];
        x.date = "2019-05-01".into();
        x.license_url = "http://creativecommons.org/licenses/by/3.0/".into();
        let q = similar_query(&x);
        assert_eq!(q.chip.map(|c| CHIPS[c].1), Some("jazz"));
        assert_eq!(MOODS[q.mood], "Relaxing");
        assert_eq!(q.source, 1);
        let tg = tags_for(&x);
        assert_eq!(tg.genre, "Jazz");
        assert!(tg
            .copyright
            .starts_with("2019 Band. Licensed under CC BY 3.0"));
        assert!(tg.comment.contains("licensed under CC BY 3.0"));
        assert!(licence_text(&x).contains("commercial use OK"));
        assert!(target(&x, Path::new("/m")).ends_with("Band/Band - Song.mp3"));
    }

    #[test]
    fn cover_cache_names_are_stable() {
        assert_eq!(hash_name(""), "cbf29ce484222325");
        assert_ne!(hash_name("https://a/1.jpg"), hash_name("https://a/2.jpg"));
    }

    /// Live Internet Archive search + download with tags (no key). `--ignored`.
    #[test]
    #[ignore]
    fn live_internet_archive() {
        let q = Query {
            text: "kevin macleod".into(),
            source: 2,
            ..Default::default()
        };
        let (ids, _) = ia_search_ids(&q).unwrap();
        let all: Vec<Track> = ids
            .iter()
            .take(6)
            .flat_map(|(id, d)| ia_item(id, 8, *d))
            .collect();
        let v = merge(&[all], &q);
        assert!(!v.is_empty());
        let dir = std::env::temp_dir().join(format!("cedarapple-fm-{}", std::process::id()));
        let small = v
            .iter()
            .min_by_key(|t| if t.duration == 0 { u32::MAX } else { t.duration })
            .unwrap();
        let p = download(small, &dir, &DlProgress::default()).unwrap();
        use lofty::prelude::*;
        let tf = lofty::read_from_path(&p).unwrap();
        let tag = tf.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some(small.title.as_str()));
        assert!(p.with_extension("licence.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live Jamendo search; needs `JAMENDO_CLIENT_ID` in the environment. `--ignored`.
    #[test]
    #[ignore]
    fn live_jamendo() {
        let Ok(key) = std::env::var("JAMENDO_CLIENT_ID") else {
            eprintln!("JAMENDO_CLIENT_ID not set, skipped");
            return;
        };
        let q = Query {
            text: "piano".into(),
            source: 1,
            ..Default::default()
        };
        let (v, _) = jamendo_search(&key, &q).unwrap();
        let v = merge(&[v], &q);
        assert!(!v.is_empty());
        assert!(v[0].stream.starts_with("http"));
    }
}
