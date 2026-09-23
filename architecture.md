# CedarApple Architecture

A minimal media player built with Rust and Slint, backed by `libmpv`. This document outlines the architecture and key design decisions.

## High-Level Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              UI Layer (Slint)                                 │
│  ┌───────────────┐   ┌─────────────┐   ┌─────────────┐   ┌───────────────┐  │
│  │   AppWindow    │   │  Playback   │   │   Playlist  │   │    Settings   │  │
│  │   Component   │   │  Controls   │   │  View       │   │   Dialog     │  │
│  └───────────────┘   └─────────────┘   └─────────────┘   └───────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Application State                                  │
│              (AppState: Arc<Mutex<AppState>>)                                │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐    │
│  │ Active      │  │ Current     │  │ Playlist    │  │ Providers       │    │
│  │ Playlist    │  │ Item        │  │ Index       │  │ Map             │    │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Backend / Extension Layer                           │
│  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐  ┌─────────────┐  │
│  │ LocalProvider │  │SeanimeClient  │  │ Parts VM      │  │ DiscordRPC │  │
│  └───────────────┘  └───────────────┘  └───────────────┘  └─────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Media Engine                                       │
│              (MPV Handle + OpenGL Render Context)                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Core Modules

### `src/main.rs` — Entry Point & Glue

- Initializes the Slint UI via `AppWindow::new()`.
- Creates shared `Arc<Mutex<AppState>>` and passes it to the UI and MPV backend.
- Sets up hardware-accelerated OpenGL rendering through the `RenderingNotifier`:
  - Creates an `MpvRenderCtx` when the window is set up.
  - Manages GL resources (`GLResources`) per render size.
  - Clears state on teardown.
- Binds UI callbacks (`on_open_file`, `on_toggle_pause`, `on_seek`, etc.) to MPV commands and state updates.
- Runs the main Slint event loop and cleans up MPV on exit.

### `src/app.rs` — Message Types

Defines two enums that flow between UI and backend:

- `UiCommand`: actions the UI can request (play/pause, seek, load view).
- `AppEvent`: events dispatched to the UI (command, MPV event ID, Seanime WS message, script reload).

### `src/app_state.rs` — Global Mutable State

Centralized, thread-safe application state:

- `active_playlist`: optional list of media items.
- `current_item_id`, `current_title`, `current_artist`, etc.: metadata for the currently playing item.
- `active_providers`: map from provider ID to `Arc<dyn MediaProvider>`.

All state access goes through `Mutex` guards to ensure consistency.

### `src/player/` — Media Playback

- `mpv.rs`: wraps `libmpv_sys` into a safe handle API:
  - `MpvHandle::new()` / `stop()`
  - Property getters/setters, command execution.
- `fbo.rs`: holds OpenGL framebuffer resources used by the render notifier.
- `pipeline.rs`: configures hardware acceleration in MPV.
- `mod.rs` exposes: `MpvHandle`, `MpvRenderCtx`, `open_player`, and `configure_hardware_acceleration`.

`open_player` loads a file asynchronously, creates an MPV session, registers callbacks for tracks, time, duration, and rendering, then hands control back to the UI.

### `src/backend/` — Media Providers

Implements the `MediaProvider` trait:

```rust
trait MediaProvider {
    fn id(&self) -> &'static str;
    async fn resolve_stream(&self, uri: &str) -> Result<String, SessionError>;
    async fn fetch_metadata(&self, uri: &str) -> Result<MediaMetadata, SessionError>;
}
```

- `local`: resolves a local path directly, no streaming needed.
- `seanime`: HTTP client that resolves anime streams and fetches metadata (not wired to UI yet).

### `src/navigation.rs` — UI Navigation

Provides helper functions like `format_time` for displaying durations and progress.

### `src/extensions/` — Scripting VM

A small bytecode interpreter for `.pts` plugins. The VM is defined but not connected to any UI component; see `PLUGIN_GUIDE.md` for details.

### `src/ui/` — Slint Components

- `AppWindow.slint`: main window with video surface, controls, playlist panel, settings dialog.
- `PlaylistView.slint`: displays media items and allows selection.
- Other `.slint` files define reusable controls.

The UI communicates with the backend via signal/slot wiring and by invoking `invoke_from_event_loop` for blocking operations like file dialogs.

### `src/discord.rs` — Discord Rich Presence

Uses the Discord RPC SDK to report playback info (title, artist, position, paused state) periodically while media is loaded. Updates are throttled to avoid flooding the RPC server.

### `src/image_cache.rs` — Asset Cache

Caches downloaded images (e.g., anime posters) keyed by URI. Uses an LRU eviction policy and stores data in a flat file under `~/.config/cedarapple/cache/`.

---

## Key Design Patterns

### Dependency Inversion

The UI never directly instantiates MPV or providers. Instead, it receives `Arc<Mutex<AppState>>` containing `Arc<dyn MediaProvider>` entries. This makes swapping providers trivial and keeps UI code decoupled from implementation details.

### Shared State with `Arc<Mutex<_>>`

Most cross-module data is wrapped in `Arc<Mutex<T>>`:

- `AppState` is shared between UI and backend.
- `MpvHandle`, `DiscordRPC`, and provider instances are also wrapped.

This enables safe concurrent access without cloning large structs.

### Async-Await for Blocking Calls

File dialogs and network requests are run on Tokio's blocking pool via `spawn_blocking`, preventing them from freezing the UI event loop.

### Rendering Notifier Integration

Slint's rendering notifier is used to:

- Create and destroy the MPV OpenGL context at the right times.
- Manage GL resources per render size.
- Clear OpenGL state before each frame to avoid conflicts with Slint's Femtovg renderer.

### Throttled Side Effects

Discord presence updates and provider progress reports are throttled (500ms and 1s respectively) to reduce unnecessary work.

---

## File Layout

```
src/
├── api.rs          # PlaybackExtension, PlaybackInfo
├── app.rs          # UiCommand, AppEvent
├── app_state.rs    # AppState struct
├── backend/
│   ├── mod.rs      # MediaProvider trait, LocalProvider, SeanimeProvider
│   ├── local.rs    # LocalProvider implementation
│   └── seanime.rs  # SeanimeClient implementation
├── discord.rs      # DiscordRPC
├── extensions/     # Parts scripting VM
├── image_cache.rs  # CachedImage, cache management
├── main.rs         # Entry point, Slint setup, MPV glue
├── navigation.rs   # format_time
├── player/
│   ├── mod.rs      # exports
│   ├── fbo.rs      # GLResources
│   ├── mpv.rs      # MpvHandle, open_player
│   └── pipeline.rs # configure_hardware_acceleration
└── ui/             # Slint components
    └── ...
```

---

## Build System

Uses Cargo with a vendored `libmpv-dev` submodule for Windows. On Linux/macOS, `libmpv` must be installed system-wide (`apt install libmpv-dev` / `brew install mpv`).

```bash
cargo build --release
cargo run --release
```

---

## Extensibility Notes

- **Providers**: Adding a new source just requires implementing `MediaProvider` and registering it in `AppState::new()`.
- **Plugins**: The Parts VM is isolated in `src/extensions/`; future UI wiring would involve adding signals to `AppEvent` and components to listen for `ScriptReloaded`.
- **UI themes**: Slint's theming system allows runtime customization of colors, fonts, and layout without recompilation.
