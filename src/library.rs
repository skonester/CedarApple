//! Provider browsing shared by local folders, Seanime and Parts scripts.
//! Network requests, directory reads and VM calls run away from the UI thread.

use crate::api::{DynProvider, MediaItem, MediaItemType, PlaybackInfo};
use crate::backend::seanime::{SeanimeClient, SeanimeWsEvent, SeanimeWsListener};
use crate::backend::{LocalProvider, SeanimeProvider};
use crate::extensions::{ExtensionBridge, ExtensionWatcher, PartsExtensionBridge};
use crate::{AppWindow, LibraryRow, LibraryState};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
enum Page {
    #[default]
    Root,
    Folder(String),
    Search(String),
    NextUp,
}

struct Library {
    ui: slint::Weak<AppWindow>,
    state: Arc<Mutex<crate::app_state::AppState>>,
    source: RefCell<String>,
    nav: RefCell<Vec<(Page, String)>>,
    items: RefCell<Vec<MediaItem>>,
    generation: Cell<u64>,
    config_generation: Cell<u64>,
    bridge: RefCell<Option<Arc<PartsExtensionBridge>>>,
    watcher: RefCell<Option<ExtensionWatcher>>,
    ws: RefCell<Option<SeanimeWsListener>>,
    plugin_status: RefCell<String>,
    plugin_state: Cell<i64>,
    configuration_error: RefCell<String>,
}

thread_local! {
    static LIBRARY: RefCell<Option<Rc<Library>>> = const { RefCell::new(None) };
}

fn with(f: impl FnOnce(&Library, &AppWindow)) {
    LIBRARY.with(|slot| {
        if let Some(l) = slot.borrow().as_ref()
            && let Some(ui) = l.ui.upgrade()
        {
            f(l, &ui);
        }
    });
}

fn source_id(name: &str) -> &'static str {
    match name {
        "Seanime" => "seanime",
        "Parts plugins" => "parts",
        _ => "local",
    }
}

impl Library {
    fn invalidate(&self) -> u64 {
        let n = self.generation.get().wrapping_add(1);
        self.generation.set(n);
        n
    }

    fn provider(&self) -> Option<DynProvider> {
        self.state
            .lock()
            .unwrap()
            .active_providers
            .get(source_id(&self.source.borrow()))
            .cloned()
    }

    fn load(&self, ui: &AppWindow) {
        let generation = self.invalidate();
        let st = ui.global::<LibraryState>();
        st.set_rows(ModelRc::default());
        st.set_error(self.configuration_error.borrow().clone().into());
        self.items.borrow_mut().clear();
        let page = self
            .nav
            .borrow()
            .last()
            .cloned()
            .unwrap_or((Page::Root, "Home".into()));
        st.set_location(page.1.into());
        st.set_can_back(self.nav.borrow().len() > 1);
        st.set_source(self.source.borrow().clone().into());
        st.set_status(if source_id(&self.source.borrow()) == "parts" {
            self.plugin_status.borrow().clone().into()
        } else {
            "".into()
        });
        let Some(provider) = self.provider() else {
            st.set_busy(false);
            st.set_error(
                "This source is disabled. Enable it in Configure and apply the settings.".into(),
            );
            return;
        };
        st.set_busy(true);
        let preserve_order = matches!(page.0, Page::NextUp);
        tokio::task::spawn_blocking(move || {
            let result = tokio::runtime::Handle::current().block_on(async {
                match page.0 {
                    Page::Root => provider.get_root().await,
                    Page::Folder(id) => provider.get_children(&id).await,
                    Page::Search(q) => provider.search(&q).await,
                    Page::NextUp => provider.get_next_up().await,
                }
            });
            let _ = slint::invoke_from_event_loop(move || {
                with(|l, ui| {
                    if l.generation.get() != generation {
                        return;
                    }
                    let st = ui.global::<LibraryState>();
                    st.set_busy(false);
                    match result {
                        Ok(mut items) => {
                            let mut seen = HashSet::new();
                            items.retain(|i| seen.insert(i.id.clone()));
                            if !preserve_order {
                                items.sort_by(|a, b| {
                                    (a.item_type != MediaItemType::Folder)
                                        .cmp(&(b.item_type != MediaItemType::Folder))
                                        .then_with(|| {
                                            crate::torrent_ui::natural_cmp(&a.name, &b.name)
                                        })
                                });
                            }
                            let rows = items
                                .iter()
                                .map(|i| LibraryRow {
                                    title: i.name.clone().into(),
                                    folder: i.item_type == MediaItemType::Folder,
                                    detail: if i.item_type == MediaItemType::Folder {
                                        "Folder".to_string()
                                    } else {
                                        i.series_name.clone().unwrap_or_else(|| "Play".into())
                                    }
                                    .into(),
                                })
                                .collect::<Vec<_>>();
                            st.set_rows(ModelRc::new(VecModel::from(rows)));
                            *l.items.borrow_mut() = items;
                        }
                        Err(e) => st.set_error(e.to_string().into()),
                    }
                })
            });
        });
    }

