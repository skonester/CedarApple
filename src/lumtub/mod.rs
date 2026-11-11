//! Native Lumtub browser. Its Rust listing parser is copied; the Flutter
//! controller is adapted to Slint and playback goes through CedarApple's mpv.
mod catalog;
mod innertube;
mod listing;
mod store;
mod tools;

use crate::{AppWindow, LumtubRow, LumtubState};
use catalog::{Item, Page, PageOrVideo};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;
use store::Store;

struct Browser {
    ui: slint::Weak<AppWindow>,
    nav: RefCell<Vec<(Page, String)>>,
    items: RefCell<Vec<Item>>,
    store: RefCell<Store>,
    generation: Cell<u64>,
    continuation: RefCell<Option<String>>,
    continuation_page: RefCell<Option<Page>>,
    tokens: RefCell<HashSet<String>>,
    shelves: RefCell<VecDeque<Page>>,
    request: RefCell<Option<tokio::task::JoinHandle<()>>>,
    resolution: RefCell<Option<tokio::task::JoinHandle<()>>>,
    covers: RefCell<Option<tokio::task::JoinHandle<()>>>,
    cover_generation: Cell<u64>,
    pending: RefCell<Option<Item>>,
    playing: RefCell<Option<Item>>,
}

thread_local! {
    static BROWSER: RefCell<Option<Rc<Browser>>> = const { RefCell::new(None) };
}

fn with(f: impl FnOnce(&Browser, &AppWindow)) {
    BROWSER.with(|slot| {
        if let Some(b) = slot.borrow().as_ref()
            && let Some(ui) = b.ui.upgrade()
        {
            f(b, &ui);
        }
    });
}

fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(rows))
}

