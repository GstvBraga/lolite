#![windows_subsystem = "windows"]
mod fancy;
mod i18n;
mod riot;
mod social;

use eframe::egui;
use i18n::tr;
use riot::Api;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Shared {
    status: String,
    error: String,
    lcu: Option<Api>,
    phase: String,
    summoner: String,
    /// (id, name, group)
    queues: Vec<(i64, String, &'static str)>,
    champs: Vec<(i64, String)>,
    /// (id, name, game modes it's allowed in)
    spells: Vec<(i64, String, Vec<String>)>,
    queue_modes: HashMap<i64, String>,
    queue_maps: HashMap<i64, i64>,
    /// (id, name, valid)
    rune_pages: Vec<(i64, String, bool)>,
    rune_current: i64,
    /// Riot's recommended pages for my champ/role: (label, page body)
    recommended: Vec<(String, Value)>,
    lobby: Value,
    search: Value,
    ready: Value,
    session: Value,
    pickable: Vec<i64>,
    invites: Vec<Value>,
    friends: Vec<Value>,
    /// Direct-message conversation id (= friend pid) -> (sender name, unread count).
    unread: HashMap<String, (String, i64)>,
    my_pid: String,
    /// Friend pid whose chat is open; empty when none.
    chat_with: String,
    /// Display name for `chat_with`, for chats with people not in the friend list.
    chat_name: String,
    messages: Vec<Value>,
    /// When `search`/`ready` last changed; queue and ready-check timers count on from there between events.
    timer_at: Option<Instant>,
    /// Wakes the poller for an immediate refresh (value is an event uri, "" = game state).
    poke: Option<Sender<String>>,
    auto_launch: bool,
    auto_accept: bool,
    want_ux: bool,
    need_login: bool,
    /// Auto pick: up to 5 champions per lane (order of `LANES`), in priority order; all empty = off.
    auto_picks: Picks,
}

type S = Arc<Mutex<Shared>>;

/// Arena "Bravery": lock a random champion for bonus gold.
const BRAVERY: i64 = -3;
const ROLES: &[&str] = &["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY", "FILL"];
const LANES: [&str; 5] = ["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY"];
type Picks = [Vec<i64>; 5];

/// Re-reads the lockfile each tick but only rebuilds the client when it changes.
#[derive(Default)]
struct Cached(String, Option<Api>);

impl Cached {
    fn get(&mut self, path: &str) -> Option<Api> {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if text != self.0 {
            self.1 = Api::from_lockfile(path);
            self.0 = text;
        }
        self.1.clone()
    }
}

fn ids(v: Option<Value>) -> Vec<i64> {
    v.and_then(|v| v.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect())).unwrap_or_default()
}

fn queue_group(q: &Value) -> &'static str {
    if q["category"] == "VersusAi" {
        return "Co-op vs AI";
    }
    match q["gameMode"].as_str().unwrap_or("") {
        "CLASSIC" | "SWIFTPLAY" => "Summoner's Rift",
        "ARAM" | "KIWI" => "ARAM",
        "CHERRY" => "Arena",
        "TFT" => "Teamfight Tactics",
        _ => "Featured",
    }
}

fn coop_prefix() -> &'static str {
    tr("Co-op vs AI: ", "Cooperativo vs IA: ")
}

fn load_static(api: &Api, s: &S) {
    let all_queues = api.get("/lol-game-queues/v1/queues").and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let queue_modes = all_queues.iter().filter_map(|q| Some((q["id"].as_i64()?, q["gameMode"].as_str()?.to_string()))).collect();
    let queue_maps = all_queues.iter().filter_map(|q| Some((q["id"].as_i64()?, q["mapId"].as_i64()?))).collect();
    let mut queues: Vec<(i64, String, &'static str)> = all_queues
        .iter()
        .filter(|q| {
            q["queueAvailability"] == "Available"
                && q["isCustom"] == false
                && (q["category"] == "PvP" || q["category"] == "VersusAi")
                && !q["gameMode"].as_str().unwrap_or("").starts_with("TUTORIAL")
        })
        .filter_map(|q| {
            // TFT descriptions are bare ("Normal", "Ranked"); its `name` is the readable one.
            let key = if q["gameMode"] == "TFT" { "name" } else { "description" };
            let id = q["id"].as_i64()?;
            let name = i18n::queue(id).or(q[key].as_str()).filter(|n| !n.is_empty())?;
            let name = if q["category"] == "VersusAi" { format!("{}{name}", coop_prefix()) } else { name.to_string() };
            Some((id, name, queue_group(q)))
        })
        .collect();
    queues.sort_by(|a, b| a.1.cmp(&b.1));

    let mut champs: Vec<(i64, String)> = api
        .get("/lol-game-data/assets/v1/champion-summary.json")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|c| Some((c["id"].as_i64().filter(|id| *id > 0)?, c["name"].as_str()?.to_string())))
        .collect();
    // By name, then id, so a champion's mode variants come right after it (see `base_champs`).
    champs.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    let mut spells: Vec<(i64, String, Vec<String>)> = api
        .get("/lol-game-data/assets/v1/summoner-spells.json")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|sp| {
            let modes: Vec<String> = sp["gameModes"].as_array()?.iter().filter_map(|m| m.as_str().map(String::from)).collect();
            Some((sp["id"].as_i64()?, sp["name"].as_str().filter(|n| !n.is_empty())?.to_string(), modes))
        })
        .collect();
    spells.sort_by(|a, b| a.1.cmp(&b.1));

    let summoner = api
        .get("/lol-summoner/v1/current-summoner")
        .map(|v| format!("{}#{}", v["gameName"].as_str().unwrap_or("?"), v["tagLine"].as_str().unwrap_or("")))
        .unwrap_or_default();
    let my_pid = api.get("/lol-chat/v1/me").and_then(|v| v["pid"].as_str().map(String::from)).unwrap_or_default();
    if setting("lang").is_none() {
        let locale = api.get("/riotclient/region-locale").and_then(|v| v["locale"].as_str().map(String::from)).unwrap_or_default();
        i18n::set_pt(locale.starts_with("pt"));
    }

    let mut g = s.lock().unwrap();
    g.my_pid = my_pid;
    g.queues = queues;
    g.champs = champs;
    g.spells = spells;
    g.queue_modes = queue_modes;
    g.queue_maps = queue_maps;
    g.summoner = summoner;
}

/// Riot ID for a puuid; LCU leaves summonerName empty in lobbies and invites.
fn riot_id(api: &Api, names: &mut HashMap<String, String>, puuid: &str) -> Option<String> {
    if puuid.is_empty() {
        return None;
    }
    if !names.contains_key(puuid) {
        let v = api.get(&format!("/lol-summoner/v2/summoners/puuid/{puuid}"))?;
        names.insert(puuid.to_string(), format!("{}#{}", v["gameName"].as_str()?, v["tagLine"].as_str().unwrap_or("")));
    }
    names.get(puuid).cloned()
}

