#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod api;
mod app;
mod app_state;
mod backend;
mod discord;
mod extensions;
mod image_cache;
mod history;
mod navigation;
mod net_route;
mod player;
mod playback;
mod scrub;
mod torrent;
mod torrent_storage;
mod torrent_ui;
mod ui;

use app_state::AppState;
use backend::local::LocalProvider;
use discord::DiscordRPC;
use glow::HasContext;
use libmpv_sys::*;
use player::{GLResources, MpvHandle, MpvRenderCtx, configure_hardware_acceleration};
use slint::{BorrowedOpenGLTextureBuilder, BorrowedOpenGLTextureOrigin, ComponentHandle};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
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

    // A torrent session holds a descriptor per open file; on macOS the default
    // limit is low enough to matter (see torrent_storage.rs).
    if let Err(e) = librqbit::try_increase_nofile_limit() {
        eprintln!("[CedarApple] could not raise the open-file limit: {e:#}");
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

    // Everything between the window and mpv: controls, the torrent flow,
    // the queue, history. See src/playback.rs.
    let timers = playback::install(&ui, mpv.clone(), state.clone(), discord.clone());

    // Rendering needs a fast tick; playback readouts are polled at 10 Hz
    // inside `playback::tick`.
    let ui_render = ui.as_weak();
    let mpv_r = mpv_render.clone();
    let render_timer = slint::Timer::default();
    render_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        move || {
            let Some(ui) = ui_render.upgrade() else { return };
            playback::tick(&ui);
            if let Some(rctx) = mpv_r.borrow().as_ref()
                && (unsafe { mpv_render_context_update(rctx.get()) } & 1) != 0
            {
                ui.window().request_redraw();
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
        smoke_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(4),
            move || {
                let n = tick.get() + 1;
                tick.set(n);
                let Some(ui) = ui_s.upgrade() else { return };
                unsafe {
                    if n == 1 {
                        ui.set_has_file(true);
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
            },
        );
    }

    eprintln!("[CedarApple] Entering Slint event loop...");
    let result = ui.run();
    eprintln!("[CedarApple] Event loop exited.");
    playback::shutdown();
    drop(timers);

    MpvHandle::stop(&mpv);
    Ok(result?)
}

unsafe extern "C" fn get_proc_address_mpv(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe {
        let get_proc_address = &*(ctx as *const &dyn Fn(&CStr) -> *const c_void);
        let name = CStr::from_ptr(name);
        get_proc_address(name) as *mut c_void
    }
}
