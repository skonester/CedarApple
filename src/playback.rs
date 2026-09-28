//! Everything between the Slint window and mpv: what a click does, what the
//! window shows, and the torrent flow that runs before the player opens.
//!
//! The behaviour is Frame Player's, ported from its Svelte modules into Rust
//! (GPL-3.0-or-later, Copyright (c) Evgenii Zakharov):
//!
//! * opening — `open.svelte.ts` (`openTorrent`, `openRememberedTorrent`,
//!   `playTorrentFile`) and `playlist.svelte.ts` (`queueTorrent`,
//!   `queueAround`): a torrent resolves to a file list, one video is picked,
//!   and every other video of the torrent goes into mpv's playlist around it;
//! * the torrent while it plays — `torrent.svelte.ts` (`trackTorrentPlayback`,
//!   `prefetchNext`, `attachTorrentSubtitles`, `markWatched`,
//!   `releaseTorrent`): status once a second, the on-disk map every two,
//!   subtitles inside the release attached, the next episode fetched ahead;
//! * the controls — `playback.svelte.ts` and `player.svelte.ts` (volume,
//!   mute, speed steps, the loop cycle, the OSD texts) and `seek.svelte.ts`
//!   (the drag, in `scrub.rs`).
//!
//! Svelte's reactive state becomes one `Ctx` owned by the UI thread. Work that
//! has to wait (the torrent engine, file dialogs) runs on tokio and comes back
//! through `on_ui`, which hands it the same `Ctx`.

use crate::api::{PlaybackExtension, PlaybackInfo};
use crate::app_state::AppState;
use crate::discord::DiscordRPC;
use crate::history;
use crate::player::MpvHandle;
use crate::scrub::{DRAG_SETTLE, Scrub};
use crate::torrent::{SessionPrefs, TorrentInfo, TorrentService};
use crate::torrent_ui::*;
use crate::{AppWindow, Band, ChapterInfo, PickFile, QueueEntry, TorrentRowData, TrackInfo};
use libmpv_sys::*;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the chrome stays up after the pointer stops.
const IDLE_AFTER: Duration = Duration::from_secs(3);
/// How long an OSD message stays up.
const OSD_FOR: Duration = Duration::from_millis(1200);
/// How often the position of what is playing is written down.
const SAVE_EVERY: Duration = Duration::from_secs(5);
/// A position under this is not worth resuming from.
const RESUME_MIN_S: f64 = 5.0;

/// One entry of the play queue, in the order mpv's playlist holds them.
#[derive(Clone)]
struct QueueItem {
    url: String,
    name: String,
    /// What the position is remembered under: the path, or `torrent:<hash>/<i>`.
    id: String,
    /// `(infohash, index)` for a torrent file.
    torrent: Option<(String, usize)>,
    /// Path inside the torrent, for matching subtitles and watched names.
    torrent_path: String,
}

/// A start-screen torrent row, as the row's buttons need it.
struct RowInfo {
    path: String,
    info_hash: Option<String>,
    magnet: Option<String>,
}

struct Ctx {
    ui: slint::Weak<AppWindow>,
    mpv: MpvHandle,
    torrents: Arc<TorrentService>,
    state: Arc<Mutex<AppState>>,
    discord: Arc<DiscordRPC>,

    scrub: RefCell<Scrub>,
    settle_timer: slint::Timer,
    osd_timer: slint::Timer,
    hwdec_timer: slint::Timer,

    queue: RefCell<Vec<QueueItem>>,
    /// The torrent the queue came from, for subtitles and prefetching.
    torrent_info: RefCell<Option<TorrentInfo>>,
    /// The torrent whose "What to watch" list is up.
    pick_info: RefCell<Option<TorrentInfo>>,
    rows: RefCell<HashMap<String, RowInfo>>,
    chapters: RefCell<Vec<(String, f64)>>,

    last_activity: Cell<Instant>,
    last_poll: Cell<Instant>,
    last_save: Cell<Instant>,
    last_report: Cell<Instant>,
    last_presence: Cell<Instant>,
    last_playlist_pos: Cell<i64>,
    /// A file has loaded since the queue was set; idle after this is the end.
    had_file: Cell<bool>,
    loop_mode: Cell<i32>,
    peerless_since: Cell<Option<Instant>>,
    subs_attached_for: RefCell<String>,
    prefetched: RefCell<HashSet<String>>,
}

thread_local! {
    static CTX: RefCell<Option<Rc<Ctx>>> = const { RefCell::new(None) };
}

fn ctx() -> Rc<Ctx> {
    CTX.with(|c| c.borrow().clone().expect("playback context"))
}

/// Run `f` on the UI thread with the context — the way back from tokio.
fn on_ui(f: impl FnOnce(&Ctx) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || f(&ctx()));
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

impl Ctx {
    fn with_ui(&self, f: impl FnOnce(&AppWindow)) {
        if let Some(ui) = self.ui.upgrade() {
            f(&ui);
        }
    }

