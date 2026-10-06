#![windows_subsystem = "windows"]
mod fancy;
mod riot;

use eframe::egui;
use riot::Api;
use serde_json::{Value, json};
use std::collections::HashMap;
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
    auto_launch: bool,
    auto_accept: bool,
    want_ux: bool,
    need_login: bool,
}

type S = Arc<Mutex<Shared>>;

/// Arena "Bravery": lock a random champion for bonus gold.
const BRAVERY: i64 = -3;
const ARENA_QUEUES: &[i64] = &[1700, 1710, 1750];
const ROLES: &[&str] = &["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY", "FILL"];

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
            let name = q[key].as_str().filter(|n| !n.is_empty())?;
            let name = if q["category"] == "VersusAi" { format!("Co-op vs AI: {name}") } else { name.to_string() };
            Some((q["id"].as_i64()?, name, queue_group(q)))
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
    champs.sort_by(|a, b| a.1.cmp(&b.1));

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

    let mut g = s.lock().unwrap();
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

fn poller(s: S, ctx: egui::Context) {
    let rc_lockfile = riot::rc_lockfile();
    let (mut rc_cache, mut lcu_cache) = (Cached::default(), Cached::default());
    let mut last_spawn: Option<Instant> = None;
    let mut launched_at: Option<Instant> = None;
    let mut last_kill_ux: Option<Instant> = None;
    let mut last_phase = String::new();
    let mut last_pending = 0;
    let mut names: HashMap<String, String> = HashMap::new();
    let mut rec_key = String::new();
    let mut recommended: Vec<(String, Value)> = vec![];

    loop {
        let (auto_launch, auto_accept, want_ux, need_static) = {
            let g = s.lock().unwrap();
            (g.auto_launch, g.auto_accept, g.want_ux, g.champs.is_empty() || g.queues.is_empty())
        };
        let lcu = lcu_cache.get(riot::LCU_LOCKFILE).filter(Api::alive);
        let phase = lcu.as_ref().and_then(|a| a.get("/lol-gameflow/v1/gameflow-phase")).and_then(|v| v.as_str().map(String::from));

        match (lcu, phase) {
            (Some(api), Some(phase)) => {
                if last_phase.is_empty() {
                    // Only present if we had to show Riot's login window.
                    riot::kill(&[riot::RIOT_ELECTRON]);
                }
                last_spawn = None;
                launched_at = None;
                if need_static {
                    load_static(&api, &s);
                }
                // LCU respawns its UI after games etc; keep it dead unless asked for.
                // It also force-shows itself ~30s into champ select, so recheck faster there.
                let every = Duration::from_secs(if phase == "ChampSelect" { 3 } else { 15 });
                let ux_due = phase != last_phase || last_kill_ux.is_none_or(|t| t.elapsed() > every);
                if !want_ux && phase != "InProgress" && ux_due && api.send("POST", "/riotclient/kill-ux", None).is_ok() {
                    last_kill_ux = Some(Instant::now());
                }

                let in_lobby = matches!(phase.as_str(), "Lobby" | "Matchmaking" | "ReadyCheck");
                let queued = matches!(phase.as_str(), "Matchmaking" | "ReadyCheck");
                let mut lobby = if in_lobby { api.get("/lol-lobby/v2/lobby") } else { None };
                if let Some(members) = lobby.as_mut().and_then(|l| l.get_mut("members")).and_then(Value::as_array_mut) {
                    for m in members {
                        fill_name(&api, &mut names, m, "summonerName", "puuid");
                    }
                }
                let search = if queued { api.get("/lol-matchmaking/v1/search") } else { None };
                let ready = if phase == "ReadyCheck" { api.get("/lol-matchmaking/v1/ready-check") } else { None };
                if auto_accept && ready.as_ref().is_some_and(|r| r["playerResponse"] == "None") {
                    let _ = api.send("POST", "/lol-matchmaking/v1/ready-check/accept", None);
                }
                let champ = phase == "ChampSelect";
                let session = if champ { api.get("/lol-champ-select/v1/session") } else { None };
                let pickable = if champ { ids(api.get("/lol-champ-select/v1/pickable-champion-ids")) } else { vec![] };
                let rune_pages: Vec<(i64, String, bool)> = if champ {
                    api.get("/lol-perks/v1/pages")
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default()
                        .iter()
                        .filter(|p| p["isEditable"] == true || p["isActive"] == true)
                        .filter_map(|p| Some((p["id"].as_i64()?, p["name"].as_str()?.to_string(), p["isValid"] != false)))
                        .collect()
                } else {
                    vec![]
                };
                if let Some(path) = session.as_ref().and_then(|sess| recommended_path(sess, &s)) {
                    if path != rec_key {
                        recommended = fetch_recommended(&api, &path);
                        rec_key = path;
                    }
                } else {
                    recommended.clear();
                    rec_key.clear();
                }
                let rune_current = if champ { api.get("/lol-perks/v1/currentpage").and_then(|p| p["id"].as_i64()).unwrap_or(0) } else { 0 };
                let mut invites: Vec<Value> = api
                    .get("/lol-lobby/v2/received-invitations")
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|i| i["state"] == "Pending")
                    .collect();
                for inv in &mut invites {
                    fill_name(&api, &mut names, inv, "fromSummonerName", "fromPuuid");
                }
                // Flash the taskbar for new invites or incoming role-swap requests.
                let pending = invites.len() + session.as_ref().map_or(0, received_swaps);
                if pending > last_pending {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                }
                last_pending = pending;

                let mut g = s.lock().unwrap();
                g.status = "Connected".into();
                g.need_login = false;
                g.lcu = Some(api);
                g.phase = phase.clone();
                g.lobby = lobby.unwrap_or(Value::Null);
                g.search = search.unwrap_or(Value::Null);
                g.ready = ready.unwrap_or(Value::Null);
                g.session = session.unwrap_or(Value::Null);
                g.pickable = pickable;
                g.rune_pages = rune_pages;
                g.rune_current = rune_current;
                g.recommended = recommended.clone();
                g.invites = invites;
                last_phase = phase;
            }
            _ => {
                let (status, need_login) =
                    if auto_launch { launch_step(&mut rc_cache, &rc_lockfile, &mut last_spawn, &mut launched_at) } else { ("Stopped".into(), false) };
                let mut g = s.lock().unwrap();
                g.status = status;
                g.need_login = need_login;
                g.lcu = None;
                g.phase.clear();
                g.queues.clear();
                g.champs.clear();
                g.invites.clear();
                last_phase.clear();
            }
        }
        ctx.request_repaint();
        let fast = last_phase == "ChampSelect" || last_phase == "ReadyCheck";
        std::thread::sleep(Duration::from_millis(if fast { 400 } else { 1000 }));
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
            ("Starting Riot Client…".into(), false)
        }
        Some(200) => {
            // Early POSTs fail (e.g. 464 EULA) until RCS finishes loading, so retry; after success give League time to boot.
            if launched_at.is_none_or(|t| t.elapsed() > Duration::from_secs(30)) {
                let ok = rc.unwrap().send("POST", "/product-launcher/v1/products/league_of_legends/patchlines/live", None).is_ok();
                if ok {
                    *launched_at = Some(Instant::now());
                }
            }
            ("Starting League…".into(), false)
        }
        Some(_) => ("Not logged in".into(), true),
    }
}