    fn pick(&self, ui: &AppWindow, index: i32) {
        if ui.global::<LibraryState>().get_busy() {
            return;
        }
        let Some(item) = usize::try_from(index)
            .ok()
            .and_then(|i| self.items.borrow().get(i).cloned())
        else {
            return;
        };
        if item.item_type == MediaItemType::Folder {
            self.nav
                .borrow_mut()
                .push((Page::Folder(item.id), item.name));
            self.load(ui);
            return;
        }
        let Some(provider) = self.provider() else {
            return;
        };
        let provider_id = source_id(&self.source.borrow()).to_string();
        let generation = self.invalidate();
        ui.global::<LibraryState>().set_busy(true);
        ui.global::<LibraryState>().set_error("".into());
        tokio::task::spawn_blocking(move || {
            let url = provider.get_stream_url(&item.id);
            let resume = tokio::runtime::Handle::current()
                .block_on(provider.get_resume_position(&item.id))
                .ok()
                .flatten()
                .or(item.resume_position_secs);
            let _ = slint::invoke_from_event_loop(move || {
                with(|l, ui| {
                    if l.generation.get() != generation {
                        return;
                    }
                    let st = ui.global::<LibraryState>();
                    st.set_busy(false);
                    if url.trim().is_empty() || (provider_id == "seanime" && url == item.id) {
                        st.set_error("No playable file is available for this item.".into());
                        return;
                    }
                    crate::playback::play_provider(provider_id, provider, item, url, resume);
                    st.set_open(false);
                })
            });
        });
    }
}

fn normalize_server(address: &str) -> Result<String, String> {
    let url = url::Url::parse(address.trim())
        .map_err(|_| "Enter a complete Seanime URL, such as http://localhost:3211.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Seanime needs an HTTP or HTTPS server address without a query or fragment.".into(),
        );
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// Rebuild the available sources on startup or the Settings Apply action.
pub fn configure(ui: &AppWindow) -> Result<(), String> {
    let config = crate::settings::get().library;
    let mut configuration_error = String::new();
    let server = if config.seanime_enabled {
        match normalize_server(&config.seanime_url) {
            Ok(server) => Some(server),
            Err(e) => {
                configuration_error = e;
                None
            }
        }
    } else {
        None
    };
    with(|l, _| {
        *l.configuration_error.borrow_mut() = configuration_error.clone();
        l.invalidate();
        let n = l.config_generation.get().wrapping_add(1);
        l.config_generation.set(n);
        crate::playback::stop_provider();
        l.plugin_state.set(3);
        if let Some(watcher) = l.watcher.borrow_mut().take() {
            tokio::task::spawn_blocking(move || drop(watcher));
        }
        l.bridge.borrow_mut().take();
        if let Some(ws) = l.ws.borrow_mut().take() {
            ws.stop();
        }
        let mut providers = l.state.lock().unwrap();
        providers.active_providers.clear();
        providers.active_providers.insert(
            "local".into(),
            Arc::new(LocalProvider::with_dir(&config.folder)),
        );
        let mut sources = vec![slint::SharedString::from("Local folders")];
        if let Some(server) = server {
            let provider = Arc::new(SeanimeProvider::new(Arc::new(SeanimeClient::new(&server))));
            let ws = SeanimeWsListener::new();
            provider.set_ws_sender(ws.sender());
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            ws.start(server, tx);
            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    if matches!(event, SeanimeWsEvent::LibraryUpdated) {
                        let _ = slint::invoke_from_event_loop(move || {
                            with(|l, ui| {
                                if l.config_generation.get() == n
                                    && ui.global::<LibraryState>().get_open()
                                    && source_id(&l.source.borrow()) == "seanime"
                                    && !ui.global::<LibraryState>().get_busy()
                                {
                                    l.load(ui);
                                }
                            })
                        });
                    }
                }
            });
            providers
                .active_providers
                .insert("seanime".into(), provider);
            *l.ws.borrow_mut() = Some(ws);
            sources.push("Seanime".into());
        }
        if config.parts_enabled {
            let bridge = Arc::new(PartsExtensionBridge::new());
            providers
                .active_providers
                .insert("parts".into(), bridge.clone());
            *l.bridge.borrow_mut() = Some(bridge.clone());
            *l.plugin_status.borrow_mut() = "Loading plugins…".into();
            let reload_lock = Arc::new(Mutex::new(()));
            let initial_lock = reload_lock.clone();
            let initial = bridge.clone();
            tokio::task::spawn_blocking(move || {
                let _lock = initial_lock.lock().unwrap();
                let result = initial.load_scripts().map_err(|e| e.to_string());
                plugin_result(n, &initial, result);
            });
            if let Some(dir) = crate::extensions::bridge::get_parts_dir() {
                let mut watcher = ExtensionWatcher::new();
                watcher.start(vec![dir], move |path| {
                    let _lock = reload_lock.lock().unwrap();
                    let path = PathBuf::from(path);
                    let result = if path.exists() {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("script");
                        bridge
                            .load_single_script(name, path.clone())
                            .map_err(|e| e.to_string())
                    } else {
                        bridge.unload_script(&path);
                        Ok(())
                    };
                    plugin_result(n, &bridge, result);
                });
                *l.watcher.borrow_mut() = Some(watcher);
            }
            sources.push("Parts plugins".into());
        }
        drop(providers);
        if !sources
            .iter()
            .any(|s| s.as_str() == l.source.borrow().as_str())
        {
            *l.source.borrow_mut() = "Local folders".into();
        }
        ui.global::<LibraryState>()
            .set_sources(ModelRc::new(VecModel::from(sources)));
        *l.nav.borrow_mut() = vec![(Page::Root, "Home".into())];
        if ui.global::<LibraryState>().get_open() {
            l.load(ui);
        }
    });
    if configuration_error.is_empty() {
        Ok(())
    } else {
        Err(configuration_error)
    }
}

