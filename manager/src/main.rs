//! desk-manager: a tiny local web viewer for claude-desk. No async runtime, no HTTP crate.
//! Lists every window in the desk (ext-foreign-toplevel-list), serves a live PNG per window
//! (via `grim -T`), and offers desktop switch + resize. Loopback only.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use wayland_client::backend::ObjectId;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::{protocol::wl_registry, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};

const PAGE: &str = include_str!("page.html");

#[derive(Default, Clone)]
struct Win { id: String, title: String, app: String }

#[derive(Default)]
struct State { wins: HashMap<ObjectId, Win>, order: Vec<ObjectId> }

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(s: &mut Self, _: &ExtForeignToplevelListV1, e: ext_foreign_toplevel_list_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = e {
            s.order.push(toplevel.id());
            s.wins.insert(toplevel.id(), Win::default());
        }
    }
    wayland_client::event_created_child!(State, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ())
    ]);
}
impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(s: &mut Self, h: &ExtForeignToplevelHandleV1, e: ext_foreign_toplevel_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use ext_foreign_toplevel_handle_v1::Event::*;
        let id = h.id();
        match e {
            Identifier { identifier } => { if let Some(w) = s.wins.get_mut(&id) { w.id = identifier } }
            Title { title } => { if let Some(w) = s.wins.get_mut(&id) { w.title = title } }
            AppId { app_id } => { if let Some(w) = s.wins.get_mut(&id) { w.app = app_id } }
            Closed => { s.wins.remove(&id); s.order.retain(|o| o != &id); }
            _ => {}
        }
    }
}

fn state_dir() -> PathBuf {
    if let Ok(s) = std::env::var("DESK_STATE") { return s.into(); }
    let base = std::env::var("XDG_STATE_HOME").map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state"));
    base.join("claude-desk")
}

fn desk_display() -> Result<String, String> {
    std::fs::read_to_string(state_dir().join("display")).map(|s| s.trim().to_string())
        .map_err(|_| "desk is not running (desk start)".into())
}

/// Connect to the nested desk (not the host session) and read its windows.
fn list_windows(display: &str) -> Result<Vec<Win>, String> {
    let rt = std::env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR unset")?;
    let stream = std::os::unix::net::UnixStream::connect(PathBuf::from(rt).join(display)).map_err(|e| e.to_string())?;
    let conn = Connection::from_socket(stream).map_err(|e| e.to_string())?;
    let (gl, mut q) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
    let qh = q.handle();
    let _list: ExtForeignToplevelListV1 = gl.bind(&qh, 1..=1, ()).map_err(|_| "compositor lacks ext-foreign-toplevel-list")?;
    let mut st = State::default();
    q.roundtrip(&mut st).map_err(|e| e.to_string())?;
    q.roundtrip(&mut st).map_err(|e| e.to_string())?;
    Ok(st.order.iter().filter_map(|id| st.wins.get(id).cloned()).filter(|w| !w.id.is_empty()).collect())
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

fn esc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), c if (c as u32) < 0x20 => o.push(' '), c => o.push(c) }
    }
    o
}

fn desk_cmd(args: &[&str]) -> Result<String, String> {
    let bin = std::env::var("DESK_BIN").unwrap_or_else(|_| "desk".into());
    let o = Command::new(bin).args(args).output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).into()) }
    else { Err(String::from_utf8_lossy(&o.stderr).into()) }
}

fn grim(display: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let o = Command::new("grim").args(args).arg("-")
        .env_clear()
        .env("XDG_RUNTIME_DIR", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default())
        .env("WAYLAND_DISPLAY", display).stderr(Stdio::null()).output().map_err(|e| e.to_string())?;
    if o.status.success() && !o.stdout.is_empty() { Ok(o.stdout) } else { Err("capture failed".into()) }
}

