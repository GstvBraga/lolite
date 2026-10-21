use base64::Engine;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// (League folder, RiotClientServices.exe), set once at startup and when changed in options.
static PATHS: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

/// Empty overrides fall back to what the Riot Client installer recorded, then to the default C: install.
pub fn set_paths(league_dir: &str, riot_client: &str) {
    let installs: Value = std::fs::read_to_string(format!(r"{}\Riot Games\RiotClientInstalls.json", std::env::var("PROGRAMDATA").unwrap_or_default()))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let league = match league_dir {
        "" => installs["associated_client"]
            .as_object()
            .and_then(|m| m.keys().find(|k| k.contains("League of Legends")).cloned())
            .unwrap_or(r"C:\Riot Games\League of Legends".into()),
        d => d.into(),
    };
    let rc = match riot_client {
        "" => installs["rc_default"].as_str().unwrap_or(r"C:\Riot Games\Riot Client\RiotClientServices.exe").into(),
        r => r.into(),
    };
    *PATHS.lock().unwrap() = (league, rc);
}

pub fn paths() -> (String, String) {
    PATHS.lock().unwrap().clone()
}

pub fn lcu_lockfile() -> String {
    std::path::Path::new(&paths().0).join("lockfile").to_string_lossy().into()
}

const NO_WINDOW: u32 = 0x0800_0000;

pub fn rc_lockfile() -> String {
    format!(r"{}\Riot Games\Riot Client\Config\lockfile", std::env::var("LOCALAPPDATA").unwrap_or_default())
}

/// Local API of Riot Client or League Client, read from its lockfile (`name:pid:port:password:proto`).
#[derive(Clone)]
pub struct Api {
    port: u16,
    base: String,
    auth: String,
    agent: ureq::Agent,
    tls: native_tls::TlsConnector,
}

impl Api {
    pub fn from_lockfile(path: &str) -> Option<Api> {
        let text = std::fs::read_to_string(path).ok()?;
        let parts: Vec<&str> = text.trim().split(':').collect();
        if parts.len() < 4 {
            return None;
        }
        // Riot uses a self-signed cert on 127.0.0.1.
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .ok()?;
        let agent = ureq::AgentBuilder::new()
            .tls_connector(Arc::new(tls.clone()))
            .timeout(Duration::from_secs(15))
            .build();
        let port: u16 = parts[2].parse().ok()?;
        let token = base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", parts[3]));
        Some(Api { port, base: format!("https://127.0.0.1:{port}"), auth: format!("Basic {token}"), agent, tls })
    }

    /// Subscribes to the client's WebSocket and sends the uri of every API event starting with one of `watch` to `tx`.
    /// Sends `CLOSED` when the connection ends. Returns false if the connection could not be opened.
    /// Minimal WebSocket client (handshake, text frames, ping/pong): all this needs, without another dependency.
    pub fn events(&self, watch: &'static [&'static str], tx: Sender<String>) -> bool {
        let Ok(tcp) = TcpStream::connect(("127.0.0.1", self.port)) else { return false };
        let Ok(mut ws) = self.tls.connect("127.0.0.1", tcp) else { return false };
        let req = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: wamp\r\nAuthorization: {}\r\n\r\n",
            self.port, self.auth
        );
        if ws.write_all(req.as_bytes()).is_err() {
            return false;
        }
        let mut head = Vec::new();
        let mut b = [0u8];
        while !head.ends_with(b"\r\n\r\n") {
            if ws.read_exact(&mut b).is_err() || head.len() > 8192 {
                return false;
            }
            head.push(b[0]);
        }
        if !head.starts_with(b"HTTP/1.1 101") || send_frame(&mut ws, 1, br#"[5,"OnJsonApiEvent"]"#).is_err() {
            return false;
        }
        std::thread::spawn(move || {
            let mut msg = Vec::new();
            while let Ok((fin, op, data)) = read_frame(&mut ws) {
                match op {
                    0 | 1 => msg.extend_from_slice(&data),
                    8 => break,
                    9 => {
                        let _ = send_frame(&mut ws, 10, &data);
                        continue;
                    }
                    _ => continue,
                }
                if !fin {
                    continue;
                }
                // Event: [8, "OnJsonApiEvent", {"uri": ..., "data": ..., "eventType": ...}]
                let uri = serde_json::from_slice::<Value>(&msg).ok().and_then(|v| v[2]["uri"].as_str().map(String::from));
                msg.clear();
                if let Some(uri) = uri.filter(|u| watch.iter().any(|w| u.starts_with(w)))
                    && tx.send(uri).is_err()
                {
                    return;
                }
            }
            let _ = tx.send(CLOSED.into());
        });
        true
    }

    /// Identifies this client instance (port changes when it restarts).
    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn get(&self, path: &str) -> Option<Value> {
        let resp = self.agent.get(&format!("{}{path}", self.base)).set("Authorization", &self.auth).call().ok()?;
        resp.into_json().ok()
    }

    /// Fast check for a stale lockfile: Windows takes ~2s to refuse a closed localhost port.
    pub fn alive(&self) -> bool {
        std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], self.port).into(), Duration::from_millis(300)).is_ok()
    }

    /// HTTP status of a GET, or None if nothing answered.
    pub fn probe(&self, path: &str) -> Option<u16> {
        if !self.alive() {
            return None;
        }
        match self.agent.get(&format!("{}{path}", self.base)).set("Authorization", &self.auth).call() {
            Ok(r) => Some(r.status()),
            Err(ureq::Error::Status(code, _)) => Some(code),
            Err(_) => None,
        }
    }

    pub fn send(&self, method: &str, path: &str, body: Option<Value>) -> Result<(), String> {
        let req = self.agent.request(method, &format!("{}{path}", self.base)).set("Authorization", &self.auth);
        let res = match body {
            Some(b) => req.send_json(b),
            None => req.set("Content-Length", "0").call(),
        };
        match res {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(code, r)) => Err(format!("{method} {path}: {code} {}", r.into_string().unwrap_or_default())),
            Err(e) => Err(format!("{method} {path}: {e}")),
        }
    }
}

