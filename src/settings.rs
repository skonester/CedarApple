//! Settings: the options mpv and the player take, kept in `settings.json`
//! beside history.json, applied at startup and the moment one changes.
//!
//! Ported from Vivi's settings (src/config.rs, src/settings/apply.rs,
//! src/settings/rows.rs, src/settings/schema.rs; MIT, Copyright (c) 2026
//! Ruslan Popelyshyn): the same options, mpv mappings, ranges and wording,
//! for the categories that act on something CedarApple has. Vivi's theme,
//! accent, toolbar profiles, skins, library, extension store and remotes have
//! no counterpart here and are left out. Free Music's own settings show as a
//! category but stay in free-music.json (src/free_music).
//!
//! CedarApple's defaults win where it already had a behaviour: software
//! decoding, volume up to 100 %, controls hiding after 3 s, ←/→ 5 s and J/L 10 s.

use crate::player::MpvHandle;
use crate::{AppWindow, SettingRow, SettingsState};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

/// Seconds without pointer movement before the chrome hides (read every tick).
pub static AUTOHIDE_SECS: AtomicU32 = AtomicU32::new(3);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub interface: Interface,
    pub playback: Playback,
    pub video: Video,
    pub audio: Audio,
    pub subtitles: Subtitles,
    pub controls: Controls,
    pub network: Network,
    pub performance: Performance,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            interface: Interface::default(),
            playback: Playback::default(),
            video: Video::default(),
            audio: Audio::default(),
            subtitles: Subtitles::default(),
            controls: Controls::default(),
            network: Network::default(),
            performance: Performance::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct General {
    pub save_log: bool,
    pub log_file: String,
    /// "Errors only" | "Warnings" | "Everything"
    pub log_level: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            save_log: false,
            log_file: dirs()
                .map(|d| d.cache_dir().join("cedarapple.log"))
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            log_level: "Errors only".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Interface {
    pub autohide_secs: u32,
}

impl Default for Interface {
    fn default() -> Self {
        Self { autohide_secs: 3 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Playback {
    /// Remember where each file was left and continue from there.
    pub resume: bool,
    /// mpv `hwdec`.
    pub hwdec: String,
    pub speed: f64,
    /// "Off" | "One" | "All"
    pub repeat: String,
    pub precise_seek: bool,
    /// yt-dlp quality for web links (mpv `ytdl-format`).
    pub quality: String,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            resume: true,
            // CedarApple renders into a GL FBO and has always decoded in
            // software by default (src/player/pipeline.rs); the GPU button
            // and this setting turn hardware decoding on.
            hwdec: "no".into(),
            speed: 1.0,
            repeat: "Off".into(),
            precise_seek: false,
            quality: "Best available".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Video {
    pub enabled: bool,
    pub stop_screensaver: bool,
    pub hue: i64,
    pub brightness: i64,
    pub contrast: i64,
    pub saturation: i64,
    pub gamma: i64,
    /// mpv `deinterlace`: "auto" | "yes" | "no"
    pub deinterlace: String,
    /// mpv `video-aspect-override` ("no" = automatic).
    pub aspect: String,
    pub screenshot_dir: String,
    /// mpv `screenshot-format`: "png" | "jpg" | "webp"
    pub screenshot_format: String,
}

impl Default for Video {
    fn default() -> Self {
        Self {
            enabled: true,
            stop_screensaver: true,
            hue: 0,
            brightness: 0,
            contrast: 0,
            saturation: 0,
            gamma: 0,
            deinterlace: "auto".into(),
            aspect: "no".into(),
            screenshot_dir: directories::UserDirs::new()
                .and_then(|u| u.picture_dir().map(|p| p.to_path_buf()))
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            screenshot_format: "png".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Audio {
    pub enabled: bool,
    /// mpv `audio-device` ("auto" = the system default).
    pub device: String,
    pub volume_max: i64,
    pub startup_volume: i64,
    pub normalize: bool,
    /// mpv `alang`.
    pub language: String,
    pub delay: f64,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            enabled: true,
            device: "auto".into(),
            volume_max: 100,
            startup_volume: 100,
            normalize: false,
            language: String::new(),
            delay: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Subtitles {
    pub enabled: bool,
    /// mpv `slang`.
    pub language: String,
    /// mpv `sub-codepage`.
    pub encoding: String,
    /// mpv `sub-auto`.
    pub auto_load: String,
    pub font: String,
    pub scale: f64,
    pub bold: bool,
    pub color: String,
    pub opacity: i64,
    pub outline: f64,
    pub outline_color: String,
    pub shadow: bool,
    pub position: i64,
}

impl Default for Subtitles {
    fn default() -> Self {
        Self {
            enabled: true,
            language: String::new(),
            encoding: "auto".into(),
            auto_load: "fuzzy".into(),
            font: "sans-serif".into(),
            scale: 1.0,
            bold: false,
            color: "#FFFFFF".into(),
            opacity: 100,
            outline: 3.0,
            outline_color: "#000000".into(),
            shadow: false,
            position: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Controls {
    /// ← / → seek step in seconds (Shift: one second, always).
    pub short_skip: u32,
    /// J / L seek step in seconds.
    pub long_skip: u32,
    /// ↑ / ↓ and wheel volume step.
    pub volume_step: u32,
    /// "Volume" | "Seek" | "Nothing"
    pub wheel: String,
    pub wheel_reverse: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            short_skip: 5,
            long_skip: 10,
            volume_step: 5,
            wheel: "Volume".into(),
            wheel_reverse: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Network {
    /// "Low" | "Normal" | "High" (demuxer cache size).
    pub buffering: String,
    /// mpv `cache`: "auto" | "yes" | "no"
    pub cache: String,
    pub cache_secs: i64,
    pub http_proxy: String,
    pub tls_verify: bool,
    pub user_agent: String,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            buffering: "Normal".into(),
            cache: "yes".into(),
            cache_secs: 60,
            http_proxy: String::new(),
            tls_verify: true,
            user_agent: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Performance {
    pub threads: i64,
    pub skip_loop_filter: String,
    pub framedrop: bool,
    pub video_sync: String,
}

impl Default for Performance {
    fn default() -> Self {
        Self {
            threads: 0,
            skip_loop_filter: "default".into(),
            framedrop: true,
            video_sync: "audio".into(),
        }
    }
}

// ---- Storage -----------------------------------------------------------------

fn dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "CedarApple")
}

fn path() -> Option<PathBuf> {
    let d = dirs()?;
    std::fs::create_dir_all(d.data_dir()).ok()?;
    Some(d.data_dir().join("settings.json"))
}

fn store() -> &'static Mutex<Config> {
    static STORE: OnceLock<Mutex<Config>> = OnceLock::new();
    STORE.get_or_init(|| {
        let c = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Mutex::new(c)
    })
}

/// The current settings.
pub fn get() -> Config {
    store().lock().unwrap().clone()
}

/// Through a temp file and a rename, like history.json.
fn save(c: &Config) {
    let Some(p) = path() else { return };
    let Ok(bytes) = serde_json::to_vec_pretty(c) else { return };
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

fn update(f: impl FnOnce(&mut Config)) -> Config {
    let mut c = store().lock().unwrap();
    f(&mut c);
    save(&c);
    c.clone()
}

/// The GPU button changed hwdec: remember it as the setting.
pub fn remember_hwdec(v: &str) {
    update(|c| c.playback.hwdec = v.to_string());
}

// ---- Applying ----------------------------------------------------------------

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// `#RRGGBB` + opacity % → mpv `#AARRGGBB`.
pub fn mpv_color(rgb: &str, opacity: i64) -> String {
    let h = rgb.trim().trim_start_matches('#');
    let h = if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        h
    } else {
        "FFFFFF"
    };
    format!("#{:02X}{h}", (opacity.clamp(0, 100) * 255 / 100) as u8)
}

pub fn ytdl_format(q: &str) -> String {
    match q.trim_end_matches('p').parse::<u32>() {
        Ok(h) => format!("bestvideo[height<=?{h}]+bestaudio/best[height<=?{h}]/best"),
        Err(_) if q == "Audio only" => "bestaudio/best".into(),
        Err(_) => "bestvideo+bestaudio/best".into(),
    }
}

/// The mpv options the settings drive, as Vivi's `apply_player` sets them:
/// (property, value). Loudness normalisation is a filter, see [`apply_mpv`].
fn mpv_props(c: &Config) -> Vec<(&'static str, String)> {
    let (file, list) = match c.playback.repeat.as_str() {
        "One" => ("inf", "no"),
        "All" => ("no", "inf"),
        _ => ("no", "no"),
    };
    let mut v: Vec<(&'static str, String)> = vec![
        // Playback
        ("hwdec", c.playback.hwdec.clone()),
        ("speed", c.playback.speed.to_string()),
        ("hr-seek", if c.playback.precise_seek { "yes" } else { "default" }.into()),
        ("ytdl-format", ytdl_format(&c.playback.quality)),
        ("loop-file", file.into()),
        ("loop-playlist", list.into()),
        // Video
        ("vid", if c.video.enabled { "auto" } else { "no" }.into()),
        ("stop-screensaver", yes(c.video.stop_screensaver).into()),
        ("hue", c.video.hue.to_string()),
        ("brightness", c.video.brightness.to_string()),
        ("contrast", c.video.contrast.to_string()),
        ("saturation", c.video.saturation.to_string()),
        ("gamma", c.video.gamma.to_string()),
        ("deinterlace", c.video.deinterlace.clone()),
        ("video-aspect-override", c.video.aspect.clone()),
        ("screenshot-format", c.video.screenshot_format.clone()),
        // Audio
        ("aid", if c.audio.enabled { "auto" } else { "no" }.into()),
        ("audio-device", c.audio.device.clone()),
        ("alang", c.audio.language.clone()),
        ("volume-max", c.audio.volume_max.max(100).to_string()),
        ("audio-delay", c.audio.delay.to_string()),
        // Subtitles
        ("sub-visibility", yes(c.subtitles.enabled).into()),
        ("slang", c.subtitles.language.clone()),
        ("sub-scale", c.subtitles.scale.to_string()),
        ("sub-auto", c.subtitles.auto_load.clone()),
        ("sub-codepage", c.subtitles.encoding.clone()),
        ("sub-bold", yes(c.subtitles.bold).into()),
        ("sub-color", mpv_color(&c.subtitles.color, c.subtitles.opacity)),
        ("sub-border-size", c.subtitles.outline.to_string()),
        ("sub-border-color", mpv_color(&c.subtitles.outline_color, 100)),
        ("sub-shadow-offset", if c.subtitles.shadow { "2" } else { "0" }.into()),
        ("sub-pos", c.subtitles.position.to_string()),
        // Network
        (
            "demuxer-max-bytes",
            match c.network.buffering.as_str() {
                "Low" => "32MiB",
                "High" => "512MiB",
                _ => "150MiB",
            }
            .into(),
        ),
        ("cache", c.network.cache.clone()),
        ("cache-secs", c.network.cache_secs.to_string()),
        ("http-proxy", c.network.http_proxy.trim().into()),
        ("tls-verify", yes(c.network.tls_verify).into()),
        // Performance
        ("vd-lavc-threads", c.performance.threads.to_string()),
        ("vd-lavc-skiploopfilter", c.performance.skip_loop_filter.clone()),
        ("framedrop", if c.performance.framedrop { "vo" } else { "no" }.into()),
        ("video-sync", c.performance.video_sync.clone()),
    ];
    // Empty means "leave mpv's own default".
    for (k, val) in [
        ("screenshot-directory", c.video.screenshot_dir.trim()),
        ("sub-font", c.subtitles.font.trim()),
        ("user-agent", c.network.user_agent.trim()),
    ] {
        if !val.is_empty() {
            v.push((k, val.into()));
        }
    }
    // Diagnostics
    if c.general.save_log && !c.general.log_file.is_empty() {
        v.push(("log-file", c.general.log_file.clone()));
        v.push((
            "msg-level",
            match c.general.log_level.as_str() {
                "Warnings" => "all=warn",
                "Everything" => "all=v",
                _ => "all=error",
            }
            .into(),
        ));
    } else {
        v.push(("log-file", String::new()));
    }
    v
}

fn apply_normalize(mpv: &MpvHandle, on: bool) {
    // Labelled filter, so other audio filters are left alone.
    let has = mpv
        .get_property_string("af")
        .is_some_and(|f| f.contains("cedar-norm"));
    if has && !on {
        mpv.command(&["af", "remove", "@cedar-norm"]);
    }
    if on && !has {
        mpv.command(&["af", "add", "@cedar-norm:lavfi=[dynaudnorm]"]);
    }
}

/// Every option, at startup.
pub fn apply_mpv(mpv: &MpvHandle, c: &Config) {
    for (k, v) in mpv_props(c) {
        mpv.set_property_string(k, &v);
    }
    apply_normalize(mpv, c.audio.normalize);
}

/// Only what a change touched, so the speed, repeat or picture the viewer set
/// for this file survive an unrelated setting.
fn apply_changes(mpv: &MpvHandle, old: &Config, new: &Config) {
    let before = mpv_props(old);
    for (k, v) in mpv_props(new) {
        if !before.iter().any(|(bk, bv)| *bk == k && *bv == v) {
            mpv.set_property_string(k, &v);
        }
    }
    if old.audio.normalize != new.audio.normalize {
        apply_normalize(mpv, new.audio.normalize);
    }
}

/// Shift+L changed the repeat mode: remember it as the setting.
pub fn remember_repeat(mode: i32) {
    update(|c| {
        c.playback.repeat = match mode {
            1 => "All",
            2 => "One",
            _ => "Off",
        }
        .into()
    });
}

/// What the window reads from the settings: control steps, wheel, volume range.
pub fn apply_ui(ui: &AppWindow, c: &Config) {
    AUTOHIDE_SECS.store(c.interface.autohide_secs.clamp(1, 10), Ordering::Relaxed);
    ui.set_skip_short(c.controls.short_skip.max(1) as f32);
    ui.set_skip_long(c.controls.long_skip.max(1) as f32);
    ui.set_volume_step(c.controls.volume_step.clamp(1, 50) as f32);
    ui.set_wheel_action(c.controls.wheel.clone().into());
    ui.set_wheel_reverse(c.controls.wheel_reverse);
    ui.set_volume_max(c.audio.volume_max.max(100) as f32);
    ui.set_loop_mode(match c.playback.repeat.as_str() {
        "All" => 1,
        "One" => 2,
        _ => 0,
    });
}

// ---- The rows ----------------------------------------------------------------

pub const CATEGORIES: [&str; 11] = [
    "Playback",
    "Video",
    "Audio",
    "Subtitles",
    "Controls",
    "Interface",
    "Network",
    "Performance",
    "Privacy",
    "General",
    "Free Music",
];

const ASPECTS: [(&str, &str); 6] = [
    ("Automatic", "no"),
    ("16:9", "16:9"),
    ("4:3", "4:3"),
    ("21:9", "21:9"),
    ("2.35:1", "2.35:1"),
    ("1:1", "1:1"),
];
const OUTLINES: [(&str, f64); 4] = [("None", 0.0), ("Thin", 1.5), ("Normal", 3.0), ("Thick", 5.0)];

fn outline_label(v: f64) -> &'static str {
    OUTLINES
        .iter()
        .min_by(|a, b| (a.1 - v).abs().total_cmp(&(b.1 - v).abs()))
        .map_or("Normal", |o| o.0)
}

fn trim_num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Builds one category's rows.
#[derive(Default)]
struct Rows(Vec<SettingRow>);

impl Rows {
    fn push(&mut self, id: &str, kind: &str, title: &str, info: &str, value: String) -> &mut Self {
        self.0.push(SettingRow {
            id: id.into(),
            kind: kind.into(),
            title: title.into(),
            info: info.into(),
            value: value.clone().into(),
            display: value.into(),
            options: ModelRc::default(),
            num: 0.0,
            min: 0.0,
            max: 1.0,
            step: 1.0,
        });
        self
    }
    fn header(&mut self, t: &str) -> &mut Self {
        self.push("", "header", t, "", String::new())
    }
    fn toggle(&mut self, id: &str, t: &str, info: &str, on: bool) -> &mut Self {
        self.push(id, "toggle", t, info, on.to_string())
    }
    fn options(&mut self, o: &[&str]) {
        if let Some(r) = self.0.last_mut() {
            r.options = ModelRc::new(VecModel::from(
                o.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>(),
            ));
        }
    }
    fn choice(&mut self, id: &str, t: &str, info: &str, val: &str, o: &[&str]) -> &mut Self {
        self.push(id, "choice", t, info, val.into());
        self.options(o);
        self
    }
    /// A few options side by side.
    fn seg(&mut self, id: &str, t: &str, info: &str, val: &str, o: &[&str]) -> &mut Self {
        self.push(id, "seg", t, info, val.into());
        self.options(o);
        self
    }
    fn slider(
        &mut self,
        id: &str,
        t: &str,
        info: &str,
        val: f64,
        (min, max, step): (f64, f64, f64),
        unit: &str,
    ) -> &mut Self {
        self.push(id, "slider", t, info, trim_num(val));
        if let Some(r) = self.0.last_mut() {
            r.num = val as f32;
            r.min = min as f32;
            r.max = max as f32;
            r.step = step as f32;
            r.display = format!("{}{unit}", trim_num(val)).into();
        }
        self
    }
    fn text(&mut self, id: &str, t: &str, info: &str, val: &str, placeholder: &str) -> &mut Self {
        self.push(id, "text", t, info, val.into());
        self.options(&[placeholder]);
        self
    }
    /// A text box with a "Choose…" button (`<id>.choose` comes back as an action).
    fn path(&mut self, id: &str, t: &str, info: &str, val: &str) -> &mut Self {
        self.push(id, "path", t, info, val.into())
    }
    fn action(&mut self, id: &str, t: &str, info: &str, button: &str) -> &mut Self {
        self.push(id, "action", t, info, button.into())
    }
}

fn rows(cat: &str, c: &Config, devices: &[(String, String)]) -> Vec<SettingRow> {
    let mut r = Rows::default();
    let pct = "%";
    match cat {
        "Playback" => {
            r.header("Decoding")
                .choice("playback.hwdec", "Hardware-accelerated decoding", "auto-safe uses the GPU when it is known to work; no decodes in software (mpv hwdec). The GPU button in the top bar switches it too.", &c.playback.hwdec, &["no", "auto-safe", "auto", "d3d11va", "dxva2", "nvdec", "vaapi", "vulkan", "videotoolbox"])
                .header("Startup")
                .slider("playback.speed", "Default playback speed", "Speed applied to every new file (mpv speed).", c.playback.speed, (0.25, 2.0, 0.05), "×")
                .toggle("playback.resume", "Remember the playback position", "Continue each file from where it was left.", c.playback.resume)
                .header("Tracks")
                .choice("playback.quality", "Preferred video quality", "Stream quality picked by yt-dlp for web links (mpv ytdl-format).", &c.playback.quality, &["Best available", "2160p", "1080p", "720p", "480p", "Audio only"])
                .header("Order")
                .seg("playback.repeat", "Repeat", "Off, repeat the current file (mpv loop-file) or the whole queue (loop-playlist).", &c.playback.repeat, &["Off", "One", "All"])
                .header("Seeking")
                .toggle("playback.precise_seek", "Seek precisely", "Seek to the exact frame instead of the nearest keyframe (mpv hr-seek). Slower on some files.", c.playback.precise_seek);
        }
        "Video" => {
            let aspect = ASPECTS
                .iter()
                .find(|a| a.1 == c.video.aspect)
                .map_or("Automatic", |a| a.0);
            r.header("Output")
                .toggle("video.enabled", "Enable video", "Turn off to play only the audio of video files (mpv vid=no).", c.video.enabled)
                .toggle("video.stop_screensaver", "Stop the screensaver during playback", "mpv stop-screensaver.", c.video.stop_screensaver)
                .header("Picture")
                .slider("video.hue", "Hue", "Default hue (mpv hue).", c.video.hue as f64, (-100.0, 100.0, 1.0), pct)
                .slider("video.brightness", "Brightness", "Default brightness (mpv brightness).", c.video.brightness as f64, (-100.0, 100.0, 1.0), pct)
                .slider("video.contrast", "Contrast", "Default contrast (mpv contrast).", c.video.contrast as f64, (-100.0, 100.0, 1.0), pct)
                .slider("video.saturation", "Saturation", "Default saturation (mpv saturation).", c.video.saturation as f64, (-100.0, 100.0, 1.0), pct)
                .slider("video.gamma", "Gamma", "Default gamma (mpv gamma).", c.video.gamma as f64, (-100.0, 100.0, 1.0), pct)
                .action("video.reset_picture", "Reset picture", "Set hue, brightness, contrast, saturation and gamma back to 0.", "Reset")
                .header("Deinterlacing")
                .choice("video.deinterlace", "Deinterlacing", "mpv deinterlace: auto only deinterlaces interlaced content.", &c.video.deinterlace, &["auto", "yes", "no"])
                .header("Framing")
                .choice("video.aspect", "Aspect ratio", "Force a display aspect ratio (mpv video-aspect-override).", aspect, &ASPECTS.map(|a| a.0))
                .header("Snapshots")
                .path("video.screenshot_dir", "Save snapshots to", "Folder for snapshots taken with S (mpv screenshot-directory).", &c.video.screenshot_dir)
                .seg("video.screenshot_format", "Format", "mpv screenshot-format.", &c.video.screenshot_format, &["png", "jpg", "webp"]);
        }
        "Audio" => {
            let current = devices
                .iter()
                .find(|d| d.0 == c.audio.device)
                .map_or(c.audio.device.as_str(), |d| d.1.as_str());
            let mut names: Vec<&str> = vec!["auto"];
            names.extend(devices.iter().filter(|d| d.0 != "auto").map(|d| d.1.as_str()));
            r.header("Output")
                .toggle("audio.enabled", "Enable audio", "Turn off to play video without sound (mpv aid=no).", c.audio.enabled)
                .choice("audio.device", "Output device", "mpv audio-device; auto uses the system default.", current, &names)
                .header("Volume")
                .slider("audio.volume_max", "Maximum volume", "Top of the volume bar (mpv volume-max); above 100% amplifies and may distort.", c.audio.volume_max as f64, (100.0, 300.0, 10.0), pct)
                .slider("audio.startup_volume", "Startup volume", "Volume when CedarApple starts.", c.audio.startup_volume as f64, (0.0, 100.0, 5.0), pct)
                .header("Loudness")
                .toggle("audio.normalize", "Normalize loudness", "Even out quiet and loud passages (lavfi dynaudnorm filter).", c.audio.normalize)
                .header("Tracks")
                .text("audio.language", "Preferred audio language", "Comma-separated language codes in priority order (mpv alang).", &c.audio.language, "e.g. ja,en")
                .slider("audio.delay", "Default audio delay", "mpv audio-delay applied to every file.", c.audio.delay, (-2.0, 2.0, 0.05), " s");
        }
        "Subtitles" => {
            r.header("Tracks & Languages")
                .toggle("subtitles.enabled", "Enable subtitles", "Show subtitles (mpv sub-visibility).", c.subtitles.enabled)
                .text("subtitles.language", "Preferred language", "Comma-separated language codes in priority order (mpv slang).", &c.subtitles.language, "e.g. en,es")
                .choice("subtitles.encoding", "Text encoding", "Character set of text subtitles (mpv sub-codepage).", &c.subtitles.encoding, &["auto", "utf8", "cp1251", "cp1252", "latin1", "koi8-u", "shift_jis", "gb18030"])
                .choice("subtitles.auto_load", "Load subtitle files automatically", "Load .srt/.ass files next to the video: exact name, fuzzy match or every file in the folder (mpv sub-auto).", &c.subtitles.auto_load, &["no", "exact", "fuzzy", "all"])
                .header("Look")
                .text("subtitles.font", "Font", "mpv sub-font (text subtitles only).", &c.subtitles.font, "sans-serif")
                .slider("subtitles.scale", "Font size", "Subtitle scale factor (mpv sub-scale, 1 = default).", c.subtitles.scale, (0.5, 3.0, 0.1), "×")
                .toggle("subtitles.bold", "Bold", "mpv sub-bold.", c.subtitles.bold)
                .text("subtitles.color", "Text color", "#RRGGBB (mpv sub-color).", &c.subtitles.color, "#FFFFFF")
                .slider("subtitles.opacity", "Text opacity", "Opacity of the text color.", c.subtitles.opacity as f64, (0.0, 100.0, 5.0), pct)
                .choice("subtitles.outline", "Outline thickness", "mpv sub-border-size.", outline_label(c.subtitles.outline), &OUTLINES.map(|o| o.0))
                .text("subtitles.outline_color", "Outline color", "#RRGGBB (mpv sub-border-color).", &c.subtitles.outline_color, "#000000")
                .toggle("subtitles.shadow", "Shadow", "Drop shadow under the text (mpv sub-shadow-offset).", c.subtitles.shadow)
                .header("Position")
                .slider("subtitles.position", "Vertical position", "mpv sub-pos: 100 is the bottom edge.", c.subtitles.position as f64, (50.0, 150.0, 1.0), "");
        }
        "Controls" => {
            r.header("Skip Amounts")
                .slider("controls.short_skip", "Short skip", "Seek step for ← and → (Shift: one second).", c.controls.short_skip as f64, (1.0, 30.0, 1.0), " s")
                .slider("controls.long_skip", "Long skip", "Seek step for J and L.", c.controls.long_skip as f64, (5.0, 120.0, 5.0), " s")
                .slider("controls.volume_step", "Volume step", "Volume change for ↑, ↓ and the scroll wheel.", c.controls.volume_step as f64, (1.0, 20.0, 1.0), pct)
                .header("Mouse")
                .seg("controls.wheel", "Scroll wheel", "What the wheel does over the picture.", &c.controls.wheel, &["Volume", "Seek", "Nothing"])
                .toggle("controls.wheel_reverse", "Reverse scroll direction", "Wheel down raises the volume / seeks forward.", c.controls.wheel_reverse);
        }
        "Interface" => {
            r.header("Playback Controls").slider(
                "interface.autohide",
                "Hide controls after",
                "Seconds without mouse movement before the bars and the cursor hide during playback.",
                c.interface.autohide_secs as f64,
                (1.0, 10.0, 1.0),
                " s",
            );
        }
        "Network" => {
            r.header("Streaming")
                .choice("network.buffering", "Buffering", "Size of the read-ahead cache: Low 32 MiB, Normal 150 MiB, High 512 MiB (mpv demuxer-max-bytes).", &c.network.buffering, &["Low", "Normal", "High"])
                .choice("network.cache", "Cache", "mpv cache: auto enables it for network streams only.", &c.network.cache, &["auto", "yes", "no"])
                .slider("network.cache_secs", "Read ahead", "mpv cache-secs.", c.network.cache_secs as f64, (5.0, 600.0, 5.0), " s")
                .header("Connection")
                .text("network.http_proxy", "HTTP proxy", "mpv http-proxy, e.g. http://host:3128. Empty uses no proxy.", &c.network.http_proxy, "No proxy")
                .toggle("network.tls_verify", "Verify certificates against the system trust store", "mpv tls-verify.", c.network.tls_verify)
                .text("network.user_agent", "User agent", "mpv user-agent for HTTP requests. Empty uses mpv's default.", &c.network.user_agent, "mpv's default");
        }
        "Performance" => {
            r.header("Decoding")
                .slider("performance.threads", "Decoding threads", "mpv vd-lavc-threads; 0 = automatic.", c.performance.threads as f64, (0.0, 16.0, 1.0), "")
                .choice("performance.skip_loop_filter", "Skip the H.264 loop filter", "Faster decoding at some quality cost (mpv vd-lavc-skiploopfilter).", &c.performance.skip_loop_filter, &["default", "nonref", "bidir", "nonkey", "all"])
                .header("Timing")
                .toggle("performance.framedrop", "Drop frames when the system can't keep up", "mpv framedrop=vo.", c.performance.framedrop)
                .choice("performance.video_sync", "Clock synchronisation", "mpv video-sync.", &c.performance.video_sync, &["audio", "display-resample", "display-vdrop", "desync"]);
        }
        "Privacy" => {
            r.header("History")
                .toggle("playback.resume", "Remember the playback position", "Continue each file from where it was left.", c.playback.resume)
                .action("privacy.clear_positions", "Clear resume positions", "Delete every saved playback position. The torrent list stays.", "Clear");
        }
        "General" => {
            r.header("Diagnostics")
                .toggle("general.save_log", "Save a log", "Write mpv's log to a file (mpv log-file).", c.general.save_log)
                .path("general.log_file", "Log file", "Where the log is written.", &c.general.log_file)
                .choice("general.log_level", "Log details", "How much detail the log file gets (mpv msg-level).", &c.general.log_level, &["Errors only", "Warnings", "Everything"]);
        }
        "Free Music" => {
            let (key, folder, hint) = crate::free_music::settings();
            r.header("Jamendo")
                .text("free_music.jamendo", "Jamendo client ID", "For Jamendo, the biggest catalogue: searched together with the Internet Archive once set. Get a free client ID at devportal.jamendo.com (sign up → My Apps → Add application). Only the public client ID is needed, never the secret.", &key, "Paste the client ID…")
                .header("Downloads")
                .path("free_music.folder", "Download folder", &format!("Where downloads go, one folder per artist, each song with its tags, cover and a .licence.txt. Empty: {hint}"), &folder);
        }
        _ => {}
    }
    r.0
}

/// Write one setting. `false` when the id is unknown or the value does not parse.
fn set(c: &mut Config, id: &str, v: &str) -> bool {
    let on = v == "true";
    let num = v.parse::<f64>();
    macro_rules! n {
        () => {
            match num {
                Ok(n) if n.is_finite() => n,
                _ => return false,
            }
        };
    }
    match id {
        "playback.hwdec" => c.playback.hwdec = v.into(),
        "playback.speed" => c.playback.speed = (n!() * 100.0).round() / 100.0,
        "playback.resume" => c.playback.resume = on,
        "playback.quality" => c.playback.quality = v.into(),
        "playback.repeat" => c.playback.repeat = v.into(),
        "playback.precise_seek" => c.playback.precise_seek = on,
        "video.enabled" => c.video.enabled = on,
        "video.stop_screensaver" => c.video.stop_screensaver = on,
        "video.hue" => c.video.hue = n!().round() as i64,
        "video.brightness" => c.video.brightness = n!().round() as i64,
        "video.contrast" => c.video.contrast = n!().round() as i64,
        "video.saturation" => c.video.saturation = n!().round() as i64,
        "video.gamma" => c.video.gamma = n!().round() as i64,
        "video.deinterlace" => c.video.deinterlace = v.into(),
        "video.aspect" => {
            c.video.aspect = ASPECTS.iter().find(|a| a.0 == v).map_or("no", |a| a.1).into()
        }
        "video.screenshot_dir" => c.video.screenshot_dir = v.into(),
        "video.screenshot_format" => c.video.screenshot_format = v.into(),
        "audio.enabled" => c.audio.enabled = on,
        "audio.device" => c.audio.device = v.into(),
        "audio.volume_max" => c.audio.volume_max = n!().round() as i64,
        "audio.startup_volume" => c.audio.startup_volume = n!().round() as i64,
        "audio.normalize" => c.audio.normalize = on,
        "audio.language" => c.audio.language = v.into(),
        "audio.delay" => c.audio.delay = (n!() * 100.0).round() / 100.0,
        "subtitles.enabled" => c.subtitles.enabled = on,
        "subtitles.language" => c.subtitles.language = v.into(),
        "subtitles.encoding" => c.subtitles.encoding = v.into(),
        "subtitles.auto_load" => c.subtitles.auto_load = v.into(),
        "subtitles.font" => c.subtitles.font = v.into(),
        "subtitles.scale" => c.subtitles.scale = (n!() * 10.0).round() / 10.0,
        "subtitles.bold" => c.subtitles.bold = on,
        "subtitles.color" => c.subtitles.color = v.into(),
        "subtitles.opacity" => c.subtitles.opacity = n!().round() as i64,
        "subtitles.outline" => {
            c.subtitles.outline = OUTLINES.iter().find(|o| o.0 == v).map_or(3.0, |o| o.1)
        }
        "subtitles.outline_color" => c.subtitles.outline_color = v.into(),
        "subtitles.shadow" => c.subtitles.shadow = on,
        "subtitles.position" => c.subtitles.position = n!().round() as i64,
        "controls.short_skip" => c.controls.short_skip = n!().round() as u32,
        "controls.long_skip" => c.controls.long_skip = n!().round() as u32,
        "controls.volume_step" => c.controls.volume_step = n!().round() as u32,
        "controls.wheel" => c.controls.wheel = v.into(),
        "controls.wheel_reverse" => c.controls.wheel_reverse = on,
        "interface.autohide" => c.interface.autohide_secs = n!().round() as u32,
        "network.buffering" => c.network.buffering = v.into(),
        "network.cache" => c.network.cache = v.into(),
        "network.cache_secs" => c.network.cache_secs = n!().round() as i64,
        "network.http_proxy" => c.network.http_proxy = v.into(),
        "network.tls_verify" => c.network.tls_verify = on,
        "network.user_agent" => c.network.user_agent = v.into(),
        "performance.threads" => c.performance.threads = n!().round() as i64,
        "performance.skip_loop_filter" => c.performance.skip_loop_filter = v.into(),
        "performance.framedrop" => c.performance.framedrop = on,
        "performance.video_sync" => c.performance.video_sync = v.into(),
        "general.save_log" => c.general.save_log = on,
        "general.log_file" => c.general.log_file = v.into(),
        "general.log_level" => c.general.log_level = v.into(),
        _ => return false,
    }
    true
}

// ---- The dialog --------------------------------------------------------------

struct Dialog {
    ui: slint::Weak<AppWindow>,
    mpv: MpvHandle,
    category: String,
    model: Rc<VecModel<SettingRow>>,
    /// mpv's output devices: (name, description).
    devices: Vec<(String, String)>,
}

thread_local! {
    static DIALOG: RefCell<Option<Dialog>> = const { RefCell::new(None) };
}

fn with(f: impl FnOnce(&mut Dialog, &AppWindow)) {
    DIALOG.with(|d| {
        if let Some(d) = d.borrow_mut().as_mut()
            && let Some(ui) = d.ui.upgrade()
        {
            f(d, &ui);
        }
    });
}

/// mpv's `audio-device-list`: (name, description).
fn audio_devices(mpv: &MpvHandle) -> Vec<(String, String)> {
    #[derive(Deserialize)]
    struct Dev {
        name: String,
        #[serde(default)]
        description: String,
    }
    mpv.get_property_string("audio-device-list")
        .and_then(|js| serde_json::from_str::<Vec<Dev>>(&js).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            let desc = if d.description.is_empty() || d.name == "auto" {
                d.name.clone()
            } else {
                d.description
            };
            (d.name, desc)
        })
        .collect()
}

impl Dialog {
    /// Show `self.category`, changing only the rows that differ (a slider being
    /// dragged or a box being typed in keeps its element).
    fn show(&mut self, ui: &AppWindow) {
        let rows = rows(&self.category, &get(), &self.devices);
        let m = &self.model;
        let same_shape = m.row_count() == rows.len()
            && rows
                .iter()
                .enumerate()
                .all(|(i, r)| m.row_data(i).is_some_and(|o| o.id == r.id && o.kind == r.kind));
        if same_shape {
            for (i, r) in rows.into_iter().enumerate() {
                if m.row_data(i).as_ref() != Some(&r) {
                    m.set_row_data(i, r);
                }
            }
        } else {
            m.set_vec(rows);
        }
        let st = ui.global::<SettingsState>();
        st.set_category(self.category.clone().into());
    }

    fn changed(&mut self, ui: &AppWindow, id: &str, v: &str) {
        if let Some(rest) = id.strip_prefix("free_music.") {
            crate::free_music::set_setting(rest, v);
            self.show(ui);
            return;
        }
        // The output device row shows descriptions; the setting is the name.
        let v = if id == "audio.device" {
            self.devices
                .iter()
                .find(|d| d.1 == v)
                .map_or(v.to_string(), |d| d.0.clone())
        } else {
            v.to_string()
        };
        let old = get();
        let mut ok = true;
        let c = update(|c| ok = set(c, id, &v));
        if !ok {
            return;
        }
        apply_changes(&self.mpv, &old, &c);
        apply_ui(ui, &c);
        self.show(ui);
    }

    fn action(&mut self, ui: &AppWindow, id: &str) {
        match id {
            "video.reset_picture" => {
                let old = get();
                let c = update(|c| {
                    c.video.hue = 0;
                    c.video.brightness = 0;
                    c.video.contrast = 0;
                    c.video.saturation = 0;
                    c.video.gamma = 0;
                });
                apply_changes(&self.mpv, &old, &c);
                self.show(ui);
            }
            "privacy.clear_positions" => {
                crate::history::clear_positions();
                ui.global::<SettingsState>()
                    .set_note("Resume positions cleared.".into());
            }
            "video.screenshot_dir.choose" | "general.log_file.choose" | "free_music.folder.choose" => {
                let id = id.trim_end_matches(".choose").to_string();
                let file = id == "general.log_file";
                tokio::spawn(async move {
                    let picked = tokio::task::spawn_blocking(move || {
                        let d = rfd::FileDialog::new();
                        if file {
                            d.set_file_name("cedarapple.log").save_file()
                        } else {
                            d.pick_folder()
                        }
                    })
                    .await
                    .ok()
                    .flatten();
                    let Some(p) = picked else { return };
                    let p = p.to_string_lossy().into_owned();
                    let _ = slint::invoke_from_event_loop(move || {
                        with(|d, ui| d.changed(ui, &id, &p));
                    });
                });
            }
            _ => {}
        }
    }
}

/// Load the settings, apply them, and wire the dialog. Before any file plays.
pub fn install(ui: &AppWindow, mpv: MpvHandle) {
    let c = get();
    apply_mpv(&mpv, &c);
    mpv.set_double("volume", c.audio.startup_volume.clamp(0, 100) as f64);
    apply_ui(ui, &c);

    let model = Rc::new(VecModel::<SettingRow>::default());
    let st = ui.global::<SettingsState>();
    st.set_rows(ModelRc::from(model.clone()));
    st.set_categories(ModelRc::new(VecModel::from(
        CATEGORIES.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>(),
    )));
    DIALOG.with(|d| {
        *d.borrow_mut() = Some(Dialog {
            ui: ui.as_weak(),
            mpv,
            category: CATEGORIES[0].into(),
            model,
            devices: Vec::new(),
        })
    });

    st.on_opened(|cat| {
        with(|d, ui| {
            if CATEGORIES.contains(&cat.as_str()) {
                d.category = cat.to_string();
            }
            d.devices = audio_devices(&d.mpv);
            let st = ui.global::<SettingsState>();
            st.set_note("".into());
            d.show(ui);
            st.set_open(true);
        })
    });
    st.on_pick(|cat| {
        with(|d, ui| {
            d.category = cat.to_string();
            ui.global::<SettingsState>().set_note("".into());
            d.show(ui);
        })
    });
    st.on_changed(|id, v| with(|d, ui| d.changed(ui, id.as_str(), v.as_str())));
    st.on_action(|id| with(|d, ui| d.action(ui, id.as_str())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_quality() {
        assert_eq!(mpv_color("#ff0000", 100), "#FFff0000");
        assert_eq!(mpv_color("bogus", 50), "#7FFFFFFF");
        assert_eq!(ytdl_format("720p"), "bestvideo[height<=?720]+bestaudio/best[height<=?720]/best");
        assert_eq!(ytdl_format("Audio only"), "bestaudio/best");
    }

    #[test]
    fn every_row_writes_back() {
        // Each id a row shows is one `set` knows, with its own displayed value.
        let c = Config::default();
        for cat in CATEGORIES.iter().filter(|c| **c != "Free Music") {
            for r in rows(cat, &c, &[]) {
                if matches!(r.kind.as_str(), "header" | "action") {
                    continue;
                }
                let mut c2 = c.clone();
                assert!(set(&mut c2, &r.id, &r.value), "{} = {}", r.id, r.value);
                assert_eq!(c2, c, "{} round-trips", r.id);
            }
        }
    }

    #[test]
    fn bad_numbers_are_refused() {
        let mut c = Config::default();
        assert!(!set(&mut c, "video.hue", "abc"));
        assert!(!set(&mut c, "no.such", "1"));
        assert!(set(&mut c, "subtitles.outline", "Thick") && c.subtitles.outline == 5.0);
        assert!(set(&mut c, "video.aspect", "4:3") && c.video.aspect == "4:3");
    }

    #[test]
    fn old_files_fill_in_defaults() {
        let c: Config = serde_json::from_str(r#"{"audio":{"volume_max":150}}"#).unwrap();
        assert_eq!(c.audio.volume_max, 150);
        assert_eq!(c.audio.startup_volume, 100);
        assert_eq!(c.controls, Controls::default());
    }
}