/// Fills `field` from `puuid_field` when the client left it empty.
fn fill_name(api: &Api, names: &mut HashMap<String, String>, v: &mut Value, field: &str, puuid_field: &str) {
    if v[field].as_str().unwrap_or("").is_empty() {
        let puuid = v[puuid_field].as_str().unwrap_or("").to_string();
        if let Some(name) = riot_id(api, names, &puuid) {
            v[field] = Value::String(name);
        }
    }
}

/// LCU event uris that change what we show; the client emits many more, which are ignored.
const WATCH: &[&str] = &["/lol-gameflow/", "/lol-lobby/", "/lol-matchmaking/", "/lol-champ-select/", "/lol-perks/", "/lol-chat/v1/friends", "/lol-chat/v1/conversations"];

/// Stores `v` and reports whether it differed, so the UI repaints only when something changed.
fn put<T: PartialEq>(slot: &mut T, v: T) -> bool {
    if *slot == v {
        return false;
    }
    *slot = v;
    true
}

/// What the next pass refetches. Events mark parts dirty; a quiet timeout marks everything.
#[derive(Clone, Copy)]
struct Dirty {
    game: bool,
    chat: bool,
    friends: bool,
}

const ALL: Dirty = Dirty { game: true, chat: true, friends: true };

/// Everything for the current phase except chat: lobby, queue, ready check, champ select, runes, invites.
struct Game {
    lobby: Value,
    search: Value,
    ready: Value,
    session: Value,
    pickable: Vec<i64>,
    rune_pages: Vec<(i64, String, bool)>,
    rune_current: i64,
    invites: Vec<Value>,
}

fn fetch_game(api: &Api, phase: &str, names: &mut HashMap<String, String>) -> Game {
    let in_lobby = matches!(phase, "Lobby" | "Matchmaking" | "ReadyCheck");
    let queued = matches!(phase, "Matchmaking" | "ReadyCheck");
    let champ = phase == "ChampSelect";
    let mut lobby = if in_lobby { api.get("/lol-lobby/v2/lobby") } else { None };
    if let Some(members) = lobby.as_mut().and_then(|l| l.get_mut("members")).and_then(Value::as_array_mut) {
        for m in members {
            fill_name(api, names, m, "summonerName", "puuid");
        }
    }
    let mut invites: Vec<Value> = api
        .get("/lol-lobby/v2/received-invitations")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i["state"] == "Pending")
        .collect();
    for inv in &mut invites {
        fill_name(api, names, inv, "fromSummonerName", "fromPuuid");
    }
    Game {
        lobby: lobby.unwrap_or(Value::Null),
        search: if queued { api.get("/lol-matchmaking/v1/search") } else { None }.unwrap_or(Value::Null),
        ready: if phase == "ReadyCheck" { api.get("/lol-matchmaking/v1/ready-check") } else { None }.unwrap_or(Value::Null),
        session: if champ { api.get("/lol-champ-select/v1/session") } else { None }.unwrap_or(Value::Null),
        pickable: if champ { ids(api.get("/lol-champ-select/v1/pickable-champion-ids")) } else { vec![] },
        rune_pages: if champ {
            api.get("/lol-perks/v1/pages")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter(|p| p["isEditable"] == true || p["isActive"] == true)
                .filter_map(|p| Some((p["id"].as_i64()?, p["name"].as_str()?.to_string(), p["isValid"] != false)))
                .collect()
        } else {
            vec![]
        },
        rune_current: if champ { api.get("/lol-perks/v1/currentpage").and_then(|p| p["id"].as_i64()).unwrap_or(0) } else { 0 },
        invites,
    }
}

/// Unread direct-message conversations, skipping the open one.
fn fetch_unread(api: &Api, chat_with: &str) -> HashMap<String, (String, i64)> {
    api.get("/lol-chat/v1/conversations")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|c| c["type"] == "chat" && c["id"] != chat_with)
        .filter_map(|c| {
            let n = c["unreadMessageCount"].as_i64().filter(|n| *n > 0)?;
            let name = match (c["gameName"].as_str().filter(|n| !n.is_empty()), c["gameTag"].as_str()) {
                (Some(n), Some(t)) if !t.is_empty() => format!("{n}#{t}"),
                (Some(n), _) => n.to_string(),
                _ => c["name"].as_str().unwrap_or("?").to_string(),
            };
            Some((c["id"].as_str()?.to_string(), (name, n)))
        })
        .collect()
}