pub const CLOSED: &str = "!closed";

type Tls = native_tls::TlsStream<TcpStream>;

/// Returns (final fragment, opcode, payload). Server frames are never masked.
fn read_frame(ws: &mut Tls) -> std::io::Result<(bool, u8, Vec<u8>)> {
    let mut h = [0u8; 2];
    ws.read_exact(&mut h)?;
    let len = match h[1] & 0x7F {
        126 => {
            let mut n = [0u8; 2];
            ws.read_exact(&mut n)?;
            u16::from_be_bytes(n) as usize
        }
        127 => {
            let mut n = [0u8; 8];
            ws.read_exact(&mut n)?;
            u64::from_be_bytes(n) as usize
        }
        n => n as usize,
    };
    let mut data = vec![0u8; len];
    ws.read_exact(&mut data)?;
    Ok((h[0] & 0x80 != 0, h[0] & 0x0F, data))
}

/// Client frames must be masked; the key doesn't matter for a local server.
fn send_frame(ws: &mut Tls, op: u8, data: &[u8]) -> std::io::Result<()> {
    const KEY: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
    let mut f = vec![0x80 | op];
    match data.len() {
        n if n < 126 => f.push(0x80 | n as u8),
        n if n <= 0xFFFF => {
            f.push(0x80 | 126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(0x80 | 127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(&KEY);
    f.extend(data.iter().enumerate().map(|(i, b)| b ^ KEY[i % 4]));
    ws.write_all(&f)
}

fn run_hidden(exe: &str, args: &[&str]) {
    let _ = Command::new(exe).args(args).creation_flags(NO_WINDOW).spawn().and_then(|mut c| c.wait());
}

#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn IsWindowVisible(hwnd: isize) -> i32;
}

/// Whether a visible top-level window matches `class` and/or `title`. A few microseconds, no process spawn.
pub fn window_shown(class: Option<&str>, title: Option<&str>) -> bool {
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (c, t) = (class.map(wide), title.map(wide));
    let ptr = |v: &Option<Vec<u16>>| v.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
    // SAFETY: both pointers are null or NUL-terminated buffers that outlive the call.
    unsafe {
        let hwnd = FindWindowW(ptr(&c), ptr(&t));
        hwnd != 0 && IsWindowVisible(hwnd) != 0
    }
}

pub fn kill(images: &[&str]) {
    for img in images {
        run_hidden("taskkill", &["/F", "/T", "/IM", img]);
    }
}

pub fn start_riot_client(headless: bool) {
    let args: &[&str] = if headless { &["--headless"] } else { &["--launch-product=league_of_legends", "--launch-patchline=live"] };
    let _ = Command::new(paths().1).args(args).spawn();
}

pub const RIOT_ELECTRON: &str = "Riot Client.exe";

const RIOT_PROCS: &[&str] = &[
    "League of Legends.exe",
    "LeagueClientUxRender.exe",
    "LeagueClientUx.exe",
    "LeagueClient.exe",
    "LeagueCrashHandler64.exe",
    "Riot Client.exe",
    "RiotClientUx.exe",
    "RiotClientUxRender.exe",
    "RiotClientServices.exe",
    "RiotClientCrashHandler.exe",
];

pub fn kill_riot(vanguard: bool) {
    kill(RIOT_PROCS);
    if vanguard {
        // vgc is a service and vgtray runs elevated, so this needs a UAC prompt.
        // vgk (kernel driver) stays loaded until reboot.
        run_hidden(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Start-Process cmd -Verb RunAs -WindowStyle Hidden -ArgumentList '/c sc stop vgc & taskkill /F /IM vgtray.exe'",
            ],
        );
    }
}
