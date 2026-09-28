# CedarApple

![GPL3](GPL3.png)

A small Rust + Slint media player. A learning project, not a polished product - it plays a
video file you pick, with basic transport controls. Built on `libmpv` for playback. Useful for developers/educational purposes.

## License

CedarApple is free software under the **GNU General Public License, version 3
or later** — see [LICENSE](LICENSE). Copyright (c) 2026 skonester and
Evgenii Zakharov.

The player started from code by tfo-dot released under the MIT License, which
permits using it in a GPL-licensed work; CedarApple as a whole is distributed
under the GPL. The MIT notice for that code is kept in
[licenses/MIT-tfo-dot.txt](licenses/MIT-tfo-dot.txt), as its license requires.

Much of the player is ported from [Frame Player](https://github.com/risenxxx/frame-player)
(Copyright (c) Evgenii Zakharov, GPL-3.0-or-later): the torrent engine is its
Rust code, taken nearly verbatim, and the controls, start screen, torrent flow
and seek behaviour are its Svelte UI rebuilt in Slint and Rust. Every ported
file says so at its top; [FRAME_PLAYER_ADOPTION.md](FRAME_PLAYER_ADOPTION.md)
lists them. The vendored librqbit crates are Apache-2.0
([licenses/Apache-2.0.txt](licenses/Apache-2.0.txt), changes described in
[vendor/README-librqbit.md](vendor/README-librqbit.md)).

## Building

Needs Rust (via [rustup](https://rustup.rs/)) and `libmpv`:

- **Linux**: `sudo apt install libmpv-dev` (or `pacman -S mpv`).
- **macOS**: `brew install mpv`.
- **Windows**: this repo already ships a working `mpv-dev/` folder (the ~100MB
  `libmpv-2.dll` plus an `mpv.lib` built from it) - `cargo build --release` just picks it
  up, nothing to download or set up. If you ever swap in a newer `libmpv-2.dll`, re-run
  `powershell -File mpv-dev/gen_def.ps1` to regenerate `mpv.lib` to match it (needs the VS
  Build Tools); see the comment at the top of that script for why that step exists at all.

```bash
cargo run --release
```

## What's here / not here

A Slint player over libmpv with Frame Player's controls and its torrent
streaming:

- **Start screen**: open a file, or open a link — a magnet, a `.torrent`, or
  any URL mpv plays. Torrents already on disk are listed with their size and
  where you left off; click one to continue, or delete it (just the watched
  episodes, or everything).
- **Torrent streaming**: a magnet resolves to its file list, you pick what to
  watch, and the other videos queue around it. Only the file being played is
  downloaded, seeding is off, subtitles inside the release are attached, and
  the next episode is fetched once the current one is complete. The seekbar
  shades what is already on disk; a readout top-right gives peers and speed
  and says when playback is waiting for data. Data lives in the app's cache
  folder (`%LOCALAPPDATA%\CedarApple\cache\torrents` on Windows).
- **Controls**: drag-to-seek that shows the frame under the pointer and lands
  exactly where you let go, chapter marks and names, queue, chapter, audio and
  subtitle menus, repeat (off/all/one), volume, speed, fullscreen, an on-screen
  readout for changes, and resume from where each file was left.
- **Keys** (Frame Player's map): Space/K pause · ←/→ 5 s (Shift: 1 s) · J/L
  10 s · Home/End · , / . frame step · [ / ] speed, Backspace resets ·
  PgUp/PgDn previous/next · Ctrl+←/→ chapter · ↑/↓ volume · M mute · Shift+L
  repeat · F/F11 fullscreen · O open file · Ctrl+L open a link · C info card.
  Right-click the picture for speed presets.

Not yet: Frame Player's torrent settings (seeding, port forwarding, proxy,
encryption, route — the engine supports them, CedarApple uses the defaults),
seekbar thumbnails, casting and watch-together. Two older pieces
still sit in the source but aren't wired to anything: a Seanime client
(`src/backend/seanime/`) and a small scripting VM for `.pts` plugins
(`src/extensions/`, documented in [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md)).
