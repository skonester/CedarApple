# CedarApple

![GPL3](GPL3.png)

CedarApple is a native media player written in Rust with a Slint UI and
`libmpv` playback. It opens local files, web links, magnet links, and torrent
files. It also includes a Creative Commons/public-domain music browser and
downloader.

The application is usable, but it is still an actively developed project
rather than a polished, general-purpose media library.

## Features

### Playback

- Open a local video or audio file, or a URL that mpv can play.
- Pause, seek, frame-step, change speed, adjust volume, mute, and toggle
  fullscreen.
- Playlist/queue navigation, chapters, audio-track and subtitle-track
  selection, repeat modes, and an on-screen display for changes.
- Drag the seek bar to preview and seek precisely.
- Resume positions are saved between launches.
- Hardware decoding can be enabled from the player or Settings. Rendering uses
  mpv through an OpenGL framebuffer.
- Screenshots, aspect-ratio and deinterlacing controls, subtitle styling,
  audio options, and mpv logging are available in Settings.
- When Discord is running, CedarApple publishes the current playback title and
  paused/playing state as Discord Rich Presence.

### Torrent streaming

- Open a magnet link or `.torrent` file from the start screen or the Open Link
  dialog.
- Resolve the torrent file list, choose a video, and stream it to mpv through a
  loopback HTTP server.
- Prioritize the file being watched instead of downloading a whole season
  first. Seeding is disabled by default.
- Show torrent progress, peers, speed, and cache-waiting status in the player.
- Show downloaded piece ranges on the seek bar.
- Keep torrent metadata and watched-file state so previously opened torrents
  can be reopened from the start screen.
- Reveal a torrent's files or delete watched files/all files from the torrent
  list.

Torrent data is stored below the platform cache directory in a `torrents`
subdirectory. Settings → Torrents exposes and saves seeding, UPnP port
forwarding, a SOCKS5 proxy, peer encryption, and the network route. Changing a
preference stops the current torrent; reopen it to continue with the new
preferences. The proxy covers outgoing peer TCP and HTTP trackers; DHT and
UDP trackers use the selected network route.

### Library and providers

Open **Library** from the start screen, player toolbar, or context menu. Pick
a source to browse folders, search, view continue-watching items, or play a
result. Configure sources in **Settings → Library**, then click **Apply**:

- **Local folders:** browse videos in the selected library folder.
- **Seanime:** enable it and enter your existing server's HTTP/HTTPS address.
  Manage sign-in in Seanime. Library and episode browsing, playback, resume,
  and watch-history reporting use the existing Seanime client. Library changes
  received through its WebSocket refresh the open browser.
- **Parts plugins:** enable `.pts` scripts and use **Open folder** to find their
  installation directory. Scripts load at startup while enabled and reload
  when added or edited; deleted scripts unload. Provider hooks appear under
  **Parts plugins**. Playback events and player-control functions are connected.

Provider playback currently opens one selected item at a time. A built-in
Seanime account login, artwork grids, and arbitrary plugin UI layout injection
are not implemented. Only enable scripts you trust; they can access the network
and control playback. See [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md).

### Free Music

The Free Music dialog searches and browses:

- **Jamendo**, when a free Jamendo client ID is configured.
- **Internet Archive**, including its Free Music Archive collection mirror,
  without an API key.

It supports genre and mood browsing, source/licence/length filters, sorting,
album and artist pages, similar-track radio, queues, and track playback.
Downloads include metadata tags, cover art, licence information, and a
`.licence.txt` attribution file. Downloaded tracks can be played locally.

Configure the Jamendo client ID and download folder in Settings → Free Music.
The ID can also be supplied through the `JAMENDO_CLIENT_ID` environment
variable, or baked into release builds with the
`CEDARAPPLE_JAMENDO_CLIENT_ID` repository variable. Without an ID, Internet
Archive results remain available.

### Keyboard shortcuts

