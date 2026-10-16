//! The anonymous usage ping (Swift UsagePingTests, plus the daemon-side timer and URL rules).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use cleat_rs::usage_ping::{
    body, device_id, is_due, resolved_url, send, CurlPost, Pinger, PingTransport, ENDPOINT, INTERVAL,
};

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, String)>>);

impl PingTransport for Recorder {
    fn post(&self, url: &str, body: &str) {
        self.0.lock().unwrap().push((url.into(), body.into()));
    }
}

fn dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("cleat-usageping-{tag}-{}-{nanos}", std::process::id()))
}

fn looks_like_uuid(s: &str) -> bool {
    s == s.to_lowercase() && s.len() == 36 && s.matches('-').count() == 4
}

#[test]
fn off_sends_nothing_and_makes_no_device_file() {
    let d = dir("off");
    let rec = Recorder::default();
    assert!(!send(false, "http://127.0.0.1:9/ping", &d.join("device"), "1.2.3", &rec));
    assert!(rec.0.lock().unwrap().is_empty());
    assert!(!d.exists());
}

#[test]
fn request_carries_the_three_fields_and_nothing_else() {
    let d = dir("body");
    let rec = Recorder::default();
    assert!(send(true, "http://127.0.0.1:9/ping", &d.join("device"), "1.2.3", &rec));
    let sent = rec.0.lock().unwrap();
    assert_eq!(sent.len(), 1);
    let (url, text) = &sent[0];
    assert_eq!(url, "http://127.0.0.1:9/ping");
    let device = std::fs::read_to_string(d.join("device")).unwrap();
    assert!(looks_like_uuid(&device), "{device}");
    // Keys sorted, as the Swift build's .sortedKeys writes them.
    assert_eq!(text, &format!(r#"{{"device":"{device}","product":"cleat","version":"1.2.3"}}"#));
    assert_eq!(text, &body(&device, "1.2.3"));
    assert!(text.len() <= 1024);
    let args = CurlPost::args(url).join(" ");
    assert!(!args.to_lowercase().contains("authorization"), "{args}");
    assert!(args.contains("--max-time 20"));
}

#[test]
fn device_id_is_kept_and_a_file_that_is_not_a_uuid_is_replaced() {
    let d = dir("id");
    let file = d.join("device");
    let first = device_id(&file).unwrap();
    assert_eq!(device_id(&file).as_deref(), Some(first.as_str()));

    std::fs::write(&file, "not a uuid").unwrap();
    let second = device_id(&file).unwrap();
    assert!(looks_like_uuid(&second));
    assert_ne!(second, first);
    assert_eq!(device_id(&file).as_deref(), Some(second.as_str()));

    // A stored upper-case id is kept, lower-cased.
    std::fs::write(&file, "ABCDEF01-2345-4678-9ABC-DEF012345678\n").unwrap();
    assert_eq!(device_id(&file).as_deref(), Some("abcdef01-2345-4678-9abc-def012345678"));
}

#[test]
fn no_ping_when_the_device_id_cannot_be_kept() {
    let d = dir("stuck");
    // A directory sits where the file should be, so the id can be neither read nor written.
    std::fs::create_dir_all(d.join("device")).unwrap();
    let rec = Recorder::default();
    assert!(!send(true, "http://127.0.0.1:9/ping", &d.join("device"), "1", &rec));
    assert!(rec.0.lock().unwrap().is_empty());
}

#[test]
fn is_due_boundaries() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    assert!(is_due(None, now));
    assert!(!is_due(Some(now - (INTERVAL - Duration::from_secs(60))), now));
    assert!(is_due(Some(now - INTERVAL), now));
    // A clock set back does not make it due.
    assert!(!is_due(Some(now + Duration::from_secs(60)), now));
}

#[test]
fn a_release_build_ignores_the_override_and_a_dev_build_needs_one() {
    let stand_in = "http://127.0.0.1:9/api/v1/ping";
    assert_eq!(resolved_url(false, Some(stand_in)).as_deref(), Some(ENDPOINT));
    assert_eq!(resolved_url(false, None).as_deref(), Some(ENDPOINT));
    assert_eq!(resolved_url(true, Some(stand_in)).as_deref(), Some(stand_in));
    assert_eq!(resolved_url(true, None), None);
    assert_eq!(resolved_url(true, Some("")), None);
}

fn wait_for(rec: &Recorder, n: usize) {
    let end = Instant::now() + Duration::from_secs(5);
    while rec.0.lock().unwrap().len() < n && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_pinger_sends_once_on_apply_and_off_or_no_url_sends_nothing() {
    let d = dir("pinger");
    let rec = Arc::new(Recorder::default());
    let mut on = Pinger::new(Some("http://127.0.0.1:9/p".into()), d.join("device"), "9.9".into(), rec.clone());
    on.apply(true);
    on.apply(true); // a second apply does not start a second worker
    wait_for(&rec, 1);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rec.0.lock().unwrap().len(), 1);
    on.apply(false);

    let d2 = dir("pinger-off");
    let quiet = Arc::new(Recorder::default());
    let mut off = Pinger::new(Some("http://127.0.0.1:9/p".into()), d2.join("device"), "9.9".into(), quiet.clone());
    off.apply(false);
    let mut no_url = Pinger::new(None, d2.join("device"), "9.9".into(), quiet.clone());
    no_url.apply(true);
    std::thread::sleep(Duration::from_millis(100));
    assert!(quiet.0.lock().unwrap().is_empty());
    assert!(!d2.exists());
}

/// A `header = "Authorization: ..."` line in the user's ~/.curlrc must not reach the server. Runs
/// the very command `CurlPost::post` runs (`CurlPost::command`), with HOME and CURL_HOME on a
/// temp dir that holds such a .curlrc, against a stand-in server on a random local port.
#[test]
fn curl_ignores_the_users_curlrc() {
    use std::io::{Read, Write};
    use std::process::Stdio;

    let home = dir("curlrc");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join(".curlrc"), "header = \"Authorization: Bearer leaked\"\n").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/api/v1/ping", listener.local_addr().unwrap().port());
    let server = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 1024];
        while !String::from_utf8_lossy(&seen).contains("\"version\":\"1.2.3\"}") {
            match conn.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => seen.extend_from_slice(&buf[..n]),
            }
        }
        let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        String::from_utf8_lossy(&seen).into_owned()
    });

    let mut command = CurlPost::command(&url);
    command.env("HOME", &home).env("CURL_HOME", &home).stdin(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let text = body("abcdef01-2345-4678-9abc-def012345678", "1.2.3");
    child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    child.wait().unwrap();

    let request = server.join().unwrap();
    assert!(request.contains("Content-Type: application/json"), "{request}");
    assert!(request.contains(&text), "{request}");
    assert!(!request.to_lowercase().contains("authorization"), "{request}");
}