    fn osd(&self, text: &str, progress: Option<f64>) {
        self.with_ui(|ui| {
            ui.set_osd_text(text.into());
            ui.set_osd_progress(progress.map(|p| p as f32).unwrap_or(-1.0));
            ui.set_osd_visible(true);
        });
        let ui = self.ui.clone();
        self.osd_timer.start(slint::TimerMode::SingleShot, OSD_FOR, move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_osd_visible(false);
            }
        });
    }

    fn current(&self) -> Option<QueueItem> {
        let pos = self.mpv.get_int("playlist-pos").unwrap_or(-1);
        if pos < 0 {
            return None;
        }
        self.queue.borrow().get(pos as usize).cloned()
    }

    /// Write down where the current file is, so it can be resumed.
    fn save_position(&self) {
        let Some(item) = self.current() else { return };
        let (Some(pos), Some(dur)) = (
            self.mpv.get_double("time-pos"),
            self.mpv.get_double("duration"),
        ) else {
            return;
        };
        if pos < 1.0 {
            return;
        }
        history::save_position(&item.id, pos, dur, &item.name);
        if let Some((hash, _)) = &item.torrent
            && dur > 0.0
            && pos / dur > history::FINISHED_FRACTION
        {
            history::mark_watched(hash, base_name(&item.torrent_path));
        }
    }

    /// Hand mpv a queue and start playing `start`. The others go around it
    /// without being opened — for a torrent that is what keeps them from
    /// costing anything until they are played.
    fn load_queue(&self, items: Vec<QueueItem>, start: usize) {
        if items.is_empty() {
            return;
        }
        self.save_position();
        let start = start.min(items.len() - 1);

        // A different torrent (or none) replaces the one feeding the player.
        let new_hash = items[start].torrent.as_ref().map(|(h, _)| h.clone());
        let old_hash = self
            .torrent_info
            .borrow()
            .as_ref()
            .map(|i| i.info_hash.clone());
        if old_hash.is_some() && old_hash != new_hash {
            self.release_torrent();
        }

        let mpv = &self.mpv;
        mpv.command(&["loadfile", &items[start].url, "replace"]);
        for (i, it) in items.iter().enumerate() {
            if i != start {
                mpv.command(&["loadfile", &it.url, "append"]);
            }
        }
        // The appended list starts with the ones that belong BEFORE `start`;
        // move each into place so the queue reads in order.
        for k in 0..start {
            mpv.command(&["playlist-move", &(k + 1).to_string(), &k.to_string()]);
        }
        mpv.set_flag("pause", false);

        let first = items[start].clone();
        *self.queue.borrow_mut() = items;
        self.had_file.set(false);
        self.last_playlist_pos.set(-2);
        self.subs_attached_for.borrow_mut().clear();
        self.peerless_since.set(None);
        self.chapters.borrow_mut().clear();
        self.with_ui(|ui| {
            ui.set_has_file(true);
            ui.set_video_title(first.name.clone().into());
            ui.set_position(0.0);
            ui.set_duration_seconds(0.0);
            ui.set_chapters(model(Vec::<ChapterInfo>::new()));
            ui.set_buffered(model(Vec::<Band>::new()));
            ui.set_chip_visible(false);
            ui.set_loading_text(if first.torrent.is_some() {
                "Opening the torrent…".into()
            } else {
                "Opening…".into()
            });
            ui.set_loading_sub("".into());
            ui.set_is_loading(true);
            ui.set_link_open(false);
            ui.set_pick_open(false);
        });
        self.last_activity.set(Instant::now());
        self.sync_queue_model();
    }

    fn sync_queue_model(&self) {
        let pos = self.mpv.get_int("playlist-pos").unwrap_or(0);
        let entries: Vec<QueueEntry> = self
            .queue
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, q)| QueueEntry {
                name: q.name.clone().into(),
                current: i as i64 == pos,
            })
            .collect();
        let count = entries.len() as i32;
        self.with_ui(|ui| {
            ui.set_queue(model(entries));
            ui.set_playlist_count(count);
            ui.set_playlist_pos(pos.max(0) as i32);
        });
    }

    /// Back to the start screen.
    fn close_file(&self) {
        self.save_position();
        self.mpv.command(&["stop"]);
        self.queue.borrow_mut().clear();
        self.release_torrent();
        self.had_file.set(false);
        {
            let mut s = self.state.lock().unwrap();
            s.current_item_id = None;
            s.current_title.clear();
        }
        self.with_ui(|ui| {
            ui.set_has_file(false);
            ui.set_is_loading(false);
            ui.set_video_title("".into());
            ui.set_chip_visible(false);
            ui.set_buffered(model(Vec::<Band>::new()));
            ui.set_queue(model(Vec::<QueueEntry>::new()));
            ui.set_playlist_count(0);
            ui.set_stalled(false);
        });
        refresh_rows();
    }

    /// Stop feeding the torrent: paused, not deleted, so reopening continues.
    fn release_torrent(&self) {
        let Some(info) = self.torrent_info.borrow_mut().take() else {
            return;
        };
        self.prefetched.borrow_mut().clear();
        let torrents = self.torrents.clone();
        tokio::spawn(async move { torrents.release(&info.info_hash).await });
    }

    fn update_tracks(&self) {
        let Some(js) = self.mpv.get_property_string("track-list") else {
            return;
        };
        let Ok(tracks) = serde_json::from_str::<Vec<Track>>(&js) else {
            return;
        };
        let mut audio = Vec::new();
        let mut subs = vec![TrackInfo {
            id: -1,
            name: "Off".into(),
            active: !tracks.iter().any(|t| t.track_type == TrackType::Sub && t.active),
        }];
        for t in &tracks {
            match t.track_type {
                TrackType::Audio => audio.push(t.as_track_info()),
                TrackType::Sub => subs.push(t.as_track_info()),
                _ => {}
            }
        }
        self.with_ui(|ui| {
            ui.set_audio_tracks(model(audio));
            ui.set_subtitle_tracks(model(subs));
        });
    }

    fn update_chapters(&self) {
        #[derive(serde::Deserialize)]
        struct Ch {
            #[serde(default)]
            title: Option<String>,
            time: f64,
        }
        let list: Vec<Ch> = self
            .mpv
            .get_property_string("chapter-list")
            .and_then(|js| serde_json::from_str(&js).ok())
            .unwrap_or_default();
        let chapters: Vec<(String, f64)> = list
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let title = c
                    .title
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| format!("Chapter {}", i + 1));
                (title, c.time)
            })
            .collect();
        let items: Vec<ChapterInfo> = chapters
            .iter()
            .map(|(t, time)| ChapterInfo {
                title: t.clone().into(),
                time: *time as f32,
            })
            .collect();
        *self.chapters.borrow_mut() = chapters;
        self.with_ui(|ui| ui.set_chapters(model(items)));
    }

    /// A file finished loading: tracks, chapters, the resume point, and the
    /// subtitles a torrent carries beside its video.
    fn on_file_loaded(&self) {
        self.had_file.set(true);
        self.scrub.borrow_mut().reset_probe();
        self.update_tracks();
        self.update_chapters();
        self.with_ui(|ui| {
            ui.set_is_loading(false);
            ui.set_loading_sub("".into());
        });

        let Some(item) = self.current() else { return };
        if let Some(p) = history::position(&item.id)
            && p.pos > RESUME_MIN_S
            && (p.dur <= 0.0 || p.pos / p.dur < history::FINISHED_FRACTION)
        {
            self.mpv
                .command(&["seek", &format!("{:.3}", p.pos), "absolute+exact"]);
            self.osd("Resuming", None);
        }

        if let (Some((hash, _)), Some(info)) = (&item.torrent, self.torrent_info.borrow().as_ref())
            && &info.info_hash == hash
            && *self.subs_attached_for.borrow() != item.url
        {
            *self.subs_attached_for.borrow_mut() = item.url.clone();
            for sub in torrent_subtitles(info, &item.torrent_path) {
                self.mpv.command(&["sub-add", &sub.url, "auto"]);
            }
        }
    }

    /// A file failed to open. Said out loud: a black window that never starts
    /// is the failure nobody can diagnose.
    fn on_file_failed(&self) {
        let torrent = self.current().is_some_and(|i| i.torrent.is_some());
        self.with_ui(|ui| ui.set_is_loading(false));
        self.osd(
            if torrent {
                "Could not read that file from the torrent — the data never arrived."
            } else {
                "Could not open it"
            },
            None,
        );
    }

    fn set_speed(&self, speed: f64) {
        let next = (speed.clamp(0.25, 4.0) * 100.0).round() / 100.0;
        self.mpv.set_double("speed", next);
        self.with_ui(|ui| ui.set_speed(next as f32));
        self.osd(
            &format!("Speed: {}×", trim_float(next)),
            Some((next - 0.25) / 3.75),
        );
    }

    fn set_volume(&self, value: f64) {
        let v = value.clamp(0.0, 100.0).round();
        self.mpv.set_double("volume", v);
        self.mpv.set_flag("mute", false);
        self.with_ui(|ui| {
            ui.set_volume(v as f32);
            ui.set_is_muted(false);
        });
        self.osd(&format!("Volume: {v}"), Some(v / 100.0));
    }

    /// The 16 ms tick: mpv's events every time, playback readouts at 10 Hz.
    #[allow(non_upper_case_globals)]
    fn tick(&self, ui: &AppWindow) {
        let visible = self.last_activity.get().elapsed() < IDLE_AFTER
            || ui.get_is_paused()
            || ui.get_is_scrubbing()
            || ui.get_keep_chrome()
            || !ui.get_has_file();
        if ui.get_controls_visible() != visible {
            ui.set_controls_visible(visible);
        }

        loop {
            let ev = unsafe { &*mpv_wait_event(self.mpv.get(), 0.0) };
            match ev.event_id {
                mpv_event_id_MPV_EVENT_NONE => break,
                mpv_event_id_MPV_EVENT_FILE_LOADED => self.on_file_loaded(),
                mpv_event_id_MPV_EVENT_TRACKS_CHANGED => self.update_tracks(),
                mpv_event_id_MPV_EVENT_COMMAND_REPLY => {
                    let reply = ev.reply_userdata;
                    self.scrub.borrow_mut().on_reply(&self.mpv, reply);
                }
                mpv_event_id_MPV_EVENT_END_FILE if !ev.data.is_null() => {
                    let end = unsafe { &*(ev.data as *const mpv_event_end_file) };
                    if end.reason == mpv_end_file_reason_MPV_END_FILE_REASON_ERROR as i32 {
                        self.on_file_failed();
                    }
                }
                _ => {}
            }
        }

        if self.last_poll.get().elapsed() < Duration::from_millis(100) {
            return;
        }
        self.last_poll.set(Instant::now());
        self.poll(ui);
    }

    fn poll(&self, ui: &AppWindow) {
        let mpv = &self.mpv;
        ui.set_is_fullscreen(ui.window().is_fullscreen());
        if !ui.get_has_file() {
            if self.last_presence.get().elapsed() >= Duration::from_secs(1) {
                self.last_presence.set(Instant::now());
                self.discord.on_playback_stop();
            }
            return;
        }

        // The whole queue played out: back to the start screen.
        if self.had_file.get() && mpv.get_flag("idle-active") {
            self.close_file();
            return;
        }

        let pos = mpv.get_double("time-pos");
        let dur = mpv.get_double("duration").unwrap_or(0.0);
        let paused = mpv.get_flag("pause");
        if let Some(p) = pos
            && !self.scrub.borrow().owns_display()
        {
            ui.set_position(p as f32);
        }
        ui.set_duration_seconds(dur.max(0.0) as f32);
        ui.set_is_paused(paused);
        ui.set_volume(mpv.get_double("volume").unwrap_or(100.0).clamp(0.0, 100.0) as f32);
        ui.set_is_muted(mpv.get_flag("mute"));
        ui.set_speed(mpv.get_double("speed").unwrap_or(1.0) as f32);
        ui.set_stalled(mpv.get_flag("paused-for-cache"));
        ui.set_current_chapter(mpv.get_int("chapter").unwrap_or(-1) as i32);
        ui.set_loop_mode(self.loop_mode.get());

        ui.set_ends_at(match pos {
            Some(p) if dur > 0.0 && !paused => {
                let speed = mpv.get_double("speed").unwrap_or(1.0).max(0.01);
                let left = ((dur - p) / speed).max(0.0);
                chrono::Duration::try_milliseconds((left * 1000.0) as i64)
                    .map(|d| format!("ends at {}", (chrono::Local::now() + d).format("%-I:%M %p")))
                    .unwrap_or_default()
                    .into()
            }
            _ => "".into(),
        });

        // The file changed inside the queue: title, queue marks, presence.
        let playlist_pos = mpv.get_int("playlist-pos").unwrap_or(-1);
        if playlist_pos != self.last_playlist_pos.get() {
            self.last_playlist_pos.set(playlist_pos);
            self.sync_queue_model();
            if let Some(item) = self.current() {
                ui.set_video_title(item.name.clone().into());
                let mut s = self.state.lock().unwrap();
                s.current_title = item.name.clone();
                s.current_artist = String::new();
                s.current_item_id = Some((
                    if item.torrent.is_some() { "torrent" } else { "local" }.to_string(),
                    item.id.clone(),
                ));
            }
        }

        if self.last_save.get().elapsed() >= SAVE_EVERY && !paused {
            self.last_save.set(Instant::now());
            self.save_position();
        }

        // Discord presence and provider progress, only while something plays.
        let playing = self.state.lock().unwrap().current_item_id.is_some();
        if playing && self.last_presence.get().elapsed() >= Duration::from_millis(500) {
            self.last_presence.set(Instant::now());
            let info = {
                let s = self.state.lock().unwrap();
                PlaybackInfo {
                    title: s.current_title.clone(),
                    artist: s.current_artist.clone(),
                    series_name: s.current_series_name.clone(),
                    season_index: s.current_season_index,
                    episode_index: s.current_episode_index,
                    is_paused: paused,
                    position_secs: pos.unwrap_or(0.0) as i64,
                    duration_secs: dur as i64,
                }
            };
            self.discord.on_playback_update(info);
        }
        if playing
            && pos.is_some_and(|p| p > 0.0)
            && self.last_report.get().elapsed() >= Duration::from_secs(1)
        {
            self.last_report.set(Instant::now());
            let report = {
                let s = self.state.lock().unwrap();
                s.current_item_id
                    .as_ref()
                    .and_then(|(p_id, id)| s.active_providers.get(p_id).map(|p| (p.clone(), id.clone())))
            };
            if let Some((provider, item_id)) = report {
                let t = pos.unwrap_or(0.0) as i64;
                tokio::spawn(async move {
                    let _ = provider.report_playback_progress(&item_id, t, paused).await;
                });
            }
        }
    }
}