fn respond(s: &mut TcpStream, code: &str, ctype: &str, body: &[u8]) {
    let h = format!("HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    let _ = s.write_all(h.as_bytes());
    let _ = s.write_all(body);
}

fn handle(mut s: TcpStream, port: u16) {
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut line = String::new();
    if r.read_line(&mut line).is_err() { return; }
    let (mut host_ok, mut xdesk) = (false, false);
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" { break; }
        let l = h.to_ascii_lowercase();
        if let Some(v) = l.strip_prefix("host:") { let v = v.trim(); host_ok = v == format!("127.0.0.1:{port}") || v == format!("localhost:{port}"); }
        if l.starts_with("x-desk:") { xdesk = true; }
    }
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    if !host_ok { return respond(&mut s, "403 Forbidden", "text/plain", b"bad host"); }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let q = |k: &str| query.split('&').find_map(|p| p.strip_prefix(&format!("{k}=")).map(String::from)).unwrap_or_default();
    match (method, path) {
        ("GET", "/") => respond(&mut s, "200 OK", "text/html; charset=utf-8", PAGE.as_bytes()),
        ("GET", "/api/windows") => match desk_display().and_then(|d| list_windows(&d)) {
            Ok(w) => {
                let items: Vec<String> = w.iter().map(|w| format!("{{\"id\":\"{}\",\"title\":\"{}\",\"app\":\"{}\"}}", esc(&w.id), esc(&w.title), esc(&w.app))).collect();
                respond(&mut s, "200 OK", "application/json", format!("[{}]", items.join(",")).as_bytes())
            }
            Err(e) => respond(&mut s, "503 Service Unavailable", "text/plain", e.as_bytes()),
        },
        ("GET", "/screen.png") => match desk_display().and_then(|d| grim(&d, &[])) {
            Ok(b) => respond(&mut s, "200 OK", "image/png", &b),
            Err(e) => respond(&mut s, "503 Service Unavailable", "text/plain", e.as_bytes()),
        },
        ("GET", "/window.png") => {
            let id = q("id");
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c)) {
                return respond(&mut s, "400 Bad Request", "text/plain", b"bad id");
            }
            match desk_display().and_then(|d| grim(&d, &["-T", &id])) {
                Ok(b) => respond(&mut s, "200 OK", "image/png", &b),
                Err(e) => respond(&mut s, "503 Service Unavailable", "text/plain", e.as_bytes()),
            }
        }
        ("POST", "/api/ws") | ("POST", "/api/resize") if !xdesk => respond(&mut s, "403 Forbidden", "text/plain", b"missing X-Desk header"),
        ("POST", "/api/ws") => {
            let n = q("n");
            if !["1", "2", "3", "4"].contains(&n.as_str()) { return respond(&mut s, "400 Bad Request", "text/plain", b"n must be 1-4"); }
            match desk_cmd(&["ws", &n]) { Ok(_) => respond(&mut s, "200 OK", "text/plain", b"ok"), Err(e) => respond(&mut s, "500 Internal Server Error", "text/plain", e.as_bytes()) }
        }
        ("POST", "/api/resize") => {
            let sz = q("size");
            let ok = sz.split_once('x').map(|(a, b)| a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok() && sz.len() <= 11).unwrap_or(false);
            if !ok { return respond(&mut s, "400 Bad Request", "text/plain", b"size like 1280x720"); }
            match desk_cmd(&["resize", &sz]) { Ok(m) => respond(&mut s, "200 OK", "text/plain", m.as_bytes()), Err(e) => respond(&mut s, "500 Internal Server Error", "text/plain", e.as_bytes()) }
        }
        _ => respond(&mut s, "404 Not Found", "text/plain", b"not found"),
    }
}

fn main() {
    let port: u16 = std::env::var("DESK_MANAGER_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5920);
    let l = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| { eprintln!("desk-manager: cannot bind 127.0.0.1:{port}: {e}"); std::process::exit(1) });
    eprintln!("desk-manager on http://127.0.0.1:{port}");
    for c in l.incoming().flatten() {
        std::thread::spawn(move || handle(c, port));
    }
}