fn poller(s: S, ctx: egui::Context, tx: Sender<String>, rx: Receiver<String>) {
    let rc_lockfile = riot::rc_lockfile();
    let (mut rc_cache, mut lcu_cache) = (Cached::default(), Cached::default());
    let mut last_spawn: Option<Instant> = None;
    let mut launched_at: Option<Instant> = None;
    let mut last_phase = String::new();
    let mut last_pending = 0;
    let mut last_turn = false;
    let mut picked = AutoPick::default();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut rec_key = String::new();
    let mut recommended: Vec<(String, Value)> = vec![];
    // Event stream state: with it we fetch on change instead of polling.
    let mut ws_api: Option<String> = None;
    let mut ws_tried: Option<Instant> = None;
    let mut dirty = ALL;
    let mut chat_at: Option<Instant> = None;
    let mut friends_at: Option<Instant> = None;
    let due = |at: Option<Instant>, secs: u64| at.is_none_or(|t| t.elapsed() >= Duration::from_secs(secs));

    loop {
        let (auto_launch, auto_accept, auto_picks, need_static, chat_with) = {
            let g = s.lock().unwrap();
            (g.auto_launch, g.auto_accept, g.auto_picks.clone(), g.champs.is_empty() || g.queues.is_empty(), g.chat_with.clone())
        };
        let lcu = lcu_cache.get(&riot::lcu_lockfile()).filter(Api::alive);
        let phase = lcu.as_ref().and_then(|a| a.get("/lol-gameflow/v1/gameflow-phase")).and_then(|v| v.as_str().map(String::from));
        let mut changed = false;

        match (lcu, phase) {
            (Some(api), Some(phase)) => {
                if last_phase.is_empty() {
                    // Only present if we had to show Riot's login window.
                    riot::kill(&[riot::RIOT_ELECTRON]);
                }
                last_spawn = None;
                launched_at = None;
                if ws_api.as_deref() != Some(api.base()) && due(ws_tried, 10) {
                    ws_tried = Some(Instant::now());
                    if api.events(WATCH, tx.clone()) {
                        ws_api = Some(api.base().to_string());
                        dirty = ALL;
                    }
                }
                if need_static {
                    load_static(&api, &s);
                    changed = true;
                }
                // In game, chat and friends wait so the game gets the CPU; friends refresh at most every 5s anyway.
                let in_game = phase == "InProgress";
                let do_game = dirty.game || phase != last_phase;
                let do_chat = dirty.chat && (!in_game || due(chat_at, 5));
                let do_friends = dirty.friends && due(friends_at, if in_game { 30 } else { 5 });

                let game = do_game.then(|| fetch_game(&api, &phase, &mut names));
                if let Some(game) = &game {
                    if auto_accept && game.ready["playerResponse"] == "None" {
                        let _ = api.send("POST", "/lol-matchmaking/v1/ready-check/accept", None);
                    }
                    // Pop the window up for a found match (even when auto-accepting) and when our pick/ban turn starts.
                    let turn = turn_in_progress(&game.session);
                    if (phase == "ReadyCheck" && last_phase != "ReadyCheck") || (turn && !last_turn) {
                        bring_to_front(&ctx);
                    }
                    last_turn = turn;
                    if auto_picks.iter().any(|l| !l.is_empty()) && phase == "ChampSelect" {
                        auto_pick(&api, &s, game, &auto_picks, &mut picked);
                    }
                    match recommended_path(&game.session, &s) {
                        Some(path) if path != rec_key => {
                            recommended = fetch_recommended(&api, &path);
                            rec_key = path;
                        }
                        Some(_) => {}
                        None => {
                            recommended.clear();
                            rec_key.clear();
                        }
                    }
                }
                let unread = do_chat.then(|| fetch_unread(&api, &chat_with));
                let messages = (do_chat && !chat_with.is_empty())
                    .then(|| api.get(&format!("/lol-chat/v1/conversations/{chat_with}/messages")).and_then(|v| v.as_array().cloned()).unwrap_or_default());
                let friends = do_friends.then(|| api.get("/lol-chat/v1/friends").and_then(|v| v.as_array().cloned()).unwrap_or_default());
                if do_chat {
                    dirty.chat = false;
                    chat_at = Some(Instant::now());
                }
                if do_friends {
                    dirty.friends = false;
                    friends_at = Some(Instant::now());
                }
                dirty.game = false;

                let mut g = s.lock().unwrap();
                changed |= put(&mut g.status, tr("Connected", "Conectado").to_string());
                changed |= put(&mut g.need_login, false);
                changed |= g.lcu.is_none();
                g.lcu = Some(api);
                changed |= put(&mut g.phase, phase.clone());
                if let Some(game) = game {
                    let timers = put(&mut g.search, game.search) | put(&mut g.ready, game.ready);
                    if timers {
                        g.timer_at = Some(Instant::now());
                    }
                    changed |= timers
                        | put(&mut g.lobby, game.lobby)
                        | put(&mut g.session, game.session)
                        | put(&mut g.pickable, game.pickable)
                        | put(&mut g.rune_pages, game.rune_pages)
                        | put(&mut g.rune_current, game.rune_current)
                        | put(&mut g.invites, game.invites)
                        | put(&mut g.recommended, recommended.clone());
                }
                if let Some(unread) = unread {
                    changed |= put(&mut g.unread, unread);
                }
                // The chat may have been closed or switched while we were fetching.
                if let Some(messages) = messages.filter(|_| g.chat_with == chat_with) {
                    changed |= put(&mut g.messages, messages);
                }
                if let Some(friends) = friends {
                    changed |= put(&mut g.friends, friends);
                }
                // Flash the taskbar for new invites, incoming role-swap requests or unread messages.
                let pending = g.invites.len() + received_swaps(&g.session) + g.unread.values().map(|u| u.1).sum::<i64>() as usize;
                if pending > last_pending {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                }
                last_pending = pending;
                last_phase = phase;
            }
            _ => {
                let (status, need_login) =
                    if auto_launch { launch_step(&mut rc_cache, &rc_lockfile, &mut last_spawn, &mut launched_at) } else { (tr("Stopped", "Parado").into(), false) };
                ws_api = None;
                let mut g = s.lock().unwrap();
                changed |= put(&mut g.status, status) | put(&mut g.need_login, need_login) | g.lcu.is_some();
                g.lcu = None;
                g.phase.clear();
                g.queues.clear();
                g.champs.clear();
                g.invites.clear();
                g.friends.clear();
                g.unread.clear();
                g.chat_with.clear();
                g.messages.clear();
                last_phase.clear();
            }
        }
        if changed {
            ctx.request_repaint();
        }

        // Wait for events; without the event stream, poll like before. Quiet timeouts refresh everything as a safety net.
        let fast = last_phase == "ChampSelect" || last_phase == "ReadyCheck";
        let wait = match (ws_api.is_some(), fast) {
            (true, true) => Duration::from_secs(3),
            (true, false) => Duration::from_secs(10),
            (false, true) => Duration::from_millis(400),
            (false, false) => Duration::from_secs(1),
        };
        match rx.recv_timeout(wait) {
            Ok(uri) => {
                // Events come in bursts; gather them before fetching once.
                std::thread::sleep(Duration::from_millis(60));
                for uri in std::iter::once(uri).chain(rx.try_iter()) {
                    if uri == riot::CLOSED {
                        ws_api = None;
                        dirty = ALL;
                    } else if uri.starts_with("/lol-chat/v1/friends") {
                        dirty.friends = true;
                    } else if uri.starts_with("/lol-chat/") {
                        dirty.chat = true;
                    } else {
                        dirty.game = true;
                    }
                }
            }
            Err(_) => dirty = ALL,
        }
    }
}

fn poke(g: &Shared, uri: &str) {
    if let Some(tx) = &g.poke {
        let _ = tx.send(uri.into());
    }
}

/// Seconds since the queue/ready-check data last changed.
fn timer_secs(g: &Shared) -> f64 {
    g.timer_at.map_or(0.0, |t| t.elapsed().as_secs_f64())
}

fn queue_time(g: &Shared) -> f64 {
    g.search["timeInQueue"].as_f64().unwrap_or(0.0) + timer_secs(g)
}

fn ready_time(g: &Shared) -> f64 {
    g.ready["timer"].as_f64().unwrap_or(0.0) + timer_secs(g)
}

/// Milliseconds left in the champ select phase, counting down between session updates.
fn champ_time_left(sess: &Value) -> f64 {
    let t = &sess["timer"];
    let left = t["adjustedTimeLeftInPhase"].as_f64().unwrap_or(0.0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_millis() as f64);
    match t["internalNowInEpochMs"].as_f64().filter(|at| *at > 0.0) {
        Some(at) => (at + left - now).max(0.0),
        None => left,
    }
}

