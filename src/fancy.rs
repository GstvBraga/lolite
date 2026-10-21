//! "Improved layout": graphite surfaces, one accent for "you / act", Segoe UI, countdown rings.
//! Same data and actions as the classic layout. Painter shapes only; no images or animation, so cost matches classic.
use crate::*;
use eframe::egui::{
    Align2, Button, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Rect, RichText, Sense, Shape, Stroke, TextStyle, Ui,
    pos2, vec2,
};

const CANVAS: Color32 = Color32::from_rgb(0x15, 0x17, 0x1C);
const SURFACE: Color32 = Color32::from_rgb(0x1C, 0x1F, 0x26);
const RAISED: Color32 = Color32::from_rgb(0x25, 0x2A, 0x34);
const HOVER: Color32 = Color32::from_rgb(0x2E, 0x34, 0x40);
const LINE: Color32 = Color32::from_rgb(0x2C, 0x31, 0x3C);
const TEXT: Color32 = Color32::from_rgb(0xED, 0xEF, 0xF3);
const SUB: Color32 = Color32::from_rgb(0x90, 0x97, 0xA6);
const ACCENT: Color32 = Color32::from_rgb(0x5B, 0x7C, 0xFA);
const ACCENT_TINT: Color32 = Color32::from_rgb(0x26, 0x30, 0x55);
const GOOD: Color32 = Color32::from_rgb(0x3D, 0xD6, 0x8C);
const WARN: Color32 = Color32::from_rgb(0xF5, 0xB8, 0x41);
const BAD: Color32 = Color32::from_rgb(0xF2, 0x55, 0x5A);

const GROUPS: &[&str] = &["Summoner's Rift", "ARAM", "Arena", "Featured", "Teamfight Tactics", "Co-op vs AI"];

fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Segoe UI from Windows (nothing embedded in the exe); egui's fonts stay as fallback for symbols.
/// The "bold" family always exists (egui's font without Segoe): font changes apply a frame late, so it must be bound before the first draw.
pub fn fonts(segoe: bool) -> FontDefinitions {
    let mut f = FontDefinitions::default();
    let defaults = f.families[&FontFamily::Proportional].clone();
    let dir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
    let mut bold = defaults.clone();
    let files: &[(&str, &str)] = if segoe { &[("segoe", "segoeui.ttf"), ("segoe-bold", "segoeuib.ttf")] } else { &[] };
    for &(key, file) in files {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            f.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
            let family = if key == "segoe" { f.families.get_mut(&FontFamily::Proportional).unwrap() } else { &mut bold };
            family.insert(0, key.into());
        }
    }
    f.families.insert(bold_family(), bold);
    f
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_fonts(fonts(true));
    ctx.all_styles_mut(|st| {
        let mut v = egui::Visuals::dark();
        v.panel_fill = CANVAS;
        v.window_fill = SURFACE;
        v.window_stroke = Stroke::new(1.0, LINE);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.extreme_bg_color = RAISED;
        v.faint_bg_color = SURFACE;
        v.override_text_color = Some(TEXT);
        v.weak_text_color = Some(SUB);
        v.selection.bg_fill = ACCENT_TINT;
        v.selection.stroke = Stroke::new(1.5, ACCENT);
        v.hyperlink_color = ACCENT;
        for (w, fill, stroke) in [
            (&mut v.widgets.inactive, RAISED, Color32::TRANSPARENT),
            (&mut v.widgets.hovered, HOVER, Color32::TRANSPARENT),
            (&mut v.widgets.active, HOVER, ACCENT),
            (&mut v.widgets.open, HOVER, Color32::TRANSPARENT),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0, stroke);
            w.corner_radius = CornerRadius::same(8);
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.noninteractive.corner_radius = CornerRadius::same(8);
        st.visuals = v;
        st.spacing.item_spacing = vec2(8.0, 8.0);
        st.spacing.button_padding = vec2(12.0, 6.0);
        st.spacing.interact_size.y = 30.0;
        for (ts, size) in [(TextStyle::Heading, 20.0), (TextStyle::Body, 14.0), (TextStyle::Button, 14.0), (TextStyle::Small, 12.0)] {
            st.text_styles.insert(ts, FontId::proportional(size));
        }
    });
}

fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).family(bold_family()).size(size)
}

fn sub(text: impl Into<String>) -> RichText {
    RichText::new(text).size(13.0).color(SUB)
}

/// Grouping surface. `title` is the card's bold heading; empty for none.
fn card(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(bold(title, 15.0));
                ui.add_space(2.0);
            }
            add(ui);
        });
    ui.add_space(12.0);
}

fn primary(ui: &mut Ui, text: &str, fill: Color32, enabled: bool) -> bool {
    let b = Button::new(bold(text, 15.0).color(if fill == ACCENT { Color32::WHITE } else { CANVAS }))
        .fill(fill)
        .corner_radius(CornerRadius::same(8))
        .min_size(vec2(ui.available_width(), 42.0));
    // Button text follows the parent's alignment; center it.
    ui.vertical_centered(|ui| ui.add_enabled(enabled, b)).inner.clicked()
}

/// Secondary action as plain text, so it never competes with the primary button.
fn ghost(ui: &mut Ui, text: &str, color: Color32) -> bool {
    ui.add(Button::new(RichText::new(text).color(color)).frame(false)).clicked()
}

fn bar(ui: &mut Ui, frac: f64, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(3), RAISED);
    let w = (rect.width() * frac.clamp(0.0, 1.0) as f32).max(6.0);
    ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(w, 6.0)), CornerRadius::same(3), color);
}

/// Countdown ring: arc of the time left with the seconds in the middle.
fn ring(ui: &mut Ui, frac: f64, secs: i64, radius: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(radius * 2.0 + 6.0, radius * 2.0 + 6.0), Sense::hover());
    let c = rect.center();
    let p = ui.painter();
    p.circle_stroke(c, radius, Stroke::new(4.0, RAISED));
    let frac = frac.clamp(0.0, 1.0) as f32;
    if frac > 0.0 {
        let n = (64.0 * frac).ceil().max(2.0) as usize;
        let pts: Vec<_> = (0..=n)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * frac * i as f32 / n as f32;
                pos2(c.x + radius * a.cos(), c.y + radius * a.sin())
            })
            .collect();
        p.add(Shape::line(pts, Stroke::new(4.0, color)));
    }
    p.text(c, Align2::CENTER_CENTER, secs.max(0).to_string(), FontId::new(radius * 0.8, bold_family()), TEXT);
}

/// Modern on/off switch.
fn toggle(ui: &mut Ui, on: &mut bool, label: &str) {
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 20.0), Sense::click());
        if resp.clicked() {
            *on = !*on;
        }
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(10), if *on { ACCENT } else { RAISED });
        let x = if *on { rect.right() - 10.0 } else { rect.left() + 10.0 };
        p.circle_filled(pos2(x, rect.center().y), 7.0, if *on { Color32::WHITE } else { SUB });
        if ui.add(egui::Label::new(RichText::new(label).color(if *on { TEXT } else { SUB })).sense(Sense::click())).clicked() {
            *on = !*on;
        }
    });
}

/// Small rounded tag, e.g. a role or a ban.
/// Allocated as one fixed-size widget so wrapped rows move it to the next line instead of squeezing it.
fn tag(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.into(), FontId::proportional(12.0), color);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(14.0, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, 6, color.gamma_multiply(0.16));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
}

fn phase_step(phase: &str) -> Option<usize> {
    Some(match phase {
        "Lobby" => 0,
        "Matchmaking" => 1,
        "ReadyCheck" => 2,
        "ChampSelect" => 3,
        "GameStart" | "InProgress" | "Reconnect" | "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => 4,
        _ => return None,
    })
}

fn phase_color(phase: &str) -> Color32 {
    match phase {
        "" => SUB,
        "None" => GOOD,
        "GameStart" | "InProgress" => BAD,
        "Reconnect" => WARN,
        _ => ACCENT,
    }
}

