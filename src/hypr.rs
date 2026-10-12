//! Optional Hyprland integration via its IPC socket.
//!
//! Wayland does not let clients position their own windows, so on Hyprland we
//! ask the compositor directly to float, move, resize and pin note windows, and
//! read back where the user put them. On any other compositor this is inert.
//!
//! Since 0.55, Hyprland configured with `hyprland.lua` only accepts Lua
//! dispatchers (`hl.dsp.window.pin({ ... })`), while older versions and
//! `hyprland.conf` setups only accept the classic syntax (`pin address:0x...`).
//! We send Lua first and switch to the classic syntax for good the first time
//! Hyprland answers "Invalid dispatcher".
//!
//! Requests run on the GTK main thread, so they're short and bounded: each
//! step times out after `TIMEOUT`, and after a timeout we leave Hyprland alone
//! for `BACKOFF` so a stuck compositor can't keep freezing the notes.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

fn socket_path() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let mut candidates = Vec::new();
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        candidates.push(Path::new(&runtime).join("hypr").join(&sig).join(".socket.sock"));
    }
    candidates.push(Path::new("/tmp/hypr").join(&sig).join(".socket.sock")); // Hyprland < 0.40
    candidates.into_iter().find(|p| p.exists())
}

/// A window command, sent in whichever syntax the running Hyprland accepts.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Float(String),
    Resize(String, i32, i32),
    Move(String, i32, i32),
    Pin(String, bool),
}

impl Command {
    /// For Hyprland >= 0.55 with a Lua config.
    pub fn lua(&self) -> String {
        match self {
            Command::Float(a) => format!(r#"hl.dsp.window.float({{ window = "address:{a}", action = "enable" }})"#),
            Command::Resize(a, w, h) => {
                format!(r#"hl.dsp.window.resize({{ window = "address:{a}", x = {w}, y = {h} }})"#)
            }
            Command::Move(a, x, y) => format!(r#"hl.dsp.window.move({{ window = "address:{a}", x = {x}, y = {y} }})"#),
            Command::Pin(a, on) => {
                let action = if *on { "enable" } else { "disable" };
                format!(r#"hl.dsp.window.pin({{ window = "address:{a}", action = "{action}" }})"#)
            }
        }
    }

    /// For older Hyprland, or newer ones still using `hyprland.conf`.
    pub fn classic(&self) -> String {
        match self {
            Command::Float(a) => format!("setfloating address:{a}"),
            Command::Resize(a, w, h) => format!("resizewindowpixel exact {w} {h},address:{a}"),
            Command::Move(a, x, y) => format!("movewindowpixel exact {x} {y},address:{a}"),
            // The classic pin only toggles; callers check the current state first.
            Command::Pin(a, _) => format!("pin address:{a}"),
        }
    }
}

// 0: not known yet.
const SYNTAX_LUA: u8 = 1;
const SYNTAX_CLASSIC: u8 = 2;

const TIMEOUT: Duration = Duration::from_millis(500);
const BACKOFF: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct Hyprland {
    pub socket_path: PathBuf,
    /// Which dispatcher syntax works, shared by all clones.
    syntax: Arc<AtomicU8>,
    /// No requests until then, after one timed out. Shared by all clones.
    quiet_until: Arc<Mutex<Option<Instant>>>,
}

impl Hyprland {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Hyprland { socket_path: socket_path.into(), syntax: Arc::default(), quiet_until: Arc::default() }
    }

    pub fn detect() -> Option<Self> {
        if std::env::var_os("REALER_XPAD_NO_HYPR").is_some() {
            return None;
        }
        socket_path().map(Self::new)
    }

    pub fn request(&self, message: &str) -> io::Result<String> {
        let mut quiet = self.quiet_until.lock().unwrap_or_else(|e| e.into_inner());
        if quiet.is_some_and(|until| Instant::now() < until) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "Hyprland was unresponsive"));
        }
        let result = self.exchange(message);
        let timed_out =
            matches!(&result, Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock));
        *quiet = timed_out.then(|| Instant::now() + BACKOFF);
        result
    }

    fn exchange(&self, message: &str) -> io::Result<String> {
        let mut sock = connect(&self.socket_path, TIMEOUT)?;
        sock.set_read_timeout(Some(TIMEOUT))?;
        sock.set_write_timeout(Some(TIMEOUT))?;
        sock.write_all(message.as_bytes())?;
        let mut reply = String::new();
        sock.read_to_string(&mut reply)?;
        Ok(reply)
    }

    pub fn clients(&self) -> Vec<Value> {
        self.request("j/clients").ok().and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default()
    }

    pub fn find(&self, initial_title: &str, pid: Option<u32>) -> Option<Value> {
        Self::find_in(&self.clients(), initial_title, pid).cloned()
    }

    /// Like `find`, in a client list fetched once for several windows.
    pub fn find_in<'a>(clients: &'a [Value], initial_title: &str, pid: Option<u32>) -> Option<&'a Value> {
        clients
            .iter()
            .find(|c| c["initialTitle"] == initial_title && pid.is_none_or(|pid| c["pid"].as_u64() == Some(pid.into())))
    }

    /// Send `commands` in one request. True if Hyprland accepted all of them.
    pub fn batch(&self, commands: &[Command]) -> bool {
        if self.syntax.load(Ordering::Relaxed) != SYNTAX_CLASSIC {
            let Some(replies) = self.dispatch(commands.iter().map(Command::lua)) else { return false };
            if replies.iter().all(|r| r.starts_with("ok")) {
                self.syntax.store(SYNTAX_LUA, Ordering::Relaxed);
                return true;
            }
            let lua_unknown = replies.iter().any(|r| r.contains("Invalid dispatcher"));
            if !lua_unknown || self.syntax.load(Ordering::Relaxed) == SYNTAX_LUA {
                return false;
            }
            self.syntax.store(SYNTAX_CLASSIC, Ordering::Relaxed);
        }
        self.dispatch(commands.iter().map(Command::classic)).is_some_and(|r| r.iter().all(|r| r.starts_with("ok")))
    }

    /// One `[[BATCH]]` request; returns one reply per command.
    fn dispatch(&self, commands: impl Iterator<Item = String>) -> Option<Vec<String>> {
        let message: Vec<String> = commands.map(|c| format!("dispatch {c}")).collect();
        let reply = self.request(&format!("[[BATCH]]{}", message.join(";"))).ok()?;
        Some(reply.split("\n\n\n").map(|r| r.trim().to_string()).collect())
    }

    pub fn place(&self, address: &str, pos: Option<(i32, i32)>, width: i32, height: i32) -> bool {
        let mut commands = vec![Command::Float(address.into()), Command::Resize(address.into(), width, height)];
        if let Some((x, y)) = pos {
            commands.push(Command::Move(address.into(), x, y));
        }
        self.batch(&commands)
    }

    pub fn set_pinned(&self, client: &Value, pinned: bool) -> bool {
        if client["pinned"].as_bool().unwrap_or(false) == pinned {
            return true;
        }
        let address = client["address"].as_str().unwrap_or_default().to_string();
        let mut commands = vec![Command::Pin(address.clone(), pinned)];
        if pinned && !client["floating"].as_bool().unwrap_or(false) {
            commands.insert(0, Command::Float(address));
        }
        self.batch(&commands)
    }

    pub fn geometry(client: &Value) -> (i32, i32, i32, i32) {
        let n = |v: &Value| v.as_i64().unwrap_or(0) as i32;
        (n(&client["at"][0]), n(&client["at"][1]), n(&client["size"][0]), n(&client["size"][1]))
    }
}

