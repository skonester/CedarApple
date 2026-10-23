use std::ffi::{CStr, c_void};

pub fn configure_hardware_acceleration(mpv: *mut libmpv_sys::mpv_handle) {
    let hwdec = std::ffi::CString::new("hwdec").unwrap();
    // Disable hardware decoding to guarantee clean, artifact-free frame rendering into OpenGL FBO
    let hwdec_val = std::ffi::CString::new("no").unwrap();
    unsafe {
        libmpv_sys::mpv_set_property_string(mpv, hwdec.as_ptr(), hwdec_val.as_ptr());
    }
}

/// mpv's `gpu-hwdec-interop` for the render context, taken from Vivi's
/// `render.rs` (MIT, Copyright (c) 2026 Ruslan Popelyshyn). Left on auto,
/// libmpv's render API loads *every* interop when the context is made (CUDA
/// included, which loads libcuda and can wake a laptop's discrete GPU while the
/// window renders on the integrated one), so pick the one that matches the GL
/// driver. It only matters once the GPU button turns hwdec on, but mpv reads it
/// when the render context is created. `None` = leave mpv's default; always on
/// Windows and macOS, where the interops are cheap.
pub fn pick_interop(hwdec: &str, vendor: &str, renderer: &str) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    // An explicit hardware decoder needs its own interop.
    match hwdec {
        h if h.starts_with("nvdec") || h.starts_with("cuda") => return Some("cuda".into()),
        h if h.starts_with("vaapi") => return Some("vaapi".into()),
        h if h.starts_with("vulkan") || h.starts_with("drm") || h.starts_with("vdpau") => {
            return None;
        }
        _ => {}
    }
    let gl = format!("{vendor} {renderer}").to_ascii_lowercase();
    if gl.contains("nvidia") && !gl.contains("nouveau") {
        Some("cuda".into())
    } else if ["mesa", "amd", "radeon", "intel", "x.org", "llvmpipe", "nouveau"]
        .iter()
        .any(|k| gl.contains(k))
    {
        Some("vaapi".into())
    } else {
        None
    }
}

/// GL_VENDOR and GL_RENDERER of the current context (empty when unavailable).
pub fn gl_strings(get_proc_address: &dyn Fn(&CStr) -> *const c_void) -> (String, String) {
    type GetString = extern "system" fn(u32) -> *const u8;
    let p = get_proc_address(c"glGetString");
    if p.is_null() {
        return Default::default();
    }
    // SAFETY: glGetString(GLenum) -> const GLubyte*, per the OpenGL specification.
    let f = unsafe { std::mem::transmute::<*const c_void, GetString>(p) };
    let get = |name: u32| {
        let p = f(name);
        if p.is_null() {
            return String::new();
        }
        // SAFETY: a non-null result is a NUL-terminated string owned by the driver.
        unsafe { CStr::from_ptr(p.cast()) }
            .to_string_lossy()
            .into_owned()
    };
    (get(0x1F00), get(0x1F01))
}

#[cfg(test)]
mod tests {
    use super::pick_interop;

    #[test]
    fn interop_follows_hwdec_and_gl_vendor() {
        let p = pick_interop;
        if !cfg!(target_os = "linux") {
            assert_eq!(p("auto-safe", "AMD", "Radeon"), None);
            return;
        }
        assert_eq!(
            p("auto-safe", "AMD", "AMD Radeon Graphics (radeonsi, renoir)"),
            Some("vaapi".into())
        );
        assert_eq!(
            p("auto-safe", "Intel", "Mesa Intel(R) Xe Graphics"),
            Some("vaapi".into())
        );
        assert_eq!(
            p("auto", "NVIDIA Corporation", "NVIDIA GeForce RTX 3050"),
            Some("cuda".into())
        );
        assert_eq!(p("auto-safe", "Mesa", "NV167 (nouveau)"), Some("vaapi".into()));
        assert_eq!(p("nvdec", "AMD", "Radeon"), Some("cuda".into()));
        assert_eq!(p("vaapi-copy", "NVIDIA Corporation", ""), Some("vaapi".into()));
        assert_eq!(p("vulkan", "AMD", "Radeon"), None);
        assert_eq!(p("auto-safe", "Some Vendor", "GPU"), None);
    }
}
