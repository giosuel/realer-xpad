//! Optional Hyprland integration via its IPC socket.
//!
//! Wayland does not let clients position their own windows, so on Hyprland we
//! ask the compositor directly to float, move, resize and pin note windows, and
//! read back where the user put them. On any other compositor this is inert.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

#[derive(Clone, Debug)]
pub struct Hyprland {
    pub socket_path: PathBuf,
}

impl Hyprland {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Hyprland { socket_path: socket_path.into() }
    }

    pub fn detect() -> Option<Self> {
        if std::env::var_os("REALER_XPAD_NO_HYPR").is_some() {
            return None;
        }
        socket_path().map(Self::new)
    }

    pub fn request(&self, message: &str) -> std::io::Result<String> {
        let mut sock = UnixStream::connect(&self.socket_path)?;
        sock.set_read_timeout(Some(Duration::from_secs(1)))?;
        sock.set_write_timeout(Some(Duration::from_secs(1)))?;
        sock.write_all(message.as_bytes())?;
        let mut reply = String::new();
        sock.read_to_string(&mut reply)?;
        Ok(reply)
    }

    pub fn clients(&self) -> Vec<Value> {
        self.request("j/clients").ok().and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default()
    }

    pub fn find(&self, initial_title: &str, pid: Option<u32>) -> Option<Value> {
        self.clients()
            .into_iter()
            .find(|c| c["initialTitle"] == initial_title && pid.is_none_or(|pid| c["pid"].as_u64() == Some(pid.into())))
    }

    pub fn batch(&self, commands: &[String]) -> bool {
        let message: Vec<String> = commands.iter().map(|c| format!("dispatch {c}")).collect();
        self.request(&format!("[[BATCH]]{}", message.join(";"))).is_ok_and(|r| r.contains("ok"))
    }

    pub fn place(&self, address: &str, pos: Option<(i32, i32)>, width: i32, height: i32) -> bool {
        let target = format!("address:{address}");
        let mut commands =
            vec![format!("setfloating {target}"), format!("resizewindowpixel exact {width} {height},{target}")];
        if let Some((x, y)) = pos {
            commands.push(format!("movewindowpixel exact {x} {y},{target}"));
        }
        self.batch(&commands)
    }

    pub fn set_pinned(&self, client: &Value, pinned: bool) -> bool {
        if client["pinned"].as_bool().unwrap_or(false) == pinned {
            return true;
        }
        let target = format!("address:{}", client["address"].as_str().unwrap_or_default());
        let mut commands = vec![format!("pin {target}")];
        if pinned && !client["floating"].as_bool().unwrap_or(false) {
            commands.insert(0, format!("setfloating {target}"));
        }
        self.batch(&commands)
    }

    pub fn geometry(client: &Value) -> (i32, i32, i32, i32) {
        let n = |v: &Value| v.as_i64().unwrap_or(0) as i32;
        (n(&client["at"][0]), n(&client["at"][1]), n(&client["size"][0]), n(&client["size"][1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, Mutex};

    /// Minimal stand-in for Hyprland's request socket.
    struct Fake {
        requests: Arc<Mutex<Vec<String>>>,
        _dir: tempfile::TempDir,
        hypr: Hyprland,
    }

    fn client() -> Value {
        serde_json::json!({"address": "0xabc", "pid": 42, "initialTitle": "realer-xpad-1", "at": [100, 200],
                           "size": [300, 250], "floating": false, "pinned": false})
    }

    fn fake() -> Fake {
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
                let reply = if msg == "j/clients" { clients.clone() } else { "ok".into() };
                log.lock().unwrap().push(msg);
                let _ = conn.write_all(reply.as_bytes());
            }
        });
        Fake { requests, hypr: Hyprland::new(path), _dir: dir }
    }

    #[test]
    fn find_by_initial_title() {
        let f = fake();
        assert_eq!(f.hypr.find("realer-xpad-1", None).unwrap()["address"], "0xabc");
        assert!(f.hypr.find("realer-xpad-1", Some(7)).is_none());
        assert!(f.hypr.find("missing", None).is_none());
        assert_eq!(Hyprland::geometry(&f.hypr.find("realer-xpad-1", Some(42)).unwrap()), (100, 200, 300, 250));
    }

    #[test]
    fn place_sends_batch() {
        let f = fake();
        assert!(f.hypr.place("0xabc", Some((10, 20)), 300, 250));
        assert_eq!(
            f.requests.lock().unwrap().last().unwrap(),
            "[[BATCH]]dispatch setfloating address:0xabc;\
             dispatch resizewindowpixel exact 300 250,address:0xabc;\
             dispatch movewindowpixel exact 10 20,address:0xabc"
        );
    }

    #[test]
    fn place_without_position_only_resizes() {
        let f = fake();
        f.hypr.place("0xabc", None, 300, 250);
        assert!(!f.requests.lock().unwrap().last().unwrap().contains("movewindowpixel"));
    }

    #[test]
    fn pin_floats_first_and_is_idempotent() {
        let f = fake();
        assert!(f.hypr.set_pinned(&client(), true));
        assert_eq!(
            f.requests.lock().unwrap().last().unwrap(),
            "[[BATCH]]dispatch setfloating address:0xabc;dispatch pin address:0xabc"
        );
        let n = f.requests.lock().unwrap().len();
        assert!(f.hypr.set_pinned(&client(), false)); // already unpinned: no request
        assert_eq!(f.requests.lock().unwrap().len(), n);
    }

    #[test]
    fn dead_socket_is_harmless() {
        let dir = tempfile::tempdir().unwrap();
        let h = Hyprland::new(dir.path().join("nope.sock"));
        assert!(h.clients().is_empty());
        assert!(!h.batch(&["pin".into()]));
    }
}
