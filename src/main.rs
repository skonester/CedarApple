#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod api;
mod app;
mod app_state;
mod backend;
mod discord;
mod extensions;
mod image_cache;
mod navigation;
mod player;
mod ui;

use crate::api::{PlaybackExtension, PlaybackInfo};
use app_state::AppState;
use backend::local::LocalProvider;
use discord::DiscordRPC;
use glow::HasContext;
use libmpv_sys::*;
use navigation::format_time;
use player::{GLResources, MpvHandle, MpvRenderCtx, configure_hardware_acceleration, open_player};
use slint::{BorrowedOpenGLTextureBuilder, BorrowedOpenGLTextureOrigin, ComponentHandle};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[CedarApple] Starting up native media client...");

    unsafe {
        std::env::set_var("LC_NUMERIC", "C");
        let c_locale = CString::new("C").unwrap();
        libc::setlocale(libc::LC_ALL, c_locale.as_ptr());
        libc::setlocale(libc::LC_NUMERIC, c_locale.as_ptr());
    }

    eprintln!("[CedarApple] Initializing UI...");
    let ui = AppWindow::new()?;

    let state = Arc::new(Mutex::new(AppState::new()));

    eprintln!("[CedarApple] Creating MpvHandle...");
    let mpv = MpvHandle::new();
    configure_hardware_acceleration(mpv.get());

    // Initialize Discord RPC
    let discord = Arc::new(DiscordRPC::new());

    // The "local" provider resolves an on-disk path directly, which is all
    // the Open File flow needs. The Seanime client (src/backend/seanime)
    // and the Parts scripting bridge (src/extensions) still exist but have
    // no UI wired to them right now - see README.md.
    let local_provider = Arc::new(LocalProvider::new());
    {
        let mut s = state.lock().unwrap();
        s.active_providers
            .insert("local".to_string(), local_provider.clone());
    }

    let mpv_render: Rc<RefCell<Option<MpvRenderCtx>>> = Rc::new(RefCell::new(None));
    let gl_resources: Rc<RefCell<Option<GLResources>>> = Rc::new(RefCell::new(None));

    {
        eprintln!("[CedarApple] Setting up rendering notifier...");
        let ui_weak = ui.as_weak();
        let mpv_h = mpv.clone();
        let mpv_r = mpv_render.clone();
        let gl_res = gl_resources.clone();

        ui.window()
            .set_rendering_notifier(move |rendering_state, api| match rendering_state {
                slint::RenderingState::RenderingSetup => {
                    if let slint::GraphicsAPI::NativeOpenGL { get_proc_address } = api {
                        let api_type = CString::new("opengl").unwrap();
                        let mut init_params = mpv_opengl_init_params {
                            get_proc_address: Some(get_proc_address_mpv),
                            get_proc_address_ctx: get_proc_address as *const _ as *mut c_void,
                            extra_exts: std::ptr::null(),
                        };
                        let mut params = [
                            mpv_render_param {
                                type_: mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                                data: api_type.as_ptr() as *mut c_void,
                            },
                            mpv_render_param {
                                type_: mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                                data: &mut init_params as *mut _ as *mut c_void,
                            },
                            mpv_render_param {
                                type_: 0,
                                data: ptr::null_mut(),
                            },
                        ];

                        let mut ctx: *mut mpv_render_context = ptr::null_mut();
                        let res = unsafe {
                            mpv_render_context_create(&mut ctx, mpv_h.get(), params.as_mut_ptr())
                        };
                        if res >= 0 {
                            *mpv_r.borrow_mut() = Some(MpvRenderCtx(ctx));
                            eprintln!("[CedarApple] MPV Render Context created successfully");
                        } else {
                            eprintln!("[CedarApple] Failed to create MPV Render Context: {}", res);
                        }
                    }
                }
                slint::RenderingState::BeforeRendering => {
                    if let Some(render_ctx) = mpv_r.borrow().as_ref()
                        && let Some(ui) = ui_weak.upgrade()
                    {
                        let sf = ui.window().scale_factor();
                        let raw_w = (ui.get_video_width() * sf) as u32;
                        let raw_h = (ui.get_video_height() * sf) as u32;

                        // Ensure even dimensions for planar color conversion & alignment
                        let width = ((raw_w + 1) & !1).max(2);
                        let height = ((raw_h + 1) & !1).max(2);

                        if width > 0 && height > 0 {
                            let mut res_lock = gl_res.borrow_mut();
                            if res_lock
                                .as_ref()
                                .is_none_or(|r| r.width != width || r.height != height)
                                && let slint::GraphicsAPI::NativeOpenGL { get_proc_address } = api
                            {
                                let gl = unsafe {
                                    glow::Context::from_loader_function(|s| match CString::new(s) {
                                        Ok(name) => get_proc_address(&name) as *const _,
                                        _ => std::ptr::null(),
                                    })
                                };
                                *res_lock = Some(GLResources::new(gl, width, height));
                            }

                            if let Some(res) = res_lock.as_ref() {
                                let mut fbo = mpv_opengl_fbo {
                                    fbo: res.fbo.0.get() as i32,
                                    w: width as i32,
                                    h: height as i32,
                                    internal_format: 0x8058, // GL_RGBA8
                                };

                                let mut params = [
                                    mpv_render_param {
                                        type_: mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                                        data: &mut fbo as *mut _ as *mut c_void,
                                    },
                                    mpv_render_param {
                                        type_:
                                            mpv_render_param_type_MPV_RENDER_PARAM_ADVANCED_CONTROL,
                                        data: &mut 1 as *mut _ as *mut c_void,
                                    },
                                    mpv_render_param {
                                        type_: 0,
                                        data: ptr::null_mut(),
                                    },
                                ];

                                unsafe {
                                    // Reset OpenGL state that Slint's Femtovg renderer may have left behind
                                    res.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(res.fbo));
                                    res.gl.viewport(0, 0, width as i32, height as i32);
                                    res.gl.disable(glow::SCISSOR_TEST);
                                    res.gl.disable(glow::DEPTH_TEST);
                                    res.gl.disable(glow::STENCIL_TEST);
                                    res.gl.disable(glow::CULL_FACE);
                                    res.gl.disable(glow::BLEND);
                                    res.gl.clear_color(0.0, 0.0, 0.0, 1.0);
                                    res.gl.clear(glow::COLOR_BUFFER_BIT);

                                    let res_render = mpv_render_context_render(
                                        render_ctx.get(),
                                        params.as_mut_ptr(),
                                    );
                                    if res_render < 0 {
                                        eprintln!("[CedarApple] Render error: {}", res_render);
                                    }

                                    res.gl.bind_framebuffer(glow::FRAMEBUFFER, None);

                                    let image =
                                        BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(
                                            std::num::NonZeroU32::new(res.texture.0.get()).unwrap(),
                                            [width, height].into(),
                                        )
                                        .origin(BorrowedOpenGLTextureOrigin::TopLeft)
                                        .build();
                                    ui.set_video_frame(image);
                                }
                            }
                        }
                    }
                }
                slint::RenderingState::RenderingTeardown => {
                    if let Some(render_ctx) = mpv_r.borrow_mut().take() {
                        unsafe {
                            mpv_render_context_free(render_ctx.get());
                        }
                    }
                    *gl_res.borrow_mut() = None;
                }
                _ => {}
            })
            .unwrap();
    }

    // --- Open File ---
    let ui_open = ui.as_weak();
    let state_open = state.clone();
    let mpv_open = mpv.clone();
    ui.on_open_file(move || {
        let ui_weak = ui_open.clone();
        let state_arc = state_open.clone();
        let mpv_h = mpv_open.clone();

        tokio::spawn(async move {
            // rfd's dialog is a blocking native call; run it on Tokio's
            // blocking pool so it doesn't stall the async runtime.
            let picked = tokio::task::spawn_blocking(|| {
                rfd::FileDialog::new()
                    .add_filter(
                        "Video",
                        &["mkv", "mp4", "avi", "webm", "mov", "flv", "ts", "m4v"],
                    )
                    .pick_file()
            })
            .await
            .ok()
            .flatten();

            let Some(path) = picked else { return };
            let path_str = path.to_string_lossy().to_string();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.clone());

            {
                let mut s = state_arc.lock().unwrap();
                s.active_playlist = None;
                s.current_title = name;
                s.current_artist = String::new();
                s.current_series_name = None;
                s.current_season_index = None;
                s.current_episode_index = None;
                s.current_item_id = Some(("local".to_string(), path_str));
            }

            let _ = slint::invoke_from_event_loop({
                let ui_weak = ui_weak.clone();
                move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_is_loading(true);
                    }
                }
            });

            open_player(ui_weak, state_arc, mpv_h).await;
        });
    });

    let mpv_track = mpv.clone();
    let ui_track = ui.as_weak();
    ui.on_select_audio_track(move |id| {
        let name = CString::new("aid").unwrap();
        let val = CString::new(id.to_string()).unwrap();
        unsafe {
            mpv_set_property_string(mpv_track.get(), name.as_ptr(), val.as_ptr());
            if let Some(ui) = ui_track.upgrade() {
                update_tracks(&ui, mpv_track.get());
            }
        }
    });

    let mpv_sub = mpv.clone();
    let ui_sub = ui.as_weak();
    ui.on_select_subtitle_track(move |id| {
        let name = CString::new("sid").unwrap();
        let val = CString::new(id.to_string()).unwrap();
        unsafe {
            mpv_set_property_string(mpv_sub.get(), name.as_ptr(), val.as_ptr());
            if let Some(ui) = ui_sub.upgrade() {
                update_tracks(&ui, mpv_sub.get());
            }
        }
    });

    let mpv_speed = mpv.clone();
    let ui_speed = ui.as_weak();
    ui.on_change_speed(move |speed| {
        let name = CString::new("speed").unwrap();
        let speed_val = speed as f64;
        unsafe {
            mpv_set_property(
                mpv_speed.get(),
                name.as_ptr(),
                mpv_format_MPV_FORMAT_DOUBLE,
                &speed_val as *const _ as *mut c_void,
            );
            if let Some(ui) = ui_speed.upgrade() {
                ui.set_playback_speed(format!("{:.2}x", speed_val).into());
            }
        }
    });

    let last_activity = Arc::new(Mutex::new(std::time::Instant::now()));
    let la_clone = last_activity.clone();
    ui.on_user_activity(move || {
        *la_clone.lock().unwrap() = std::time::Instant::now();
    });

    let mpv_toggle = mpv.clone();
    ui.on_toggle_pause(move || {
        let c_pause = CString::new("pause").unwrap();
        let mut paused: c_int = 0;
        unsafe {
            mpv_get_property(
                mpv_toggle.get(),
                c_pause.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &mut paused as *mut _ as *mut c_void,
            );
            let new_p = if paused == 0 { 1 } else { 0 };
            mpv_set_property(
                mpv_toggle.get(),
                c_pause.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &new_p as *const _ as *mut c_void,
            );
        }
    });

    let mpv_card = mpv.clone();
    ui.on_toggle_info_card(move |lean| {
        // spincard registers both bindings under its own script name; the
        // lean variant hides whatever `lean_hide` lists (the synopsis, by
        // default). Pressing one while the other is up switches in place.
        mpv_card.script_binding(if lean {
            "spincard/toggle-lean"
        } else {
            "spincard/toggle"
        });
    });

    // GPU button. Note this is NOT an OpenGL/Vulkan switch: libmpv's render
    // API only defines "opengl" and "sw" (render.h), and CedarApple hands mpv
    // a GL FBO, so the render backend is fixed by the embedding. hwdec is the
    // GPU lever that is actually live here - it moves decoding on and off the
    // GPU without touching how frames reach the window.
    let mpv_gpu = mpv.clone();
    let hwdec_readout = std::rc::Rc::new(slint::Timer::default());
    ui.on_toggle_hwdec(move || {
        let current = mpv_gpu
            .get_property_string("hwdec")
            .unwrap_or_else(|| "no".to_string());
        let next = if current == "no" || current.is_empty() {
            "auto"
        } else {
            "no"
        };
        mpv_gpu.set_property_string("hwdec", next);
        mpv_gpu.show_text(&format!("hwdec: {next}..."), 1500);

        // "auto" is a request, not an outcome - mpv picks (or refuses) a
        // decoder when the video re-inits, so read hwdec-current back a beat
        // later and report what it actually settled on.
        let mpv_read = mpv_gpu.clone();
        hwdec_readout.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(800),
            move || {
                let effective = mpv_read
                    .get_property_string("hwdec-current")
                    .unwrap_or_default();
                let msg = match effective.as_str() {
                    "" | "no" => "hwdec: off (CPU decoding)".to_string(),
                    other => format!("hwdec: {other} (GPU decoding)"),
                };
                mpv_read.show_text(&msg, 3000);
            },
        );
    });

    let mpv_seek = mpv.clone();
    ui.on_seek(move |perc| {
        let scmd = CString::new("seek").unwrap();
        let sval = CString::new(perc.to_string()).unwrap();
        let smode = CString::new("absolute-percent").unwrap();
        let mut sargs = [scmd.as_ptr(), sval.as_ptr(), smode.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(mpv_seek.get(), sargs.as_mut_ptr());
        }
    });

    let mpv_skip = mpv.clone();
    ui.on_skip_by(move |secs| {
        let scmd = CString::new("seek").unwrap();
        let sval = CString::new(secs.to_string()).unwrap();
        let mut sargs = [scmd.as_ptr(), sval.as_ptr(), ptr::null()];
        unsafe {
            mpv_command(mpv_skip.get(), sargs.as_mut_ptr());
        }
    });

    let ui_next = ui.as_weak();
    let mpv_next = mpv.clone();
    let state_next = state.clone();
    ui.on_next(move || {
        {
            let mut s = state_next.lock().unwrap();
            let Some((items, idx)) = s.active_playlist.as_mut() else {
                return;
            };
            if *idx + 1 >= items.len() {
                return;
            }
            *idx += 1;
            let item = items[*idx].clone();
            s.current_title = item.name.clone();
            s.current_artist = item.series_name.clone();
            s.current_series_name = Some(item.series_name.to_string());
            s.current_season_index = Some(item.season_index);
            s.current_episode_index = Some(item.index);
            s.current_item_id = Some((item.p_id, item.item_id));
        }

        let mpv_h = mpv_next.clone();
        let ui_weak = ui_next.clone();
        let state_arc = state_next.clone();
        tokio::spawn(async move {
            open_player(ui_weak, state_arc, mpv_h).await;
        });
    });

    let ui_prev = ui.as_weak();
    let mpv_prev = mpv.clone();
    let state_prev = state.clone();
    ui.on_previous(move || {
        {
            let mut s = state_prev.lock().unwrap();
            let Some((items, idx)) = s.active_playlist.as_mut() else {
                return;
            };
            if *idx == 0 {
                return;
            }
            *idx -= 1;
            let item = items[*idx].clone();
            s.current_title = item.name.clone();
            s.current_artist = item.series_name.clone();
            s.current_series_name = Some(item.series_name.to_string());
            s.current_season_index = Some(item.season_index);
            s.current_episode_index = Some(item.index);
            s.current_item_id = Some((item.p_id, item.item_id));
        }

        let mpv_h = mpv_prev.clone();
        let ui_weak = ui_prev.clone();
        let state_arc = state_prev.clone();
        tokio::spawn(async move {
            open_player(ui_weak, state_arc, mpv_h).await;
        });
    });

    let ui_render = ui.as_weak();
    let mpv_r = mpv_render.clone();
    let mpv_h = mpv.clone();
    let state_timer = state.clone();
    let discord_timer = discord.clone();
    let last_report = Arc::new(Mutex::new(std::time::Instant::now()));
    let last_ext_update = Arc::new(Mutex::new(std::time::Instant::now()));

    let c_time = CString::new("time-pos").unwrap();
    let c_dur = CString::new("duration").unwrap();
    let c_perc = CString::new("percent-pos").unwrap();
    let c_pause = CString::new("pause").unwrap();
    let c_speed = CString::new("speed").unwrap();

    let render_timer = slint::Timer::default();
    render_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        {
            let state_arc = state_timer.clone();
            move || {
                let ui = match ui_render.upgrade() {
                    Some(ui) => ui,
                    None => return,
                };

                if let Ok(last) = last_activity.lock() {
                    let is_p = ui.get_is_paused();
                    ui.set_controls_visible(
                        last.elapsed() < std::time::Duration::from_secs(3) || is_p,
                    );
                }

                unsafe {
                    loop {
                        let ev = mpv_wait_event(mpv_h.get(), 0.0);
                        if (*ev).event_id == mpv_event_id_MPV_EVENT_NONE {
                            break;
                        }

                        if (*ev).event_id == mpv_event_id_MPV_EVENT_TRACKS_CHANGED
                            || (*ev).event_id == mpv_event_id_MPV_EVENT_FILE_LOADED
                        {
                            update_tracks(&ui, mpv_h.get());
                        }
                    }

                    if let Some(rctx) = mpv_r.borrow().as_ref()
                        && (mpv_render_context_update(rctx.get()) & 1) != 0
                    {
                        ui.window().request_redraw();
                    }

                    let mut time: f64 = 0.0;
                    let mut dur: i64 = 0;
                    let mut perc: f64 = 0.0;
                    let mut paused: c_int = 0;

                    if mpv_get_property(
                        mpv_h.get(),
                        c_perc.as_ptr(),
                        mpv_format_MPV_FORMAT_DOUBLE,
                        &mut perc as *mut _ as *mut c_void,
                    ) >= 0
                    {
                        ui.set_progress(perc as f32);
                    }

                    let got_time = mpv_get_property(
                        mpv_h.get(),
                        c_time.as_ptr(),
                        mpv_format_MPV_FORMAT_DOUBLE,
                        &mut time as *mut _ as *mut c_void,
                    ) >= 0;

                    if got_time {
                        ui.set_time_pos(format_time(time as i64).into());
                    }

                    let got_dur = mpv_get_property(
                        mpv_h.get(),
                        c_dur.as_ptr(),
                        mpv_format_MPV_FORMAT_INT64,
                        &mut dur as *mut _ as *mut c_void,
                    ) >= 0;

                    if got_dur && dur > 0 {
                        ui.set_duration(format_time(dur).into());
                    }
                    if got_time && got_dur {
                        let remaining_secs = dur - time as i64;
                        if remaining_secs >= 0 {
                            ui.set_remaining_time(
                                format!("-{}", format_time(remaining_secs)).into(),
                            );
                            let current_time = chrono::Local::now();
                            if let Some(delta) = chrono::Duration::try_seconds(remaining_secs) {
                                let end_time = current_time + delta;
                                ui.set_ends_at(
                                    format!("ends at {}", end_time.format("%-I:%M %p")).into(),
                                );
                            } else {
                                ui.set_ends_at("".into());
                            }
                        } else {
                            ui.set_remaining_time("-00:00".into());
                            ui.set_ends_at("".into());
                        }
                    } else {
                        ui.set_remaining_time("-00:00".into());
                        ui.set_ends_at("".into());
                    }

                    if mpv_get_property(
                        mpv_h.get(),
                        c_pause.as_ptr(),
                        mpv_format_MPV_FORMAT_FLAG,
                        &mut paused as *mut _ as *mut c_void,
                    ) >= 0
                    {
                        ui.set_is_paused(paused != 0);
                    }

                    let mut speed: f64 = 1.0;
                    if mpv_get_property(
                        mpv_h.get(),
                        c_speed.as_ptr(),
                        mpv_format_MPV_FORMAT_DOUBLE,
                        &mut speed as *mut _ as *mut c_void,
                    ) >= 0
                    {
                        ui.set_playback_speed(format!("{:.2}x", speed).into());
                    }

                    // Discord RPC updates, only while something is actually loaded
                    let is_in_player = state_arc.lock().unwrap().current_item_id.is_some();
                    if is_in_player {
                        if let Ok(mut last) = last_ext_update.lock()
                            && last.elapsed() >= std::time::Duration::from_millis(500)
                        {
                            *last = std::time::Instant::now();

                            let info = {
                                let s = state_arc.lock().unwrap();
                                PlaybackInfo {
                                    title: s.current_title.clone(),
                                    artist: s.current_artist.clone(),
                                    series_name: s.current_series_name.clone(),
                                    season_index: s.current_season_index,
                                    episode_index: s.current_episode_index,
                                    is_paused: paused != 0,
                                    position_secs: time as i64,
                                    duration_secs: dur,
                                }
                            };
                            discord_timer.on_playback_update(info);
                        }
                    } else {
                        if let Ok(mut last) = last_ext_update.lock()
                            && last.elapsed() >= std::time::Duration::from_secs(1)
                        {
                            *last = std::time::Instant::now();
                            discord_timer.on_playback_stop();
                        }
                    }

                    // Periodic Progress Sync to Providers (every 1 second)
                    if is_in_player
                        && got_time
                        && time > 0.0
                        && let Ok(mut last) = last_report.lock()
                        && last.elapsed() >= std::time::Duration::from_secs(1)
                    {
                        *last = std::time::Instant::now();

                        let report_data = {
                            let s = state_arc.lock().unwrap();
                            s.current_item_id.as_ref().and_then(|(p_id, id)| {
                                s.active_providers
                                    .get(p_id)
                                    .map(|p| (p.clone(), id.clone(), p_id.clone()))
                            })
                        };

                        if let Some((provider, item_id, _p_id)) = report_data {
                            let time_i64 = time as i64;
                            let is_p = paused != 0;
                            tokio::spawn(async move {
                                let _ = provider
                                    .report_playback_progress(&item_id, time_i64, is_p)
                                    .await;
                            });
                        }
                    }
                }
            }
        },
    );

    // TEMP SMOKE TEST - remove before commit.
    let smoke = std::env::var("CEDAR_SPINCARD_SMOKE").ok();
    let smoke_timer = slint::Timer::default();
    if let Some(path) = smoke {
        let mpv_s = mpv.clone();
        let ui_s = ui.as_weak();
        let tick = std::cell::Cell::new(0u32);
        smoke_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(4), move || {
            let n = tick.get() + 1;
            tick.set(n);
            let Some(ui) = ui_s.upgrade() else { return };
            unsafe {
                if n == 1 {
                    ui.set_video_title("smoke".into());
                    let k = CString::new("loop-file").unwrap();
                    let v = CString::new("inf").unwrap();
                    mpv_set_property_string(mpv_s.get(), k.as_ptr(), v.as_ptr());
                    let cmd = CString::new("loadfile").unwrap();
                    let url = CString::new(path.clone()).unwrap();
                    let mut args = [cmd.as_ptr(), url.as_ptr(), ptr::null()];
                    mpv_command(mpv_s.get(), args.as_mut_ptr());
                } else if n == 4 {
                    ui.invoke_toggle_hwdec();
                }
            }
        });
    }

    eprintln!("[CedarApple] Entering Slint event loop...");
    let result = ui.run();
    eprintln!("[CedarApple] Event loop exited.");

    MpvHandle::stop(&mpv);
    Ok(result?)
}