fn act(s: &S, method: &'static str, path: String, body: Option<Value>) {
    let s = s.clone();
    std::thread::spawn(move || {
        let api = s.lock().unwrap().lcu.clone();
        let res = match api {
            Some(api) => api.send(method, &path, body),
            None => Err("League client not connected".into()),
        };
        s.lock().unwrap().error = res.err().unwrap_or_default();
    });
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn queue_name(g: &Shared, id: i64) -> String {
    g.queues.iter().find(|q| q.0 == id).map_or(format!("Queue {id}"), |q| q.1.clone())
}

fn member_name(m: &Value) -> &str {
    m["summonerName"].as_str().filter(|n| !n.is_empty()).unwrap_or("Player")
}

fn champ_name(g: &Shared, id: i64) -> String {
    match id {
        BRAVERY => "Bravery (random)".into(),
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

/// Champions offered for this turn, filtered by search.
fn champ_options(g: &Shared, t: &Turn, search: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    if !t.is_ban {
        let mut specials: Vec<i64> = g.pickable.iter().copied().filter(|id| *id < 0).collect();
        if ARENA_QUEUES.contains(&g.session["queueId"].as_i64().unwrap_or(0)) && !specials.contains(&BRAVERY) {
            specials.push(BRAVERY);
        }
        out.extend(specials.into_iter().map(|id| (id, if id == BRAVERY { champ_name(g, id) } else { format!("Special {id}") })));
    }
    // Ban: Riot's bannable list didn't match champion ids in Arena; the server validates bans anyway.
    let allowed = |id: i64| t.is_ban || g.pickable.is_empty() || g.pickable.contains(&id);
    let q = search.to_lowercase();
    out.extend(g.champs.iter().filter(|c| allowed(c.0) && c.1.to_lowercase().contains(&q)).cloned());
    out
}

/// Role-swap (draft/ranked) and champion-trade (ARAM etc.) buttons for a teammate's cell.
fn swap_buttons(ui: &mut egui::Ui, s: &S, g: &Shared, cell: Option<i64>) {
    for (key, path, label) in [("positionSwaps", "position-swaps", "Swap role"), ("trades", "trades", "Trade champ")] {
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
            ui.weak("swap sent");
            if ui.small_button("Cancel").clicked() {
                send("cancel");
            }
        }
        "RECEIVED" => {
            let accept = egui::Button::new(egui::RichText::new(format!("Accept {}", label.to_lowercase())).strong().color(egui::Color32::BLACK)).fill(egui::Color32::from_rgb(0x0A, 0xC8, 0xB9));
            if ui.add(accept).clicked() {
                send("accept");
            }
            if ui.small_button("Decline").clicked() {
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
        ui.label(format!("Your champion: {}", champ_name(g, mine)));
        let rolls = sess["rerollsRemaining"].as_i64().unwrap_or(0);
        if ui.add_enabled(rolls > 0, egui::Button::new(format!("Reroll ({rolls})"))).clicked() {
            act(s, "POST", "/lol-champ-select/v1/session/my-selection/reroll".into(), None);
        }
    });
    let benched: Vec<i64> = sess["benchChampions"].as_array().into_iter().flatten().filter_map(|c| c["championId"].as_i64()).collect();
    if benched.is_empty() {
        ui.weak("Bench is empty");
        return;
    }
    ui.label("Bench (click to swap):");
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
            (p["assignedPosition"].as_str().unwrap_or("").to_string(), if id == 0 { "—".into() } else { champ_name(g, id) })
        })
        .collect()
}

/// Comma-separated ban names for "myTeamBans" or "theirTeamBans".
fn bans(g: &Shared, key: &str) -> String {
    let names: Vec<String> = ids(Some(g.session["bans"][key].clone())).into_iter().filter(|id| *id > 0).map(|id| champ_name(g, id)).collect();
    if names.is_empty() { "—".into() } else { names.join(", ") }
}

/// Summoner spells + rune page pickers for champ select; shared by both layouts.
fn loadout(ui: &mut egui::Ui, s: &S, g: &Shared) {
    let sess = &g.session;
    let me = sess["myTeam"].as_array().into_iter().flatten().find(|p| p["cellId"] == sess["localPlayerCellId"]);
    let (d, f) = me.map_or((0, 0), |p| (p["spell1Id"].as_i64().unwrap_or(0), p["spell2Id"].as_i64().unwrap_or(0)));
    let mode = g.queue_modes.get(&sess["queueId"].as_i64().unwrap_or(0)).cloned().unwrap_or_default();
    let spells: Vec<&(i64, String, Vec<String>)> = g.spells.iter().filter(|sp| sp.2.contains(&mode)).collect();
    let spell_name = |id: i64| g.spells.iter().find(|sp| sp.0 == id).map_or("—".to_string(), |sp| sp.1.clone());

    if !spells.is_empty() {
        ui.horizontal(|ui| {
            ui.label("Spells");
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
            ui.label("Runes");
            let current = g.rune_pages.iter().find(|p| p.0 == g.rune_current).map_or("—".to_string(), |p| p.1.clone());
            let mut pick = g.rune_current;
            egui::ComboBox::from_id_salt("runes").width(200.0).selected_text(current).show_ui(ui, |ui| {
                for (id, name, valid) in &g.rune_pages {
                    let label = if *valid { name.clone() } else { format!("{name} (incomplete)") };
                    ui.selectable_value(&mut pick, *id, label);
                }
            });
            if pick != g.rune_current {
                act(s, "PUT", "/lol-perks/v1/currentpage".into(), Some(json!(pick)));
            }
        });
    }

    if !g.recommended.is_empty() {
        ui.label("Recommended runes (saved to a \"LoLite\" page, or overwrites the current page if all pages are used):");
        for (label, page) in &g.recommended {
            ui.horizontal(|ui| {
                if ui.small_button("Use").clicked() {
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
            let ids: Vec<i64> = perks.iter().filter_map(|p| p["id"].as_i64()).collect();
            Some((label, json!({ "primaryStyleId": r["primaryPerkStyleId"], "subStyleId": r["secondaryPerkStyleId"], "selectedPerkIds": ids })))
        })
        .collect()
}

/// Writes a recommended page into the "LoLite" page, a new page if there is room, or else the current page.
fn use_runes(s: &S, mut page: Value) {
    let s = s.clone();
    std::thread::spawn(move || {
        let Some(api) = s.lock().unwrap().lcu.clone() else { return };
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
        let res = match target {
            Some(id) => {
                page["id"] = json!(id);
                api.send("PUT", &format!("/lol-perks/v1/pages/{id}"), Some(page)).and_then(|_| api.send("PUT", "/lol-perks/v1/currentpage", Some(json!(id))))
            }
            None => api.send("POST", "/lol-perks/v1/pages", Some(page)),
        };
        s.lock().unwrap().error = res.err().unwrap_or_default();
    });
}

fn hover(s: &S, t: &Turn, id: i64) {
    act(s, "PATCH", format!("/lol-champ-select/v1/session/actions/{}", t.aid), Some(json!({ "championId": id })));
}

fn lock_in(s: &S, t: &Turn) {
    act(s, "PATCH", format!("/lol-champ-select/v1/session/actions/{}", t.aid), Some(json!({ "championId": t.current, "completed": true })));
}

fn settings_path() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("LoLite").join("settings.txt")
}

fn load_improved() -> bool {
    std::fs::read_to_string(settings_path()).is_ok_and(|t| t.contains("improved_layout=1"))
}

fn save_improved(on: bool) {
    let p = settings_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let _ = std::fs::write(p, format!("improved_layout={}\n", on as u8));
}

struct App {
    s: S,
    queue: i64,
    search: String,
    kill_vanguard: bool,
    improved: bool,
    /// Which layout's style is currently applied to the context.
    styled: Option<bool>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.styled != Some(self.improved) {
            if self.improved {
                fancy::apply_style(ui.ctx());
            } else {
                ui.ctx().all_styles_mut(|st| *st = egui::Style::default());
                ui.ctx().set_visuals(egui::Visuals::dark());
            }
            self.styled = Some(self.improved);
        }
        if self.improved {
            self.draw_fancy(ui);
        } else {
            egui::CentralPanel::default().show(ui, |ui| self.draw(ui));
        }
    }
}

impl App {
    fn options_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("⚙ Options", |ui| {
            if ui.checkbox(&mut self.improved, "Improved layout").changed() {
                save_improved(self.improved);
            }
            ui.checkbox(&mut self.kill_vanguard, "Kill Riot also stops Vanguard")
                .on_hover_text("Stops vgc service + tray (UAC prompt). Kernel driver stays until reboot.");
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

    fn draw(&mut self, ui: &mut egui::Ui) {
        let s = self.s.clone();
        let mut g = s.lock().unwrap();

        ui.horizontal(|ui| {
            ui.strong(if g.summoner.is_empty() { "LoLite" } else { &g.summoner });
            ui.label(match g.phase.as_str() {
                "" => g.status.as_str(),
                "None" => "Idle",
                p => p,
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.options_menu(ui));
        });
        ui.horizontal(|ui| {
            if g.lcu.is_some() {
                if ui.button(if g.want_ux { "Hide full client" } else { "Open full client" }).clicked() {
                    self.toggle_full_client(&mut g);
                }
            } else if !g.auto_launch && ui.button("Start League").clicked() {
                g.auto_launch = true;
            } else if g.need_login && ui.button("Log in (opens Riot Client)").clicked() {
                Self::log_in();
            }
            if ui.button("Kill Riot").clicked() {
                self.kill_riot(&mut g);
            }
            ui.checkbox(&mut self.kill_vanguard, "also Vanguard")
                .on_hover_text("Stops vgc service + tray (UAC prompt). Kernel driver stays until reboot.");
        });
        if !g.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &g.error);
        }

        for inv in g.invites.clone() {
            let id = inv["invitationId"].as_str().unwrap_or("").to_string();
            ui.horizontal(|ui| {
                let from = inv["fromSummonerName"].as_str().filter(|n| !n.is_empty()).unwrap_or("friend");
                let queue = queue_name(&g, inv["gameConfig"]["queueId"].as_i64().unwrap_or(0));
                ui.strong(format!("Invite: {from}  {queue}"));
                if ui.add_enabled(inv["canAcceptInvitation"] != false, egui::Button::new("Accept")).clicked() {
                    act(&s, "POST", format!("/lol-lobby/v2/received-invitations/{id}/accept"), None);
                }
                if ui.button("Decline").clicked() {
                    act(&s, "POST", format!("/lol-lobby/v2/received-invitations/{id}/decline"), None);
                }
            });
        }
        ui.separator();

        if g.lcu.is_none() {
            return;
        }
        match g.phase.as_str() {
            "None" => self.queue_picker(ui, &g),
            "Lobby" => {
                self.lobby(ui, &g);
                ui.horizontal(|ui| {
                    if ui.add_enabled(g.lobby["canStartActivity"] == true, egui::Button::new("Find match")).clicked() {
                        act(&s, "POST", "/lol-lobby/v2/lobby/matchmaking/search".into(), None);
                    }
                    if ui.button("Leave").clicked() {
                        act(&s, "DELETE", "/lol-lobby/v2/lobby".into(), None);
                    }
                });
                ui.separator();
                self.queue_picker(ui, &g);
            }
            "Matchmaking" => {
                self.lobby(ui, &g);
                ui.heading(format!(
                    "In queue {} (est. {})",
                    mmss(g.search["timeInQueue"].as_f64().unwrap_or(0.0)),
                    mmss(g.search["estimatedQueueTime"].as_f64().unwrap_or(0.0))
                ));
                if ui.button("Cancel").clicked() {
                    act(&s, "DELETE", "/lol-lobby/v2/lobby/matchmaking/search".into(), None);
                }
            }
            "ReadyCheck" => {
                ui.heading("Match found!");
                ui.label(format!("Your response: {}", g.ready["playerResponse"].as_str().unwrap_or("?")));
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("ACCEPT").min_size(egui::vec2(120.0, 40.0))).clicked() {
                        act(&s, "POST", "/lol-matchmaking/v1/ready-check/accept".into(), None);
                    }
                    if ui.button("Decline").clicked() {
                        act(&s, "POST", "/lol-matchmaking/v1/ready-check/decline".into(), None);
                    }
                });
            }
            "ChampSelect" => self.champ_select(ui, &g),
            "GameStart" | "InProgress" => {
                ui.heading("In game");
            }
            "Reconnect" => {
                if ui.button("Reconnect").clicked() {
                    act(&s, "POST", "/lol-gameflow/v1/reconnect".into(), None);
                }
            }
            "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => {
                ui.heading("Game over");
                if ui.button("Play again").clicked() {
                    act(&s, "POST", "/lol-lobby/v2/play-again".into(), None);
                }
            }
            _ => {}
        }
        ui.separator();
        ui.checkbox(&mut g.auto_accept, "Auto-accept match");
    }

    fn queue_picker(&mut self, ui: &mut egui::Ui, g: &Shared) {
        let selected = if self.queue == 0 { "Pick a queue".into() } else { queue_name(g, self.queue) };
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("queue").width(220.0).selected_text(selected).show_ui(ui, |ui| {
                for (id, n, _) in &g.queues {
                    ui.selectable_value(&mut self.queue, *id, n);
                }
            });
            if ui.add_enabled(self.queue != 0, egui::Button::new("Create lobby")).clicked() {
                act(&self.s, "POST", "/lol-lobby/v2/lobby".into(), Some(json!({ "queueId": self.queue })));
            }
        });
    }

    fn lobby(&mut self, ui: &mut egui::Ui, g: &Shared) {
        let l = &g.lobby;
        ui.label(queue_name(g, l["gameConfig"]["queueId"].as_i64().unwrap_or(0)));
        for m in l["members"].as_array().into_iter().flatten() {
            ui.label(format!(
                "• {}  {} {}",
                member_name(m),
                m["firstPositionPreference"].as_str().unwrap_or(""),
                m["secondPositionPreference"].as_str().unwrap_or("")
            ));
        }
        if g.phase == "Lobby" && l["gameConfig"]["showPositionSelector"] == true {
            self.role_pickers(ui, g);
        }
    }

    fn role_pickers(&self, ui: &mut egui::Ui, g: &Shared) {
        let me = &g.lobby["localMember"];
        let mut first = me["firstPositionPreference"].as_str().unwrap_or("").to_string();
        let mut second = me["secondPositionPreference"].as_str().unwrap_or("").to_string();
        let (f0, s0) = (first.clone(), second.clone());
        let shown = |r: &str| if r.is_empty() || r == "UNSELECTED" { "—".to_string() } else { r.to_string() };
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("r1").selected_text(shown(&first)).show_ui(ui, |ui| {
                for r in ROLES {
                    ui.selectable_value(&mut first, r.to_string(), *r);
                }
            });
            // Riot rejects a secondary role when the primary is FILL (QPInvalidPositionSelectionRestriction).
            if first != "FILL" {
                egui::ComboBox::from_id_salt("r2").selected_text(shown(&second)).show_ui(ui, |ui| {
                    for r in ROLES.iter().filter(|r| **r != first) {
                        ui.selectable_value(&mut second, r.to_string(), *r);
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

    fn champ_select(&mut self, ui: &mut egui::Ui, g: &Shared) {
        let sess = &g.session;
        let me = sess["localPlayerCellId"].as_i64();
        let left = sess["timer"]["adjustedTimeLeftInPhase"].as_f64().unwrap_or(0.0) / 1000.0;
        ui.heading(format!("{}  {}s", sess["timer"]["phase"].as_str().unwrap_or(""), left as i64));

        for p in sess["myTeam"].as_array().into_iter().flatten() {
            let id = p["championId"].as_i64().filter(|i| *i != 0).or(p["championPickIntent"].as_i64()).unwrap_or(0);
            let you = if p["cellId"].as_i64() == me { " (you)" } else { "" };
            ui.horizontal(|ui| {
                ui.label(format!("{:8} {}{you}", p["assignedPosition"].as_str().unwrap_or(""), champ_name(g, id)));
                swap_buttons(ui, &self.s, g, p["cellId"].as_i64());
            });
        }
        ui.separator();
        bench(ui, &self.s, g);
        loadout(ui, &self.s, g);
        let enemies = enemy_team(g);
        if !enemies.is_empty() {
            ui.separator();
            ui.label("Enemy team:");
            for (pos, champ) in enemies {
                ui.label(format!("{pos:8} {champ}"));
            }
            ui.label(format!("Bans — ours: {} | theirs: {}", bans(g, "myTeamBans"), bans(g, "theirTeamBans")));
        }
        ui.separator();

        let t = match my_turn(g) {
            Ok(t) => t,
            Err(locked) => {
                ui.label(locked.map_or("Waiting…".into(), |id| format!("Locked in: {}", champ_name(g, id))));
                return;
            }
        };
        ui.horizontal(|ui| {
            ui.label(if t.is_ban { "Ban:" } else if t.active { "Pick:" } else { "Pre-pick:" });
            ui.text_edit_singleline(&mut self.search);
            let verb = if t.is_ban { "Ban" } else { "Lock in" };
            if ui.add_enabled(t.active && t.current != 0, egui::Button::new(format!("{verb} {}", champ_name(g, t.current)))).clicked() {
                lock_in(&self.s, &t);
            }
        });
        let options = champ_options(g, &t, &self.search);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (id, name) in options {
                if ui.selectable_label(id == t.current, name).clicked() {
                    hover(&self.s, &t, id);
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    let s: S = Arc::new(Mutex::new(Shared { auto_launch: true, ..Default::default() }));
    let icon = egui::IconData { rgba: include_bytes!("../assets/icon64.rgba").to_vec(), width: 64, height: 64 };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("LoLite").with_inner_size([460.0, 680.0]).with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "LoLite",
        opts,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let s2 = s.clone();
            std::thread::spawn(move || poller(s2, ctx));
            Ok(Box::new(App { s, queue: 0, search: String::new(), kill_vanguard: false, improved: load_improved(), styled: None }))
        }),
    )
}
