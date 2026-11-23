//! Loopback JSON-lines control server: one request per line, one reply per line.
//! This is the transport the MCP server (`cadcraft-cli mcp --connect`) wraps.
//!
//! The server only reads requests: a line that is not a JSON object with a string `method` (an
//! HTTP request line, a stray `nc`, binary data) gets one error reply and the connection is
//! closed, so nothing sent after it runs. Every resource a client can hold is bounded
//! ([`Limits`]): simultaneous connections, request line length, queued requests, and how long a
//! socket may sit idle or blocked on a write.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::time::Duration;

use cadcraft_ui_egui::ControlRequest;
use serde_json::{Value, json};

/// Resource bounds for the control server.
#[derive(Clone, Copy, Debug)]
struct Limits {
    /// Connections served at once; more get an error line and are closed.
    max_clients: usize,
    /// Longest request line in bytes (newline excluded); a longer one closes the connection.
    max_line: usize,
    /// Requests waiting for the app at once; more are answered `busy`.
    queue: usize,
    /// How long a connection may go without sending a byte before it is closed.
    idle_timeout: Duration,
    /// How long a reply write may block before the connection is closed.
    write_timeout: Duration,
    /// How long a request waits for the app's answer.
    reply_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_clients: 16,
            // Generous, so requests that carry whole drawings or images as base64 still fit.
            max_line: 256 * 1024 * 1024,
            queue: 64,
            idle_timeout: Duration::from_secs(600),
            write_timeout: Duration::from_secs(30),
            reply_timeout: Duration::from_secs(60),
        }
    }
}

pub fn start(port: u16, ctx: egui::Context) -> Receiver<ControlRequest> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => {
            log::info!("control server listening on 127.0.0.1:{port}");
            serve_listener(l, Limits::default(), ctx)
        }
        Err(e) => {
            log::error!("control server failed to bind 127.0.0.1:{port}: {e}");
            sync_channel::<ControlRequest>(0).1
        }
    }
}

/// Accept connections on `listener` under `limits`; requests arrive on the returned receiver.
fn serve_listener(listener: TcpListener, limits: Limits, ctx: egui::Context) -> Receiver<ControlRequest> {
    let (tx, rx) = sync_channel::<ControlRequest>(limits.queue.max(1));
    std::thread::spawn(move || accept_loop(listener, tx, ctx, limits));
    rx
}

/// A connection's place among [`Limits::max_clients`], given back when the connection ends (also
/// when its thread can't be started, or panics).
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept_loop(listener: TcpListener, tx: SyncSender<ControlRequest>, ctx: egui::Context, limits: Limits) {
    let open = Arc::new(AtomicUsize::new(0));
    for mut stream in listener.incoming().flatten() {
        let _ = stream.set_write_timeout(Some(limits.write_timeout));
        if open.fetch_add(1, Ordering::SeqCst) >= limits.max_clients {
            open.fetch_sub(1, Ordering::SeqCst);
            let _ = writeln!(stream, "{}", json!({"ok": false, "error": "too many control connections"}));
            continue;
        }
        let slot = Slot(Arc::clone(&open));
        let tx = tx.clone();
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().spawn(move || {
            let _slot = slot;
            serve(stream, &tx, &ctx, limits);
        });
    }
}

/// Read one `\n`-terminated line of at most `max` bytes into `buf` (newline stripped).
/// `Ok(false)` at end of stream; `Err` when the line is too long or the read fails.
fn read_bounded_line(reader: &mut impl BufRead, buf: &mut Vec<u8>, max: usize) -> Result<bool, String> {
    buf.clear();
    // `take` bounds the bytes buffered for one line: a peer can't make us allocate more than `max`.
    let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    let n = reader.take(limit).read_until(b'\n', buf).map_err(|e| match e.kind() {
        // The idle timeout ran out (Windows reports it as TimedOut, Unix as WouldBlock).
        ErrorKind::TimedOut | ErrorKind::WouldBlock => "request line not completed in time; closing the connection".to_string(),
        _ => e.to_string(),
    })?;
    if n == 0 {
        return Ok(false);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        return Ok(true);
    }
    if buf.len() > max {
        return Err(format!("request line longer than {max} bytes; closing the connection"));
    }
    // Last line without a newline, then end of stream.
    Ok(true)
}

/// One request line, parsed.
enum Line {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Blank,
    /// Not a request: reply with this error and close the connection.
    Reject(String),
}