unsafe fn update_tracks(ui: &AppWindow, mpv_h: *mut mpv_handle) {
    let c_tracks = CString::new("track-list").unwrap();
    let tptr = unsafe { mpv_get_property_string(mpv_h, c_tracks.as_ptr()) };
    if !tptr.is_null() {
        let js = unsafe { CStr::from_ptr(tptr) }.to_string_lossy();
        if let Ok(tracks) = serde_json::from_str::<Vec<Track>>(&js) {
            let mut alist = Vec::new();
            let mut slist = Vec::new();
            let mut active_audio_name = "Default".to_string();
            let mut active_sub_name = "None".to_string();

            for t in &tracks {
                let track_info = t.as_track_info();
                if t.active {
                    match t.track_type {
                        TrackType::Audio => active_audio_name = track_info.name.to_string(),
                        TrackType::Sub => active_sub_name = track_info.name.to_string(),
                        _ => (),
                    }
                }
                match t.track_type {
                    TrackType::Audio => alist.push(track_info),
                    TrackType::Sub => slist.push(track_info),
                    _ => (),
                }
            }

            ui.set_audio_tracks(slint::ModelRc::from(std::rc::Rc::new(
                slint::VecModel::from(alist),
            )));
            ui.set_subtitle_tracks(slint::ModelRc::from(std::rc::Rc::new(
                slint::VecModel::from(slist),
            )));
            ui.set_audio_track_name(active_audio_name.into());
            ui.set_subtitle_track_name(active_sub_name.into());
        }
        unsafe { mpv_free(tptr as *mut c_void) };
    }
}

#[derive(serde::Deserialize, Debug)]
struct Track {
    id: i32,
    #[serde(rename = "type")]
    track_type: TrackType,
    #[serde(rename = "selected", default)]
    active: bool,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    codec: Option<String>,
}

impl Track {
    fn as_track_info(&self) -> TrackInfo {
        let name = self
            .title
            .as_ref()
            .or(self.lang.as_ref())
            .cloned()
            .unwrap_or_else(|| "Unknown".into());
        TrackInfo {
            active: self.active,
            id: self.id,
            name: match &self.codec {
                Some(c) if !c.is_empty() => format!("{} ({})", name, c).into(),
                _ => name.into(),
            },
        }
    }
}

#[derive(serde::Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
enum TrackType {
    Audio,
    Video,
    Sub,
    #[serde(other)]
    Other,
}

unsafe extern "C" fn get_proc_address_mpv(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe {
        let get_proc_address = &*(ctx as *const &dyn Fn(&CStr) -> *const c_void);
        let name = CStr::from_ptr(name);
        get_proc_address(name) as *mut c_void
    }
}