/// Stepper: lobby → queue → found → pick → game, as five segments.
fn phase_track(ui: &mut Ui, phase: &str) {
    let steps = [tr("Lobby", "Saguão"), tr("Queue", "Fila"), tr("Found", "Encontrada"), tr("Pick", "Escolha"), tr("Game", "Jogo")];
    let current = phase_step(phase);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let p = ui.painter();
    let gap = 6.0;
    let w = (rect.width() - gap * (steps.len() - 1) as f32) / steps.len() as f32;
    for (i, label) in steps.iter().enumerate() {
        let x = rect.left() + i as f32 * (w + gap);
        let (fill, color) = match current {
            Some(c) if i == c => (ACCENT, TEXT),
            Some(c) if i < c => (SUB.gamma_multiply(0.6), SUB),
            _ => (RAISED, SUB.gamma_multiply(0.7)),
        };
        p.rect_filled(Rect::from_min_size(pos2(x, rect.top()), vec2(w, 4.0)), CornerRadius::same(2), fill);
        p.text(pos2(x, rect.top() + 9.0), Align2::LEFT_TOP, *label, FontId::proportional(12.0), color);
    }
}

fn group_name(group: &str) -> &str {
    match group {
        "Featured" => tr("Featured", "Em destaque"),
        "Co-op vs AI" => tr("Co-op vs AI", "Cooperativo vs IA"),
        g => g,
    }
}

