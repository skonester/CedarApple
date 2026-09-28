use crate::app_state::AppState;
use libmpv_sys::*;
use slint::Weak;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::path::PathBuf;
use std::ptr;
use std::sync::{Arc, Mutex};

/// Where mpv looks for `scripts/` and `script-opts/`: the `mpv/` folder next to
/// the executable, which build.rs fills from `assets/mpv/`. The source tree is
/// a fallback so a build whose assets never got copied still finds them.
fn config_dir() -> Option<PathBuf> {
    let next_to_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("mpv")));
    let in_tree = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/mpv");

    [next_to_exe, Some(in_tree)]
        .into_iter()
        .flatten()
        .find(|p| p.is_dir())
}

#[derive(Clone)]
pub struct MpvHandle(*mut mpv_handle);

impl MpvHandle {
    pub fn new() -> Self {
        unsafe {
            let handle = mpv_create();

            if handle.is_null() {
                panic!("Failed to create mpv context");
            }

            let cache_opts = [
                ("vo", "libmpv"),
                ("gpu-api", "opengl"),
                ("hwdec", "no"),
                ("cache", "yes"),
                ("demuxer-max-bytes", "150M"),
                ("demuxer-max-back-bytes", "75M"),
                ("vd-lavc-threads", "0"),
                ("terminal", "yes"),
                ("stop-screensaver", "yes"),
                // mpv's own seek bar would fight CedarApple's; the card needs
                // osd-level raised (below), so silence this explicitly.
                ("osd-on-seek", "no"),
                ("network-timeout", "10"),
            ];

            for (opt, val) in cache_opts {
                let c_opt = CString::new(opt).unwrap();
                let c_val = CString::new(val).unwrap();
                mpv_set_property_string(handle, c_opt.as_ptr(), c_val.as_ptr());
            }

            // 0 blanks mpv's OSD message layer outright. 1 leaves it live but
            // silent: client-API commands never raise an OSD message on their
            // own, and osd-on-seek=no above covers seeks. Scripts draw into
            // separate OSD layers, which is how spincard's card reaches the
            // render FBO.
            let c_osd = CString::new("osd-level").unwrap();
            let mut osd_level: i64 = 1;
            mpv_set_property(
                handle,
                c_osd.as_ptr(),
                mpv_format_MPV_FORMAT_INT64,
                &mut osd_level as *mut _ as *mut c_void,
            );

            // Point mpv at CedarApple's own config dir so it auto-loads the
            // bundled scripts/ (spincard) and reads script-opts/spincard.conf.
            // libmpv starts with config loading OFF and no config dir at all,
            // so both have to be set, and set BEFORE mpv_initialize. Naming the
            // dir explicitly also means the user's own ~/mpv config is never
            // picked up - what ships is what runs.
            if let Some(dir) = config_dir() {
                let dir_str = dir.to_string_lossy().into_owned();
                for (opt, val) in [("config", "yes"), ("config-dir", dir_str.as_str())] {
                    let c_opt = CString::new(opt).unwrap();
                    let c_val = CString::new(val).unwrap();
                    mpv_set_option_string(handle, c_opt.as_ptr(), c_val.as_ptr());
                }
                eprintln!("[CedarApple] mpv config-dir: {}", dir.display());
            } else {
                eprintln!("[CedarApple] no mpv config dir found; scripts disabled");
            }

            if mpv_initialize(handle) < 0 {
                panic!("Failed to initialize mpv context");
            }

            let c_warn = CString::new("warn").unwrap();
            mpv_request_log_messages(handle, c_warn.as_ptr());

            MpvHandle(handle)
        }
    }

    pub fn get(&self) -> *mut mpv_handle {
        self.0
    }

    /// Fire a script binding, e.g. `spincard/toggle`. Embedded libmpv gets no
    /// keyboard input of its own, so a script's `mp.add_key_binding` never
    /// triggers; this is how the UI reaches one.
    pub fn script_binding(&self, name: &str) {
        let cmd = CString::new("script-binding").unwrap();
        let Ok(arg) = CString::new(name) else { return };
        let mut args = [cmd.as_ptr(), arg.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(self.get(), args.as_mut_ptr());
        }
    }

    /// Read a property as a string, or None if mpv has no value for it (no
    /// file loaded, unknown name). The buffer mpv hands back is its own, so it
    /// is copied and freed here rather than borrowed.
    pub fn get_property_string(&self, name: &str) -> Option<String> {
        let c_name = CString::new(name).ok()?;
        unsafe {
            let raw = mpv_get_property_string(self.get(), c_name.as_ptr());
            if raw.is_null() {
                return None;
            }
            let out = CStr::from_ptr(raw).to_string_lossy().into_owned();
            mpv_free(raw as *mut c_void);
            Some(out)
        }
    }

