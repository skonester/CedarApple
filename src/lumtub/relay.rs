//! A loopback HTTP server libmpv reads YouTube streams through. YouTube
//! throttles a request for more than about 10 MB of a stream to a crawl,
//! and libmpv asks for the whole rest of the file at once; the relay
//! answers each libmpv request by fetching the stream in smaller pieces
//! with YouTube's own `range` parameter, as yt-dlp and the YouTube website
//! do. Only addresses the app registered are served, only on 127.0.0.1.
//!
//! Adapted from Lumtub's VOD relay. Each connection runs on its own thread;
//! CedarApple's HTTP client verifies TLS against the system certificate store.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bytes fetched from YouTube per request.
pub const CHUNK: u64 = 9 << 20;
/// Registered streams kept; older ones stop being served.
const KEEP: usize = 16;
/// Log lines kept until the app reads them.
const LOG_KEEP: usize = 200;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub url: String,
    pub size: u64,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub mime: Option<String>,
    /// `http://host:port` to fetch through, none for a direct connection.
    #[serde(default)]
    pub proxy: Option<String>,
    /// Bytes fetched from YouTube per request.
    #[serde(default = "chunk")]
    pub chunk: u64,
}

fn chunk() -> u64 {
    CHUNK
}

#[derive(Default)]
struct Shared {
    targets: HashMap<String, Target>,
    order: VecDeque<String>,
    log: VecDeque<String>,
}

pub struct Relay {
    port: u16,
    shared: Arc<Mutex<Shared>>,
}

pub trait Fetch: Send + Sync {
    fn get(&self, target: &Target, url: &str) -> Result<(u16, Box<dyn Read + Send>), String>;
}

/// Lumtub's relay transport, using CedarApple's system certificate verifier
/// and proxy preference. Clients are reused across the picture and sound.
#[derive(Default)]
pub struct Web {
    clients: Mutex<HashMap<(Option<String>, bool), reqwest::blocking::Client>>,
}

impl Web {
    fn client(&self, proxy: &Option<String>) -> Result<reqwest::blocking::Client, String> {
        let verify = crate::settings::get().network.tls_verify;
        let key = (proxy.clone(), verify);
        let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        let mut builder = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_nodelay(true)
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .tls_danger_accept_invalid_certs(!verify)
            .redirect(reqwest::redirect::Policy::limited(10));
        builder = match proxy {
            Some(p) => builder.proxy(reqwest::Proxy::all(p).map_err(|_| "Invalid HTTP proxy")?),
            None => builder.no_proxy(),
        };
        let client = builder
            .build()
            .map_err(|_| "Could not prepare stream connection")?;
        clients.insert(key, client.clone());
        Ok(client)
    }

    fn ask(&self, target: &Target, url: &str) -> Result<(u16, Box<dyn Read + Send>), String> {
        let client = self.client(&target.proxy)?;
        let mut request = client.get(url).header("Accept-Encoding", "identity");
        if let Some(agent) = &target.user_agent {
            request = request.header("User-Agent", agent);
        }
        // reqwest errors contain signed URLs: keep them out of app logs.
        let reply = request.send().map_err(|e| {
            if e.is_timeout() {
                "Stream connection timed out"
            } else {
                "Stream connection failed"
            }
            .to_string()
        })?;
        Ok((reply.status().as_u16(), Box::new(reply)))
    }
}

impl Fetch for Web {
    fn get(&self, target: &Target, url: &str) -> Result<(u16, Box<dyn Read + Send>), String> {
        let url = super::backup::steer(url);
        let answer = self.ask(target, &url);
        let status = answer.as_ref().ok().map(|(status, _)| *status);
        if super::backup::worth_backup(status)
            && let Some(other) = super::backup::backup(&url)
            && let Ok(reply) = self.ask(target, &other)
            && !super::backup::worth_backup(Some(reply.0))
        {
            let why = match &answer {
                Ok((status, _)) => format!("HTTP {status}"),
                Err(e) => e.clone(),
            };
            super::backup::avoid(&url, &other, &why);
            return Ok(reply);
        }
        answer
    }
}