fn turn_in_progress(sess: &Value) -> bool {
    let me = &sess["localPlayerCellId"];
    sess["actions"].as_array().into_iter().flatten().filter_map(Value::as_array).flatten().any(|a| {
        a["actorCellId"] == *me && a["isInProgress"] == true && a["completed"] == false
    })
}

fn bring_to_front(ctx: &egui::Context) {
    use egui::{ViewportCommand as C, WindowLevel as L};
    // Windows often refuses to hand over focus; a topmost/normal toggle still raises the window above others.
    for cmd in [C::Minimized(false), C::WindowLevel(L::AlwaysOnTop), C::WindowLevel(L::Normal), C::Focus] {
        ctx.send_viewport_cmd(cmd);
    }
}

/// League reopens its own UI by itself (match found, champ select, after games). Checks every second and closes it
/// unless the player asked for the full client. Its ux-state can say "Quit" while a window is up, so the League
/// ("RCLIENT") and Riot Client windows are checked too. Changes go to lolite.log to trace pop-ups.
fn ux_guard(s: S) {
    let (mut last_phase, mut last_state, mut last_windows) = (String::new(), None, (false, false));
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let (api, phase, want_ux) = {
            let g = s.lock().unwrap();
            (g.lcu.clone(), g.phase.clone(), g.want_ux)
        };
        let Some(api) = api else { continue };
        let state = api.get("/riotclient/ux-state");
        let windows = (riot::window_shown(Some("RCLIENT"), None), riot::window_shown(None, Some("Riot Client")));
        let new_phase = phase != last_phase;
        let changed = new_phase || state != last_state || windows != last_windows;
        if changed {
            log(&format!("phase={phase} ux-state={state:?} want_ux={want_ux} league_window={} riot_window={}", windows.0, windows.1));
        }
        let shown = state.as_ref().is_some_and(|v| v != "Quit") || windows.0;
        // In game League keeps its UI hidden on its own.
        if !want_ux && phase != "InProgress" {
            if shown || new_phase {
                let res = api.send("POST", "/riotclient/kill-ux", None);
                if shown && changed {
                    log(&format!("kill-ux: {res:?}"));
                }
            }
            // LCU only runs once the login is done, so the Riot Client window isn't needed for it anymore.
            if windows.1 {
                riot::kill(&[riot::RIOT_ELECTRON]);
            }
        }
        (last_phase, last_state, last_windows) = (phase, state, windows);
    }
}

fn log(line: &str) {
    use std::io::Write;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(settings_path().with_file_name("lolite.log")) {
        let _ = writeln!(f, "{secs} {line}");
    }
}

/// One step of: start headless Riot Client -> wait for saved login -> press Play.
/// Returns (status, needs_login).
fn launch_step(rc_cache: &mut Cached, rc_lockfile: &str, last_spawn: &mut Option<Instant>, launched_at: &mut Option<Instant>) -> (String, bool) {
    let rc = rc_cache.get(rc_lockfile);
    match rc.as_ref().and_then(|a| a.probe("/rso-auth/v1/authorization")) {
        None => {
            if last_spawn.is_none_or(|t| t.elapsed() > Duration::from_secs(30)) {
                riot::start_riot_client(true);
                *last_spawn = Some(Instant::now());
            }
            (tr("Starting Riot Client…", "Iniciando Riot Client…").into(), false)
        }
        Some(200) => {
            // Early POSTs fail (e.g. 464 EULA) until RCS finishes loading, so retry; after success give League time to boot.
            if launched_at.is_none_or(|t| t.elapsed() > Duration::from_secs(30)) {
                let ok = rc.unwrap().send("POST", "/product-launcher/v1/products/league_of_legends/patchlines/live", None).is_ok();
                if ok {
                    *launched_at = Some(Instant::now());
                }
            }
            (tr("Starting League…", "Iniciando League…").into(), false)
        }
        Some(_) => (tr("Not logged in", "Não conectado à conta").into(), true),
    }
}

fn act(s: &S, method: &'static str, path: String, body: Option<Value>) {
    let s = s.clone();
    std::thread::spawn(move || {
        let api = s.lock().unwrap().lcu.clone();
        let res = match api {
            Some(api) => api.send(method, &path, body),
            None => Err(tr("League client not connected", "Cliente do League não conectado").into()),
        };
        s.lock().unwrap().error = res.err().unwrap_or_default();
    });
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn queue_name(g: &Shared, id: i64) -> String {
    g.queues.iter().find(|q| q.0 == id).map_or(format!("{} {id}", tr("Queue", "Fila")), |q| q.1.clone())
}

fn member_name(m: &Value) -> &str {
    m["summonerName"].as_str().filter(|n| !n.is_empty()).unwrap_or(tr("Player", "Jogador"))
}

fn champ_name(g: &Shared, id: i64) -> String {
    match id {
        BRAVERY => tr("Bravery (random)", "Bravura (aleatório)").into(),
        _ => g.champs.iter().find(|c| c.0 == id).map_or("—".into(), |c| c.1.clone()),
    }
}

/// The local player's current champ select action.
struct Turn {
    aid: i64,
    is_ban: bool,
    active: bool,
    current: i64,
}

/// Ok(turn) if we have an action to take, else Err(Some(champ)) when locked or Err(None) when waiting.
fn my_turn(g: &Shared) -> Result<Turn, Option<i64>> {
    let sess = &g.session;
    let me = sess["localPlayerCellId"].as_i64();
    let actions: Vec<&Value> = sess["actions"].as_array().into_iter().flatten().filter_map(|a| a.as_array()).flatten().collect();
    let mine = |a: &Value| a["actorCellId"].as_i64() == me && a["completed"] == false;
    let action = actions
        .iter()
        .find(|a| mine(a) && a["isInProgress"] == true)
        .or_else(|| actions.iter().find(|a| mine(a) && a["type"] == "pick"));
    match action {
        Some(a) => Ok(Turn {
            aid: a["id"].as_i64().unwrap_or(0),
            is_ban: a["type"] == "ban",
            active: a["isInProgress"] == true,
            current: a["championId"].as_i64().unwrap_or(0),
        }),
        None => Err(actions
            .iter()
            .find(|a| a["actorCellId"].as_i64() == me && a["type"] == "pick" && a["completed"] == true)
            .and_then(|a| a["championId"].as_i64())),
    }
}

/// One entry per champion: Riot also lists mode variants (e.g. id 60080 "Jade_Pantheon") under the base name.
fn base_champs(g: &Shared) -> impl Iterator<Item = &(i64, String)> {
    g.champs.iter().enumerate().filter(|(i, c)| *i == 0 || g.champs[i - 1].1 != c.1).map(|(_, c)| c)
}

/// Champions offered for this turn, filtered by search.
fn champ_options(g: &Shared, t: &Turn, search: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    if !t.is_ban {
        let mut specials: Vec<i64> = g.pickable.iter().copied().filter(|id| *id < 0).collect();
        // Arena gets new queue ids over time (1700, 1710, 1740, 1750...), its game mode stays CHERRY.
        let arena = g.queue_modes.get(&g.session["queueId"].as_i64().unwrap_or(0)).is_some_and(|m| m == "CHERRY");
        if arena && !specials.contains(&BRAVERY) {
            specials.push(BRAVERY);
        }
        out.extend(specials.into_iter().map(|id| (id, if id == BRAVERY { champ_name(g, id) } else { format!("{} {id}", tr("Special", "Especial")) })));
    }
    // Ban: Riot's bannable list didn't match champion ids in Arena; the server validates bans anyway.
    // Without a pickable list (bans, or not loaded yet) offer each champion once; with one, exactly what it allows.
    let q = search.to_lowercase();
    let matches = |c: &&(i64, String)| c.1.to_lowercase().contains(&q);
    if t.is_ban || g.pickable.is_empty() {
        out.extend(base_champs(g).filter(matches).cloned());
    } else {
        out.extend(g.champs.iter().filter(|c| g.pickable.contains(&c.0)).filter(matches).cloned());
    }
    out
}

/// Role-swap (draft/ranked) and champion-trade (ARAM etc.) buttons for a teammate's cell.
fn swap_buttons(ui: &mut egui::Ui, s: &S, g: &Shared, cell: Option<i64>) {
    for (key, path, label) in [
        ("positionSwaps", "position-swaps", tr("Swap role", "Trocar rota")),
        ("trades", "trades", tr("Trade champ", "Trocar campeão")),
    ] {
        if let Some(swap) = g.session[key].as_array().into_iter().flatten().find(|w| w["cellId"].as_i64() == cell) {
            swap_button(ui, s, swap, path, label);
        }
    }
}

/// State is AVAILABLE/BUSY/INVALID/RECEIVED/SENT.
fn swap_button(ui: &mut egui::Ui, s: &S, swap: &Value, path: &str, label: &str) {
    let id = swap["id"].as_i64().unwrap_or(0);
    let send = |verb: &str| act(s, "POST", format!("/lol-champ-select/v1/session/{path}/{id}/{verb}"), None);
    match swap["state"].as_str().unwrap_or("") {
        "AVAILABLE" => {
            if ui.small_button(label).clicked() {
                send("request");
            }
        }
        "SENT" => {
            ui.weak(tr("swap sent", "troca enviada"));
            if ui.small_button(tr("Cancel", "Cancelar")).clicked() {
                send("cancel");
            }
        }
        "RECEIVED" => {
            let text = format!("{} {}", tr("Accept", "Aceitar"), label.to_lowercase());
            let accept = egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::BLACK)).fill(egui::Color32::from_rgb(0x0A, 0xC8, 0xB9));
            if ui.add(accept).clicked() {
                send("accept");
            }
            if ui.small_button(tr("Decline", "Recusar")).clicked() {
                send("decline");
            }
        }
        _ => {}
    }
}