fn parse_line(bytes: &[u8]) -> Line {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Line::Reject("not a control request (invalid UTF-8); closing the connection".into());
    };
    let text = text.trim();
    if text.is_empty() {
        return Line::Blank;
    }
    if is_http_request_line(text) {
        return Line::Reject("HTTP is not served here: this port takes JSON lines; closing the connection".into());
    }
    let msg = match serde_json::from_str::<Value>(text) {
        Ok(v) => v,
        Err(e) => return Line::Reject(format!("bad JSON ({e}); expected one JSON request object per line; closing the connection")),
    };
    let Some(obj) = msg.as_object() else {
        return Line::Reject("not a control request (expected a JSON object); closing the connection".into());
    };
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return Line::Reject("not a control request (missing string `method`); closing the connection".into());
    };
    Line::Request {
        id: obj.get("id").cloned().unwrap_or(Value::Null),
        method: method.to_string(),
        params: obj.get("params").cloned().unwrap_or(json!({})),
    }
}

/// An HTTP/1 request line (`POST / HTTP/1.1`) or the HTTP/2 preface (`PRI * HTTP/2.0`).
fn is_http_request_line(text: &str) -> bool {
    const METHODS: [&str; 10] = ["GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH", "PRI"];
    text.split_once(' ').is_some_and(|(method, _)| METHODS.contains(&method))
        || text.rsplit_once(' ').is_some_and(|(_, version)| version.starts_with("HTTP/"))
}