impl Relay {
    /// Listens on a random loopback port.
    pub fn start(fetch: Arc<dyn Fetch>) -> std::io::Result<Relay> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let relay = Relay {
            port,
            shared: shared.clone(),
        };
        std::thread::Builder::new()
            .name("relay".into())
            .spawn(move || {
                let open = Arc::new(AtomicUsize::new(0));
                for client in listener.incoming().flatten() {
                    // libmpv keeps a few connections; anything past the cap is
                    // another local program, turned away.
                    if open.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                        continue;
                    }
                    open.fetch_add(1, Ordering::Relaxed);
                    let shared = shared.clone();
                    let fetch = fetch.clone();
                    let done = open.clone();
                    let spawned = std::thread::Builder::new()
                        .name("relay-client".into())
                        .spawn(move || {
                            serve(client, &shared, fetch.as_ref());
                            done.fetch_sub(1, Ordering::Relaxed);
                        });
                    if spawned.is_err() {
                        open.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            })?;
        Ok(relay)
    }

    /// A loopback address that serves [target], or none when its size is
    /// unknown (it is needed for seeking).
    pub fn register(&self, mut target: Target) -> Option<String> {
        if target.size == 0 || target.url.is_empty() || target.chunk == 0 {
            return None;
        }
        if target.mime.is_none() {
            target.mime = query(&target.url, "mime");
        }
        let token = token(&target.url);
        let mut shared = self.shared.lock().unwrap();
        shared.targets.insert(token.clone(), target);
        shared.order.push_back(token.clone());
        while shared.order.len() > KEEP {
            if let Some(old) = shared.order.pop_front() {
                shared.targets.remove(&old);
            }
        }
        Some(format!("http://127.0.0.1:{}/{}", self.port, token))
    }

    /// The log lines since the last call.
    pub fn take_log(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.shared.lock().unwrap().log.drain(..).collect();
        lines.extend(super::backup::take_log());
        lines
    }
}

fn log(shared: &Mutex<Shared>, line: String) {
    let mut shared = shared.lock().unwrap();
    if shared.log.len() >= LOG_KEEP {
        shared.log.pop_front();
    }
    shared.log.push_back(line);
}

fn token(url: &str) -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let mut hash = Sha256::new();
    hash.update(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_le_bytes(),
    );
    hash.update(COUNT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    hash.update(std::process::id().to_le_bytes());
    hash.update(url.as_bytes());
    hash.finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The value of [name] in [url]'s query, percent-decoded.
pub(crate) fn query(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?')?.1.split('#').next()?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| decode(value))
    })
}

pub(crate) fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                Some(b) => {
                    out.push(b);
                    i += 3;
                    continue;
                }
                None => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a `Range` header asks for out of a stream's bytes.
#[derive(Debug, PartialEq)]
pub enum Range {
    Whole,
    /// The first and last byte.
    Part(u64, u64),
    Unsatisfiable,
}

pub fn byte_range(header: Option<&str>, size: u64) -> Range {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else {
        return Range::Whole;
    };
    let Some((from, to)) = spec.split_once('-') else {
        return Range::Whole;
    };
    if !from.chars().all(|c| c.is_ascii_digit()) || !to.chars().all(|c| c.is_ascii_digit()) {
        return Range::Whole;
    }
    let from: Option<u64> = from.parse().ok();
    let to: Option<u64> = to.parse().ok();
    let (start, end) = match (from, to) {
        (None, Some(suffix)) => (size.saturating_sub(suffix), size - 1),
        (start, to) => (start.unwrap_or(0), to.map_or(size - 1, |t| t.min(size - 1))),
    };
    if start >= size || start > end {
        Range::Unsatisfiable
    } else {
        Range::Part(start, end)
    }
}