fn received_swaps(session: &Value) -> usize {
    ["positionSwaps", "trades"].iter().map(|k| session[*k].as_array().map_or(0, |a| a.iter().filter(|w| w["state"] == "RECEIVED").count())).sum()
}

/// ARAM-style bench: your champion, reroll, and the champions you can swap to.
fn bench(ui: &mut egui::Ui, s: &S, g: &Shared) {
    let sess = &g.session;
    if sess["benchEnabled"] != true {
        return;
    }
    let me = sess["myTeam"].as_array().into_iter().flatten().find(|p| p["cellId"] == sess["localPlayerCellId"]);
    let mine = me.and_then(|p| p["championId"].as_i64()).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label(format!("{} {}", tr("Your champion:", "Seu campeão:"), champ_name(g, mine)));
        let rolls = sess["rerollsRemaining"].as_i64().unwrap_or(0);
        if ui.add_enabled(rolls > 0, egui::Button::new(format!("{} ({rolls})", tr("Reroll", "Rerrolar")))).clicked() {
            act(s, "POST", "/lol-champ-select/v1/session/my-selection/reroll".into(), None);
        }
    });
    let benched: Vec<i64> = sess["benchChampions"].as_array().into_iter().flatten().filter_map(|c| c["championId"].as_i64()).collect();
    if benched.is_empty() {
        ui.weak(tr("Bench is empty", "Banco vazio"));
        return;
    }
    ui.label(tr("Bench (click to swap):", "Banco (clique para trocar):"));
    ui.horizontal_wrapped(|ui| {
        for id in benched {
            if ui.button(champ_name(g, id)).clicked() {
                act(s, "POST", format!("/lol-champ-select/v1/session/bench/swap/{id}"), None);
            }
        }
    });
}

/// (position, champion) for each enemy; champions show once revealed.
fn enemy_team(g: &Shared) -> Vec<(String, String)> {
    g.session["theirTeam"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let id = p["championId"].as_i64().unwrap_or(0);
            (i18n::role(p["assignedPosition"].as_str().unwrap_or("")).to_string(), if id == 0 { "—".into() } else { champ_name(g, id) })
        })
        .collect()
}

