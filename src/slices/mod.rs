//! Lua-backed Slices: manifests, an mpv script adapter and a native search grid.
mod manifest;
#[cfg(test)]
mod tests;

use crate::player::MpvHandle;
use crate::{AppWindow, SliceCard, SliceInfo, SlicesState};
use manifest::Installed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    disabled: HashSet<String>,
}

impl Preferences {
    fn path() -> Option<std::path::PathBuf> {
        Some(
            directories::ProjectDirs::from("", "", "CedarApple")?
                .data_dir()
                .join("slices.json"),
        )
    }
    fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("No application data folder")?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())
    }
}

struct Runtime {
    client: String,
    session: String,
    ready: bool,
    prepared: bool,
    started: Instant,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct Entry {
    id: String,
    title: String,
    channel: String,
    uploader: String,
    duration: Option<f64>,
    thumbnail: String,
}

impl Entry {
    fn thumbnail(&self) -> String {
        if !self.thumbnail.is_empty() {
            self.thumbnail.clone()
        } else if video_id(&self.id) {
            format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", self.id)
        } else {
            String::new()
        }
    }
    fn detail(&self) -> String {
        let channel = if self.channel.is_empty() {
            &self.uploader
        } else {
            &self.channel
        };
        let duration = self
            .duration
            .filter(|d| d.is_finite() && *d >= 0.0)
            .map(|d| {
                let s = d as u64;
                if s >= 3600 {
                    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
                } else {
                    format!("{}:{:02}", s / 60, s % 60)
                }
            })
            .unwrap_or_else(|| "Live / duration unknown".into());
        if channel.is_empty() {
            duration
        } else {
            format!("{channel} · {duration}")
        }
    }
}

#[derive(Deserialize)]
struct View {
    #[serde(default)]
    query: String,
    #[serde(default = "first_page")]
    page: i32,
    #[serde(default)]
    selected: i32,
    #[serde(default, deserialize_with = "read_entries")]
    results: Vec<Entry>,
    #[serde(default)]
    busy: bool,
    #[serde(default)]
    error: String,
    #[serde(default)]
    has_next: bool,
    #[serde(default)]
    status: String,
}
fn first_page() -> i32 {
    1
}
fn read_entries<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Entry>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    // mpv Lua can serialize an empty table as {}, rather than [].
    if value.is_null() || value.as_object().is_some_and(|m| m.is_empty()) {
        return Ok(Vec::new());
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}
fn video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

struct Host {
    ui: slint::Weak<AppWindow>,
    mpv: MpvHandle,
    installed: RefCell<Vec<Installed>>,
    preferences: RefCell<Preferences>,
    runtimes: RefCell<HashMap<String, Runtime>>,
    active: RefCell<String>,
    sequence: Cell<u64>,
    generation: Cell<u64>,
    covers_generation: Cell<u64>,
    entries: RefCell<Vec<Entry>>,
    images: RefCell<HashMap<String, slint::Image>>,
    preparation: RefCell<Option<tokio::task::JoinHandle<()>>>,
    resolution: RefCell<Option<tokio::task::JoinHandle<()>>>,
    covers: RefCell<Option<tokio::task::JoinHandle<()>>>,
    playing: RefCell<Option<(String, String)>>,
    last_error: RefCell<String>,
}

thread_local! {
    static HOST: RefCell<Option<Rc<Host>>> = const { RefCell::new(None) };
}
fn with(f: impl FnOnce(&Host, &AppWindow)) {
    HOST.with(|slot| {
        if let Some(host) = slot.borrow().as_ref()
            && let Some(ui) = host.ui.upgrade()
        {
            f(host, &ui);
        }
    });
}
fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(rows))
}
fn abort(task: &RefCell<Option<tokio::task::JoinHandle<()>>>) {
    if let Some(task) = task.borrow_mut().take() {
        task.abort();
    }
}