| Key | Action |
| --- | --- |
| `Space` / `K` | Play or pause |
| `Left` / `Right` | Seek by the configured short step; hold `Shift` for 1 second |
| `J` / `L` | Seek by the configured long step; uppercase `L` cycles repeat |
| `Home` / `End` | Seek to the beginning/end |
| `,` / `.` | Previous/next frame |
| `[` / `]` | Decrease/increase speed |
| `Backspace` | Reset speed to 1x |
| `Page Up` / `Page Down` | Previous/next queue item |
| `Ctrl+Left` / `Ctrl+Right` | Previous/next chapter |
| `Up` / `Down` | Increase/decrease volume |
| `M` | Mute/unmute |
| `F` / `F11` | Toggle fullscreen |
| `O` | Open a file |
| `Ctrl+L` | Open a link |
| `C` / `Shift+C` | Show the info card; `Shift+C` uses the compact version |
| `S` | Take a screenshot |
| `Ctrl+,` | Open Settings |
| `Esc` | Close an open menu or leave fullscreen |

The short seek step, long seek step, volume step, and other player behavior
can be changed in Settings.

## Building from source

Install Rust with [rustup](https://rustup.rs/), then install the platform
dependencies:

- **Linux (Ubuntu/Debian):**
  `sudo apt install libmpv-dev libxkbcommon-dev libfontconfig1-dev`
- **Linux (Arch):** install the equivalent `mpv`, `libxkbcommon`, and
  `fontconfig` development packages.
- **macOS:** `brew install mpv`
- **Windows:** the repository includes the required `mpv-dev/` directory,
  including `libmpv-2.dll` and an MSVC-compatible `mpv.lib`. No separate mpv
  download is needed.

Run the application with:

```text
cargo run --release
```

Run the same checks used by continuous integration with:

```text
cargo build --profile ci --locked
cargo test --profile ci --locked
```

On Windows, if `mpv-dev/libmpv-2.dll` is replaced with a compatible newer DLL,
regenerate the import library with:

```text
powershell -File mpv-dev/gen_def.ps1
```

That command requires the Visual Studio Build Tools.

## Files and data

CedarApple uses the platform-specific application data and cache directories
provided by the `directories` crate. The exact parent differs by operating
system; the important files are:

- `history.json`: remembered torrents, watched torrent files, and resume
  positions.
- `settings.json`: player, UI, torrent, and library/provider settings.
- `free-music.json`: Free Music settings.
- `cache/torrents/`: torrent session data and downloaded torrent content.
- `cache/free-music-covers/`: cached Free Music artwork.

The in-app Settings dialog can also configure the screenshot folder and
optional mpv log file.

## Release artifacts

Pushing a tag named `vX.Y.Z` runs
[`.github/workflows/release.yml`](.github/workflows/release.yml) and publishes:

- A portable Windows x86_64 ZIP containing `cedarapple.exe`,
  `libmpv-2.dll`, the bundled mpv scripts, and licences.
- A Linux x86_64 tarball that expects libmpv to be installed on the system.
- A Linux x86_64 AppImage with a slim libmpv build bundled.
- A Linux x86_64 Flatpak bundle.

The workflow also publishes SHA-256 checksums. See
[packaging/README.md](packaging/README.md) for the manual packaging commands.

## Project status and documentation

The current UI includes direct file/link playback, torrent playback, Free
Music, and an initial provider Library. Seanime requires a running, configured
server; Parts requires enabled, installed scripts. Both are disabled by default.

See [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md) for the scripting runtime reference,
[architecture.md](architecture.md) for the module overview, and
[ports.md](ports.md) for the Frame Player and Vivi code that was ported.

## License and attribution

CedarApple is free software under the **GNU General Public License, version 3
or later**; see [LICENSE](LICENSE). Copyright (c) 2026 skonester and
Evgenii Zakharov.

The torrent engine and related UI were ported from
[Frame Player](https://github.com/risenxxx/frame-player), GPL-3.0-or-later,
Copyright (c) Evgenii Zakharov. The Free Music browser, rendering changes, and
Linux packaging were ported from
[Vivi](https://github.com/Loafer19/vivi), MIT, Copyright (c) 2026 Ruslan
Popelyshyn.

Vendored `librqbit` crates are Apache-2.0. Additional inherited MIT notices and
the Apache-2.0 text are kept in [licenses/](licenses/); changes to the vendored
crates are described in [vendor/README-librqbit.md](vendor/README-librqbit.md).