/// Comma-separated names of our (ally) or their completed bans.
/// Read from the ban actions: `session.bans` stays empty in most queues.
fn bans(g: &Shared, ally: bool) -> String {
    let names: Vec<String> = g.session["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .filter(|a| a["type"] == "ban" && a["completed"] == true && a["isAllyAction"] == ally)
        .filter_map(|a| a["championId"].as_i64().filter(|id| *id > 0))
        .map(|id| champ_name(g, id))
        .collect();
    if names.is_empty() { "—".into() } else { names.join(", ") }
}

/// Pick turns in order as (label, ally, in progress); the label is the champion once known, else the role.
fn pick_order(g: &Shared) -> Vec<(String, bool, bool)> {
    let sess = &g.session;
    let players: Vec<&Value> = ["myTeam", "theirTeam"].iter().flat_map(|t| sess[*t].as_array().into_iter().flatten()).collect();
    sess["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .filter(|a| a["type"] == "pick")
        .map(|a| {
            let p = players.iter().find(|p| p["cellId"] == a["actorCellId"]);
            let id = a["championId"].as_i64().filter(|i| *i > 0).or_else(|| p.and_then(|p| p["championPickIntent"].as_i64())).unwrap_or(0);
            let label = if id > 0 {
                champ_name(g, id)
            } else if a["actorCellId"] == sess["localPlayerCellId"] {
                tr("You", "Você").into()
            } else {
                p.and_then(|p| p["assignedPosition"].as_str()).filter(|r| !r.is_empty()).map_or("?".into(), |r| i18n::role(r).to_string())
            };
            (label, a["isAllyAction"] == true, a["isInProgress"] == true)
        })
        .collect()
}

/// Summoner spells + rune page pickers for champ select.
fn loadout(ui: &mut egui::Ui, s: &S, g: &Shared) {
    let sess = &g.session;
    let me = sess["myTeam"].as_array().into_iter().flatten().find(|p| p["cellId"] == sess["localPlayerCellId"]);
    let (d, f) = me.map_or((0, 0), |p| (p["spell1Id"].as_i64().unwrap_or(0), p["spell2Id"].as_i64().unwrap_or(0)));
    let mode = g.queue_modes.get(&sess["queueId"].as_i64().unwrap_or(0)).cloned().unwrap_or_default();
    let spells: Vec<&(i64, String, Vec<String>)> = g.spells.iter().filter(|sp| sp.2.contains(&mode)).collect();
    let spell_name = |id: i64| g.spells.iter().find(|sp| sp.0 == id).map_or("—".to_string(), |sp| sp.1.clone());

    if !spells.is_empty() {
        ui.horizontal(|ui| {
            ui.label(tr("Spells", "Feitiços"));
            let (mut nd, mut nf) = (d, f);
            for (salt, val) in [("spell_d", &mut nd), ("spell_f", &mut nf)] {
                egui::ComboBox::from_id_salt(salt).width(110.0).selected_text(spell_name(*val)).show_ui(ui, |ui| {
                    for sp in &spells {
                        ui.selectable_value(val, sp.0, &sp.1);
                    }
                });
            }
            // Picking the spell already in the other slot swaps them, like the official client.
            if nd != d && nd == f {
                nf = d;
            } else if nf != f && nf == d {
                nd = f;
            }
            if (nd, nf) != (d, f) {
                act(s, "PATCH", "/lol-champ-select/v1/session/my-selection".into(), Some(json!({ "spell1Id": nd, "spell2Id": nf })));
            }
        });
    }

    if !g.rune_pages.is_empty() {
        ui.horizontal(|ui| {
            ui.label(tr("Runes", "Runas"));
            let current = g.rune_pages.iter().find(|p| p.0 == g.rune_current).map_or("—".to_string(), |p| p.1.clone());
            let mut pick = g.rune_current;
            egui::ComboBox::from_id_salt("runes").width(200.0).selected_text(current).show_ui(ui, |ui| {
                for (id, name, valid) in &g.rune_pages {
                    let label = if *valid { name.clone() } else { format!("{name} ({})", tr("incomplete", "incompleta")) };
                    ui.selectable_value(&mut pick, *id, label);
                }
            });
            if pick != g.rune_current {
                act(s, "PUT", "/lol-perks/v1/currentpage".into(), Some(json!(pick)));
            }
        });
    }

    if !g.recommended.is_empty() {
        ui.label(tr(
            "Recommended runes (saved to a \"LoLite\" page, or overwrites the current page if all pages are used):",
            "Runas recomendadas (salvas numa página \"LoLite\", ou sobrescrevem a página atual se todas estiverem em uso):",
        ));
        for (label, page) in &g.recommended {
            ui.horizontal(|ui| {
                if ui.small_button(tr("Use", "Usar")).clicked() {
                    use_runes(s, page.clone());
                }
                ui.label(label);
            });
        }
    }
}

/// Recommended-pages path for my current champion, role and map; None until a champion is hovered.
fn recommended_path(sess: &Value, s: &S) -> Option<String> {
    let me = sess["myTeam"].as_array()?.iter().find(|p| p["cellId"] == sess["localPlayerCellId"])?;
    let champ = [me["championId"].as_i64(), me["championPickIntent"].as_i64()].into_iter().flatten().find(|id| *id > 0)?;
    champ_rec_path(sess, s, champ)
}

/// Recommended-pages path for `champ` in my role on this map.
fn champ_rec_path(sess: &Value, s: &S, champ: i64) -> Option<String> {
    let me = sess["myTeam"].as_array()?.iter().find(|p| p["cellId"] == sess["localPlayerCellId"])?;
    let pos = me["assignedPosition"].as_str().filter(|p| !p.is_empty()).unwrap_or("none").to_lowercase();
    let map = *s.lock().unwrap().queue_maps.get(&sess["queueId"].as_i64()?)?;
    Some(format!("/lol-perks/v1/recommended-pages/champion/{champ}/position/{pos}/map/{map}"))
}

/// Label is the keystone and the three primary-tree runes.
fn fetch_recommended(api: &Api, path: &str) -> Vec<(String, Value)> {
    api.get(path)
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            let perks = r["perks"].as_array()?;
            let names: Vec<&str> = perks.iter().take(4).filter_map(|p| p["name"].as_str()).collect();
            let label = format!("{} — {}", names.first()?, names[1..].join(", "));
            Some((label, rec_page(r)))
        })
        .collect()
}

/// Rune page body from a recommended-pages entry.
fn rec_page(r: &Value) -> Value {
    let ids: Vec<i64> = r["perks"].as_array().into_iter().flatten().filter_map(|p| p["id"].as_i64()).collect();
    json!({ "primaryStyleId": r["primaryPerkStyleId"], "subStyleId": r["secondaryPerkStyleId"], "selectedPerkIds": ids })
}

fn use_runes(s: &S, page: Value) {
    let s = s.clone();
    std::thread::spawn(move || {
        let Some(api) = s.lock().unwrap().lcu.clone() else { return };
        s.lock().unwrap().error = save_runes(&api, page).err().unwrap_or_default();
    });
}

/// Writes a recommended page into the "LoLite" page, a new page if there is room, or else the current page.
fn save_runes(api: &Api, mut page: Value) -> Result<(), String> {
    let pages = api.get("/lol-perks/v1/pages").and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let current = api.get("/lol-perks/v1/currentpage");
    let room = api.get("/lol-perks/v1/inventory").is_some_and(|i| i["canAddCustomPage"] == true);
    let target = pages
        .iter()
        .find(|p| p["isEditable"] == true && p["name"] == "LoLite")
        .or(if room { None } else { current.as_ref().filter(|p| p["isEditable"] == true) })
        .and_then(|p| p["id"].as_i64());
    page["name"] = json!("LoLite");
    page["current"] = json!(true);
    match target {
        Some(id) => {
            page["id"] = json!(id);
            api.send("PUT", &format!("/lol-perks/v1/pages/{id}"), Some(page)).and_then(|_| api.send("PUT", "/lol-perks/v1/currentpage", Some(json!(id))))
        }
        None => api.send("POST", "/lol-perks/v1/pages", Some(page)),
    }
}

/// Auto pick progress: the pick action we pre-picked on (and with which champion), and the one we locked.
#[derive(Default)]
struct AutoPick {
    hovered: (i64, i64),
    locked: i64,
}

