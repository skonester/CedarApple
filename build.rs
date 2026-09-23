fn main() {
    slint_build::compile("ui/app-window.slint").expect("Slint build failed");

    copy_mpv_assets();

    #[cfg(windows)]
    link_mpv_windows();

    #[cfg(windows)]
    embed_windows_icon();
}

/// Embeds `images/program.ico` as the exe's icon (taskbar, Explorer, alt-tab).
/// `embed-resource` is a Windows-only build-dependency (see Cargo.toml's
/// `[target.'cfg(windows)'.build-dependencies]`), so this only runs, and only
/// needs to compile, on a Windows host.
#[cfg(windows)]
fn embed_windows_icon() {
    embed_resource::compile("app.rc", embed_resource::NONE)
        .manifest_required()
        .expect("embed app icon");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=images/program.ico");
}

/// `libmpv-sys` just emits `cargo:rustc-link-lib=mpv`, which on MSVC means the
/// linker goes looking for a file literally named `mpv.lib`. There is no such
/// thing in any official libmpv distribution for Windows - what you get from
/// the mpv-dev builds (https://sourceforge.net/projects/mpv-player-windows/files/libmpv/)
/// is `libmpv-2.dll` plus a MinGW-style `libmpv.dll.a`. Neither is usable by
/// link.exe directly, so we point the linker at a local `mpv-dev/` folder
/// where an MSVC-compatible `mpv.lib` (generated from the dll's exports via
/// `lib.exe /def:`) and the runtime dll are expected to live, and copy the
/// dll next to the build output so the binary can actually find it at runtime.
#[cfg(windows)]
fn link_mpv_windows() {
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mpv_dev = manifest_dir.join("mpv-dev");

    if !mpv_dev.is_dir() {
        return;
    }

    println!("cargo:rustc-link-search=native={}", mpv_dev.display());

    let dll_name = "libmpv-2.dll";
    let dll_src = mpv_dev.join(dll_name);
    if !dll_src.is_file() {
        return;
    }

    // OUT_DIR looks like target/<profile>/build/<pkg>-<hash>/out; the build
    // output (and where a Windows loader will look next to the exe) is
    // three levels up from there.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(target_dir) = out_dir.ancestors().nth(3) else {
        return;
    };

    let dll_dst = target_dir.join(dll_name);
    let needs_copy = match (std::fs::metadata(&dll_src), std::fs::metadata(&dll_dst)) {
        (Ok(src_meta), Ok(dst_meta)) => src_meta.len() != dst_meta.len(),
        _ => true,
    };

    if needs_copy {
        let _ = std::fs::copy(&dll_src, &dll_dst);
    }
}

/// Mirrors `assets/mpv/` (the bundled mpv config dir: `scripts/spincard` and
/// `script-opts/spincard.conf`) next to the build output as `mpv/`, the same
/// place the libmpv dll lands. `src/player/mpv.rs` hands that path to mpv as
/// `--config-dir`, so the scripts travel with the exe instead of being read
/// out of the source tree.
fn copy_mpv_assets() {
    use std::path::{Path, PathBuf};

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest_dir.join("assets").join("mpv");
    if !src.is_dir() {
        return;
    }
    println!("cargo:rerun-if-changed={}", src.display());

    // Same three-levels-up dance as the dll copy below.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(target_dir) = out_dir.ancestors().nth(3) else {
        return;
    };

    fn copy_tree(src: &Path, dst: &Path) {
        if std::fs::create_dir_all(dst).is_err() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(src) else {
            return;
        };
        for entry in entries.flatten() {
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                copy_tree(&from, &to);
            } else {
                // Re-copy whenever the source is newer, so an edited .lua or
                // .conf reaches the next run without a clean build.
                let stale = match (from.metadata(), to.metadata()) {
                    (Ok(s), Ok(d)) => match (s.modified(), d.modified()) {
                        (Ok(s), Ok(d)) => s > d,
                        _ => true,
                    },
                    _ => true,
                };
                if stale {
                    let _ = std::fs::copy(&from, &to);
                }
            }
        }
    }

    copy_tree(&src, &target_dir.join("mpv"));
}
