# Packaging

What `.github/workflows/release.yml` builds, and how to build each file by hand.
Pushing a tag `vX.Y.Z` builds all four and publishes them as one GitHub release
with a `SHA256SUMS`; "Run workflow" builds them as run artifacts only, or
releases them too when given a tag.

| File | What it is |
| --- | --- |
| `cedarapple-<tag>-windows-x86_64.zip` | Portable: `cedarapple.exe`, the committed `mpv-dev/libmpv-2.dll`, `mpv/` (spincard), licences |
| `cedarapple-<tag>-linux-x86_64.tar.gz` | The binary and `mpv/` beside it; needs libmpv installed (`libmpv2` / `mpv`) |
| `CedarApple-<tag>-x86_64.AppImage` | Any distro: a slim libmpv and its libraries bundled |
| `cedarapple-<tag>-x86_64.flatpak` | Flatpak bundle on `org.freedesktop.Platform` 25.08 |

Optional repository variable: `CEDARAPPLE_JAMENDO_CLIENT_ID`, a public Jamendo
client ID baked into the builds so Free Music searches Jamendo without setup.

## Files

- `cedarapple.desktop`, `cedarapple.png` (256 px, from `images/CedarApple.png`).
- `appimage/build-libmpv.sh` builds FFmpeg (decoders only) and libmpv into
  `deps/`; `appimage/build-appimage.sh` assembles the AppImage around
  `target/release/cedarapple` with linuxdeploy. Both are Vivi's (MIT).
- `flatpak/io.github.skonester.CedarApple.yml` builds libmpv and CedarApple;
  its `cargo-sources.json` is generated from `Cargo.lock` (not committed).

The mpv config dir travels with every build: `mpv/` beside the binary (zip,
tarball), or `share/cedarapple/mpv` beside `bin/` (AppImage, Flatpak).
`src/player/mpv.rs` looks in both.

## By hand (Linux)

```bash
# AppImage
PREFIX=$PWD/deps bash packaging/appimage/build-libmpv.sh
PKG_CONFIG_PATH=$PWD/deps/lib/pkgconfig LIBRARY_PATH=$PWD/deps/lib cargo build --release
DEPS=$PWD/deps bash packaging/appimage/build-appimage.sh CedarApple.AppImage

# Flatpak
python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
flatpak-builder --user --install-deps-from=flathub --force-clean build-dir \
  packaging/flatpak/io.github.skonester.CedarApple.yml
```

`flatpak-cargo-generator.py` is in
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools) (`cargo/`).