/// Deadlock-style auto pick: pre-picks the first priority champion for our assigned lane still available (every lane
/// in order when the queue assigns none), then on our turn sets its recommended spells and runes and locks it.
/// Steps aside once the player hovers something else themselves.
fn auto_pick(api: &Api, s: &S, game: &Game, picks: &Picks, done: &mut AutoPick) {
    let sess = &game.session;
    let me = &sess["localPlayerCellId"];
    let actions: Vec<&Value> = sess["actions"].as_array().into_iter().flatten().filter_map(Value::as_array).flatten().collect();
    let Some(a) = actions.iter().find(|a| a["actorCellId"] == *me && a["type"] == "pick" && a["completed"] == false) else { return };
    let aid = a["id"].as_i64().unwrap_or(0);
    let current = a["championId"].as_i64().unwrap_or(0);
    if aid == done.locked || (current != 0 && (aid, current) != done.hovered) {
        return;
    }
    let team: Vec<&Value> = sess["myTeam"].as_array().into_iter().flatten().collect();
    let pos = team.iter().find(|p| p["cellId"] == *me).and_then(|p| p["assignedPosition"].as_str()).unwrap_or("").to_uppercase();
    let picks = LANES.iter().position(|l| *l == pos).map_or_else(|| picks.concat(), |i| picks[i].clone());
    let taken = |id: i64| {
        actions.iter().any(|b| b["completed"] == true && b["championId"] == id) || team.iter().any(|p| p["cellId"] != *me && p["championPickIntent"] == id)
    };
    let Some(champ) = picks.iter().copied().find(|id| game.pickable.contains(id) && !taken(*id)) else { return };
    let path = format!("/lol-champ-select/v1/session/actions/{aid}");
    if a["isInProgress"] != true {
        if current != champ && api.send("PATCH", &path, Some(json!({ "championId": champ }))).is_ok() {
            done.hovered = (aid, champ);
        }
        return;
    }
    if let Some(rec) = champ_rec_path(sess, s, champ).and_then(|p| api.get(&p)).and_then(|v| v.get(0).cloned()) {
        if let [d, f] = ids(Some(rec["summonerSpellIds"].clone()))[..] {
            // Keep Flash (or whichever) on the key the player already has it on.
            let mine = team.iter().find(|p| p["cellId"] == *me);
            let (d, f) = if mine.is_some_and(|p| p["spell1Id"] == f || p["spell2Id"] == d) { (f, d) } else { (d, f) };
            let _ = api.send("PATCH", "/lol-champ-select/v1/session/my-selection", Some(json!({ "spell1Id": d, "spell2Id": f })));
        }
        if let Err(e) = save_runes(api, rec_page(&rec)) {
            s.lock().unwrap().error = e;
        }
    }
    if api.send("PATCH", &path, Some(json!({ "championId": champ, "completed": true }))).is_ok() {
        done.locked = aid;
    }
}

fn hover(s: &S, t: &Turn, id: i64) {
    act(s, "PATCH", format!("/lol-champ-select/v1/session/actions/{}", t.aid), Some(json!({ "championId": id })));
}

fn lock_in(s: &S, t: &Turn) {
    act(s, "PATCH", format!("/lol-champ-select/v1/session/actions/{}", t.aid), Some(json!({ "championId": t.current, "completed": true })));
}

fn vanguard_hint() -> &'static str {
    tr(
        "Stops vgc service + tray (UAC prompt). Kernel driver stays until reboot.",
        "Para o serviço vgc + bandeja (pede UAC). O driver do kernel fica até reiniciar.",
    )
}

/// Display text for a champ select timer phase.
fn champ_select_phase(p: &str) -> &str {
    match p {
        "PLANNING" => tr("Planning", "Planejamento"),
        "BAN_PICK" => tr("Ban & pick", "Banir e escolher"),
        "FINALIZATION" => tr("Finalizing", "Finalizando"),
        "GAME_STARTING" => tr("Game starting", "Partida iniciando"),
        p => p,
    }
}

fn settings_path() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("LoLite").join("settings.txt")
}

fn setting(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(settings_path()).ok()?;
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=').map(String::from))
}

struct App {
    s: S,
    search: String,
    kill_vanguard: bool,
    friends_open: bool,
    /// Queue group tab chosen in the mode picker.
    queue_tab: Option<&'static str>,
    show_offline: bool,
    friend_search: String,
    chat_input: String,
    /// Path overrides from options; empty means auto-detect.
    league_dir: String,
    riot_client: String,
    /// Mirrors `Shared.auto_picks` so settings can be saved while the state is locked.
    auto_picks: Picks,
    /// Auto pick search text per lane.
    pick_search: [String; 5],
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let s = self.s.clone();
        // Timers count down locally between events; only these phases need a tick.
        if matches!(s.lock().unwrap().phase.as_str(), "Matchmaking" | "ReadyCheck" | "ChampSelect") {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
        let side = egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(egui::Margin::symmetric(10, 8));
        if self.friends_open {
            egui::Panel::right("friends").resizable(true).default_size(250.0).frame(side).show(ui, |ui| self.social(ui, &mut s.lock().unwrap()));
        } else {
            egui::Panel::right("friends_collapsed").resizable(false).frame(side).show(ui, |ui| {
                if ui.small_button("«").on_hover_text(tr("Friends", "Amigos")).clicked() {
                    self.friends_open = true;
                }
                let unread = social::unread_total(&s.lock().unwrap());
                if unread > 0 {
                    ui.colored_label(egui::Color32::from_rgb(0xE8, 0x40, 0x57), egui::RichText::new(unread.to_string()).strong())
                        .on_hover_text(tr("Unread messages", "Mensagens não lidas"));
                }
            });
        }
        self.draw_fancy(ui);
    }
}

