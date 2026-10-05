use base64::Engine;
use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

pub const RCS_EXE: &str = r"D:\Riot Games\Riot Client\RiotClientServices.exe";
pub const LCU_LOCKFILE: &str = r"D:\Riot Games\League of Legends\lockfile";
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
            .tls_connector(Arc::new(tls))
            .timeout(Duration::from_secs(15))
            .build();
        let port: u16 = parts[2].parse().ok()?;
        let token = base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", parts[3]));
        Some(Api { port, base: format!("https://127.0.0.1:{port}"), auth: format!("Basic {token}"), agent })
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

fn run_hidden(exe: &str, args: &[&str]) {
    let _ = Command::new(exe).args(args).creation_flags(NO_WINDOW).spawn().and_then(|mut c| c.wait());
}

pub fn kill(images: &[&str]) {
    for img in images {
        run_hidden("taskkill", &["/F", "/T", "/IM", img]);
    }
}

pub fn start_riot_client(headless: bool) {
    let args: &[&str] = if headless { &["--headless"] } else { &["--launch-product=league_of_legends", "--launch-patchline=live"] };
    let _ = Command::new(RCS_EXE).args(args).spawn();
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