    pub fn set_property_string(&self, name: &str, value: &str) {
        let (Ok(c_name), Ok(c_val)) = (CString::new(name), CString::new(value)) else {
            return;
        };
        unsafe {
            mpv_set_property_string(self.get(), c_name.as_ptr(), c_val.as_ptr());
        }
    }

    /// Put a message on mpv's OSD for `duration_ms`. It is drawn into the same
    /// framebuffer as the video, so it lands under Slint's controls - and it
    /// needs osd-level >= 1, which is why `new()` sets that.
    pub fn show_text(&self, text: &str, duration_ms: u32) {
        let cmd = CString::new("show-text").unwrap();
        let (Ok(c_text), Ok(c_ms)) = (CString::new(text), CString::new(duration_ms.to_string()))
        else {
            return;
        };
        let mut args = [cmd.as_ptr(), c_text.as_ptr(), c_ms.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(self.get(), args.as_mut_ptr());
        }
    }

    /// Run a command and wait for mpv to accept it.
    pub fn command(&self, args: &[&str]) -> bool {
        let Ok(owned) = args.iter().map(|a| CString::new(*a)).collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        let mut ptrs: Vec<*const std::os::raw::c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());
        unsafe { mpv_command(self.get(), ptrs.as_mut_ptr()) >= 0 }
    }

    /// Run a command without waiting. Its completion arrives later as an
    /// `MPV_EVENT_COMMAND_REPLY` carrying `reply` as its userdata.
    pub fn command_async(&self, reply: u64, args: &[&str]) {
        let Ok(owned) = args.iter().map(|a| CString::new(*a)).collect::<Result<Vec<_>, _>>()
        else {
            return;
        };
        let mut ptrs: Vec<*const std::os::raw::c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());
        unsafe {
            mpv_command_async(self.get(), reply, ptrs.as_mut_ptr());
        }
    }

    pub fn get_flag(&self, name: &str) -> bool {
        let Ok(c_name) = CString::new(name) else { return false };
        let mut v: std::os::raw::c_int = 0;
        unsafe {
            mpv_get_property(
                self.get(),
                c_name.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &mut v as *mut _ as *mut c_void,
            );
        }
        v != 0
    }

    pub fn set_flag(&self, name: &str, value: bool) {
        let Ok(c_name) = CString::new(name) else { return };
        let v: std::os::raw::c_int = value as _;
        unsafe {
            mpv_set_property(
                self.get(),
                c_name.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &v as *const _ as *mut c_void,
            );
        }
    }

    pub fn get_double(&self, name: &str) -> Option<f64> {
        let c_name = CString::new(name).ok()?;
        let mut v: f64 = 0.0;
        let ok = unsafe {
            mpv_get_property(
                self.get(),
                c_name.as_ptr(),
                mpv_format_MPV_FORMAT_DOUBLE,
                &mut v as *mut _ as *mut c_void,
            )
        } >= 0;
        ok.then_some(v)
    }

    pub fn set_double(&self, name: &str, value: f64) {
        let Ok(c_name) = CString::new(name) else { return };
        unsafe {
            mpv_set_property(
                self.get(),
                c_name.as_ptr(),
                mpv_format_MPV_FORMAT_DOUBLE,
                &value as *const _ as *mut c_void,
            );
        }
    }

    pub fn get_int(&self, name: &str) -> Option<i64> {
        let c_name = CString::new(name).ok()?;
        let mut v: i64 = 0;
        let ok = unsafe {
            mpv_get_property(
                self.get(),
                c_name.as_ptr(),
                mpv_format_MPV_FORMAT_INT64,
                &mut v as *mut _ as *mut c_void,
            )
        } >= 0;
        ok.then_some(v)
    }

    pub fn stop(&self) {

        let scmd = CString::new("stop").unwrap();
        let mut sargs = [scmd.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(self.get(), sargs.as_mut_ptr());
            mpv_terminate_destroy(self.get());
        }
    }
}

impl super::MediaSession for MpvHandle {
    fn play(&mut self) {
        let c_pause = CString::new("pause").unwrap();
        let paused: std::os::raw::c_int = 0;
        unsafe {
            mpv_set_property(
                self.get(),
                c_pause.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &paused as *const _ as *mut std::os::raw::c_void,
            );
        }
    }