impl Host {
    fn send(&self, id: &str, event: &str, data: Value) {
        if let Some(runtime) = self.runtimes.borrow().get(id) {
            self.mpv.command(&[
                "script-message-to",
                &runtime.client,
                "slice-event",
                event,
                &data.to_string(),
            ]);
        }
    }
    fn send_active(&self, event: &str, data: Value) {
        let id = self.active.borrow().clone();
        self.send(&id, event, data);
    }
    fn error(&self, ui: &AppWindow, error: impl Into<String>) {
        let error = error.into();
        *self.last_error.borrow_mut() = error.clone();
        let st = ui.global::<SlicesState>();
        st.set_error(error.into());
        st.set_busy(false);
        st.set_resolving(false);
    }
    fn sync_installed(&self, ui: &AppWindow) {
        let preferences = self.preferences.borrow();
        ui.global::<SlicesState>().set_items(model(
            self.installed
                .borrow()
                .iter()
                .map(|s| SliceInfo {
                    id: s.manifest.id.as_str().into(),
                    name: s.manifest.name.as_str().into(),
                    description: s.manifest.description.as_str().into(),
                    author: s.manifest.author.as_str().into(),
                    source: s.manifest.source.as_str().into(),
                    enabled: !preferences.disabled.contains(&s.manifest.id),
                })
                .collect(),
        ));
    }
    fn cancel(&self, ui: &AppWindow) {
        self.generation.set(self.generation.get() + 1);
        abort(&self.preparation);
        abort(&self.resolution);
        abort(&self.covers);
        self.send_active("close", json!({}));
        let st = ui.global::<SlicesState>();
        st.set_busy(false);
        st.set_resolving(false);
    }
    fn manager(&self, ui: &AppWindow) {
        self.cancel(ui);
        let st = ui.global::<SlicesState>();
        st.set_open(true);
        st.set_browsing(false);
        st.set_error(self.last_error.borrow().as_str().into());
        st.set_status("Open a Slice, or switch it off to unload it.".into());
        self.sync_installed(ui);
    }
    fn open(&self, ui: &AppWindow, id: &str) {
        if self.preferences.borrow().disabled.contains(id) {
            self.manager(ui);
            self.error(ui, "Enable this Slice first.");
            return;
        }
        let Some(slice) = self
            .installed
            .borrow()
            .iter()
            .find(|s| s.manifest.id == id)
            .cloned()
        else {
            return;
        };
        self.cancel(ui);
        *self.active.borrow_mut() = id.into();
        self.entries.borrow_mut().clear();
        let st = ui.global::<SlicesState>();
        st.set_open(true);
        st.set_browsing(true);
        st.set_title(slice.manifest.name.as_str().into());
        st.set_query("".into());
        st.set_result_query("".into());
        st.set_cards(model(Vec::new()));
        st.set_error("".into());
        st.set_page(1);
        st.set_selected(-1);
        st.set_has_next(false);
        st.set_busy(true);
        st.set_status("Preparing the Slice…".into());
        self.last_error.borrow_mut().clear();
        if !self.runtimes.borrow().contains_key(id) {
            if let Err(error) = self.load(&slice) {
                self.error(ui, error);
                return;
            }
        } else {
            let ready = self.runtimes.borrow().get(id).is_some_and(|r| r.ready);
            if ready {
                self.prepare(ui);
            }
        }
    }
    fn load(&self, slice: &Installed) -> Result<(), String> {
        self.sequence.set(self.sequence.get() + 1);
        let session = format!(
            "{}_{}",
            slice.manifest.id.replace('-', "_"),
            self.sequence.get()
        );
        let client = format!("cedar_slice_{session}");
        let folder = directories::ProjectDirs::from("", "", "CedarApple")
            .ok_or("No cache folder")?
            .cache_dir()
            .join("slice-runtime");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = folder.join(format!("{client}.lua"));
        std::fs::write(&path, manifest::loader(slice, &session)).map_err(|e| e.to_string())?;
        self.runtimes.borrow_mut().insert(
            slice.manifest.id.clone(),
            Runtime {
                client,
                session,
                ready: false,
                prepared: false,
                started: Instant::now(),
            },
        );
        if !self.mpv.command(&["load-script", &path.to_string_lossy()]) {
            self.runtimes.borrow_mut().remove(&slice.manifest.id);
            return Err("mpv could not load the Lua Slice".into());
        }
        Ok(())
    }
    fn prepare(&self, ui: &AppWindow) {
        let id = self.active.borrow().clone();
        let Some((session, prepared)) = self
            .runtimes
            .borrow()
            .get(&id)
            .map(|r| (r.session.clone(), r.prepared))
        else {
            return;
        };
        let st = ui.global::<SlicesState>();
        if prepared {
            st.set_busy(false);
            st.set_status("Enter a search · Enter plays · Shift+Enter queues".into());
            return;
        }
        abort(&self.preparation);
        let generation = self.generation.get();
        st.set_busy(true);
        st.set_status("Preparing YouTube playback tools on first use…".into());
        *self.preparation.borrow_mut() = Some(tokio::spawn(async move {
            let result = crate::lumtub::tools::ready().await;
            let _ = slint::invoke_from_event_loop(move || {
                with(|h, ui| {
                    if h.generation.get() != generation
                        || !h
                            .runtimes
                            .borrow()
                            .get(&id)
                            .is_some_and(|r| r.session == session)
                    {
                        return;
                    }
                    match result {
                        Ok((yt, _)) => {
                            if let Some(runtime) = h.runtimes.borrow_mut().get_mut(&id) {
                                runtime.prepared = true;
                            }
                            h.send(
                                &id,
                                "configure",
                                json!({"yt_dlp_path": yt.to_string_lossy(), "page_size": 8}),
                            );
                            let st = ui.global::<SlicesState>();
                            st.set_busy(false);
                            st.set_status(
                                "Enter a search · Enter plays · Shift+Enter queues".into(),
                            );
                        }
                        Err(error) => h.error(ui, error),
                    }
                })
            });
        }));
    }
    fn view(&self, ui: &AppWindow, view: View) {
        let st = ui.global::<SlicesState>();
        if !st.get_browsing() || !st.get_open() {
            return;
        }
        let entries: Vec<_> = view.results.into_iter().take(24).collect();
        let changed = self
            .entries
            .borrow()
            .iter()
            .map(|e| (&e.id, &e.thumbnail))
            .ne(entries.iter().map(|e| (&e.id, &e.thumbnail)));
        *self.entries.borrow_mut() = entries;
        st.set_page(view.page.max(1));
        st.set_selected(view.selected);
        st.set_has_next(view.has_next);
        st.set_result_query(view.query.into());
        st.set_busy(view.busy);
        st.set_error(view.error.into());
        if !st.get_resolving() {
            st.set_status(if view.busy {
                "Searching YouTube…".into()
            } else if !view.status.is_empty() {
                view.status.into()
            } else {
                "Arrows select · Enter plays · Shift+Enter queues".into()
            });
        }
        self.sync_cards(ui);
        if changed {
            self.load_covers();
        }
    }
    fn sync_cards(&self, ui: &AppWindow) {
        let images = self.images.borrow();
        ui.global::<SlicesState>().set_cards(model(
            self.entries
                .borrow()
                .iter()
                .map(|entry| {
                    let cover = images.get(&entry.thumbnail()).cloned();
                    SliceCard {
                        title: entry.title.as_str().into(),
                        detail: entry.detail().into(),
                        cover: cover.clone().unwrap_or_default(),
                        has_cover: cover.is_some(),
                    }
                })
                .collect(),
        ));
    }
    fn load_covers(&self) {
        abort(&self.covers);
        self.covers_generation.set(self.covers_generation.get() + 1);
        let epoch = self.covers_generation.get();
        let urls: HashSet<_> = self
            .entries
            .borrow()
            .iter()
            .map(Entry::thumbnail)
            .filter(|u| u.starts_with("https://") && !self.images.borrow().contains_key(u))
            .collect();
        *self.covers.borrow_mut() = Some(tokio::spawn(async move {
            use futures::StreamExt;
            let client = reqwest::Client::new();
            let mut jobs = futures::stream::iter(urls)
                .map(|url| {
                    let client = client.clone();
                    async move {
                        let response = client
                            .get(&url)
                            .timeout(Duration::from_secs(10))
                            .send()
                            .await
                            .ok()?
                            .error_for_status()
                            .ok()?;
                        if response.content_length().is_some_and(|n| n > 2_000_000) {
                            return None;
                        }
                        let bytes = response.bytes().await.ok()?;
                        if bytes.len() > 2_000_000 {
                            return None;
                        }
                        let pixels = tokio::task::spawn_blocking(move || {
                            let img = image::load_from_memory(&bytes)
                                .ok()?
                                .thumbnail(480, 270)
                                .to_rgba8();
                            Some((img.width(), img.height(), img.into_raw()))
                        })
                        .await
                        .ok()??;
                        Some((url, pixels))
                    }
                })
                .buffer_unordered(6);
            while let Some(result) = jobs.next().await {
                let Some((url, (width, height, bytes))) = result else {
                    continue;
                };
                let _ = slint::invoke_from_event_loop(move || {
                    with(|h, ui| {
                        if h.covers_generation.get() != epoch {
                            return;
                        }
                        let pixels =
                            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                                &bytes, width, height,
                            );
                        let mut cache = h.images.borrow_mut();
                        if cache.len() >= 128 {
                            cache.clear();
                        }
                        cache.insert(url, slint::Image::from_rgba8(pixels));
                        drop(cache);
                        h.sync_cards(ui);
                    })
                });
            }
        }));
    }
    fn play(&self, ui: &AppWindow, data: Value) {
        let id = data["id"].as_str().unwrap_or("").to_owned();
        if !video_id(&id) {
            self.error(ui, "This result is not a playable YouTube video.");
            return;
        }
        if ui.global::<SlicesState>().get_resolving() {
            return;
        }
        let append = data["append"].as_bool().unwrap_or(false);
        let generation = self.generation.get();
        let st = ui.global::<SlicesState>();
        st.set_resolving(true);
        st.set_error("".into());
        st.set_status(if append {
            "Preparing video for the queue…".into()
        } else {
            "Opening video…".into()
        });
        *self.resolution.borrow_mut() = Some(tokio::spawn(async move {
            let result = crate::lumtub::tools::resolve(&id).await;
            let _ = slint::invoke_from_event_loop(move || {
                with(|h, ui| {
                    if h.generation.get() != generation {
                        return;
                    }
                    let st = ui.global::<SlicesState>();
                    st.set_resolving(false);
                    match result {
                        Ok(video) => {
                            *h.playing.borrow_mut() = Some((id.clone(), video.title.clone()));
                            crate::playback::play_youtube(
                                &video.url,
                                &id,
                                &video.title,
                                &video.user_agent,
                                append,
                            );
                            if append {
                                st.set_status(format!("Queued: {}", video.title).into());
                            } else {
                                h.send_active("close", json!({}));
                                st.set_open(false);
                            }
                        }
                        Err(error) => h.error(ui, error),
                    }
                })
            });
        }));
    }
    fn enable(&self, ui: &AppWindow, id: &str, enabled: bool) {
        if !self.installed.borrow().iter().any(|s| s.manifest.id == id) {
            return;
        }
        if enabled {
            self.preferences.borrow_mut().disabled.remove(id);
        } else {
            self.preferences.borrow_mut().disabled.insert(id.into());
            if self.active.borrow().as_str() == id {
                self.cancel(ui);
                self.active.borrow_mut().clear();
            }
            self.send(id, "shutdown", json!({}));
            self.runtimes.borrow_mut().remove(id);
        }
        if let Err(error) = self.preferences.borrow().save() {
            self.error(ui, format!("Could not save Slices: {error}"));
        }
        self.sync_installed(ui);
    }
}

