# Frame Player in CedarApple

CedarApple and [Frame Player](https://github.com/risenxxx/frame-player) are both
GPL-3.0 (Frame Player: GPL-3.0-or-later, Copyright (c) Evgenii Zakharov), so
Frame Player's code is used directly rather than reimplemented from its ideas.
Every ported file names its source at the top.

What can move over, and how, depends on the language:

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