impl App {
    pub(crate) fn draw_fancy(&mut self, ui: &mut Ui) {
        let s = self.s.clone();
        let mut g = s.lock().unwrap();

        egui::Panel::top("header").frame(Frame::new().fill(CANVAS).inner_margin(Margin { left: 18, right: 14, top: 14, bottom: 8 })).show(ui, |ui| {
            ui.horizontal(|ui| {
                let name = if g.summoner.is_empty() { "LoLite".to_string() } else { g.summoner.clone() };
                let (game_name, tagline) = name.split_once('#').unwrap_or((&name, ""));
                social::avatar(ui, game_name, 18.0, ACCENT);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.label(bold(game_name, 15.0));
                        if !tagline.is_empty() {
                            ui.label(sub(format!("#{tagline}")));
                        }
                    });
                    let status = if g.phase.is_empty() { g.status.clone() } else { i18n::phase(&g.phase).to_string() };
                    ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.5, phase_color(&g.phase));
                        ui.label(sub(status));
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.options_menu(ui));
            });
            if g.lcu.is_some() {
                ui.add_space(10.0);
                phase_track(ui, &g.phase);
            }
        });

        egui::Panel::bottom("footer").frame(Frame::new().fill(CANVAS).inner_margin(Margin::symmetric(18, 10))).show(ui, |ui| {
            if !g.error.is_empty() {
                ui.label(RichText::new(&g.error).size(12.0).color(BAD));
            }
            ui.horizontal(|ui| {
                toggle(ui, &mut g.auto_accept, tr("Auto-accept", "Aceitar automaticamente"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ghost(ui, tr("Kill Riot", "Fechar Riot"), BAD) {
                        self.kill_riot(&mut g);
                    }
                    if g.lcu.is_some() {
                        let text = if g.want_ux { tr("Hide full client", "Ocultar cliente completo") } else { tr("Full client", "Cliente completo") };
                        if ghost(ui, text, SUB) {
                            self.toggle_full_client(&mut g);
                        }
                    }
                });
            });
        });

        egui::CentralPanel::default().frame(Frame::new().fill(CANVAS).inner_margin(Margin { left: 18, right: 18, top: 6, bottom: 6 })).show(ui, |ui| {
            // Timer and pick order stay pinned while the picker and runes scroll.
            if g.phase == "ChampSelect" {
                champ_select_header(ui, &g);
            }
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.fancy_body(ui, &mut g));
        });
    }

    fn fancy_body(&mut self, ui: &mut Ui, g: &mut Shared) {
        for inv in g.invites.clone() {
            let id = inv["invitationId"].as_str().unwrap_or("").to_string();
            card(ui, "", |ui| {
                let from = inv["fromSummonerName"].as_str().filter(|n| !n.is_empty()).unwrap_or(tr("A friend", "Um amigo"));
                ui.horizontal(|ui| {
                    social::avatar(ui, from, 16.0, ACCENT);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(bold(format!("{from} {}", tr("invited you", "convidou você")), 15.0));
                        ui.label(sub(queue_name(g, inv["gameConfig"]["queueId"].as_i64().unwrap_or(0))));
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let join = Button::new(bold(tr("Join party", "Entrar no grupo"), 14.0).color(Color32::WHITE)).fill(ACCENT).min_size(vec2(140.0, 34.0));
                    if ui.add_enabled(inv["canAcceptInvitation"] != false, join).clicked() {
                        act(&self.s, "POST", format!("/lol-lobby/v2/received-invitations/{id}/accept"), None);
                    }
                    if ghost(ui, tr("Decline", "Recusar"), SUB) {
                        act(&self.s, "POST", format!("/lol-lobby/v2/received-invitations/{id}/decline"), None);
                    }
                });
            });
        }

        if g.lcu.is_none() {
            card(ui, "", |ui| {
                ui.label(bold(&g.status, 20.0));
                if g.need_login {
                    ui.label(sub(tr(
                        "Log in once with \"Stay signed in\" ticked; LoLite handles the rest.",
                        "Entre uma vez com \"Manter conectado\" marcado; o LoLite cuida do resto.",
                    )));
                    ui.add_space(6.0);
                    if primary(ui, tr("Log in", "Entrar"), ACCENT, true) {
                        Self::log_in();
                    }
                } else if !g.auto_launch {
                    ui.add_space(6.0);
                    if primary(ui, tr("Start League", "Iniciar League"), ACCENT, true) {
                        g.auto_launch = true;
                    }
                } else {
                    ui.label(sub(tr("This takes about 25 seconds.", "Isso leva cerca de 25 segundos.")));
                    ui.add_space(4.0);
                    ui.add(egui::Spinner::new().color(ACCENT));
                }
            });
            return;
        }

        match g.phase.as_str() {
            "None" => self.fancy_play(ui, g, tr("Choose a mode", "Escolha um modo")),
            "Lobby" => {
                self.fancy_lobby(ui, g);
                self.fancy_play(ui, g, tr("Change mode", "Mudar modo"));
            }
            "Matchmaking" => {
                card(ui, "", |ui| {
                    let t = queue_time(g);
                    let est = g.search["estimatedQueueTime"].as_f64().unwrap_or(0.0);
                    ui.horizontal(|ui| {
                        ui.label(bold(mmss(t), 40.0));
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.add_space(8.0);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(bold(tr("Searching for a match", "Procurando partida"), 15.0));
                            ui.label(sub(format!("{} {}", tr("Estimated", "Estimado"), mmss(est))));
                        });
                    });
                    if est > 0.0 {
                        bar(ui, t / est, ACCENT);
                    }
                    ui.add_space(2.0);
                    if ghost(ui, tr("Cancel search", "Cancelar busca"), BAD) {
                        act(&self.s, "DELETE", "/lol-lobby/v2/lobby/matchmaking/search".into(), None);
                    }
                });
                self.fancy_lobby(ui, g);
            }
            "ReadyCheck" => card(ui, "", |ui| {
                let elapsed = ready_time(g);
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    ring(ui, 1.0 - elapsed / 12.0, (12.0 - elapsed).ceil() as i64, 34.0, GOOD);
                    ui.label(bold(tr("Match found", "Partida encontrada"), 22.0));
                });
                ui.add_space(8.0);
                match g.ready["playerResponse"].as_str().unwrap_or("") {
                    "Accepted" => {
                        ui.vertical_centered(|ui| ui.label(RichText::new(tr("Accepted. Waiting for the other players.", "Aceita. Aguardando os outros jogadores.")).color(GOOD)));
                    }
                    "Declined" => {
                        ui.vertical_centered(|ui| ui.label(RichText::new(tr("Declined", "Recusada")).color(BAD)));
                    }
                    _ => {
                        let accept = Button::new(bold(tr("Accept", "Aceitar"), 18.0).color(CANVAS)).fill(GOOD).corner_radius(CornerRadius::same(8)).min_size(vec2(ui.available_width(), 52.0));
                        ui.vertical_centered(|ui| {
                            if ui.add(accept).clicked() {
                                act(&self.s, "POST", "/lol-matchmaking/v1/ready-check/accept".into(), None);
                            }
                            if ghost(ui, tr("Decline", "Recusar"), SUB) {
                                act(&self.s, "POST", "/lol-matchmaking/v1/ready-check/decline".into(), None);
                            }
                        });
                    }
                }
            }),
            "ChampSelect" => self.fancy_champ_select(ui, g),
            "GameStart" | "InProgress" => card(ui, "", |ui| {
                ui.label(bold(tr("In game", "Em jogo"), 20.0));
                ui.label(sub(tr("LoLite stays idle while you play.", "O LoLite fica ocioso enquanto você joga.")));
            }),
            "Reconnect" => card(ui, "", |ui| {
                ui.label(bold(tr("Disconnected from the game", "Desconectado da partida"), 20.0));
                ui.add_space(6.0);
                if primary(ui, tr("Reconnect", "Reconectar"), ACCENT, true) {
                    act(&self.s, "POST", "/lol-gameflow/v1/reconnect".into(), None);
                }
            }),
            "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => card(ui, "", |ui| {
                ui.label(bold(tr("Game over", "Fim de jogo"), 20.0));
                ui.add_space(6.0);
                if primary(ui, tr("Play again", "Jogar novamente"), ACCENT, true) {
                    act(&self.s, "POST", "/lol-lobby/v2/play-again".into(), None);
                }
            }),
            p => {
                ui.label(sub(p));
            }
        }
    }

    fn fancy_play(&mut self, ui: &mut Ui, g: &Shared, heading: &str) {
        let current = g.lobby["gameConfig"]["queueId"].as_i64().unwrap_or(0);
        let groups: Vec<&str> = GROUPS.iter().copied().filter(|gr| g.queues.iter().any(|q| q.2 == *gr)).collect();
        // Tab: what the player picked, else the group of the lobby's queue, else the first.
        let lobby_group = g.queues.iter().find(|q| q.0 == current).map(|q| q.2);
        let tab = self.queue_tab.filter(|t| groups.contains(t)).or(lobby_group).or(groups.first().copied());
        card(ui, heading, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for gr in &groups {
                    let on = tab == Some(*gr);
                    let text = RichText::new(group_name(gr)).size(13.0).color(if on { TEXT } else { SUB });
                    if ui.add(Button::new(text).fill(if on { RAISED } else { Color32::TRANSPARENT }).corner_radius(CornerRadius::same(14))).clicked() {
                        self.queue_tab = Some(gr);
                    }
                }
            });
            ui.add_space(6.0);
            let queues: Vec<_> = g.queues.iter().filter(|q| Some(q.2) == tab).collect();
            let cols = 2;
            let w = (ui.available_width() - ui.spacing().item_spacing.x * (cols - 1) as f32) / cols as f32;
            for row in queues.chunks(cols) {
                ui.horizontal(|ui| {
                    for (id, name, _) in row {
                        let label = name.strip_prefix(coop_prefix()).unwrap_or(name);
                        let tile = Button::new(RichText::new(label).size(14.0)).min_size(vec2(w, 44.0)).selected(*id == current);
                        if ui.add(tile).clicked() && *id != current {
                            act(&self.s, "POST", "/lol-lobby/v2/lobby".into(), Some(json!({ "queueId": id })));
                        }
                    }
                });
            }
        });
    }

    fn fancy_lobby(&mut self, ui: &mut Ui, g: &Shared) {
        let l = &g.lobby;
        let in_lobby = g.phase == "Lobby";
        card(ui, "", |ui| {
            ui.horizontal(|ui| {
                ui.label(bold(queue_name(g, l["gameConfig"]["queueId"].as_i64().unwrap_or(0)), 20.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = l["members"].as_array().map_or(0, Vec::len);
                    ui.label(sub(format!("{n}/{}", l["gameConfig"]["maxLobbySize"].as_i64().unwrap_or(5))));
                });
            });
            ui.add_space(4.0);
            for m in l["members"].as_array().into_iter().flatten() {
                ui.horizontal(|ui| {
                    let name = member_name(m);
                    social::avatar(ui, name, 14.0, if m["isLeader"] == true { ACCENT } else { HOVER });
                    ui.label(RichText::new(name).size(14.0));
                    if m["isLeader"] == true {
                        ui.label(sub(tr("leader", "líder")));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for k in ["secondPositionPreference", "firstPositionPreference"] {
                            if let Some(r) = m[k].as_str().filter(|r| !r.is_empty() && *r != "UNSELECTED") {
                                tag(ui, i18n::role(r), SUB);
                            }
                        }
                    });
                });
            }
            if in_lobby {
                if l["gameConfig"]["showPositionSelector"] == true {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(sub(tr("Your roles", "Suas rotas")));
                        self.role_pickers(ui, g);
                    });
                }
                ui.add_space(8.0);
                if primary(ui, tr("Find match", "Procurar partida"), ACCENT, l["canStartActivity"] == true) {
                    act(&self.s, "POST", "/lol-lobby/v2/lobby/matchmaking/search".into(), None);
                }
                ui.vertical_centered(|ui| {
                    if ghost(ui, tr("Leave lobby", "Sair do saguão"), SUB) {
                        act(&self.s, "DELETE", "/lol-lobby/v2/lobby".into(), None);
                    }
                });
            }
        });
    }

    fn fancy_champ_select(&mut self, ui: &mut Ui, g: &Shared) {
        let sess = &g.session;
        match my_turn(g) {
            Ok(t) => {
                let (heading, accent) = match (t.is_ban, t.active) {
                    (true, _) => (tr("Ban a champion", "Banir um campeão"), BAD),
                    (false, true) => (tr("Pick your champion", "Escolha seu campeão"), ACCENT),
                    (false, false) => (tr("Pre-pick, so your team knows your plan", "Pré-escolha, para seu time saber seu plano"), ACCENT),
                };
                card(ui, heading, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(tr("Search champion…", "Buscar campeão…")).desired_width(f32::INFINITY).margin(Margin::symmetric(10, 7)));
                    ui.add_space(2.0);
                    let verb = if t.is_ban { tr("Ban", "Banir") } else { tr("Lock in", "Confirmar") };
                    let label = if t.current == 0 { verb.to_string() } else { format!("{verb} {}", champ_name(g, t.current)) };
                    if primary(ui, &label, accent, t.active && t.current != 0) {
                        lock_in(&self.s, &t);
                    }
                    ui.add_space(4.0);
                    let options = champ_options(g, &t, &self.search);
                    // Own short scroll so runes stay right below instead of after every champion.
                    egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
                        let cols = ((ui.available_width() / 120.0) as usize).clamp(2, 6);
                        let w = (ui.available_width() - ui.spacing().item_spacing.x * (cols - 1) as f32) / cols as f32;
                        for row in options.chunks(cols) {
                            ui.horizontal(|ui| {
                                for (id, name) in row {
                                    let tile = Button::new(RichText::new(name).size(13.0)).min_size(vec2(w, 32.0)).selected(*id == t.current);
                                    if ui.add(tile).clicked() {
                                        hover(&self.s, &t, *id);
                                    }
                                }
                            });
                        }
                    });
                });
            }
            Err(locked) => card(ui, "", |ui| match locked {
                Some(id) => {
                    ui.label(sub(tr("Locked in", "Confirmado")));
                    ui.label(bold(champ_name(g, id), 20.0).color(ACCENT));
                }
                None => {
                    ui.label(sub(tr("Waiting for your turn…", "Aguardando sua vez…")));
                }
            }),
        }

        card(ui, tr("Spells and runes", "Feitiços e runas"), |ui| {
            loadout(ui, &self.s, g);
            ui.label(sub(tr("Edit rune pages in the full client.", "Edite páginas de runas no cliente completo.")));
        });
        if sess["benchEnabled"] == true {
            card(ui, tr("Bench", "Banco"), |ui| bench(ui, &self.s, g));
        }

        let enemies = enemy_team(g);
        card(ui, "", |ui| {
            // Side by side when there's room, stacked on narrow windows.
            if !enemies.is_empty() && ui.available_width() >= 460.0 {
                ui.columns(2, |cols| {
                    self.ally_team(&mut cols[0], g);
                    enemy_list(&mut cols[1], g, &enemies);
                });
            } else {
                self.ally_team(ui, g);
                if !enemies.is_empty() {
                    ui.add_space(8.0);
                    enemy_list(ui, g, &enemies);
                }
            }
        });
    }

    fn ally_team(&self, ui: &mut Ui, g: &Shared) {
        let sess = &g.session;
        let me = sess["localPlayerCellId"].as_i64();
        team_heading(ui, tr("Your team", "Seu time"), ACCENT);
        for p in sess["myTeam"].as_array().into_iter().flatten() {
            let id = p["championId"].as_i64().filter(|i| *i != 0).or(p["championPickIntent"].as_i64()).unwrap_or(0);
            let you = p["cellId"].as_i64() == me;
            let player = p["gameName"].as_str().filter(|n| !n.is_empty()).unwrap_or(tr("Teammate", "Aliado"));
            Frame::new()
                .fill(if you { ACCENT_TINT } else { Color32::TRANSPARENT })
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.horizontal(|ui| {
                        ui.label(bold(champ_name(g, id), 14.0).color(if id == 0 { SUB } else { TEXT }));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(i18n::role(p["assignedPosition"].as_str().unwrap_or(""))).size(12.0).color(SUB));
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label(sub(if you { tr("You", "Você") } else { player }));
                        swap_buttons(ui, &self.s, g, p["cellId"].as_i64());
                    });
                });
        }
        ban_row(ui, g, true);
    }
}