pub fn install(ui: &AppWindow, mpv: MpvHandle) {
    let (installed, errors) = manifest::discover();
    let host = Rc::new(Host {
        ui: ui.as_weak(),
        mpv,
        installed: RefCell::new(installed),
        preferences: RefCell::new(Preferences::load()),
        runtimes: RefCell::new(HashMap::new()),
        active: RefCell::new(String::new()),
        sequence: Cell::new(0),
        generation: Cell::new(0),
        covers_generation: Cell::new(0),
        entries: RefCell::new(Vec::new()),
        images: RefCell::new(HashMap::new()),
        preparation: RefCell::new(None),
        resolution: RefCell::new(None),
        covers: RefCell::new(None),
        playing: RefCell::new(None),
        last_error: RefCell::new(errors.join("\n")),
    });
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
    let st = ui.global::<SlicesState>();
    st.on_opened(|| with(|h, ui| h.manager(ui)));
    st.on_closed(|| {
        with(|h, ui| {
            h.cancel(ui);
            ui.global::<SlicesState>().set_open(false);
        })
    });
    st.on_manage(|| with(|h, ui| h.manager(ui)));
    st.on_launch(|id| with(|h, ui| h.open(ui, &id)));
    st.on_enabled(|id, enabled| with(|h, ui| h.enable(ui, &id, enabled)));
    st.on_search(|| {
        with(|h, ui| {
            let st = ui.global::<SlicesState>();
            if st.get_resolving() || st.get_busy() {
                return;
            }
            let query = st.get_query().trim().to_string();
            if !query.is_empty() {
                h.send_active("search", json!({"query": query}));
            }
        })
    });
    st.on_key(|key, append| {
        with(|h, ui| {
            if !ui.global::<SlicesState>().get_resolving() {
                h.send_active("key", json!({"key": key.as_str(), "append": append, "columns": ui.global::<SlicesState>().get_columns()}));
            }
        })
    });
    st.on_select(|index| with(|h, _| h.send_active("select", json!({"index": index}))));
    st.on_activate(|index, append| {
        with(|h, ui| {
            if !ui.global::<SlicesState>().get_resolving() {
                h.send_active("activate", json!({"index": index, "append": append}));
            }
        })
    });
    st.on_change_page(|delta| {
        with(|h, ui| {
            if !ui.global::<SlicesState>().get_resolving() {
                h.send_active("page", json!({"delta": delta}));
            }
        })
    });
    st.on_cancel(|| {
        with(|h, ui| {
            h.cancel(ui);
            ui.global::<SlicesState>()
                .set_status("Cancelled. Open the Slice again to start a new search.".into());
        })
    });
    st.on_retry(|| {
        with(|h, ui| {
            let id = h.active.borrow().clone();
            h.open(ui, &id);
        })
    });
    st.on_folder(|| {
        if let Some(path) = manifest::user_dir()
            && std::fs::create_dir_all(&path).is_ok()
        {
            crate::playback::open_folder(&path.to_string_lossy());
        }
    });
    st.on_source(|id| {
        with(|h, _| {
            if let Some(slice) = h
                .installed
                .borrow()
                .iter()
                .find(|s| s.manifest.id == id.as_str())
                && url::Url::parse(&slice.manifest.source).is_ok_and(|u| u.scheme() == "https")
            {
                crate::playback::open_folder(&slice.manifest.source);
            }
        })
    });
    st.on_refresh(|| {
        with(|h, ui| {
            h.cancel(ui);
            let ids: Vec<_> = h.runtimes.borrow().keys().cloned().collect();
            for id in ids {
                h.send(&id, "shutdown", json!({}));
            }
            h.runtimes.borrow_mut().clear();
            let (installed, errors) = manifest::discover();
            *h.installed.borrow_mut() = installed;
            *h.last_error.borrow_mut() = errors.join("\n");
            h.manager(ui);
        })
    });
}