fn trim_float(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ---- Torrents ---------------------------------------------------------------

/// Resolve a magnet or a .torrent, then play it or ask which file.
/// `resume` is the file to continue; `row` is the start-screen row it came from.
fn open_torrent(source: String, resume: Option<usize>, row: Option<String>) {
    let c = ctx();
    c.with_ui(|ui| {
        ui.set_link_error("".into());
        match &row {
            Some(folder) => ui.set_row_opening(folder.clone().into()),
            None => {
                ui.set_link_open(true);
                ui.set_link_busy(true);
            }
        }
    });
    let torrents = c.torrents.clone();
    tokio::spawn(async move {
        let result = match TorrentService::download_dir() {
            Ok(dirs) => torrents.add(&dirs, source.clone(), &SessionPrefs::default()).await,
            Err(e) => Err(e),
        };
        on_ui(move |c| {
            c.with_ui(|ui| {
                ui.set_link_busy(false);
                ui.set_row_opening("".into());
            });
            let info = match result {
                Ok(info) => info,
                Err(e) => {
                    eprintln!("[torrent] open failed: {e}");
                    c.with_ui(|ui| {
                        ui.set_link_open(true);
                        ui.set_link_text(source.clone().into());
                        ui.set_link_error(torrent_failure_text(&e).into());
                    });
                    return;
                }
            };
            let key = if source.trim().to_ascii_lowercase().starts_with("magnet:") {
                source.trim().to_string()
            } else {
                magnet_for(&info.info_hash, info.name.as_deref(), &info.trackers)
            };
            let videos = torrent_videos(&info);
            history::remember_torrent(&info.info_hash, &key, info.name.clone(), videos.len());
            refresh_rows();

            if videos.is_empty() {
                c.with_ui(|ui| {
                    ui.set_link_open(true);
                    ui.set_link_error("This torrent holds no video files.".into());
                });
                return;
            }
            let wanted = resume
                .and_then(|i| videos.iter().find(|f| f.index == i))
                .or_else(|| (videos.len() == 1).then(|| &videos[0]))
                .map(|f| f.index);
            match wanted {
                Some(index) => play_torrent_file(info, index),
                None => show_pick(info),
            }
        });
    });
}

/// "What to watch": every video of the torrent, with how far each got.
fn show_pick(info: TorrentInfo) {
    let c = ctx();
    let positions = history::torrent_positions(&info.info_hash);
    let watched: HashSet<String> = history::watched_files(&info.info_hash).into_iter().collect();
    let files: Vec<PickFile> = torrent_videos(&info)
        .iter()
        .map(|f| {
            let seen = positions.get(&f.index);
            PickFile {
                index: f.index as i32,
                name: display_name(&f.path).into(),
                path: f.path.clone().into(),
                size: fmt_size(f.size).into(),
                left: seen
                    .filter(|p| p.dur > 0.0)
                    .map(|p| format!("{} left", format_time((p.dur - p.pos).max(0.0))))
                    .unwrap_or_default()
                    .into(),
                started: seen.is_some(),
                done: watched.contains(base_name(&f.path)),
            }
        })
        .collect();
    let title = info.name.clone().unwrap_or_else(|| "What to watch".into());
    *c.pick_info.borrow_mut() = Some(info);
    c.with_ui(|ui| {
        ui.set_pick_title(title.into());
        ui.set_pick_files(model(files));
        ui.set_link_open(false);
        ui.set_pick_open(true);
    });
}

/// Play one video of a torrent, with the rest of its videos queued around it.
fn play_torrent_file(info: TorrentInfo, index: usize) {
    let c = ctx();
    let videos = torrent_videos(&info);
    let start = videos.iter().position(|f| f.index == index).unwrap_or(0);
    let items: Vec<QueueItem> = videos
        .iter()
        .map(|f| QueueItem {
            url: f.url.clone(),
            name: display_name(&f.path),
            id: torrent_id(&info.info_hash, f.index),
            torrent: Some((info.info_hash.clone(), f.index)),
            torrent_path: f.path.clone(),
        })
        .collect();
    let same = c
        .torrent_info
        .borrow()
        .as_ref()
        .is_some_and(|i| i.info_hash == info.info_hash);
    if !same {
        c.release_torrent();
    }
    *c.torrent_info.borrow_mut() = Some(info);
    c.load_queue(items, start);
    refresh_rows();
}

/// The torrent readout: status every second while a torrent file plays.
fn poll_torrent_status() {
    let c = ctx();
    let Some(item) = c.current() else {
        c.with_ui(|ui| ui.set_chip_visible(false));
        return;
    };
    let Some((hash, index)) = item.torrent.clone() else {
        c.with_ui(|ui| ui.set_chip_visible(false));
        return;
    };
    let torrents = c.torrents.clone();
    tokio::spawn(async move {
        let status = torrents.status(&hash, index).await;
        on_ui(move |c| {
            // The file may have changed while this was out.
            if c.current().and_then(|i| i.torrent) != Some((hash.clone(), index)) {
                return;
            }
            if status.state != "live" || status.peers > 0 {
                c.peerless_since.set(None);
            } else if c.peerless_since.get().is_none() {
                c.peerless_since.set(Some(Instant::now()));
            }
            let label = torrent_label(&status);
            let advice = c
                .peerless_since
                .get()
                .and_then(|since| swarm_advice(&status, since.elapsed().as_millis()));
            let progress = (status.file_size > 0)
                .then(|| status.file_done as f32 / status.file_size as f32)
                .unwrap_or(-1.0);
            c.with_ui(|ui| {
                ui.set_chip_visible(label.is_some());
                ui.set_chip_label(label.clone().unwrap_or_default().into());
                ui.set_chip_progress(progress);
                if ui.get_is_loading() {
                    let sub = match (&label, &advice) {
                        (Some(l), Some(a)) => format!("{l}\n{a}"),
                        (Some(l), None) => l.clone(),
                        (None, Some(a)) => a.clone(),
                        (None, None) => String::new(),
                    };
                    ui.set_loading_sub(sub.into());
                }
            });

            // This episode is all here: fetch the next one ahead, and only the
            // next one — more would be a background client filling the disk.
            if status.file_size > 0 && status.file_done >= status.file_size {
                prefetch_next(&hash, index);
            }
        });
    });
}

fn prefetch_next(hash: &str, index: usize) {
    let c = ctx();
    let next = {
        let info = c.torrent_info.borrow();
        let Some(info) = info.as_ref().filter(|i| i.info_hash == hash) else {
            return;
        };
        let videos = torrent_videos(info);
        let at = videos.iter().position(|f| f.index == index);
        at.and_then(|a| videos.get(a + 1)).map(|f| f.index)
    };
    let Some(next) = next else { return };
    let key = format!("{hash}/{next}");
    if !c.prefetched.borrow_mut().insert(key.clone()) {
        return;
    }
    let torrents = c.torrents.clone();
    let hash = hash.to_string();
    tokio::spawn(async move {
        if torrents.prefetch(&hash, next).await.is_err() {
            on_ui(move |c| {
                c.prefetched.borrow_mut().remove(&key);
            });
        }
    });
}

/// What is on disk, for the seekbar: every two seconds.
fn poll_torrent_buffered() {
    let c = ctx();
    let Some((hash, index)) = c.current().and_then(|i| i.torrent) else {
        return;
    };
    let torrents = c.torrents.clone();
    tokio::spawn(async move {
        let bands = torrents.buffered(&hash, index).await;
        on_ui(move |c| {
            if c.current().and_then(|i| i.torrent) != Some((hash, index)) {
                return;
            }
            let bands: Vec<Band> = bands
                .into_iter()
                .map(|(from, to)| Band {
                    from: from as f32,
                    to: to as f32,
                })
                .collect();
            c.with_ui(|ui| ui.set_buffered(model(bands)));
        });
    });
}

/// Re-read the torrents on disk for the start screen.
fn refresh_rows() {
    let torrents = ctx().torrents.clone();
    tokio::spawn(async move {
        let listed = tokio::task::spawn_blocking(move || {
            TorrentService::download_dir()
                .map(|dirs| torrents.list(&dirs))
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        on_ui(move |c| {
            let playing_hash = c.current().and_then(|i| i.torrent).map(|(h, _)| h);
            let mut rows = Vec::new();
            let mut infos = HashMap::new();
            let mut total = 0u64;
            for d in listed {
                total += d.size;
                let known = d.info_hash.as_deref().and_then(history::remembered_torrent);
                let name = known
                    .as_ref()
                    .and_then(|k| k.name.clone())
                    .or(d.name.clone())
                    .unwrap_or_else(|| {
                        if d.info_hash.is_some() {
                            "Unnamed torrent".into()
                        } else {
                            d.folder.clone()
                        }
                    });
                let magnet = known.as_ref().map(|k| k.magnet.clone()).or_else(|| {
                    d.info_hash
                        .as_deref()
                        .map(|h| magnet_for(h, Some(&name), &[]))
                });
                let mut meta = Vec::new();
                if let Some(k) = &known {
                    meta.push(format!("{} videos", k.videos));
                }
                meta.push(fmt_size(d.size));
                let resume = d
                    .info_hash
                    .as_deref()
                    .and_then(history::torrent_resume)
                    .map(|(_, p)| {
                        format!("{} — {} left", p.title, format_time((p.dur - p.pos).max(0.0)))
                    })
                    .unwrap_or_default();
                rows.push(TorrentRowData {
                    folder: d.folder.clone().into(),
                    name: name.into(),
                    meta: meta.join(" · ").into(),
                    resume: resume.into(),
                    openable: magnet.is_some(),
                    playing: d.info_hash.is_some() && d.info_hash == playing_hash,
                    watched: known.as_ref().map(|k| k.watched.len() as i32).unwrap_or(0),
                });
                infos.insert(
                    d.folder.clone(),
                    RowInfo {
                        path: d.path.clone(),
                        info_hash: d.info_hash.clone(),
                        magnet,
                    },
                );
            }
            *c.rows.borrow_mut() = infos;
            c.with_ui(|ui| {
                ui.set_torrent_rows(model(rows));
                ui.set_torrents_total(if total > 0 { fmt_size(total) } else { String::new() }.into());
            });
        });
    });
}

/// Show a torrent's folder in the system file manager.
fn reveal(path: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{path}\""))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").args(["-R", path]).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new(path));
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

fn delete_row(folder: String, watched_only: bool) {
    let c = ctx();
    let Some((path, hash)) = c
        .rows
        .borrow()
        .get(&folder)
        .map(|r| (r.path.clone(), r.info_hash.clone()))
    else {
        return;
    };
    let torrents = c.torrents.clone();
    tokio::spawn(async move {
        let result = match TorrentService::download_dir() {
            Ok(dirs) if watched_only => {
                let names = hash.as_deref().map(history::watched_files).unwrap_or_default();
                torrents.forget_files(&dirs, &path, &names).await
            }
            Ok(dirs) => torrents.forget(&dirs, &path).await,
            Err(e) => Err(e),
        };
        on_ui(move |c| {
            match result {
                Ok(freed) => {
                    if let Some(h) = &hash {
                        if watched_only {
                            history::clear_watched(h);
                        } else {
                            history::forget_torrent(h);
                        }
                    }
                    c.osd(&format!("Freed {}", fmt_size(freed)), None);
                }
                Err(e) => c.osd(&format!("Could not delete: {e}"), None),
            }
            refresh_rows();
        });
    });
}

fn local_item(path: &str) -> QueueItem {
    QueueItem {
        url: path.to_string(),
        name: display_name(path),
        id: path.to_string(),
        torrent: None,
        torrent_path: String::new(),
    }
}

// ---- Wiring -----------------------------------------------------------------

/// Install every callback on the window and start the timers. The returned
/// timers must live as long as the event loop.
pub fn install(
    ui: &AppWindow,
    mpv: MpvHandle,
    state: Arc<Mutex<AppState>>,
    discord: Arc<DiscordRPC>,
) -> Vec<slint::Timer> {
    let c = Rc::new(Ctx {
        ui: ui.as_weak(),
        mpv,
        torrents: Arc::new(TorrentService::default()),
        state,
        discord,
        scrub: RefCell::new(Scrub::default()),
        settle_timer: slint::Timer::default(),
        osd_timer: slint::Timer::default(),
        hwdec_timer: slint::Timer::default(),
        queue: RefCell::new(Vec::new()),
        torrent_info: RefCell::new(None),
        pick_info: RefCell::new(None),
        rows: RefCell::new(HashMap::new()),
        chapters: RefCell::new(Vec::new()),
        last_activity: Cell::new(Instant::now()),
        last_poll: Cell::new(Instant::now()),
        last_save: Cell::new(Instant::now()),
        last_report: Cell::new(Instant::now()),
        last_presence: Cell::new(Instant::now()),
        last_playlist_pos: Cell::new(-2),
        had_file: Cell::new(false),
        loop_mode: Cell::new(0),
        peerless_since: Cell::new(None),
        subs_attached_for: RefCell::new(String::new()),
        prefetched: RefCell::new(HashSet::new()),
    });
    CTX.with(|slot| *slot.borrow_mut() = Some(c.clone()));

    // ---- Opening -------------------------------------------------------
    ui.on_open_file(|| {
        tokio::spawn(async {
            let picked = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .add_filter("Video", VIDEO_EXTENSIONS)
                    .add_filter("Torrent", &["torrent"])
                    .add_filter("All files", &["*"])
                    .pick_files()
            })
            .await
            .ok()
            .flatten();
            let Some(paths) = picked else { return };
            on_ui(move |c| {
                let paths: Vec<String> =
                    paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                // A .torrent opened as a file is a torrent, not a video.
                if let Some(t) = paths.iter().find(|p| p.to_ascii_lowercase().ends_with(".torrent")) {
                    open_torrent(t.clone(), None, None);
                    return;
                }
                let items: Vec<QueueItem> = paths.iter().map(|p| local_item(p)).collect();
                c.load_queue(items, 0);
            });
        });
    });

    ui.on_submit_link(|text| {
        let source = text.trim().to_string();
        if source.is_empty() {
            return;
        }
        if is_torrent_source(&source) {
            open_torrent(source, None, None);
        } else {
            // A file path pasted into the box, or anything mpv opens itself.
            ctx().load_queue(vec![local_item(&source)], 0);
        }
    });

    ui.on_pick_torrent_file(|| {
        tokio::spawn(async {
            let picked = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .add_filter("Torrent", &["torrent"])
                    .pick_file()
            })
            .await
            .ok()
            .flatten();
            if let Some(p) = picked {
                let p = p.to_string_lossy().into_owned();
                on_ui(move |_| open_torrent(p, None, None));
            }
        });
    });

    ui.on_open_torrent_row(|folder| {
        let c = ctx();
        let folder = folder.to_string();
        let (magnet, hash) = match c.rows.borrow().get(&folder) {
            Some(r) => (r.magnet.clone(), r.info_hash.clone()),
            None => return,
        };
        let Some(magnet) = magnet else { return };
        let resume = hash.as_deref().and_then(history::torrent_resume).map(|(i, _)| i);
        open_torrent(magnet, resume, Some(folder));
    });

    ui.on_reveal_torrent_row(|folder| {
        if let Some(r) = ctx().rows.borrow().get(folder.as_str()) {
            reveal(&r.path);
        }
    });

    ui.on_delete_torrent_row(|folder, watched_only| delete_row(folder.to_string(), watched_only));

    ui.on_pick_torrent_video(|index| {
        let c = ctx();
        let Some(info) = c.pick_info.borrow_mut().take() else { return };
        play_torrent_file(info, index.max(0) as usize);
    });

    ui.on_close_file(|| ctx().close_file());

    // ---- Transport -----------------------------------------------------
    ui.on_toggle_pause(|| {
        let c = ctx();
        let paused = c.mpv.get_flag("pause");
        c.mpv.set_flag("pause", !paused);
        c.with_ui(|ui| ui.set_is_paused(!paused));
    });

    ui.on_seek_relative(|delta| {
        let c = ctx();
        let exact = c.scrub.borrow().want_exact(false);
        c.mpv.command(&[
            "seek",
            &format!("{delta:.3}"),
            if exact { "relative+exact" } else { "relative+keyframes" },
        ]);
    });

    ui.on_seek_absolute(|t| {
        ctx()
            .mpv
            .command(&["seek", &format!("{:.3}", t.max(0.0)), "absolute+exact"]);
    });

    ui.on_scrub_begin(|t| ctx().scrub.borrow_mut().begin(t as f64));
    ui.on_scrub_move(|t| {
        let c = ctx();
        c.scrub.borrow_mut().move_to(&c.mpv, t as f64);
        // Resting for a moment settles on the exact frame under the pointer.
        c.settle_timer.start(slint::TimerMode::SingleShot, DRAG_SETTLE, || {
            let c = ctx();
            c.scrub.borrow_mut().settle(&c.mpv);
        });
    });
    ui.on_scrub_end(|t| {
        let c = ctx();
        c.settle_timer.stop();
        c.scrub.borrow_mut().end(&c.mpv, t as f64);
        c.with_ui(|ui| ui.set_position(t));
    });
    ui.on_scrub_cancel(|| {
        let c = ctx();
        c.settle_timer.stop();
        c.scrub.borrow_mut().cancel(&c.mpv);
    });

    ui.on_seek_hover(|t| {
        let c = ctx();
        let name = c
            .chapters
            .borrow()
            .iter()
            .rev()
            .find(|(_, time)| *time <= t as f64)
            .map(|(title, _)| title.clone())
            .unwrap_or_default();
        c.with_ui(|ui| ui.set_hover_chapter(name.into()));
    });

    ui.on_frame_step(|dir| {
        ctx()
            .mpv
            .command(&[if dir < 0 { "frame-back-step" } else { "frame-step" }]);
    });

    ui.on_set_volume(|v| ctx().set_volume(v as f64));

    ui.on_toggle_mute(|| {
        let c = ctx();
        let muted = !c.mpv.get_flag("mute");
        c.mpv.set_flag("mute", muted);
        c.with_ui(|ui| ui.set_is_muted(muted));
        c.osd(if muted { "Sound: off" } else { "Sound: on" }, None);
    });

    ui.on_change_speed(|factor| {
        let c = ctx();
        let speed = c.mpv.get_double("speed").unwrap_or(1.0);
        c.set_speed(speed * factor as f64);
    });
    ui.on_set_speed(|s| ctx().set_speed(s as f64));

    // off → all → one → off, as Frame Player cycles it.
    ui.on_cycle_loop(|| {
        let c = ctx();
        let next = (c.loop_mode.get() + 1) % 3;
        c.loop_mode.set(next);
        c.mpv.set_property_string("loop-file", if next == 2 { "inf" } else { "no" });
        c.mpv.set_property_string("loop-playlist", if next == 1 { "inf" } else { "no" });
        c.with_ui(|ui| ui.set_loop_mode(next));
        c.osd(
            ["Repeat: off", "Repeat: all", "Repeat: one"][next as usize],
            None,
        );
    });

    ui.on_playlist_prev(|| {
        let c = ctx();
        c.save_position();
        c.mpv.command(&["playlist-prev"]);
    });
    ui.on_playlist_next(|| {
        let c = ctx();
        c.save_position();
        c.mpv.command(&["playlist-next"]);
    });
    ui.on_play_index(|i| {
        let c = ctx();
        c.save_position();
        c.mpv.command(&["playlist-play-index", &i.to_string()]);
    });

    ui.on_chapter_step(|d| {
        ctx().mpv.command(&["add", "chapter", &d.to_string()]);
    });
    ui.on_seek_chapter(|i| {
        ctx().mpv.set_property_string("chapter", &i.to_string());
    });

    ui.on_select_audio_track(|id| {
        let c = ctx();
        c.mpv.set_property_string("aid", &id.to_string());
        c.update_tracks();
    });
    ui.on_select_subtitle_track(|id| {
        let c = ctx();
        c.mpv
            .set_property_string("sid", &if id == -1 { "no".to_string() } else { id.to_string() });
        c.update_tracks();
        if id == -1 {
            c.osd("Subtitles off", None);
        }
    });

    ui.on_toggle_fullscreen(|| {
        ctx().with_ui(|ui| {
            let next = !ui.window().is_fullscreen();
            ui.window().set_fullscreen(next);
            ui.set_is_fullscreen(next);
        });
    });

    ui.on_user_activity(|| ctx().last_activity.set(Instant::now()));

    ui.on_toggle_info_card(|lean| {
        // spincard registers both bindings under its own script name; the
        // lean variant hides whatever `lean_hide` lists (the synopsis, by
        // default). Pressing one while the other is up switches in place.
        ctx().mpv.script_binding(if lean {
            "spincard/toggle-lean"
        } else {
            "spincard/toggle"
        });
    });

    // GPU button. Note this is NOT an OpenGL/Vulkan switch: libmpv's render
    // API only defines "opengl" and "sw" (render.h), and CedarApple hands mpv
    // a GL FBO, so the render backend is fixed by the embedding. hwdec is the
    // GPU lever that is actually live here - it moves decoding on and off the
    // GPU without touching how frames reach the window.
    ui.on_toggle_hwdec(|| {
        let c = ctx();
        let current = c.mpv.get_property_string("hwdec").unwrap_or_default();
        let next = if current == "no" || current.is_empty() { "auto" } else { "no" };
        c.mpv.set_property_string("hwdec", next);
        c.osd(&format!("hwdec: {next}…"), None);
        // "auto" is a request, not an outcome - mpv picks (or refuses) a
        // decoder when the video re-inits, so read hwdec-current back a beat
        // later and report what it actually settled on.
        c.hwdec_timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(800),
            || {
                let c = ctx();
                let effective = c.mpv.get_property_string("hwdec-current").unwrap_or_default();
                let msg = match effective.as_str() {
                    "" | "no" => "hwdec: off (CPU decoding)".to_string(),
                    other => format!("hwdec: {other} (GPU decoding)"),
                };
                c.osd(&msg, None);
            },
        );
    });

    // ---- Timers --------------------------------------------------------
    let status_timer = slint::Timer::default();
    status_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), poll_torrent_status);
    let buffer_timer = slint::Timer::default();
    buffer_timer.start(slint::TimerMode::Repeated, Duration::from_secs(2), poll_torrent_buffered);

    refresh_rows();
    vec![status_timer, buffer_timer]
}

/// Called from the render tick in main.rs.
pub fn tick(ui: &AppWindow) {
    ctx().tick(ui);
}

/// Before the window closes: remember where playback was.
pub fn shutdown() {
    ctx().save_position();
}

#[derive(serde::Deserialize, Debug)]
struct Track {
    id: i32,
    #[serde(rename = "type")]
    track_type: TrackType,
    #[serde(rename = "selected", default)]
    active: bool,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    codec: Option<String>,
}

impl Track {
    fn as_track_info(&self) -> TrackInfo {
        let name = self
            .title
            .as_ref()
            .or(self.lang.as_ref())
            .cloned()
            .unwrap_or_else(|| "Unknown".into());
        TrackInfo {
            active: self.active,
            id: self.id,
            name: match &self.codec {
                Some(c) if !c.is_empty() => format!("{} ({})", name, c).into(),
                _ => name.into(),
            },
        }
    }
}

#[derive(serde::Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
enum TrackType {
    Audio,
    Video,
    Sub,
    #[serde(other)]
    Other,
}