/// Phase, timer and pick order; drawn above the scroll area so it never scrolls away.
fn champ_select_header(ui: &mut Ui, g: &Shared) {
    let sess = &g.session;
    let left = champ_time_left(sess);
    let total = sess["timer"]["totalTimeInPhase"].as_f64().unwrap_or(0.0);
    card(ui, "", |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(bold(champ_select_phase(sess["timer"]["phase"].as_str().unwrap_or("")), 20.0));
                ui.label(sub(queue_name(g, sess["queueId"].as_i64().unwrap_or(0))));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let frac = if total > 0.0 { left / total } else { 0.0 };
                ring(ui, frac, (left / 1000.0).ceil() as i64, 22.0, if left < 10_000.0 { WARN } else { ACCENT });
            });
        });
        let order = pick_order(g);
        if !order.is_empty() {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Pick order", "Ordem de escolha")).size(12.0).color(SUB));
                for (i, (label, ally, now)) in order.iter().enumerate() {
                    let color = if *now { WARN } else if *ally { ACCENT } else { BAD };
                    tag(ui, &format!("{}{} {label}", if *now { "▶ " } else { "" }, i + 1), color);
                }
            });
        }
    });
}

fn enemy_list(ui: &mut Ui, g: &Shared, enemies: &[(String, String)]) {
    team_heading(ui, tr("Enemy team", "Time inimigo"), BAD);
    for (pos, champ) in enemies {
        Frame::new().inner_margin(Margin::symmetric(8, 5)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(bold(champ, 14.0).color(if champ == "—" { SUB } else { TEXT }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(pos).size(12.0).color(SUB)));
            });
            ui.label(sub(" "));
        });
    }
    ban_row(ui, g, false);
}

fn team_heading(ui: &mut Ui, text: &str, color: Color32) {
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, color);
        ui.label(RichText::new(text).size(13.0).color(SUB));
    });
}

fn ban_row(ui: &mut Ui, g: &Shared, ally: bool) {
    let names = bans(g, ally);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Bans", "Banimentos")).size(12.0).color(SUB));
        if names == "—" {
            ui.label(RichText::new("—").size(12.0).color(SUB));
        } else {
            for n in names.split(", ") {
                tag(ui, n, BAD);
            }
        }
    });
}