impl Browser {
    fn abort(slot: &RefCell<Option<tokio::task::JoinHandle<()>>>) {
        if let Some(task) = slot.borrow_mut().take() {
            task.abort();
        }
    }
    fn cancel(&self, ui: &AppWindow) {
        self.generation.set(self.generation.get() + 1);
        Self::abort(&self.request);
        Self::abort(&self.resolution);
        Self::abort(&self.covers);
        self.pending.borrow_mut().take();
        let st = ui.global::<LumtubState>();
        st.set_busy(false);
        st.set_resolving(false);
        st.set_status("Cancelled. Search or choose a video to continue.".into());
    }
    fn current(&self) -> (Page, String) {
        self.nav.borrow().last().cloned().unwrap_or((
            Page::Search("science and technology".into(), "Videos".into()),
            "Explore".into(),
        ))
    }
    fn show(&self, ui: &AppWindow, page: Page, title: String, push: bool) {
        self.cancel(ui);
        if push {
            self.nav.borrow_mut().push((page.clone(), title.clone()));
            if self.nav.borrow().len() > 40 {
                self.nav.borrow_mut().remove(0);
            }
        }
        self.continuation.borrow_mut().take();
        self.continuation_page.borrow_mut().take();
        self.tokens.borrow_mut().clear();
        self.shelves.borrow_mut().clear();
        let st = ui.global::<LumtubState>();
        st.set_can_back(self.nav.borrow().len() > 1);
        st.set_location(title.into());
        st.set_error("".into());
        st.set_has_more(false);
        st.set_is_channel(matches!(page, Page::Channel(..)));
        let section = match &page {
            Page::Saved => "Watch later",
            Page::History => "History",
            Page::Channels => "Channels",
            _ => "Explore",
        };
        st.set_section(section.into());
        if let Page::Channel(id, tab) = &page {
            st.set_channel_tab(tab.as_str().into());
            st.set_followed(self.store.borrow().channels.iter().any(|i| &i.id == id));
        }
        *self.items.borrow_mut() = match &page {
            Page::Saved => self.store.borrow().saved.clone(),
            Page::History => self.store.borrow().history.clone(),
            Page::Channels => self.store.borrow().channels.clone(),
            _ => Vec::new(),
        };
        st.set_generation(st.get_generation().wrapping_add(1));
        self.sync(ui);
        if page.request().is_some() {
            self.fetch(ui, false);
        } else {
            self.load_covers(ui);
        }
    }
    fn sync(&self, ui: &AppWindow) {
        let store = self.store.borrow();
        let rows = self
            .items
            .borrow()
            .iter()
            .map(|i| LumtubRow {
                title: i.title.as_str().into(),
                detail: i.detail().into(),
                description: i.description.lines().next().unwrap_or("").into(),
                kind: i.kind.as_str().into(),
                cover: Default::default(),
                has_cover: false,
                has_channel: i.kind == "video" && !i.channel_id.is_empty(),
                saved: if i.kind == "channel" {
                    store.channels.iter().any(|c| c.id == i.id)
                } else {
                    store.saved.iter().any(|s| s.id == i.id)
                },
            })
            .collect::<Vec<_>>();
        let st = ui.global::<LumtubState>();
        st.set_status(
            format!(
                "{} results · Saved on this computer · No account needed",
                rows.len()
            )
            .into(),
        );
        st.set_rows(model(rows));
    }
    fn fetch(&self, ui: &AppWindow, more: bool) {
        let st = ui.global::<LumtubState>();
        if st.get_busy() {
            return;
        }
        let mut page = if more {
            self.continuation_page
                .borrow()
                .clone()
                .unwrap_or_else(|| self.current().0)
        } else {
            self.current().0
        };
        let continuation = if more {
            self.continuation.borrow().clone()
        } else {
            None
        };
        let shelf = more && continuation.is_none();
        if shelf {
            let Some(next) = self.shelves.borrow().front().cloned() else {
                return;
            };
            page = next;
        }
        let generation = self.generation.get();
        st.set_busy(true);
        st.set_error("".into());
        st.set_status("Loading from YouTube…".into());
        Self::abort(&self.request);
        *self.request.borrow_mut() = Some(tokio::spawn(async move {
            let result = catalog::fetch(&page, continuation.as_deref()).await;
            let requested_page = page;
            let _ = slint::invoke_from_event_loop(move || {
                with(|b, ui| {
                    if b.generation.get() != generation {
                        return;
                    }
                    let st = ui.global::<LumtubState>();
                    st.set_busy(false);
                    match result {
                        Ok(page) => {
                            *b.continuation_page.borrow_mut() = Some(requested_page);
                            if shelf {
                                b.shelves.borrow_mut().pop_front();
                            }
                            for entry in page.shelves {
                                if let (Some(id), Some(params)) =
                                    (entry["browseId"].as_str(), entry["params"].as_str())
                                {
                                    if b.tokens.borrow_mut().insert(format!("shelf:{id}:{params}"))
                                    {
                                        b.shelves
                                            .borrow_mut()
                                            .push_back(Page::Shelf(id.into(), params.into()));
                                    }
                                }
                            }
                            let mut seen: HashSet<String> =
                                b.items.borrow().iter().map(|i| i.id.clone()).collect();
                            b.items.borrow_mut().extend(
                                page.entries
                                    .iter()
                                    .filter_map(Item::from_entry)
                                    .filter(|i| seen.insert(i.id.clone())),
                            );
                            let next = page
                                .continuation
                                .filter(|t| b.tokens.borrow_mut().insert(t.clone()));
                            st.set_has_more(next.is_some() || !b.shelves.borrow().is_empty());
                            *b.continuation.borrow_mut() = next;
                            b.sync(ui);
                            b.load_covers(ui);
                        }
                        Err(error) => {
                            st.set_error(error.into());
                            st.set_status("Check your connection, then retry.".into());
                        }
                    }
                })
            });
        }));
    }
    fn load_covers(&self, _ui: &AppWindow) {
        Self::abort(&self.covers);
        let generation = self.generation.get();
        self.cover_generation.set(self.cover_generation.get() + 1);
        let cover_generation = self.cover_generation.get();
        let requests: Vec<_> = self
            .items
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, i)| !i.thumbnail.is_empty())
            .map(|(n, i)| (n, i.thumbnail.clone()))
            .collect();
        // Thumbnails are bounded and decoded away from the UI thread.
        *self.covers.borrow_mut() = Some(tokio::spawn(async move {
            use futures::StreamExt;
            let client = reqwest::Client::new();
            let mut stream = futures::stream::iter(requests)
                .map(|(index, url)| {
                    let client = client.clone();
                    async move {
                        let response = client
                            .get(url)
                            .timeout(std::time::Duration::from_secs(10))
                            .send()
                            .await
                            .ok()?
                            .error_for_status()
                            .ok()?;
                        if response.content_length().is_some_and(|s| s > 2_000_000) {
                            return None;
                        }
                        let bytes = response.bytes().await.ok()?;
                        if bytes.len() > 2_000_000 {
                            return None;
                        }
                        let raw = tokio::task::spawn_blocking(move || {
                            let img = image::load_from_memory(&bytes)
                                .ok()?
                                .thumbnail(328, 188)
                                .to_rgba8();
                            Some((img.width(), img.height(), img.into_raw()))
                        })
                        .await
                        .ok()??;
                        Some((index, raw))
                    }
                })
                .buffer_unordered(6);
            while let Some(result) = stream.next().await {
                let Some((index, (width, height, bytes))) = result else {
                    continue;
                };
                let _ = slint::invoke_from_event_loop(move || {
                    with(|b, ui| {
                        if b.generation.get() != generation
                            || b.cover_generation.get() != cover_generation
                        {
                            return;
                        }
                        let rows = ui.global::<LumtubState>().get_rows();
                        if let Some(mut row) = rows.row_data(index) {
                            let pixels =
                                slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                                    &bytes, width, height,
                                );
                            row.cover = slint::Image::from_rgba8(pixels);
                            row.has_cover = true;
                            rows.set_row_data(index, row);
                        }
                    })
                });
            }
        }));
    }
    fn search(&self, ui: &AppWindow) {
        let st = ui.global::<LumtubState>();
        let query = st.get_query().trim().to_owned();
        if query.is_empty() {
            return;
        }
        match catalog::address(&query) {
            Some(PageOrVideo::Video(id)) => self.play(
                ui,
                Item {
                    title: "YouTube video".into(),
                    id,
                    kind: "video".into(),
                    ..Default::default()
                },
            ),
            Some(PageOrVideo::Page(page)) => self.show(ui, page, "YouTube".into(), true),
            None if query.starts_with("https://") || query.starts_with("http://") => {
                st.set_error("Paste a YouTube video, channel ID URL, or playlist link.".into())
            }
            None => self.show(
                ui,
                Page::Search(query.clone(), st.get_filter().to_string()),
                format!("Search: {query}"),
                true,
            ),
        }
    }
    fn play(&self, ui: &AppWindow, item: Item) {
        if item.members_only {
            ui.global::<LumtubState>().set_error("This video requires a YouTube membership. Account sign-in is not available in CedarApple yet.".into());
            return;
        }
        self.cancel(ui);
        let generation = self.generation.get();
        let st = ui.global::<LumtubState>();
        st.set_resolving(true);
        st.set_error("".into());
        st.set_status("Opening video · Preparing playback tools on first use…".into());
        *self.pending.borrow_mut() = Some(item.clone());
        *self.resolution.borrow_mut() = Some(tokio::spawn(async move {
            let result = tools::resolve(&item.id).await;
            let _ = slint::invoke_from_event_loop(move || {
                with(|b, ui| {
                    if b.generation.get() != generation {
                        return;
                    }
                    let st = ui.global::<LumtubState>();
                    st.set_resolving(false);
                    match result {
                        Ok(video) => {
                            let mut item = item;
                            if !video.title.is_empty() {
                                item.title = video.title;
                            }
                            item.channel = video.channel;
                            item.channel_id = video.channel_id;
                            item.duration = video.duration;
                            if item.thumbnail.is_empty() {
                                item.thumbnail =
                                    format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", item.id);
                            }
                            *b.playing.borrow_mut() = Some(item.clone());
                            st.set_open(false);
                            b.pending.borrow_mut().take();
                            crate::playback::play_lumtub(
                                &video.url,
                                &item.id,
                                &item.title,
                                &video.user_agent,
                            );
                        }
                        Err(error) => {
                            st.set_error(error.into());
                            st.set_status("Select Retry to open this video again.".into());
                        }
                    }
                })
            });
        }));
    }
    fn persist(&self, ui: &AppWindow) {
        if let Err(error) = self.store.borrow().save() {
            ui.global::<LumtubState>()
                .set_error(format!("Could not save Lumtub library: {error}").into());
        }
    }
    fn follow(&self, ui: &AppWindow, item: Item) {
        Store::toggle(&mut self.store.borrow_mut().channels, item);
        self.persist(ui);
        let (page, _) = self.current();
        if let Page::Channel(id, _) = page {
            ui.global::<LumtubState>()
                .set_followed(self.store.borrow().channels.iter().any(|i| i.id == id));
        }
        if matches!(self.current().0, Page::Channels) {
            *self.items.borrow_mut() = self.store.borrow().channels.clone();
        }
        self.sync(ui);
        self.load_covers(ui);
    }
    fn item(&self, index: i32) -> Option<Item> {
        usize::try_from(index)
            .ok()
            .and_then(|n| self.items.borrow().get(n).cloned())
    }
}