/// Answers the requests on one connection from libmpv, then closes it
/// gracefully: Windows resets a connection closed with bytes still unread,
/// and a reset can discard the end of the reply on its way to libmpv.
fn serve(client: TcpStream, shared: &Mutex<Shared>, fetch: &dyn Fetch) {
    let _ = client.set_nodelay(true);
    // A connection left idle that long holds a place under the cap; libmpv
    // simply opens another when it needs one.
    let _ = client.set_read_timeout(Some(IDLE_TIMEOUT));
    let Ok(mut writer) = client.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(client);
    answer(&mut reader, &mut writer, shared, fetch);
    let _ = writer.flush();
    let _ = writer.shutdown(Shutdown::Write);
    let _ = reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(2)));
    let mut rest = [0u8; 4096];
    while matches!(reader.read(&mut rest), Ok(n) if n > 0) {}
}

fn answer(
    reader: &mut BufReader<TcpStream>,
    writer: &mut TcpStream,
    shared: &Mutex<Shared>,
    fetch: &dyn Fetch,
) {
    let mut buffer = vec![0u8; 256 << 10];
    loop {
        let Some((method, path, headers)) = read_request(reader) else {
            return;
        };
        let keep_open = !headers
            .get("connection")
            .is_some_and(|v| v.eq_ignore_ascii_case("close"));
        let target = shared
            .lock()
            .unwrap()
            .targets
            .get(path.trim_start_matches('/'))
            .cloned();
        let Some(target) = target.filter(|_| method == "GET" || method == "HEAD") else {
            let _ = writer.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            if !keep_open {
                return;
            }
            continue;
        };
        let size = target.size;
        let (status, start, end) = match byte_range(headers.get("range").map(String::as_str), size)
        {
            Range::Whole => ("200 OK", 0, size - 1),
            Range::Part(s, e) => ("206 Partial Content", s, e),
            Range::Unsatisfiable => {
                let _ = write!(
                    writer,
                    "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\n\r\n"
                );
                if !keep_open {
                    return;
                }
                continue;
            }
        };
        // The first chunk is asked for before answering, so a refusal such as
        // an expired address (403) reaches libmpv as that status at once
        // rather than as a reply that breaks off, which it retries.
        let first = if method == "GET" {
            let to = (start + target.chunk - 1).min(end);
            match request(fetch, &target, start, to) {
                Ok(body) => Some(drain(body, to + 1 - start)),
                Err(stop) => {
                    let code = match stop {
                        Stop::Refused(code, _) => code,
                        _ => 502,
                    };
                    log(
                        shared,
                        format!("Chunked relay stopped before answering: {}", stop.why()),
                    );
                    let _ = write!(
                        writer,
                        "HTTP/1.1 {code} Upstream Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    return;
                }
            }
        } else {
            None
        };
        let mut head = format!(
            "HTTP/1.1 {status}\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\n",
            end - start + 1
        );
        if status.starts_with("206") {
            head.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
        }
        if let Some(mime) = &target.mime {
            head.push_str(&format!("Content-Type: {mime}\r\n"));
        }
        head.push_str(if keep_open {
            "Connection: keep-alive\r\n\r\n"
        } else {
            "Connection: close\r\n\r\n"
        });
        if writer.write_all(head.as_bytes()).is_err() {
            return;
        }
        if method == "HEAD" {
            if !keep_open {
                return;
            }
            continue;
        }
        let done = std::thread::scope(|scope| {
            let mut ahead = Ahead {
                scope,
                fetch,
                target: &target,
                next: None,
                handle: None,
                first,
            };
            let mut position = start;
            while position <= end {
                let last = (position + target.chunk - 1).min(end);
                ahead.next = (last < end).then(|| (last + 1, (last + target.chunk).min(end)));
                match copy(position, last, writer, shared, &mut buffer, &mut ahead) {
                    Ok(()) => position = last + 1,
                    Err(Stop::Client) => return false,
                    Err(stop) => {
                        log(shared, format!("Chunked relay stopped: {}", stop.why()));
                        return false;
                    }
                }
            }
            true
        });
        if !done {
            return;
        }
        let _ = writer.flush();
        if !keep_open {
            return;
        }
    }
}

pub(crate) type Request = (String, String, HashMap<String, String>);

/// Connections served at once.
const MAX_CONNECTIONS: usize = 64;
/// How long a connection may wait for its next request.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// The longest request or header line read.
const MAX_LINE: u64 = 8 << 10;

/// One line of at most [MAX_LINE] bytes; a longer one ends the request.
fn read_bounded_line(
    reader: &mut BufReader<TcpStream>,
    line: &mut String,
    deadline: Option<Instant>,
) -> Option<usize> {
    let mut bytes = Vec::new();
    loop {
        if let Some(deadline) = deadline {
            let left = deadline
                .checked_duration_since(Instant::now())
                .filter(|l| !l.is_zero())?;
            reader.get_ref().set_read_timeout(Some(left)).ok()?;
        }
        let room = MAX_LINE as usize - bytes.len();
        let (used, done) = match reader.fill_buf() {
            Ok([]) => break,
            Ok(buf) => {
                let window = &buf[..buf.len().min(room)];
                match window.iter().position(|&b| b == b'\n') {
                    Some(i) => {
                        bytes.extend_from_slice(&window[..=i]);
                        (i + 1, true)
                    }
                    None => {
                        bytes.extend_from_slice(window);
                        (window.len(), false)
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        reader.consume(used);
        if done {
            break;
        }
        if bytes.len() as u64 >= MAX_LINE {
            return None;
        }
    }
    let n = bytes.len();
    line.push_str(&String::from_utf8(bytes).ok()?);
    Some(n)
}

pub(crate) fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    read_request_within(reader, None)
}

/// Reads a request head that has to arrive whole within `total`, so a slow
/// client cannot hold a connection.
pub(crate) fn read_request_within(
    reader: &mut BufReader<TcpStream>,
    total: Option<Duration>,
) -> Option<Request> {
    let deadline = total.map(|t| Instant::now() + t);
    let mut line = String::new();
    let mut total = 0;
    read_bounded_line(reader, &mut line, deadline).filter(|&n| n > 0)?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        let n = read_bounded_line(reader, &mut header, deadline)?;
        total += n;
        if n == 0 || total > 32 << 10 {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Some((method, path, headers))
}

pub(crate) enum Stop {
    /// libmpv hung up, most often to seek.
    Client,
    /// YouTube's side failed.
    Upstream(String),
    /// YouTube answered with this status instead of the bytes, and what
    /// was asked and answered, for the log.
    Refused(u16, String),
}

impl Stop {
    pub(crate) fn why(&self) -> String {
        match self {
            Stop::Client => "libmpv hung up".into(),
            Stop::Upstream(message) => message.clone(),
            Stop::Refused(status, detail) if detail.is_empty() => {
                format!("YouTube answered {status}")
            }
            Stop::Refused(status, detail) => format!("YouTube answered {status} ({detail})"),
        }
    }
}

pub(crate) type Body = Box<dyn Read + Send>;

/// Bytes moved from a YouTube reply to its drained copy at a time.
const PIECE: usize = 64 << 10;

/// A YouTube reply read to its end on its own thread, at the network's
/// pace, and handed on as libmpv asks for it. Once libmpv's cache is full it
/// reads at playback speed; a reply left that long half-read is cut off
/// (every ~35 s on a user's log). At most [limit] bytes wait, about one
/// chunk, since a reply is never longer.
struct Drained {
    pieces: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
    current: Vec<u8>,
    at: usize,
}

pub(crate) fn drain(mut body: Body, limit: u64) -> Body {
    let (tx, rx) = std::sync::mpsc::sync_channel((limit / PIECE as u64 + 1) as usize);
    std::thread::spawn(move || {
        loop {
            let mut piece = vec![0; PIECE];
            match body.read(&mut piece) {
                Ok(0) => break,
                Ok(n) => {
                    piece.truncate(n);
                    // Gone when libmpv hung up; the reply is dropped with it.
                    if tx.send(Ok(piece)).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    break;
                }
            }
        }
    });
    Box::new(Drained {
        pieces: rx,
        current: Vec::new(),
        at: 0,
    })
}

impl Read for Drained {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        while self.at == self.current.len() {
            match self.pieces.recv() {
                Ok(piece) => {
                    self.current = piece?;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = buffer.len().min(self.current.len() - self.at);
        buffer[..n].copy_from_slice(&self.current[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// The following chunk's request, opened on a helper thread while the current
/// chunk is still being read.
struct Ahead<'s, 'e> {
    scope: &'s std::thread::Scope<'s, 'e>,
    fetch: &'e dyn Fetch,
    target: &'e Target,
    /// Range of the chunk after the current one, none for the last chunk.
    next: Option<(u64, u64)>,
    handle: Option<std::thread::ScopedJoinHandle<'s, Result<Body, Stop>>>,
    /// The first chunk's reply, opened before the response head was sent.
    first: Option<Body>,
}

impl<'s, 'e> Ahead<'s, 'e> {
    fn start(&mut self) {
        if let Some((from, to)) = self.next.take() {
            let (fetch, target) = (self.fetch, self.target);
            self.handle = Some(self.scope.spawn(move || {
                request(fetch, target, from, to).map(|body| drain(body, to + 1 - from))
            }));
        }
    }
}

/// Sends bytes [from] to [to] of the target. A reply YouTube cuts short is
/// continued from the next missing byte; libmpv sees one unbroken reply.
fn copy(
    from: u64,
    to: u64,
    into: &mut impl Write,
    shared: &Mutex<Shared>,
    buffer: &mut [u8],
    ahead: &mut Ahead,
) -> Result<(), Stop> {
    let (fetch, target) = (ahead.fetch, ahead.target);
    let mut next = from;
    let mut attempt = 0;
    while next <= to {
        // Only this chunk's first request was opened ahead; continuing a
        // broken reply asks anew and leaves the next chunk's request waiting.
        let pending = if attempt == 0 {
            ahead.handle.take()
        } else {
            None
        };
        let opened = if attempt == 0 {
            ahead.first.take()
        } else {
            None
        };
        let prefetched = opened.or_else(|| match pending {
            Some(handle) => match handle.join() {
                Ok(Ok(body)) => Some(body),
                Ok(Err(stop @ (Stop::Upstream(_) | Stop::Refused(..)))) => {
                    log(
                        shared,
                        format!(
                            "Prefetch of the next chunk failed ({}), requesting again",
                            stop.why()
                        ),
                    );
                    None
                }
                _ => None,
            },
            None => None,
        });
        let mut body = match prefetched {
            Some(body) => body,
            None => drain(request(fetch, target, next, to)?, to + 1 - next),
        };
        let mut received = 0u64;
        let mut failure = None;
        loop {
            match body.read(buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let wanted = (to + 1 - next - received).min(n as u64) as usize;
                    into.write_all(&buffer[..wanted])
                        .map_err(|_| Stop::Client)?;
                    received += wanted as u64;
                    if ahead.handle.is_none() && received * 4 >= (to + 1 - from) * 3 {
                        ahead.start();
                    }
                    if wanted < n {
                        // More than asked for would shift every later byte.
                        return Err(Stop::Upstream(format!(
                            "YouTube sent more than bytes {next}-{to}"
                        )));
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    failure = Some("stream read failed".to_string());
                    break;
                }
            }
        }
        next += received;
        if next <= to {
            let why = failure.unwrap_or_else(|| "ended early".into());
            if attempt >= 2 || received == 0 {
                return Err(Stop::Upstream(format!(
                    "YouTube broke off at byte {next} ({why})"
                )));
            }
            log(
                shared,
                format!("YouTube reply broke off at byte {next} ({why}), continuing from there"),
            );
        }
        attempt += 1;
    }
    Ok(())
}

pub(crate) fn request(
    fetch: &dyn Fetch,
    target: &Target,
    from: u64,
    to: u64,
) -> Result<Box<dyn Read + Send>, Stop> {
    // Appended as text: rebuilding the query could re-encode signed values.
    let url = format!("{}&range={from}-{to}", target.url);
    let mut attempt = 0;
    loop {
        match fetch.get(target, &url) {
            Ok((200 | 206, body)) => return Ok(body),
            Ok((status, body)) => {
                // A 403 means the address expired; the app reopens the video.
                if status == 403 || attempt >= 1 {
                    return Err(Stop::Refused(status, refusal(target, body)));
                }
            }
            Err(e) => {
                // A connection YouTube dropped, before any byte of this reply.
                if attempt >= 1 {
                    return Err(Stop::Upstream(e));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        attempt += 1;
    }
}

/// What was asked and what YouTube said when it refused an address: the
/// client it was made for, the server, how long it had left, whether a
/// user agent went with it and the start of the reply. No signature or
/// other value of the address is written.
fn refusal(target: &Target, body: Box<dyn Read + Send>) -> String {
    let mut said = Vec::new();
    let _ = body.take(300).read_to_end(&mut said);
    let said: String = String::from_utf8_lossy(&said)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let host = target
        .url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?']).next())
        .unwrap_or("?");
    let left = query(&target.url, "expire")
        .and_then(|e| e.parse::<i64>().ok())
        .map(|e| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            format!("{} s", e - now)
        });
    let mut parts = vec![
        format!(
            "client {}",
            query(&target.url, "c").unwrap_or_else(|| "?".into())
        ),
        format!("host {host}"),
        format!("expires in {}", left.as_deref().unwrap_or("?")),
        format!(
            "user agent {}",
            if target.user_agent.is_some() {
                "sent"
            } else {
                "none"
            }
        ),
        format!(
            "proxy {}",
            if target.proxy.is_some() { "yes" } else { "no" }
        ),
    ];
    for name in ["pot", "n", "sig", "lsig"] {
        if query(&target.url, name).is_some() {
            parts.push(format!("has {name}"));
        }
    }
    let said = said.trim();
    if !said.is_empty() {
        parts.push(format!("said \"{said}\""));
    }
    parts.join(", ")
}

static RELAY: OnceLock<Relay> = OnceLock::new();
static START: Mutex<()> = Mutex::new(());

/// A successful extraction can still contain unusable signed addresses.
/// Check a bounded media range before closing the browser to start playback.
pub fn check(url: &str, agent: &str, proxy: Option<String>) -> Result<(), String> {
    let target = Target {
        url: url.into(),
        size: 1,
        user_agent: Some(agent.into()),
        proxy,
        mime: None,
        chunk: CHUNK,
    };
    let (status, mut body) = Web::default().get(&target, &format!("{url}&range=0-0"))?;
    if !matches!(status, 200 | 206) {
        return Err(format!(
            "YouTube refused the video stream (HTTP {status}). Retry to request fresh addresses."
        ));
    }
    let mut first = [0];
    body.read_exact(&mut first)
        .map_err(|_| "YouTube returned an empty video stream".to_string())
}

pub fn report_errors() {
    if let Some(relay) = RELAY.get() {
        for line in relay.take_log() {
            eprintln!("[Lumtub] {line}");
        }
    }
}

/// Starts once and registers one seekable VOD stream, as NativeRelay does.
pub fn register(
    url: &str,
    size: Option<u64>,
    agent: &str,
    proxy: Option<String>,
) -> Result<String, String> {
    let size = size
        .or_else(|| query(url, "clen")?.parse().ok())
        .filter(|s| *s > 0);
    let Some(size) = size else {
        return Ok(url.to_owned());
    };
    let _guard = START.lock().unwrap_or_else(|e| e.into_inner());
    if RELAY.get().is_none() {
        let relay = Relay::start(Arc::new(Web::default()))
            .map_err(|_| "Could not start Lumtub's local stream relay")?;
        let _ = RELAY.set(relay);
    }
    RELAY
        .get()
        .unwrap()
        .register(Target {
            url: url.into(),
            size,
            user_agent: Some(agent.into()),
            proxy,
            mime: None,
            chunk: CHUNK,
        })
        .ok_or_else(|| "Could not register the video stream".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Serves [data] like googlevideo, cutting each reply after [cut] bytes.
    struct Fake {
        data: Vec<u8>,
        cut: Option<usize>,
        status: u16,
        calls: Mutex<Vec<String>>,
    }

    impl Fetch for Fake {
        fn get(&self, _: &Target, url: &str) -> Result<(u16, Box<dyn Read + Send>), String> {
            self.calls.lock().unwrap().push(url.to_string());
            let range = query(url, "range").unwrap();
            let (a, b) = range.split_once('-').unwrap();
            let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
            let mut slice = self.data[a..=b.min(self.data.len() - 1)].to_vec();
            if let Some(cut) = self.cut {
                slice.truncate(cut);
            }
            Ok((self.status, Box::new(Cursor::new(slice))))
        }
    }

    fn fake(size: usize, cut: Option<usize>, status: u16) -> Arc<Fake> {
        Arc::new(Fake {
            data: (0..size).map(|i| (i % 251) as u8).collect(),
            cut,
            status,
            calls: Mutex::new(vec![]),
        })
    }

    fn get(address: &str, range: Option<&str>) -> (String, Vec<u8>) {
        let path = address.trim_start_matches("http://");
        let (host, path) = path.split_once('/').unwrap();
        let mut stream = TcpStream::connect(host).unwrap();
        let range = range.map_or(String::new(), |r| format!("Range: {r}\r\n"));
        write!(
            stream,
            "GET /{path} HTTP/1.1\r\nHost: {host}\r\n{range}Connection: close\r\n\r\n"
        )
        .unwrap();
        let mut all = vec![];
        stream.read_to_end(&mut all).unwrap();
        let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (
            String::from_utf8_lossy(&all[..split]).into_owned(),
            all[split + 4..].to_vec(),
        )
    }

    fn target(size: u64) -> Target {
        Target {
            url: "https://example.com/videoplayback?mime=video%2Fwebm&clen=1".into(),
            size,
            user_agent: None,
            mime: None,
            proxy: None,
            chunk: CHUNK,
        }
    }

    #[test]
    fn reads_ranges() {
        assert_eq!(byte_range(None, 100), Range::Whole);
        assert_eq!(byte_range(Some("bytes=10-"), 100), Range::Part(10, 99));
        assert_eq!(byte_range(Some("bytes=10-200"), 100), Range::Part(10, 99));
        assert_eq!(byte_range(Some("bytes=-30"), 100), Range::Part(70, 99));
        assert_eq!(byte_range(Some("bytes=100-"), 100), Range::Unsatisfiable);
        assert_eq!(byte_range(Some("bytes=5-2"), 100), Range::Unsatisfiable);
        assert_eq!(
            query("https://x/v?a=1&mime=video%2Fmp4", "mime").as_deref(),
            Some("video/mp4")
        );
    }

    #[test]
    fn serves_a_range_in_chunks() {
        let size = (CHUNK * 2 + 1234) as usize;
        let source = fake(size, None, 200);
        let relay = Relay::start(source.clone()).unwrap();
        let address = relay.register(target(size as u64)).unwrap();
        let (head, body) = get(&address, Some("bytes=100-"));
        assert!(head.starts_with("HTTP/1.1 206"), "{head}");
        assert!(
            head.contains(&format!("Content-Range: bytes 100-{}/{size}", size - 1)),
            "{head}"
        );
        assert!(head.contains("Content-Type: video/webm"), "{head}");
        assert_eq!(body, source.data[100..]);
        let calls = source.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].ends_with(&format!("&range=100-{}", 100 + CHUNK - 1)));
    }

    #[test]
    fn drains_a_reply_before_it_is_read() {
        struct Counted(Arc<AtomicU64>, u64);
        impl Read for Counted {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let n = (self.1 - self.0.load(Ordering::SeqCst)).min(buffer.len() as u64) as usize;
                buffer[..n].fill(7);
                self.0.fetch_add(n as u64, Ordering::SeqCst);
                Ok(n)
            }
        }
        let read = Arc::new(AtomicU64::new(0));
        let size = 3 * PIECE as u64 + 5;
        let mut drained = drain(Box::new(Counted(read.clone(), size)), size);
        // Nothing has asked for a byte yet, and the reply is in already.
        for _ in 0..100 {
            if read.load(Ordering::SeqCst) == size {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(read.load(Ordering::SeqCst), size);
        let mut all = Vec::new();
        drained.read_to_end(&mut all).unwrap();
        assert_eq!(all.len() as u64, size);
        assert!(all.iter().all(|&b| b == 7));
    }

    #[test]
    fn prefetches_each_range_once() {
        let size = (CHUNK * 3 + 77) as usize;
        let source = fake(size, None, 200);
        let relay = Relay::start(source.clone()).unwrap();
        let address = relay.register(target(size as u64)).unwrap();
        let (_, body) = get(&address, None);
        assert!(body == source.data);
        let mut calls = source.calls.lock().unwrap().clone();
        calls.sort();
        let mut want: Vec<String> = (0..4u64)
            .map(|i| {
                format!(
                    "&range={}-{}",
                    i * CHUNK,
                    ((i + 1) * CHUNK - 1).min(size as u64 - 1)
                )
            })
            .collect();
        want.sort();
        assert_eq!(calls.len(), 4);
        for (c, w) in calls.iter().zip(&want) {
            assert!(c.ends_with(w), "{c} {w}");
        }
        assert!(relay.take_log().is_empty());
    }

    #[test]
    fn continues_a_cut_chunk_before_the_prefetched_one() {
        // Each reply breaks off past the point where the next chunk is
        // asked for, so the rest of a chunk must not come from that request.
        let size = (CHUNK * 2 + 10) as usize;
        let source = fake(size, Some((CHUNK * 4 / 5) as usize), 206);
        let relay = Relay::start(source.clone()).unwrap();
        let address = relay.register(target(size as u64)).unwrap();
        let (_, body) = get(&address, None);
        assert!(body == source.data);
    }

    #[test]
    fn continues_a_reply_cut_short() {
        let source = fake(1000, Some(400), 206);
        let relay = Relay::start(source.clone()).unwrap();
        let address = relay.register(target(1000)).unwrap();
        let (head, body) = get(&address, None);
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert_eq!(body, source.data);
        assert!(
            relay
                .take_log()
                .iter()
                .any(|l| l.contains("broke off at byte 400"))
        );
    }

    #[test]
    fn stops_on_403_and_unknown_addresses() {
        let source = fake(1000, None, 403);
        let relay = Relay::start(source.clone()).unwrap();
        let address = relay.register(target(1000)).unwrap();
        let (head, body) = get(&address, None);
        // Passed on as YouTube's status, so libmpv fails at once.
        assert!(head.starts_with("HTTP/1.1 403"), "{head}");
        assert!(body.is_empty());
        assert_eq!(source.calls.lock().unwrap().len(), 1);
        assert!(relay.take_log().iter().any(|l| l.contains("403")));
        let (head, _) = get(&format!("http://127.0.0.1:{}/nothing", relay.port), None);
        assert!(head.starts_with("HTTP/1.1 404"));
        let (head, _) = get(&address, Some("bytes=5000-"));
        assert!(head.starts_with("HTTP/1.1 416"), "{head}");
        assert_eq!(relay.register(target(0)), None);
    }

    #[test]
    fn a_refusal_says_what_was_asked_without_the_address() {
        let mut target = target(1000);
        target.url = "https://rr1---sn-x.googlevideo.com/videoplayback?c=TVHTML5&expire=9999999999&sig=SECRET&n=abc&clen=1".into();
        target.user_agent = Some("ua".into());
        let detail = refusal(&target, Box::new(Cursor::new(b"Forbidden\n".to_vec())));
        assert!(detail.contains("client TVHTML5"), "{detail}");
        assert!(
            detail.contains("host rr1---sn-x.googlevideo.com"),
            "{detail}"
        );
        assert!(detail.contains("user agent sent"), "{detail}");
        assert!(
            detail.contains("has sig") && detail.contains("has n") && !detail.contains("has pot"),
            "{detail}"
        );
        assert!(detail.contains("said \"Forbidden\""), "{detail}");
        assert!(
            !detail.contains("SECRET") && !detail.contains("abc"),
            "{detail}"
        );
    }

    #[test]
    fn closes_on_an_overlong_line() {
        let relay = Relay::start(fake(10, None, 200)).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", relay.port)).unwrap();
        let long = "a".repeat(MAX_LINE as usize * 2);
        let _ = write!(stream, "GET /{long} HTTP/1.1\r\n\r\n");
        let mut all = vec![];
        let _ = stream.read_to_end(&mut all);
        assert!(all.is_empty());
    }
}