fn serve(stream: TcpStream, tx: &SyncSender<ControlRequest>, ctx: &egui::Context, limits: Limits) {
    if stream.set_read_timeout(Some(limits.idle_timeout)).is_err() || stream.set_write_timeout(Some(limits.write_timeout)).is_err() {
        return;
    }
    let Ok(read) = stream.try_clone() else { return };
    let mut out = stream;
    let mut reader = BufReader::new(read);
    let mut buf = Vec::new();
    loop {
        let reject = match read_bounded_line(&mut reader, &mut buf, limits.max_line) {
            Ok(false) => break,
            // Nothing of a line arrived (idle timeout, or the peer is gone): close without a word.
            // A line nobody asked for would be read as the reply to the client's next request.
            Err(_) if buf.is_empty() => break,
            Err(e) => e,
            Ok(true) => match parse_line(&buf) {
                Line::Blank => continue,
                Line::Reject(e) => e,
                Line::Request { id, method, params } => {
                    let (req, pending) = ControlRequest::new(method, params);
                    let mut r = match tx.try_send(req) {
                        Ok(()) => {
                            ctx.request_repaint();
                            // On timeout the request is cancelled if the app hasn't started it, and
                            // the reply's `state` says whether it can still have run.
                            pending.wait(limits.reply_timeout)
                        }
                        Err(TrySendError::Full(_)) => json!({"ok": false, "error": "busy: too many queued requests", "state": "not-run"}),
                        Err(TrySendError::Disconnected(_)) => break,
                    };
                    if let Some(o) = r.as_object_mut() {
                        o.insert("id".into(), id);
                    }
                    if writeln!(out, "{r}").is_err() {
                        break;
                    }
                    continue;
                }
            },
        };
        // The connection closes either way; a peer that can't read the reason loses nothing.
        let _ = writeln!(out, "{}", json!({"ok": false, "error": reject}));
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_read_up_to_the_limit() {
        let mut buf = Vec::new();
        let mut input: &[u8] = b"aaaaaaaaaa\nbb\r\ncc";
        assert_eq!(read_bounded_line(&mut input, &mut buf, 10), Ok(true));
        assert_eq!(buf, b"aaaaaaaaaa", "a line exactly at the limit is accepted");
        assert_eq!(read_bounded_line(&mut input, &mut buf, 10), Ok(true));
        assert_eq!(buf, b"bb", "CRLF is stripped");
        assert_eq!(read_bounded_line(&mut input, &mut buf, 10), Ok(true));
        assert_eq!(buf, b"cc", "a last line without a newline is still read");
        assert_eq!(read_bounded_line(&mut input, &mut buf, 10), Ok(false), "then end of stream");
    }

    #[test]
    fn an_overlong_line_is_refused_without_reading_the_rest() {
        let mut buf = Vec::new();
        let long = [b'x'; 100];
        let mut input: &[u8] = &long;
        assert!(read_bounded_line(&mut input, &mut buf, 10).is_err());
        assert!(buf.len() <= 11, "at most limit + 1 bytes are buffered, got {}", buf.len());
        assert_eq!(input.len(), 100 - 11, "the rest of the line is left unread");
    }

    fn short() -> Limits {
        Limits {
            queue: 1,
            idle_timeout: Duration::from_millis(300),
            write_timeout: Duration::from_millis(300),
            reply_timeout: Duration::from_millis(300),
            ..Limits::default()
        }
    }

    /// The control server on a loopback port under `limits`; requests arrive on the receiver.
    fn serve_with(limits: Limits) -> (std::net::SocketAddr, Receiver<ControlRequest>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        (addr, serve_listener(listener, limits, egui::Context::default()))
    }

    /// The control server on a loopback port, with a stand-in UI that echoes each method.
    fn serve_echo() -> std::net::SocketAddr {
        let (addr, rx) = serve_with(Limits::default());
        std::thread::spawn(move || {
            for req in rx {
                let _ = req.reply.send(json!({"ok": true, "method": req.method}));
            }
        });
        addr
    }

    /// Send `input` on a new connection and read reply lines until the server closes it.
    fn exchange(addr: std::net::SocketAddr, input: &[u8]) -> Vec<Value> {
        let mut c = TcpStream::connect(addr).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(input).unwrap();
        let mut replies = Vec::new();
        for line in BufReader::new(c).lines() {
            // A read timeout here means the server kept the connection open.
            let line = line.unwrap();
            replies.push(serde_json::from_str(&line).unwrap());
        }
        replies
    }

    /// A browser can POST to the port: its request line closes the connection before the JSON
    /// line in its body is read, so nothing is dispatched.
    #[test]
    fn an_http_request_closes_the_connection_before_its_body_is_dispatched() {
        let (addr, rx) = serve_with(short());
        let http = b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: text/plain\r\n\r\n\
            {\"id\":1,\"method\":\"cmdline.input\",\"params\":{\"text\":\"circle 0,0 5\"}}\n";
        let replies = exchange(addr, http);
        assert_eq!(replies.len(), 1, "{replies:?}");
        assert_eq!(replies[0]["ok"], false);
        assert!(replies[0]["error"].as_str().unwrap().contains("closing the connection"), "{replies:?}");
        assert!(rx.try_recv().is_err(), "the body was dispatched");
    }

    #[test]
    fn a_line_that_is_not_a_request_closes_the_connection() {
        for junk in [&b"hello\n"[..], b"[1,2]\n", b"{\"id\":1}\n", b"{\"method\":5}\n", b"\xff\xfe\n", b"GET /x HTTP/1.1\n"] {
            let addr = serve_echo();
            let mut input = junk.to_vec();
            input.extend_from_slice(b"{\"id\":2,\"method\":\"ping\"}\n");
            let replies = exchange(addr, &input);
            assert_eq!(replies.len(), 1, "{replies:?}");
            assert_eq!(replies[0]["ok"], false);
        }
        assert!(is_http_request_line("OPTIONS * HTTP/1.1"));
        assert!(is_http_request_line("PRI * HTTP/2.0"));
        assert!(!is_http_request_line("{\"method\":\"GET /\"}"));
    }

    #[test]
    fn an_idle_connection_is_closed() {
        let (addr, _rx) = serve_with(short());
        let mut c = TcpStream::connect(addr).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let t = std::time::Instant::now();
        let mut got = Vec::new();
        c.read_to_end(&mut got).unwrap();
        assert!(got.is_empty(), "{}", String::from_utf8_lossy(&got));
        assert!(t.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn a_full_queue_answers_busy() {
        let (addr, rx) = serve_with(short());
        // No UI drains the one-place queue: the first request fills it, the second finds it full.
        let first = std::thread::spawn(move || exchange(addr, b"{\"id\":1,\"method\":\"m\"}\n"));
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let busy = loop {
            let mut c = TcpStream::connect(addr).unwrap();
            c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            c.write_all(b"{\"id\":2,\"method\":\"m\"}\n").unwrap();
            let mut line = String::new();
            BufReader::new(c).read_line(&mut line).unwrap();
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["error"].as_str().is_some_and(|e| e.starts_with("busy")) || std::time::Instant::now() > deadline {
                break v;
            }
        };
        assert_eq!((busy["id"].as_i64(), busy["state"].as_str()), (Some(2), Some("not-run")), "{busy}");
        assert_eq!(first.join().unwrap()[0]["state"], "not-run");
        drop(rx);
    }

    #[test]
    fn requests_are_answered_with_their_ids() {
        let addr = serve_echo();
        let mut c = TcpStream::connect(addr).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(b"{\"id\":3,\"method\":\"ping\"}\r\n\n{\"id\":4,\"method\":\"pong\"}\n").unwrap();
        let mut r = BufReader::new(c);
        for (id, method) in [(3, "ping"), (4, "pong")] {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let v: Value = serde_json::from_str(&line).unwrap();
            assert_eq!((v["id"].as_i64(), v["method"].as_str()), (Some(id), Some(method)), "{line}");
        }
    }

    #[test]
    fn extra_connections_are_refused() {
        let addr = serve_echo();
        // Idle connections are accepted in order and held open.
        let held: Vec<TcpStream> = (0..Limits::default().max_clients).map(|_| TcpStream::connect(addr).unwrap()).collect();
        let mut extra = TcpStream::connect(addr).unwrap();
        extra.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut line = String::new();
        BufReader::new(&mut extra).read_line(&mut line).unwrap();
        assert!(line.contains("too many control connections"), "{line}");
        drop(held);
    }
}
