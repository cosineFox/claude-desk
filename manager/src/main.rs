//! desk-manager: a tiny terminal manager for claude-desk. Lists every window in the desk
//! (ext-foreign-toplevel-list), previews the selected one live (sixel in foot, half-blocks elsewhere),
//! and offers desktop switch + resize. CLI subcommands for scripts; no args opens the TUI.
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use wayland_client::backend::ObjectId;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::{protocol::wl_registry, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};

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

fn desk_cmd(args: &[&str]) -> Result<String, String> {
    let bin = std::env::var("DESK_BIN").unwrap_or_else(|_| "desk".into());
    let o = Command::new(bin).args(args).output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).trim().into()) }
    else { Err(String::from_utf8_lossy(&o.stderr).trim().into()) }
}

/// PNG bytes of one window (or the whole desk when id is None), via grim.
fn grim(display: &str, id: Option<&str>) -> Result<Vec<u8>, String> {
    let mut c = Command::new("grim");
    if let Some(id) = id { c.args(["-T", id]); }
    let o = c.arg("-").env_clear()
        .env("XDG_RUNTIME_DIR", std::env::var("XDG_RUNTIME_DIR").unwrap_or_default())
        .env("WAYLAND_DISPLAY", display).stderr(Stdio::null()).output().map_err(|e| e.to_string())?;
    if o.status.success() && !o.stdout.is_empty() { Ok(o.stdout) } else { Err("capture failed".into()) }
}

/// Focus a window in the desk (labwc switches to its desktop), then open the VNC viewer on the desk.
fn view_window(display: &str, w: &Win, control: bool) -> Result<String, String> {
    let wlrctl = std::env::var("WLRCTL").unwrap_or_else(|_| "wlrctl".into());
    let o = Command::new(wlrctl).args(["toplevel", "focus", &format!("app_id:{}", w.app), &format!("title:{}", w.title)])
        .env_remove("DISPLAY").env("WAYLAND_DISPLAY", display).output().map_err(|e| e.to_string())?;
    if !o.status.success() { return Err(format!("focus failed: {}", String::from_utf8_lossy(&o.stderr).trim())); }
    let mode = std::env::var("DESK_WATCH_MODE").unwrap_or_default(); // tests: nowindow
    let args: Vec<&str> = if !mode.is_empty() { vec!["watch", &mode] } else if control { vec!["watch", "control"] } else { vec!["watch"] };
    desk_cmd(&args)
}

struct Img { w: usize, h: usize, px: Vec<u8> } // RGB

fn decode(png_bytes: &[u8]) -> Result<Img, String> {
    let mut r = png::Decoder::new(Cursor::new(png_bytes)).read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    let n = match info.color_type { png::ColorType::Rgb => 3, png::ColorType::Rgba => 4, _ => return Err("unsupported png".into()) };
    let mut px = Vec::with_capacity(w * h * 3);
    for p in buf[..w * h * n].chunks(n) { px.extend_from_slice(&p[..3]); }
    Ok(Img { w, h, px })
}

/// Box-average downscale to fit inside (mw, mh), keeping aspect. Never upscales.
fn fit(img: &Img, mw: usize, mh: usize) -> Img {
    let sc = (mw as f64 / img.w as f64).min(mh as f64 / img.h as f64).min(1.0);
    let (tw, th) = (((img.w as f64 * sc) as usize).max(1), ((img.h as f64 * sc) as usize).max(1));
    let mut px = vec![0u8; tw * th * 3];
    for y in 0..th {
        let (y0, y1) = (y * img.h / th, ((y + 1) * img.h / th).max(y * img.h / th + 1));
        for x in 0..tw {
            let (x0, x1) = (x * img.w / tw, ((x + 1) * img.w / tw).max(x * img.w / tw + 1));
            let mut s = [0u32; 3];
            let mut n = 0;
            for yy in y0..y1.min(img.h) { for xx in x0..x1.min(img.w) {
                let i = (yy * img.w + xx) * 3;
                s[0] += img.px[i] as u32; s[1] += img.px[i + 1] as u32; s[2] += img.px[i + 2] as u32; n += 1;
            } }
            let n = n.max(1);
            let o = (y * tw + x) * 3;
            px[o] = (s[0] / n) as u8; px[o + 1] = (s[1] / n) as u8; px[o + 2] = (s[2] / n) as u8;
        }
    }
    Img { w: tw, h: th, px }
}

