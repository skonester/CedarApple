//! Exercise the adapted Lua in the real bundled mpv interpreter. Only the
//! external search process is stubbed; messages, paging and callbacks are real.
use super::*;
use libmpv_sys::*;
use std::ffi::{CStr, CString};
use std::path::Path;

struct Session {
    handle: *mut mpv_handle,
    directory: std::path::PathBuf,
    logs: Vec<String>,
}
impl Session {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("cedar-slice-lua-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let handle = unsafe { mpv_create() };
        assert!(!handle.is_null());
        for (name, value) in [
            ("vo", "null"),
            ("ao", "null"),
            ("idle", "yes"),
            ("terminal", "no"),
            ("config", "no"),
        ] {
            unsafe {
                assert!(
                    mpv_set_option_string(
                        handle,
                        CString::new(name).unwrap().as_ptr(),
                        CString::new(value).unwrap().as_ptr()
                    ) >= 0
                );
            }
        }
        unsafe {
            assert!(mpv_initialize(handle) >= 0);
            mpv_request_log_messages(handle, c"warn".as_ptr());
        }
        Self {
            handle,
            directory,
            logs: Vec::new(),
        }
    }
    fn command(&self, args: &[&str]) {
        let owned: Vec<_> = args.iter().map(|s| CString::new(*s).unwrap()).collect();
        let mut pointers: Vec<_> = owned.iter().map(|s| s.as_ptr()).collect();
        pointers.push(std::ptr::null());
        assert!(unsafe { mpv_command(self.handle, pointers.as_mut_ptr()) } >= 0);
    }
    fn send(&self, event: &str, data: Value) {
        self.command(&[
            "script-message-to",
            "cedar_slice_test",
            "slice-event",
            event,
            &data.to_string(),
        ]);
    }
    fn next(&mut self, timeout: Duration) -> Option<(String, Value)> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            let event = unsafe { &*mpv_wait_event(self.handle, 0.02) };
            if event.event_id == mpv_event_id_MPV_EVENT_LOG_MESSAGE && !event.data.is_null() {
                let log = unsafe { &*(event.data as *const mpv_event_log_message) };
                if !log.text.is_null() {
                    self.logs.push(
                        unsafe { CStr::from_ptr(log.text) }
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
            if event.event_id != mpv_event_id_MPV_EVENT_CLIENT_MESSAGE || event.data.is_null() {
                continue;
            }
            let message = unsafe { &*(event.data as *const mpv_event_client_message) };
            if message.num_args != 4 {
                continue;
            }
            let args = unsafe { std::slice::from_raw_parts(message.args, 4) };
            let args: Vec<_> = args
                .iter()
                .map(|p| unsafe { CStr::from_ptr(*p) }.to_string_lossy())
                .collect();
            if args[0] == "cedarapple-slice" {
                return Some((args[2].to_string(), serde_json::from_str(&args[3]).unwrap()));
            }
        }
        None
    }
    fn wait(&mut self, predicate: impl Fn(&str, &Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some((kind, value)) = self.next(Duration::from_millis(100)) {
                assert_ne!(kind, "error", "Lua failed: {value}; logs: {:?}", self.logs);
                if predicate(&kind, &value) {
                    return value;
                }
            }
        }
        panic!("Lua Slice did not answer; logs: {:?}", self.logs);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            mpv_terminate_destroy(self.handle);
        }
        let _ = std::fs::remove_file(self.directory.join("cedar_slice_test.lua"));
        let _ = std::fs::remove_dir(&self.directory);
    }
}

#[test]
fn lua_slice_search_paging_queue_errors_cancellation_and_shutdown() {
    let slice = manifest::read(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/mpv/slices/youtube-search/slice.json"),
    )
    .unwrap();
    let stub = r#"
local pending, next_id = {}, 0
mp.command_native_async = function(request, callback)
    next_id = next_id + 1
    local id = next_id
    local argument = request.args[#request.args]
    local count, query = argument:match("^ytsearch(%d+):(.*)$")
    emit("test-request", {argument = argument, args = request.args})
    pending[id] = mp.add_timeout(query == "slow" and 0.3 or 0.02, function()
        pending[id] = nil
        local entries = {}
        if query ~= "empty" then
            for n = 1, math.min(17, tonumber(count)) do
                entries[#entries + 1] = {id = string.format("%011d", n), title = "Video " .. n, channel = "Test channel", duration = 61}
            end
        end
        callback(query ~= "fail", {status = query == "fail" and 1 or 0, stdout = utils.format_json({entries = entries}), stderr = ""})
    end)
    return id
end
mp.abort_async_command = function(id)
    if pending[id] then pending[id]:kill(); pending[id] = nil end
end
"#;
    let loader = manifest::loader(&slice, "test").replace(
        "local ok, err = pcall(dofile, config.entry)",
        &format!("{stub}\nlocal ok, err = pcall(dofile, config.entry)"),
    );
    let mut mpv = Session::new();
    let script = mpv.directory.join("cedar_slice_test.lua");
    std::fs::write(&script, loader).unwrap();
    mpv.command(&["load-script", &script.to_string_lossy()]);
    mpv.wait(|kind, _| kind == "ready");
    mpv.send(
        "configure",
        json!({"yt_dlp_path": "a path with spaces/yt-dlp", "page_size": 8}),
    );
    mpv.send("search", json!({"query": "birds; & quotes \""}));
    let request = mpv.wait(|kind, _| kind == "test-request");
    assert_eq!(request["argument"], "ytsearch8:birds; & quotes \"");
    assert_eq!(request["args"][0], "a path with spaces/yt-dlp");
    let view = mpv.wait(|kind, value| kind == "view" && value["busy"] == false);
    let decoded: View = serde_json::from_value(view.clone()).unwrap();
    assert_eq!(decoded.results.len(), 8);
    assert_eq!(decoded.page, 1);
    mpv.send("key", json!({"key": "down", "columns": 3}));
    let view = mpv.wait(|kind, _| kind == "view");
    assert_eq!(view["selected"], 3);
    mpv.send("activate", json!({"index": 3, "append": true}));
    let play = mpv.wait(|kind, _| kind == "play");
    assert_eq!(play["id"], "00000000004");
    assert_eq!(play["append"], true);
    mpv.send("page", json!({"delta": 1}));
    let page = mpv.wait(|kind, value| kind == "view" && value["busy"] == false);
    assert_eq!(page["page"], 2);
    assert_eq!(page["results"][0]["id"], "00000000009");
    mpv.send("search", json!({"query": "empty"}));
    let empty = mpv.wait(|kind, value| kind == "view" && value["busy"] == false);
    assert!(
        serde_json::from_value::<View>(empty)
            .unwrap()
            .results
            .is_empty()
    );
    mpv.send("search", json!({"query": "fail"}));
    let failed = mpv.wait(|kind, value| kind == "view" && value["busy"] == false);
    assert!(!failed["error"].as_str().unwrap().is_empty());
    mpv.send("search", json!({"query": "slow"}));
    mpv.wait(|kind, _| kind == "test-request");
    mpv.send("close", json!({}));
    assert!(
        mpv.next(Duration::from_millis(450)).is_none(),
        "Cancelled search returned stale results"
    );
    mpv.send("search", json!({"query": "recovered"}));
    mpv.wait(|kind, value| kind == "view" && value["busy"] == false);
    mpv.send("activate", json!({"index": 0, "append": false}));
    let play = mpv.wait(|kind, _| kind == "play");
    assert_eq!(play["append"], false);
    mpv.send("shutdown", json!({}));
}