impl App {
    fn save_settings(&self) {
        let p = settings_path();
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let lang = if i18n::pt() { "pt" } else { "en" };
        let _ = std::fs::write(
            p,
            format!(
                "lang={lang}\nleague_dir={}\nriot_client={}\n{}",
                self.league_dir,
                self.riot_client,
                LANES
                    .iter()
                    .zip(&self.auto_picks)
                    .map(|(lane, ids)| format!("picks_{}={}\n", lane.to_lowercase(), ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")))
                    .collect::<String>()
            ),
        );
    }

    fn options_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(tr("⚙ Options", "⚙ Opções"), |ui| {
            ui.checkbox(&mut self.kill_vanguard, tr("Kill Riot also stops Vanguard", "Fechar Riot também para o Vanguard"))
                .on_hover_text(vanguard_hint());
            ui.horizontal(|ui| {
                ui.label(tr("Language", "Idioma"));
                let mut pt = i18n::pt();
                ui.selectable_value(&mut pt, false, "English");
                ui.selectable_value(&mut pt, true, "Português");
                if pt != i18n::pt() {
                    i18n::set_pt(pt);
                    self.save_settings();
                    // Queue names carry a translated prefix; the poller reloads them.
                    let mut g = self.s.lock().unwrap();
                    g.queues.clear();
                    poke(&g, "");
                }
            });
            ui.separator();
            // Hints show the detected path; typing one overrides it, clearing goes back to auto-detect.
            let (league, rc) = riot::paths();
            ui.label(tr("League of Legends folder", "Pasta do League of Legends"));
            let a = ui.add(egui::TextEdit::singleline(&mut self.league_dir).hint_text(league).desired_width(320.0));
            ui.label("Riot Client (RiotClientServices.exe)");
            let b = ui.add(egui::TextEdit::singleline(&mut self.riot_client).hint_text(rc).desired_width(320.0));
            if a.lost_focus() || b.lost_focus() {
                riot::set_paths(self.league_dir.trim(), self.riot_client.trim());
                self.save_settings();
            }
        });
    }
    fn kill_riot(&self, g: &mut Shared) {
        g.auto_launch = false;
        g.want_ux = false;
        let vg = self.kill_vanguard;
        std::thread::spawn(move || riot::kill_riot(vg));
    }

    fn toggle_full_client(&self, g: &mut Shared) {
        g.want_ux = !g.want_ux;
        act(&self.s, "POST", if g.want_ux { "/riotclient/launch-ux" } else { "/riotclient/kill-ux" }.into(), None);
    }

    fn log_in() {
        // Headless RCS has no login UI; restart it with its normal window.
        std::thread::spawn(|| {
            riot::kill(&["RiotClientServices.exe"]);
            riot::start_riot_client(false);
        });
    }

    fn role_pickers(&self, ui: &mut egui::Ui, g: &Shared) {
        let me = &g.lobby["localMember"];
        let mut first = me["firstPositionPreference"].as_str().unwrap_or("").to_string();
        let mut second = me["secondPositionPreference"].as_str().unwrap_or("").to_string();
        let (f0, s0) = (first.clone(), second.clone());
        let shown = |r: &str| if r.is_empty() || r == "UNSELECTED" { "—".to_string() } else { i18n::role(r).to_string() };
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("r1").selected_text(shown(&first)).show_ui(ui, |ui| {
                for r in ROLES {
                    ui.selectable_value(&mut first, r.to_string(), i18n::role(r));
                }
            });
            // Riot rejects a secondary role when the primary is FILL (QPInvalidPositionSelectionRestriction).
            if first != "FILL" {
                egui::ComboBox::from_id_salt("r2").selected_text(shown(&second)).show_ui(ui, |ui| {
                    for r in ROLES.iter().filter(|r| **r != first) {
                        ui.selectable_value(&mut second, r.to_string(), i18n::role(r));
                    }
                });
            }
        });
        if first == "FILL" || second == first {
            second = "UNSELECTED".into();
        }
        if first != f0 || second != s0 {
            act(
                &self.s,
                "PUT",
                "/lol-lobby/v2/lobby/members/localMember/position-preferences".into(),
                Some(json!({ "firstPreference": first, "secondPreference": second })),
            );
        }
    }

    /// Collapsible auto pick lists, up to 5 champions per lane in priority order.
    fn auto_pick_editor(&mut self, ui: &mut egui::Ui, g: &mut Shared) {
        let before = self.auto_picks.clone();
        let title = egui::RichText::new(tr("Auto pick by lane", "Escolha automática por rota")).strong();
        egui::CollapsingHeader::new(title).id_salt("auto_pick").show(ui, |ui| {
            ui.label(egui::RichText::new(tr(
                "Up to 5 per lane, in order. On your turn LoLite locks the first one still available for your assigned lane (every lane in order when the queue assigns none), with its recommended runes and spells.",
                "Até 5 por rota, em ordem. Na sua vez o LoLite confirma o primeiro ainda disponível da sua rota (todas as rotas em ordem quando a fila não define rota), com runas e feitiços recomendados.",
            )).weak());
            for (i, lane) in LANES.iter().enumerate() {
                ui.add_space(4.0);
                // Search results show inline instead of in a popup, so they wrap with the panel instead of covering the friends list.
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(i18n::role(lane)).strong());
                    for (n, id) in before[i].iter().enumerate() {
                        if ui.button(format!("{}. {}  ×", n + 1, champ_name(g, *id))).on_hover_text(tr("Remove", "Remover")).clicked() {
                            self.auto_picks[i].remove(n);
                        }
                    }
                    if before[i].len() < 5 {
                        ui.add(egui::TextEdit::singleline(&mut self.pick_search[i]).hint_text(tr("+ Add champion", "+ Adicionar campeão")).desired_width(150.0));
                    }
                });
                let q = self.pick_search[i].trim().to_lowercase();
                if !q.is_empty() && before[i].len() < 5 {
                    ui.horizontal_wrapped(|ui| {
                        for (id, name) in base_champs(g).filter(|c| !before[i].contains(&c.0) && c.1.to_lowercase().contains(&q)).take(12) {
                            if ui.small_button(name).clicked() {
                                self.auto_picks[i].push(*id);
                                self.pick_search[i].clear();
                            }
                        }
                    });
                }
            }
        });
        if self.auto_picks != before {
            g.auto_picks = self.auto_picks.clone();
            self.save_settings();
        }
    }

}

fn main() -> eframe::Result {
    let (league_dir, riot_client) = (setting("league_dir").unwrap_or_default(), setting("riot_client").unwrap_or_default());
    riot::set_paths(&league_dir, &riot_client);
    let auto_picks: Picks = LANES.map(|lane| setting(&format!("picks_{}", lane.to_lowercase())).unwrap_or_default().split(',').filter_map(|id| id.parse().ok()).collect());
    let (tx, rx) = std::sync::mpsc::channel();
    let s: S = Arc::new(Mutex::new(Shared { auto_launch: true, poke: Some(tx.clone()), auto_picks: auto_picks.clone(), ..Default::default() }));
    let icon = egui::IconData { rgba: include_bytes!("../assets/icon64.rgba").to_vec(), width: 64, height: 64 };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("LoLite").with_inner_size([720.0, 680.0]).with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "LoLite",
        opts,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            fancy::apply_style(&ctx);
            let s2 = s.clone();
            std::thread::spawn(move || poller(s2, ctx, tx, rx));
            let s3 = s.clone();
            std::thread::spawn(move || ux_guard(s3));
            if let Some(lang) = setting("lang") {
                i18n::set_pt(lang == "pt");
            }
            Ok(Box::new(App {
                s,
                search: String::new(),
                kill_vanguard: false,
                friends_open: true,
                queue_tab: None,
                show_offline: false,
                friend_search: String::new(),
                chat_input: String::new(),
                league_dir,
                riot_client,
                auto_picks,
                pick_search: Default::default(),
            }))
        }),
    )
}
