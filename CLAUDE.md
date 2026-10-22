# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

LoLite: Windows-only egui/eframe desktop app that replaces the Riot Client + League UI. It never talks to Riot servers or the game; it drives the official clients through their local HTTPS APIs on `127.0.0.1` (auth from lockfiles). See README.md for user-facing behavior and the risk disclaimer.

## Commands

```sh
cargo build --release      # output: target/release/lolite.exe
cargo run                  # debug run (needs Riot Client/League installed)
cargo clippy
python assets/make_icon.py # regenerate lolite.ico + icon64.rgba after icon design changes
pwsh -File scripts/bench.ps1 -Mode full|lolite  # startup time + RAM for README numbers; kills all Riot/League processes
```

- Build needs the Windows SDK (`rc.exe`): `build.rs` embeds `assets/lolite.ico` via `winresource`.
- No tests exist. Verifying changes means running the app against a live League client.
- Rust edition 2024. Release profile uses `panic = "abort"` and `strip`, so panics in release are silent (app is `windows_subsystem = "windows"`, no console).

## Architecture

Three modules, one shared state:

- **`Shared` (`src/main.rs`)** behind `Arc<Mutex<_>>` (alias `S`). Single source of truth between the poller thread and the UI. LCU JSON (lobby, search, ready check, champ-select session, invites) is stored as raw `serde_json::Value`, not typed structs.
- **`poller` thread (`src/main.rs`)**: event-driven. `Api::events` (`src/riot.rs`, a minimal hand-written WebSocket client, no extra crate) subscribes to LCU `OnJsonApiEvent` and forwards uris matching `WATCH` into a channel. The poller blocks on that channel and refetches only the dirty group (`game` / `chat` / `friends`); a quiet timeout (10s, 3s in champ select) refreshes everything as a safety net. Without the WebSocket it falls back to polling every 1s (400ms in ChampSelect/ReadyCheck). Friends refresh at most every 5s; in game, chat waits 5s and friends 30s. UI code can wake it with `poke()`. Writes go through `put()` and `ctx.request_repaint()` runs only when something changed; timers (queue, ready check, champ select via `internalNowInEpochMs`) count locally with `request_repaint_after(1s)` in those phases only. If `Shared.auto_picks` is set, `auto_pick` pre-picks the first available priority champion for the assigned lane (all lanes in order when none) and, on our turn, applies its recommended spells (`summonerSpellIds`) and runes and locks it. If LCU is not reachable, `launch_step` runs the launch state machine: start headless `RiotClientServices.exe` → wait for `/rso-auth/v1/authorization` = 200 → POST product-launcher "Play". Non-200 means not logged in (`need_login`).
- **`ux_guard` thread**: League reopens its own UI (match found, champ select, after games). Every 1s it reads `/riotclient/ux-state` and checks for visible League (`RCLIENT` class) and "Riot Client" windows via `FindWindowW` (ux-state can say `"Quit"` while a window is up) and sends `/riotclient/kill-ux` when either shows the UI (and on phase changes), also killing a stray `Riot Client.exe`, except in game or when `want_ux` is set. Phase and ux-state changes are logged to `%APPDATA%\LoLite\lolite.log`.
- **UI actions** never block the UI thread: `act()` spawns a thread, sends the request via the cached `lcu` `Api`, and writes any error into `Shared.error`. Champ-select helpers (`hover`, `lock_in`, `use_runes`, swaps) build on it.
- **`src/riot.rs`**: `Api` (lockfile parse `name:pid:port:password:proto`, Basic auth `riot:<password>`, TLS accepting Riot's self-signed cert), `alive()` fast TCP check for stale lockfiles, process launch/kill via `taskkill` with `CREATE_NO_WINDOW`, Vanguard stop. Install paths (League folder, `RiotClientServices.exe`) live in a static set by `set_paths`: settings override, else `%PROGRAMDATA%\Riot Games\RiotClientInstalls.json`, else `C:\Riot Games\...`; Riot Client lockfile lives under `%LOCALAPPDATA%`.
- **`Cached`** re-reads a lockfile each tick and rebuilds `Api` only when its contents change.
- **UI**: `App::draw_fancy` in `src/fancy.rs` (a second `impl App` block plus its theme constants and `apply_style`, applied once at startup). It loads Segoe UI from `C:\Windows\Fonts` at runtime; the `"bold"` family falls back to egui's font when Segoe is missing (an unbound family panics). It uses the shared helpers in `main.rs` (`my_turn`, `champ_options`, `queue_name`, `act`, `role_pickers`, `auto_pick_editor`, etc.). A classic layout existed until v0.1.1; it saved only ~7 MB RAM.
- Static data (queues, champions, summoner spells, summoner name) is loaded once per LCU connection via `load_static` and cleared when LCU disconnects.
- LCU often leaves `summonerName` empty; `fill_name`/`riot_id` resolve Riot IDs from puuid with a per-poller cache.
- Friends/chat (`src/social.rs`) is the right side panel. Poller fetches `/lol-chat/v1/friends` and conversations every tick, and messages only for the open chat (`Shared.chat_with` = friend pid).
- i18n (`src/i18n.rs`): every UI string is an inline pair `tr("English", "Português")`; no key tables. New UI text must use `tr`. Data from LCU (champion/queue/spell names) already comes in the client's locale.
- Settings persist in `%APPDATA%\LoLite\settings.txt` (`lang=en|pt`, `league_dir`/`riot_client` path overrides, empty = auto-detect, `picks_top|jungle|middle|bottom|utility=id,id,...`; without `lang`, language follows `/riotclient/region-locale`).

## Conventions

- Keep the binary small and dependencies minimal (blocking `ureq`, no async runtime, no typed API models). Index `Value`s directly with `v["field"]`.
- Endpoints mirror what the official League UI sends; Riot can change them between patches.