    fn pause(&mut self) {
        let c_pause = CString::new("pause").unwrap();
        let paused: std::os::raw::c_int = 1;
        unsafe {
            mpv_set_property(
                self.get(),
                c_pause.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &paused as *const _ as *mut std::os::raw::c_void,
            );
        }
    }

    fn seek(&mut self, time_ms: i64) {
        let scmd = CString::new("seek").unwrap();
        let secs = time_ms as f64 / 1000.0;
        let sval = CString::new(secs.to_string()).unwrap();
        let smode = CString::new("absolute").unwrap();
        let mut sargs = [scmd.as_ptr(), sval.as_ptr(), smode.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(self.get(), sargs.as_mut_ptr());
        }
    }

    fn teardown(&mut self) {
        let scmd = CString::new("stop").unwrap();
        let mut sargs = [scmd.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(self.get(), sargs.as_mut_ptr());
        }
    }
}

unsafe impl Send for MpvHandle {}
unsafe impl Sync for MpvHandle {}
#[derive(Clone)]
pub struct MpvRenderCtx(pub *mut mpv_render_context);

impl MpvRenderCtx {
    pub fn get(&self) -> *mut mpv_render_context {
        self.0
    }
}

unsafe impl Send for MpvRenderCtx {}
unsafe impl Sync for MpvRenderCtx {}

pub async fn open_player(
    ui_weak: Weak<crate::AppWindow>,
    state_arc: Arc<Mutex<AppState>>,
    mpv: MpvHandle,
) {
    let (provider, item_id) = {
        let state = state_arc.lock().unwrap();

        if state.current_item_id.is_none() {
            eprintln!("[CedarApple] open_player: current_item_id is None!");
            return;
        }

        let (provider_id, i_id) = state.current_item_id.clone().unwrap();
        eprintln!(
            "[CedarApple] open_player: Loading item '{}' with provider '{}'",
            i_id, provider_id
        );

        (state.active_providers.get(&provider_id).cloned(), i_id)
    };

    let provider = match provider {
        Some(p) => p,
        None => {
            eprintln!("[CedarApple] open_player: Provider not found!");
            return;
        }
    };

    let resume_pos = provider.get_resume_position(&item_id).await.unwrap_or(None);
    let _ = provider.report_playback_start(&item_id).await;
    let stream_url = provider.get_stream_url(&item_id);
    eprintln!("[CedarApple] open_player: Resolved stream URL: {}", stream_url);

    let mut referer: Option<String> = None;
    let mut final_url = stream_url.clone();
    if let Some((url, headers_part)) = stream_url.split_once('|') {
        final_url = url.to_string();
        for part in headers_part.split(';') {
            if let Some((key, val)) = part.split_once('=')
                && key == "Referer"
            {
                referer = Some(val.to_string());
            }
        }
    }

    let (_has_prev, _has_next, title) = {
        let state = state_arc.lock().unwrap();
        let (p, n) = if let Some((items, idx)) = state.active_playlist.as_ref() {
            (*idx > 0, *idx < items.len() - 1)
        } else {
            (false, false)
        };
        (p, n, state.current_title.clone())
    };

    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ref_val) = referer {
            let c_opt = CString::new("http-header-fields").unwrap();
            let header_str = format!(
                "Referer: {},User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                ref_val
            );
            let c_val = CString::new(header_str).unwrap();
            unsafe {
                mpv_set_property_string(mpv.get(), c_opt.as_ptr(), c_val.as_ptr());
            }
        } else {
            let c_opt = CString::new("http-header-fields").unwrap();
            let c_val = CString::new("").unwrap();
            unsafe {
                mpv_set_property_string(mpv.get(), c_opt.as_ptr(), c_val.as_ptr());
            }
        }

        if let Some(pos) = resume_pos {
            let c_start = CString::new("start").unwrap();
            let c_pos = if pos > 0 {
                CString::new(pos.to_string()).unwrap()
            } else {
                CString::new("0").unwrap()
            };

            unsafe {
                mpv_set_property_string(mpv.get(), c_start.as_ptr(), c_pos.as_ptr());
            }
        }
        eprintln!("[CedarApple] MPV loading file: {}", final_url);
        let cmd = CString::new("loadfile").unwrap();
        let url = CString::new(final_url).unwrap();
        let mut args = [cmd.as_ptr(), url.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(mpv.get(), args.as_mut_ptr());
            let c_pause = CString::new("pause").unwrap();
            let paused: std::os::raw::c_int = 0;
            mpv_set_property(
                mpv.get(),
                c_pause.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &paused as *const _ as *mut std::os::raw::c_void,
            );
        }
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_video_title(title.into());
            ui.set_has_file(true);
            ui.set_is_loading(false);
        }
    });
}

