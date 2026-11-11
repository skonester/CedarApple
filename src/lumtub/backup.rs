//! A stream server YouTube names as the backup of the one in an address.
//! Every address carries its server in the host (`rr2---sn-abc.googlevideo.com`)
//! and the servers it may also be read from in `mn` (`sn-abc,sn-def`); when
//! a server answers 5xx or not at all, YouTube's own player reads from the
//! next one with `fallback_count` added. Resolving again mostly gives the
//! same server, so the relay does the same: a server that failed is passed
//! over for its backup for a while, by every stream that names it.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a failed server is passed over.
const AVOID_FOR: Duration = Duration::from_secs(10 * 60);
/// Lines kept until the relay's log is read.
const LOG_KEEP: usize = 50;

struct State {
    /// A failed host, the backup host used instead, and since when.
    avoided: Vec<(String, String, Instant)>,
    log: VecDeque<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    avoided: Vec::new(),
    log: VecDeque::new(),
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The host of [url] and where it sits in it.
fn host_of(url: &str) -> Option<(usize, &str)> {
    let start = url.find("://")? + 3;
    let rest = &url[start..];
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    Some((start, &rest[..end]))
}

fn with_host(url: &str, host: &str) -> Option<String> {
    let (start, old) = host_of(url)?;
    let mut out = format!("{}{host}{}", &url[..start], &url[start + old.len()..]);
    if !out.contains("&fallback_count=") && !out.contains("?fallback_count=") {
        out.push_str(if out.contains('?') {
            "&fallback_count=1"
        } else {
            "?fallback_count=1"
        });
    }
    Some(out)
}

/// [url] read from the backup of its server: the same replica number on
/// the next server `mn` names. None when it names no other.
pub fn backup(url: &str) -> Option<String> {
    let (_, host) = host_of(url)?;
    let (replica, rest) = host.split_once("---")?;
    let (server, domain) = rest.split_once('.')?;
    if !replica.starts_with('r') || !domain.starts_with("googlevideo.") {
        return None;
    }
    let servers = super::relay::query(url, "mn")?;
    let next = servers
        .split(',')
        .map(str::trim)
        .find(|s| !s.is_empty() && *s != server)?;
    with_host(url, &format!("{replica}---{next}.{domain}"))
}

/// [url] with its server replaced by the backup used since it failed.
pub fn steer(url: &str) -> String {
    let Some((_, host)) = host_of(url) else {
        return url.to_string();
    };
    let mut state = state();
    state
        .avoided
        .retain(|(_, _, since)| since.elapsed() < AVOID_FOR);
    match state.avoided.iter().find(|(failed, _, _)| failed == host) {
        Some((_, instead, _)) => with_host(url, instead).unwrap_or_else(|| url.to_string()),
        None => url.to_string(),
    }
}

/// Passes over the server of [failed] for that of [instead], which answered.
pub fn avoid(failed: &str, instead: &str, why: &str) {
    let (Some((_, from)), Some((_, to))) = (host_of(failed), host_of(instead)) else {
        return;
    };
    let mut state = state();
    state.avoided.retain(|(host, _, _)| host != from);
    state
        .avoided
        .push((from.to_string(), to.to_string(), Instant::now()));
    if state.log.len() >= LOG_KEEP {
        state.log.pop_front();
    }
    state.log.push_back(format!(
        "YouTube server {from} failed ({why}); reading from its backup {to}"
    ));
}

/// Whether an answer is worth trying the backup server for.
pub fn worth_backup(status: Option<u16>) -> bool {
    status.is_none_or(|s| (500..=599).contains(&s))
}

pub fn take_log() -> Vec<String> {
    state().log.drain(..).collect()
}

#[cfg(test)]
pub fn forget() {
    state().avoided.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://rr2---sn-8vq54voxpu-925e.googlevideo.com/videoplayback?expire=1&mn=sn-8vq54voxpu-925e%2Csn-4g5lznlz&sig=x";

    #[test]
    fn the_backup_is_the_next_server_named() {
        assert_eq!(
            backup(URL).unwrap(),
            "https://rr2---sn-4g5lznlz.googlevideo.com/videoplayback?expire=1&mn=sn-8vq54voxpu-925e%2Csn-4g5lznlz&sig=x&fallback_count=1"
        );
        assert_eq!(
            backup("https://rr2---sn-a.googlevideo.com/videoplayback?mn=sn-a"),
            None
        );
        assert_eq!(
            backup("https://example.com/videoplayback?mn=sn-a,sn-b"),
            None
        );
    }

    #[test]
    fn a_failed_server_is_passed_over_by_every_address_naming_it() {
        forget();
        let other = "https://rr2---sn-8vq54voxpu-925e.googlevideo.com/videoplayback?itag=140&sq=5";
        assert_eq!(steer(other), other);
        avoid(URL, &backup(URL).unwrap(), "HTTP 503");
        assert_eq!(
            steer(other),
            "https://rr2---sn-4g5lznlz.googlevideo.com/videoplayback?itag=140&sq=5&fallback_count=1"
        );
        assert!(
            take_log()
                .iter()
                .any(|l| l.contains("sn-8vq54voxpu-925e") && l.contains("503"))
        );
        forget();
    }

    #[test]
    fn only_server_errors_and_no_answer_try_the_backup() {
        assert!(worth_backup(None));
        assert!(worth_backup(Some(503)));
        assert!(!worth_backup(Some(403)));
        assert!(!worth_backup(Some(200)));
    }
}