/// Sixel with a fixed 6x6x6 palette (216 colors): simple and fast enough for previews.
fn sixel(img: &Img) -> String {
    // whole 6-row bands only: a partial last band is padded with black
    let img = &Img { w: img.w, h: (img.h - img.h % 6).max(6).min(img.h), px: img.px[..img.w * (img.h - img.h % 6).max(6).min(img.h) * 3].to_vec() };
    let lv = |v: u8| ((v as u32 * 5 + 127) / 255) as usize;
    let idx: Vec<u16> = img.px.chunks(3).map(|p| (lv(p[0]) * 36 + lv(p[1]) * 6 + lv(p[2])) as u16).collect();
    let mut out = format!("\x1bPq\"1;1;{};{}", img.w, img.h);
    let mut used = [false; 216];
    for &i in &idx { used[i as usize] = true; }
    for (i, u) in used.iter().enumerate() {
        if *u { out.push_str(&format!("#{};2;{};{};{}", i, (i / 36) * 20, ((i / 6) % 6) * 20, (i % 6) * 20)); }
    }
    for band in (0..img.h).step_by(6) {
        let mut cols: HashMap<u16, Vec<u8>> = HashMap::new();
        for dy in 0..6.min(img.h - band) {
            for x in 0..img.w {
                cols.entry(idx[(band + dy) * img.w + x]).or_insert_with(|| vec![0; img.w])[x] |= 1 << dy;
            }
        }
        let mut keys: Vec<_> = cols.keys().copied().collect();
        keys.sort();
        for (n, k) in keys.iter().enumerate() {
            out.push_str(&format!("#{}", k));
            let row = &cols[k];
            let mut x = 0;
            while x < row.len() {
                let mut run = 1;
                while x + run < row.len() && row[x + run] == row[x] { run += 1; }
                let c = (63 + row[x]) as char;
                if run > 3 { out.push_str(&format!("!{}{}", run, c)); } else { for _ in 0..run { out.push(c); } }
                x += run;
            }
            out.push(if n + 1 < keys.len() { '$' } else { '-' });
        }
    }
    out.push_str("\x1b\\");
    out
}

fn halfblocks(img: &Img, row0: usize, col0: usize) -> String {
    let mut out = String::new();
    for y in (0..img.h).step_by(2) {
        out.push_str(&format!("\x1b[{};{}H", row0 + y / 2, col0));
        for x in 0..img.w {
            let t = &img.px[(y * img.w + x) * 3..][..3];
            let b = if y + 1 < img.h { &img.px[((y + 1) * img.w + x) * 3..][..3] } else { t };
            out.push_str(&format!("\x1b[38;2;{};{};{};48;2;{};{};{}m\u{2580}", t[0], t[1], t[2], b[0], b[1], b[2]));
        }
        out.push_str("\x1b[0m");
    }
    out
}

#[derive(PartialEq, Clone, Copy)]
enum Mode { Sixel, Blocks, None }

fn preview_mode() -> Mode {
    match std::env::var("DESK_PREVIEW").as_deref() {
        Ok("sixel") => Mode::Sixel, Ok("blocks") => Mode::Blocks, Ok("none") => Mode::None,
        _ => if std::env::var("TERM").map(|t| t.starts_with("foot")).unwrap_or(false) { Mode::Sixel } else { Mode::Blocks },
    }
}

fn term_size() -> (usize, usize, usize, usize) {
    let w = rustix::termios::tcgetwinsize(std::io::stdout()).ok();
    match w {
        Some(w) if w.ws_col > 0 && w.ws_row > 0 => (w.ws_col as usize, w.ws_row as usize, w.ws_xpixel as usize, w.ws_ypixel as usize),
        _ => (80, 24, 0, 0),
    }
}

fn clip(s: &str, n: usize) -> String { s.chars().take(n).collect() }

