
- **Rust (`src-tauri/`)** is copied. Only the edges change: `#[tauri::command]`
  wrappers become plain functions, `tauri::async_runtime` becomes tokio, and
  Tauri's path resolver becomes `directories`.
- **Svelte logic (`*.svelte.ts`)** cannot run here: there is no webview. It is
  translated line for line into Rust, keeping its state, rules and constants.
- **Svelte markup and CSS (`*.svelte`)** is rebuilt in Slint with the same
  layout, SVG paths, colours, sizes and English strings.

## Ported

| CedarApple | From Frame Player | How |
| --- | --- | --- |
| `src/torrent.rs` | `src-tauri/src/torrent.rs` | Copied (engine, loopback HTTP server, storage rules, tests) |
| `src/torrent_storage.rs` | `src-tauri/src/torrent_storage.rs` | Copied |
| `src/net_route.rs` | `src-tauri/src/net_route.rs` | Copied (minus the Tauri command) |
| `vendor/librqbit*` | `src-tauri/vendor/` | Copied; Apache-2.0, see `vendor/README-librqbit.md` |
| `src/torrent_ui.rs` | `torrent.svelte.ts`, `open.svelte.ts`, `source.ts`, `format.ts`, `units.ts` | Translated |
| `src/playback.rs` | `open.svelte.ts`, `playlist.svelte.ts`, `torrent.svelte.ts`, `playback.svelte.ts`, `player.svelte.ts` | Translated |
| `src/scrub.rs` | `seek.svelte.ts` (drag contract, slow-seek probe) | Translated |
| `src/history.rs` | `torrent.svelte.ts` store, positions in `history.svelte.ts` | Translated; JSON file instead of localStorage |
| `ui/app-window.slint` | `+page.svelte`, `Controls.svelte`, `TopBar.svelte` (torrent chip), `Osd.svelte`, OSC menus, `keys.svelte.ts` | Rebuilt in Slint |
| `ui/controls.slint` | `Controls.svelte`, `SeekBar.svelte`, `app.css` | Rebuilt in Slint |
| `ui/start.slint` | `StartScreen.svelte` | Rebuilt in Slint |
| `ui/dialogs.slint` | `LinkDialog.svelte`, `TorrentPickDialog.svelte`, `Dialog.svelte` | Rebuilt in Slint |
| `ui/icons.slint`, `ui/theme.slint` | Glyphs and colours from the components above | Copied values |

Both network tests run here as they do in Frame Player:
`FP_TEST_MAGNET=1 cargo test sintel_smoke -- --nocapture`.

## Ported from Vivi

[Vivi](https://github.com/Loafer19/vivi) (MIT, Copyright (c) 2026 Ruslan
Popelyshyn) is Rust + Slint + libmpv already, so its code is copied; only the
host around it changes (its plugin system, ureq and settings pages become a
dialog of CedarApple's own, reqwest and a JSON file).

| CedarApple | From Vivi | How |
| --- | --- | --- |
| `src/free_music/catalog.rs`, `api.rs` | `src/plugins/free_music/catalog.rs`, `api.rs` | Copied (searches, licences, merge, download), tests and fixtures too |
| `src/free_music/mod.rs` | `src/plugins/free_music/mod.rs` | The plugin as a standalone controller; settings in `free-music.json` |
| `src/free_music/tags.rs`, `web.rs` | `plugins/auto_tagger` (`write_tags`), `plugins/web.rs` | The parts Free Music uses, on reqwest |
| `ui/music.slint` | `ui/plugins.slint` (`MusicDialog`, `TrackRow`, `NowPlayingCard`) | Rebuilt in CedarApple's dialog look |
| `src/player/pipeline.rs` (`pick_interop`), `src/main.rs` (update callback, GL reset) | `src/render.rs` | Copied / adapted to the FBO renderer |
| `src/player/mpv.rs` options | `src/player.rs` (`audio-fallback-to-null`, built-in scripts off) | Copied |
| `.github/workflows/release.yml`, `packaging/` | `release.yml`, `packaging/appimage`, `packaging/flatpak` | Adapted; Windows keeps the bundled `mpv-dev/libmpv-2.dll` |

## Next, in order

1. **Torrent settings.** `TorrentService` already has `set_seeding`,
   `set_port_forward`, `set_proxy`, `set_encryption` and `set_route`;
   CedarApple always passes `SessionPrefs::default()`. Port the torrent part
   of `SettingsDialog.svelte` and persist the choices beside `history.json`.
2. **Recent files on the start screen.** Positions are already saved per file
   (`history.rs`); `StartScreen.svelte`'s "Continue watching" rail is the
   missing view.
3. **Seekbar thumbnails.** Port `thumb_service.rs` (it needs `ffmpeg-the-third`)
   and `thumbs.svelte.ts`. `torrent_offline_file` and `local_path` are already
   here for thumbnails of torrent files.
4. **Folding the control row** at narrow widths (`MoreMenu.svelte`).
5. **Torrent updates**: `relocate` is ported; `updateTorrent` and the RSS
   feed (`feed.rs`) are not.

Casting, watch-together, the catalog and the updater are separate product
areas and have not been started.