fn plugin_result(generation: u64, bridge: &PartsExtensionBridge, result: Result<(), String>) {
    let names = bridge.script_names();
    let status = match result {
        Ok(()) if names.is_empty() => {
            "No .pts scripts installed. Open the plugin folder in Configure.".into()
        }
        Ok(()) => format!("Loaded plugins: {}", names.join(", ")),
        Err(e) => format!("Plugin load failed: {e}"),
    };
    let _ = slint::invoke_from_event_loop(move || {
        with(|l, ui| {
            if l.config_generation.get() != generation {
                return;
            }
            *l.plugin_status.borrow_mut() = status.clone();
            if source_id(&l.source.borrow()) == "parts" {
                ui.global::<LibraryState>().set_status(status.into());
                if ui.global::<LibraryState>().get_open() {
                    l.load(ui);
                }
            }
        })
    });
}

pub fn playback_update(info: PlaybackInfo, buffering: bool) {
    with(|l, _| {
        if let Some(bridge) = l.bridge.borrow().as_ref() {
            bridge.notify_time_update(info.position_secs * 1000);
            let state = if buffering {
                2
            } else if info.is_paused {
                0
            } else {
                1
            };
            if l.plugin_state.replace(state) != state {
                bridge.notify_state_changed(state);
            }
            bridge.notify_playback_update(info);
        }
    });
}

pub fn playback_stopped() {
    with(|l, _| {
        if let Some(bridge) = l.bridge.borrow().as_ref() {
            if l.plugin_state.replace(3) != 3 {
                bridge.notify_state_changed(3);
                bridge.notify_playback_stop();
            }
        }
    });
}