pub fn message(session: &str, event: &str, body: &str) {
    if body.len() > 2_000_000 {
        return;
    }
    with(|h, ui| {
        let id = h
            .runtimes
            .borrow()
            .iter()
            .find(|(_, r)| r.session == session)
            .map(|(id, _)| id.clone());
        let Some(id) = id else { return };
        if event == "ready" {
            if let Some(runtime) = h.runtimes.borrow_mut().get_mut(&id) {
                runtime.ready = true;
            }
            if *h.active.borrow() == id && ui.global::<SlicesState>().get_open() {
                h.prepare(ui);
            }
            return;
        }
        if *h.active.borrow() != id {
            return;
        }
        match event {
            "view" => match serde_json::from_str(body) {
                Ok(view) => h.view(ui, view),
                Err(e) => h.error(ui, format!("Invalid Slice view: {e}")),
            },
            "play" => match serde_json::from_str(body) {
                Ok(value) => h.play(ui, value),
                Err(e) => h.error(ui, e.to_string()),
            },
            "error" => {
                let value: Value = serde_json::from_str(body).unwrap_or_default();
                h.error(ui, value["message"].as_str().unwrap_or("Lua Slice failed"));
            }
            _ => {}
        }
    });
}

pub fn tick(_ui: &AppWindow) {
    with(|h, ui| {
        let expired: Vec<_> = h
            .runtimes
            .borrow()
            .iter()
            .filter(|(_, r)| !r.ready && r.started.elapsed() > Duration::from_secs(10))
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            h.send(&id, "shutdown", json!({}));
            h.runtimes.borrow_mut().remove(&id);
            if h.active.borrow().as_str() == id {
                h.error(
                    ui,
                    "The Lua Slice did not start. Check that this libmpv build supports Lua.",
                );
            }
        }
    });
}

pub fn shutdown() {
    with(|h, ui| {
        h.cancel(ui);
        let ids: Vec<_> = h.runtimes.borrow().keys().cloned().collect();
        for id in ids {
            h.send(&id, "shutdown", json!({}));
        }
    });
    HOST.with(|slot| {
        slot.borrow_mut().take();
    });
}