/// `UnixStream::connect` can block indefinitely when Hyprland isn't accepting
/// connections, so connect on a helper thread and stop waiting after `timeout`.
fn connect(path: &Path, timeout: Duration) -> io::Result<UnixStream> {
    let (tx, rx) = mpsc::channel();
    let path = path.to_path_buf();
    std::thread::spawn(move || tx.send(UnixStream::connect(path)));
    rx.recv_timeout(timeout).map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connecting to Hyprland"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::Mutex;

    /// Minimal stand-in for Hyprland's request socket. `lua` picks how it
    /// answers dispatches, like Hyprland >= 0.55 with a Lua config (`true`) or
    /// an older Hyprland / `hyprland.conf` setup (`false`).
    struct Fake {
        requests: Arc<Mutex<Vec<String>>>,
        _dir: tempfile::TempDir,
        hypr: Hyprland,
    }

    fn client() -> Value {
        serde_json::json!({"address": "0xabc", "pid": 42, "initialTitle": "realer-xpad-1", "at": [100, 200],
                           "size": [300, 250], "floating": false, "pinned": false})
    }

    /// Hyprland's reply to one `dispatch` (see `dispatchRequest` in Hyprland's HyprCtl.cpp).
    fn reply_to(dispatch: &str, lua: bool) -> String {
        let cmd = dispatch.trim().trim_start_matches("dispatch ");
        match (lua, cmd.starts_with("hl.")) {
            _ if cmd.contains("0xdead") => "window not found".into(),
            (true, true) | (false, false) => "ok".into(),
            (true, false) => "[string \"return hl.dispatch(...)\"]: syntax error\n\n → Note: dispatch in lua is \
                              a shorthand for hl.dispatch(...), your syntax might need to be updated."
                .into(),
            (false, true) => "Invalid dispatcher".into(),
        }
    }

    fn fake(lua: bool) -> Fake {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".socket.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        let mut other = client();
        other["address"] = "0xdef".into();
        other["initialTitle"] = "other".into();
        let clients = serde_json::to_string(&[client(), other]).unwrap();
        std::thread::spawn(move || {
            for mut conn in listener.incoming().flatten() {
                let mut buf = [0u8; 65536];
                let n = conn.read(&mut buf).unwrap_or(0);
                let msg = String::from_utf8_lossy(&buf[..n]).to_string();
                let reply = match msg.strip_prefix("[[BATCH]]") {
                    _ if msg == "j/clients" => clients.clone(),
                    Some(batch) => batch.split(';').map(|d| reply_to(d, lua)).collect::<Vec<_>>().join("\n\n\n"),
                    None => reply_to(&msg, lua),
                };
                log.lock().unwrap().push(msg);
                let _ = conn.write_all(reply.as_bytes());
            }
        });
        Fake { requests, hypr: Hyprland::new(path), _dir: dir }
    }

    impl Fake {
        fn sent(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[test]
    fn a_stuck_hyprland_costs_one_timeout_then_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".socket.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let held = Arc::new(Mutex::new(Vec::new()));
        let keep = held.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                keep.lock().unwrap().push(conn); // accept, never answer
            }
        });
        let hypr = Hyprland::new(path);
        let start = Instant::now();
        assert!(hypr.clients().is_empty());
        assert!(start.elapsed() < TIMEOUT * 3, "bounded by the timeout: {:?}", start.elapsed());
        let start = Instant::now();
        assert!(!hypr.clone().place("0xabc", None, 300, 250));
        assert!(start.elapsed() < Duration::from_millis(50), "no second wait during the backoff");
        assert_eq!(held.lock().unwrap().len(), 1, "no second connection");
    }

    #[test]
    fn find_by_initial_title() {
        let f = fake(true);
        assert_eq!(f.hypr.find("realer-xpad-1", None).unwrap()["address"], "0xabc");
        assert!(f.hypr.find("realer-xpad-1", Some(7)).is_none());
        assert!(f.hypr.find("missing", None).is_none());
        assert_eq!(Hyprland::geometry(&f.hypr.find("realer-xpad-1", Some(42)).unwrap()), (100, 200, 300, 250));
    }

    #[test]
    fn place_sends_lua_dispatchers() {
        let f = fake(true);
        assert!(f.hypr.place("0xabc", Some((10, 20)), 300, 250));
        assert_eq!(
            f.sent(),
            [
                r#"[[BATCH]]dispatch hl.dsp.window.float({ window = "address:0xabc", action = "enable" });dispatch hl.dsp.window.resize({ window = "address:0xabc", x = 300, y = 250 });dispatch hl.dsp.window.move({ window = "address:0xabc", x = 10, y = 20 })"#
            ]
        );
    }

    #[test]
    fn falls_back_to_classic_dispatchers_once() {
        let f = fake(false);
        assert!(f.hypr.place("0xabc", Some((10, 20)), 300, 250));
        assert_eq!(f.sent().len(), 2, "Lua first, then the classic syntax");
        assert_eq!(
            f.sent()[1],
            "[[BATCH]]dispatch setfloating address:0xabc;\
             dispatch resizewindowpixel exact 300 250,address:0xabc;\
             dispatch movewindowpixel exact 10 20,address:0xabc"
        );
        assert!(f.hypr.clone().place("0xabc", None, 300, 250), "clones share what was learned");
        assert_eq!(f.sent().len(), 3, "no more Lua attempts");
        assert!(!f.sent()[2].contains("hl.dsp") && !f.sent()[2].contains("movewindowpixel"));
    }

    #[test]
    fn failures_do_not_switch_syntax() {
        let f = fake(true);
        assert!(!f.hypr.place("0xdead", None, 300, 250));
        assert_eq!(f.sent().len(), 1, "a missing window is not a reason to try the classic syntax");
        assert!(f.hypr.place("0xabc", None, 300, 250));
        assert!(f.sent()[1].contains("hl.dsp.window.float"));
    }

    #[test]
    fn pin_floats_first_and_is_idempotent() {
        let f = fake(true);
        assert!(f.hypr.set_pinned(&client(), true));
        assert_eq!(
            f.sent().last().unwrap(),
            r#"[[BATCH]]dispatch hl.dsp.window.float({ window = "address:0xabc", action = "enable" });dispatch hl.dsp.window.pin({ window = "address:0xabc", action = "enable" })"#
        );
        let n = f.sent().len();
        assert!(f.hypr.set_pinned(&client(), false)); // already unpinned: no request
        assert_eq!(f.sent().len(), n);

        let f = fake(false);
        assert!(f.hypr.set_pinned(&client(), true));
        assert_eq!(f.sent().last().unwrap(), "[[BATCH]]dispatch setfloating address:0xabc;dispatch pin address:0xabc");
    }

    #[test]
    fn dead_socket_is_harmless() {
        let dir = tempfile::tempdir().unwrap();
        let h = Hyprland::new(dir.path().join("nope.sock"));
        assert!(h.clients().is_empty());
        assert!(!h.batch(&[Command::Pin("0xabc".into(), true)]));
    }
}