fn tui() -> Result<(), String> {
    let display = desk_display()?;
    let stdin = std::io::stdin();
    let orig = rustix::termios::tcgetattr(&stdin).map_err(|e| e.to_string())?;
    let mut raw = orig.clone();
    raw.make_raw();
    rustix::termios::tcsetattr(&stdin, rustix::termios::OptionalActions::Now, &raw).map_err(|e| e.to_string())?;
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[?1049h\x1b[?25l");
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut b = [0u8; 16];
        while let Ok(n) = std::io::stdin().read(&mut b) { if n == 0 || tx.send(b[..n].to_vec()).is_err() { break; } }
    });
    let mode = preview_mode();
    let (mut wins, mut sel, mut msg, mut input): (Vec<Win>, usize, String, Option<String>) = (vec![], 0, String::new(), None);
    let (mut last_list, mut last_prev) = (Instant::now() - Duration::from_secs(9), Instant::now() - Duration::from_secs(9));
    let mut dirty = true;
    let mut prev_id = String::new();
    let result = loop {
        if last_list.elapsed() > Duration::from_secs(2) {
            match list_windows(&display) {
                Ok(w) => {
                    let changed = w.len() != wins.len() || w.iter().zip(&wins).any(|(a, b)| a.id != b.id || a.title != b.title);
                    if changed { wins = w; if sel >= wins.len() { sel = wins.len().saturating_sub(1); } dirty = true; }
                }
                Err(e) => { msg = e; dirty = true; }
            }
            last_list = Instant::now();
        }
        let (cols, rows, xp, yp) = term_size();
        let lw = (cols / 3).clamp(24, 40).min(cols.saturating_sub(10));
        if dirty {
            let mut s = String::from("\x1b[2J\x1b[H");
            s.push_str(&format!("\x1b[1m desk manager\x1b[0m  {} window(s)\r\n", wins.len()));
            for (i, w) in wins.iter().enumerate().take(rows.saturating_sub(5)) {
                let line = clip(&format!("{} {}", w.app, w.title), lw - 4);
                if i == sel { s.push_str(&format!("\x1b[7m {:>1} {:<w$}\x1b[0m\r\n", i + 1, line, w = lw - 4)); }
                else { s.push_str(&format!(" {:>1} {}\r\n", i + 1, line)); }
            }
            s.push_str(&format!("\x1b[{};1H\x1b[2m j/k select  enter view (VNC)  c control  1-4 desktop  r resize  s save  q quit\x1b[0m", rows - 1));
            if let Some(i) = &input { s.push_str(&format!("\x1b[{};1H resize WxH: {}_", rows, i)); } else { s.push_str(&format!("\x1b[{};1H {}", rows, clip(&msg, cols - 2))); }
            let _ = out.write_all(s.as_bytes()); let _ = out.flush();
            dirty = false; prev_id.clear();
        }
        let cur = wins.get(sel).map(|w| w.id.clone());
        if mode != Mode::None && input.is_none() && cur.is_some() && (last_prev.elapsed() > Duration::from_millis(800) || cur.as_deref() != Some(&prev_id)) {
            let id = cur.clone().unwrap();
            if let Ok(img) = grim(&display, Some(&id)).and_then(|b| decode(&b)) {
                let (pc, pr) = (cols.saturating_sub(lw + 3), rows.saturating_sub(4));
                let (cw, ch) = if xp > 0 && yp > 0 { (xp / cols, yp / rows) } else { (8, 16) };
                let s = if mode == Mode::Sixel {
                    let small = fit(&img, pc * cw, pr * ch);
                    format!("\x1b[3;{}H{}", lw + 3, sixel(&small))
                } else {
                    let small = fit(&img, pc, pr * 2);
                    halfblocks(&small, 3, lw + 3)
                };
                let _ = out.write_all(s.as_bytes()); let _ = out.flush();
            }
            prev_id = id; last_prev = Instant::now();
        }
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(k) => {
                if let Some(buf) = input.as_mut() {
                    for &c in &k {
                        match c {
                            b'\r' | b'\n' => { let sz = buf.clone(); input = None; msg = desk_cmd(&["resize", &sz]).unwrap_or_else(|e| e); break; }
                            27 => { input = None; break; }
                            127 | 8 => { buf.pop(); }
                            c if (c as char).is_ascii_graphic() && buf.len() < 11 => buf.push(c as char),
                            _ => {}
                        }
                    }
                    dirty = true; continue;
                }
                let key = if k.starts_with(b"\x1b[A") { b'k' } else if k.starts_with(b"\x1b[B") { b'j' } else { k[0] };
                match key {
                    b'q' | 3 => break Ok(()),
                    b'j' if sel + 1 < wins.len() => { sel += 1; dirty = true; }
                    b'k' if sel > 0 => { sel -= 1; dirty = true; }
                    b'1'..=b'4' => { let n = (key as char).to_string(); msg = desk_cmd(&["ws", &n]).map(|_| format!("desktop {n}")).unwrap_or_else(|e| e); dirty = true; }
                    b'\r' | b'v' | b'c' => if let Some(w) = wins.get(sel) {
                        msg = view_window(&display, w, key == b'c').unwrap_or_else(|e| e); dirty = true;
                    },
                    b'r' => { input = Some(String::new()); dirty = true; }
                    b's' => if let Some(id) = &cur {
                        let f = format!("desk-window-{}.png", sel + 1);
                        msg = match grim(&display, Some(id)).and_then(|b| std::fs::write(&f, b).map_err(|e| e.to_string())) { Ok(_) => format!("saved {f}"), Err(e) => e };
                        dirty = true;
                    },
                    _ => {}
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break Ok(()),
        }
    };
    let _ = out.write_all(b"\x1b[?25h\x1b[?1049l"); let _ = out.flush();
    let _ = rustix::termios::tcsetattr(&stdin, rustix::termios::OptionalActions::Now, &orig);
    result
}

fn pick<'a>(wins: &'a [Win], k: &str) -> Option<&'a Win> {
    k.parse::<usize>().ok().and_then(|n| wins.get(n.wrapping_sub(1))).or_else(|| wins.iter().find(|w| w.id == k))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r: Result<(), String> = (|| match args.first().map(String::as_str) {
        None | Some("tui") => {
            if rustix::termios::isatty(std::io::stdout()) { tui() } else { cli_list() }
        }
        Some("list") => cli_list(),
        Some("shot") => {
            let display = desk_display()?;
            let wins = list_windows(&display)?;
            let k = args.get(1).ok_or("usage: desk-manager shot <n|id> [file.png]")?;
            let w = pick(&wins, k).ok_or("no such window")?;
            let f = args.get(2).cloned().unwrap_or_else(|| format!("desk-window-{k}.png"));
            std::fs::write(&f, grim(&display, Some(&w.id))?).map_err(|e| e.to_string())?;
            println!("{f}"); Ok(())
        }
        Some("view") => {
            let display = desk_display()?;
            let wins = list_windows(&display)?;
            let w = pick(&wins, args.get(1).ok_or("usage: desk-manager view <n|id> [control]")?).ok_or("no such window")?;
            println!("{}", view_window(&display, w, args.get(2).map(String::as_str) == Some("control"))?); Ok(())
        }
        Some("ws") | Some("resize") => { println!("{}", desk_cmd(&args.iter().map(String::as_str).collect::<Vec<_>>())?); Ok(()) }
        _ => { println!("desk-manager [tui]            interactive manager (default in a terminal)\ndesk-manager list             windows: n, app, title, id\ndesk-manager shot <n|id> [f]  save one window as PNG\ndesk-manager view <n|id> [control]  focus a window, then open the VNC viewer\ndesk-manager ws N             go to desktop N\ndesk-manager resize WxH       resize the desk\nenv: DESK_PREVIEW=sixel|blocks|none (default: sixel in foot, else blocks)"); Ok(()) }
    })();
    if let Err(e) = r { eprintln!("desk-manager: {e}"); std::process::exit(1); }
}

fn cli_list() -> Result<(), String> {
    for (i, w) in list_windows(&desk_display()?)?.iter().enumerate() { println!("{}\t{}\t{}\t{}", i + 1, w.app, w.title, w.id); }
    Ok(())
}
