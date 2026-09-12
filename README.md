# CedarApple

![GPL3](GPL3.png)

A small Rust + Slint media player. A learning project, not a polished product - it plays a
video file you pick, with basic transport controls. Built on `libmpv` for playback. Useful for developers/educational purposes.

## License

This project is dual-licensed under both the MIT License and the GNU General Public License v3.0 (GPLv3).

### MIT License

Copyright (c) 2026 tfo-dot

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

### GNU General Public License v3.0 (GPLv3)

GNU GENERAL PUBLIC LICENSE
Version 3, 29 June 2007

Copyright (C) 2007, 2026 skonester

Everyone is permitted to copy and distribute verbatim copies
of this license document, but changing it is not allowed.

Preamble

The GNU General Public License is a free, copyleft license for
software and other kinds of works.

The licenses for most software and other practical works are designed
to take away your freedom to share and modify the software. By contrast,
the GNU General Public License is intended to guarantee your freedom to
share and change free software. To protect your rights, we need to make
restrictions that forbid anyone to deny you these rights or to ask you to
submit to any kind of legal agreement. Therefore, you have the conditions
below.

You must give any other recipients of the Work or Derivative Works
a copy of this License; you must not take away or alter the substance
of the License and of its terms, and you may not impose any further
restrictions upon the recipients.

You may apply the work under the terms of the GNU General Public License
as published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version published by that same
author. This License gives you permission to copy, modify and redistribute
the work, under certain conditions. Copy, modify, and distribute the work
or any part of it under the terms of this License. The work is distributed
"as is", without warranty of any kind, express or implied; without even the
implied warranty of merchantability or fitness for a particular purpose.

You should have received a copy of the GNU General Public License
along with this program. If not, see <https://www.gnu.org/licenses/>.

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

Ctrl+O (or the Open button) picks a file to play.

## What's here / not here

It's just the player - no library browser, no accounts, no scrobbling. Two bigger pieces
still sit in the source but aren't wired to anything: a Seanime client
(`src/backend/seanime/`) and a small scripting VM for `.pts` plugins
(`src/extensions/`, documented in [PLUGIN_GUIDE.md](PLUGIN_GUIDE.md)). Both predate this
UI and would need work before they're worth turning back on.