pub fn install(ui: &AppWindow, state: Arc<Mutex<crate::app_state::AppState>>) {
    crate::extensions::bridge::set_global_controller(Arc::new(crate::playback::PluginController));
    LIBRARY.with(|slot| {
        *slot.borrow_mut() = Some(Rc::new(Library {
            ui: ui.as_weak(),
            state,
            source: RefCell::new("Local folders".into()),
            nav: RefCell::new(vec![(Page::Root, "Home".into())]),
            items: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            config_generation: Cell::new(0),
            bridge: RefCell::new(None),
            watcher: RefCell::new(None),
            ws: RefCell::new(None),
            plugin_status: RefCell::new(String::new()),
            plugin_state: Cell::new(3),
            configuration_error: RefCell::new(String::new()),
        }))
    });
    let st = ui.global::<LibraryState>();
    st.on_opened(|| {
        with(|l, ui| {
            ui.global::<LibraryState>().set_open(true);
            l.load(ui);
        })
    });
    st.on_closed(|| {
        with(|l, ui| {
            l.invalidate();
            ui.global::<LibraryState>().set_busy(false);
            ui.global::<LibraryState>().set_open(false);
        })
    });
    st.on_source_picked(|s| {
        with(|l, ui| {
            *l.source.borrow_mut() = s.to_string();
            *l.nav.borrow_mut() = vec![(Page::Root, "Home".into())];
            ui.global::<LibraryState>().set_query("".into());
            l.load(ui);
        })
    });
    st.on_item_picked(|i| with(|l, ui| l.pick(ui, i)));
    st.on_refresh(|| with(|l, ui| l.load(ui)));
    st.on_home(|| {
        with(|l, ui| {
            *l.nav.borrow_mut() = vec![(Page::Root, "Home".into())];
            ui.global::<LibraryState>().set_query("".into());
            l.load(ui);
        })
    });
    st.on_back(|| {
        with(|l, ui| {
            if l.nav.borrow().len() > 1 {
                l.nav.borrow_mut().pop();
            }
            l.load(ui);
        })
    });
    st.on_search(|q| {
        with(|l, ui| {
            if q.trim().is_empty() {
                return;
            }
            l.nav.borrow_mut().push((
                Page::Search(q.trim().into()),
                format!("Search: {}", q.trim()),
            ));
            l.load(ui);
        })
    });
    st.on_next_up(|| {
        with(|l, ui| {
            l.nav
                .borrow_mut()
                .push((Page::NextUp, "Continue watching".into()));
            l.load(ui);
        })
    });
    if let Err(e) = configure(ui) {
        st.set_error(e.into());
    }
}

pub fn shutdown() {
    LIBRARY.with(|slot| {
        if let Some(l) = slot.borrow_mut().take() {
            if let Some(ws) = l.ws.borrow_mut().take() {
                ws.stop();
            }
            l.watcher.borrow_mut().take();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MediaProvider;
    #[test]
    fn server_addresses_are_validated() {
        assert_eq!(
            normalize_server(" http://localhost:3211/ ").unwrap(),
            "http://localhost:3211"
        );
        assert!(normalize_server("localhost:3211").is_err());
        assert!(normalize_server("file:///tmp/server").is_err());
        assert!(normalize_server("https://example.com/?token=secret").is_err());
    }

    #[tokio::test]
    async fn seanime_browses_resolves_and_resumes_with_http_errors_visible() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..n]);
                }
                let request = String::from_utf8_lossy(&request);
                let (status, body) = if request.starts_with("GET /denied/") {
                    ("401 Unauthorized", r#"{"error":"Sign in required"}"#)
                } else if request.contains("/library/collection ") {
                    (
                        "200 OK",
                        r#"{"data":{"lists":[{"entries":[{"mediaId":42,"media":{"id":42,"title":{"english":"Test series"}}}]}]}}"#,
                    )
                } else if request.contains("/anime-entry/42 ") {
                    (
                        "200 OK",
                        r#"{"data":{"mediaId":42,"episodes":[{"episodeNumber":1,"displayTitle":"Episode 1","localFile":{"path":"/__cedarapple_fixture__/episode1.mkv"}}]}}"#,
                    )
                } else if request.contains("/continuity/item/42 ") {
                    (
                        "200 OK",
                        r#"{"data":{"found":true,"item":{"mediaId":42,"episodeNumber":1,"currentTime":123,"duration":1000}}}"#,
                    )
                } else {
                    panic!("Unexpected request: {request}");
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let provider = SeanimeProvider::new(Arc::new(SeanimeClient::new(&address)));
        let root = provider.get_root().await.unwrap();
        assert_eq!(root[0].name, "Test series");
        assert_eq!(root[0].item_type, MediaItemType::Folder);
        let children = provider.get_children(&root[0].id).await.unwrap();
        assert_eq!(children[0].item_type, MediaItemType::Playable);
        assert!(
            provider
                .get_stream_url(&children[0].id)
                .starts_with(&format!("{address}/api/v1/mediastream/file?path="))
        );
        assert_eq!(
            provider.get_resume_position(&children[0].id).await.unwrap(),
            Some(123)
        );
        let denied = SeanimeClient::new(format!("{address}/denied"));
        assert!(
            denied
                .get_library_collection()
                .await
                .unwrap_err()
                .to_string()
                .contains("401")
        );
        server.await.unwrap();
    }
}