pub fn install(ui: &AppWindow) {
    let browser = Rc::new(Browser {
        ui: ui.as_weak(),
        nav: RefCell::new(Vec::new()),
        items: RefCell::new(Vec::new()),
        store: RefCell::new(Store::load()),
        generation: Cell::new(0),
        continuation: RefCell::new(None),
        continuation_page: RefCell::new(None),
        tokens: RefCell::new(HashSet::new()),
        shelves: RefCell::new(VecDeque::new()),
        request: RefCell::new(None),
        resolution: RefCell::new(None),
        covers: RefCell::new(None),
        cover_generation: Cell::new(0),
        pending: RefCell::new(None),
        playing: RefCell::new(None),
    });
    BROWSER.with(|slot| *slot.borrow_mut() = Some(browser));
    let st = ui.global::<LumtubState>();
    st.on_opened(|| {
        with(|b, ui| {
            ui.global::<LumtubState>().set_open(true);
            if b.nav.borrow().is_empty() {
                b.show(
                    ui,
                    Page::Search("science and technology".into(), "Videos".into()),
                    "Explore".into(),
                    true,
                );
            } else {
                let (page, title) = b.current();
                b.show(ui, page, title, false);
            }
        })
    });
    st.on_closed(|| {
        with(|b, ui| {
            b.cancel(ui);
            ui.global::<LumtubState>().set_open(false);
        })
    });
    st.on_cancel(|| with(|b, ui| b.cancel(ui)));
    st.on_search(|| with(|b, ui| b.search(ui)));
    st.on_filter_picked(|value| {
        with(|b, ui| {
            ui.global::<LumtubState>().set_filter(value);
            b.search(ui);
        })
    });
    st.on_category_picked(|value| {
        with(|b, ui| {
            ui.global::<LumtubState>().set_category(value.clone());
            b.show(
                ui,
                Page::Search(value.to_string(), "Videos".into()),
                format!("Explore · {value}"),
                true,
            );
        })
    });
    st.on_browse(|section| {
        with(|b, ui| {
            let page = match section.as_str() {
                "Watch later" => Page::Saved,
                "History" => Page::History,
                "Channels" => Page::Channels,
                _ => Page::Search(
                    ui.global::<LumtubState>().get_category().to_string(),
                    "Videos".into(),
                ),
            };
            b.show(ui, page, section.to_string(), true);
        })
    });
    st.on_back(|| {
        with(|b, ui| {
            if b.nav.borrow().len() > 1 {
                b.nav.borrow_mut().pop();
                let (page, title) = b.current();
                b.show(ui, page, title, false);
            }
        })
    });
    st.on_more(|| with(|b, ui| b.fetch(ui, true)));
    st.on_retry(|| {
        with(|b, ui| {
            let pending = b.pending.borrow().clone();
            if let Some(item) = pending {
                b.play(ui, item);
            } else {
                b.fetch(ui, ui.global::<LumtubState>().get_has_more());
            }
        })
    });
    st.on_item_picked(|index| {
        with(|b, ui| {
            if let Some(item) = b.item(index) {
                match item.kind.as_str() {
                    "video" => b.play(ui, item),
                    "channel" => b.show(
                        ui,
                        Page::Channel(item.id, "Videos".into()),
                        item.title,
                        true,
                    ),
                    _ => b.show(ui, Page::Playlist(item.id), item.title, true),
                }
            }
        })
    });
    st.on_save_item(|index| {
        with(|b, ui| {
            if let Some(item) = b.item(index) {
                if item.kind == "channel" {
                    b.follow(ui, item);
                } else {
                    Store::toggle(&mut b.store.borrow_mut().saved, item);
                    b.persist(ui);
                    if matches!(b.current().0, Page::Saved) {
                        *b.items.borrow_mut() = b.store.borrow().saved.clone();
                    }
                    b.sync(ui);
                    b.load_covers(ui);
                }
            }
        })
    });
    st.on_channel(|index| {
        with(|b, ui| {
            if let Some(item) = b.item(index)
                && !item.channel_id.is_empty()
            {
                b.show(
                    ui,
                    Page::Channel(item.channel_id, "Videos".into()),
                    item.channel,
                    true,
                );
            }
        })
    });
    st.on_tab_picked(|tab| {
        with(|b, ui| {
            if let (Page::Channel(id, _), title) = b.current() {
                b.nav.borrow_mut().pop();
                b.show(ui, Page::Channel(id, tab.to_string()), title, true);
            }
        })
    });
    st.on_follow(|| {
        with(|b, ui| {
            if let (Page::Channel(id, _), title) = b.current() {
                b.follow(
                    ui,
                    Item {
                        id,
                        title,
                        kind: "channel".into(),
                        ..Default::default()
                    },
                );
            }
        })
    });
}

pub fn shutdown() {
    with(|b, ui| b.cancel(ui));
    BROWSER.with(|slot| {
        slot.borrow_mut().take();
    });
}

/// Record history only after mpv actually opens the resolved streams.
pub fn playback_started(id: &str) {
    with(|b, ui| {
        let item = b.playing.borrow().clone().filter(|i| i.id == id);
        if let Some(item) = item {
            b.store.borrow_mut().watched(item);
            b.persist(ui);
        }
    });
}

pub fn playback_failed(id: &str) {
    with(|b, ui| {
        let item = b.playing.borrow().clone().filter(|i| i.id == id);
        if let Some(item) = item {
            *b.pending.borrow_mut() = Some(item);
            let st = ui.global::<LumtubState>();
            st.set_open(true);
            st.set_error(
                "YouTube's stream could not be opened. Retry to request fresh stream addresses."
                    .into(),
            );
            st.set_status("Playback failed. Your browsing results are still here.".into());
        }
    });
}
