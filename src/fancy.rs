//! "Improved layout": themed cards, queue tiles, champion grid. Same data and actions as the classic layout.
use crate::*;
use eframe::egui::{Button, Color32, CornerRadius, FontId, Frame, Margin, ProgressBar, RichText, Stroke, TextStyle, Ui, vec2};

const BG: Color32 = Color32::from_rgb(0x01, 0x0A, 0x13);
const HEADER: Color32 = Color32::from_rgb(0x09, 0x14, 0x24);
const CARD: Color32 = Color32::from_rgb(0x0A, 0x16, 0x28);
const BORDER: Color32 = Color32::from_rgb(0x3C, 0x32, 0x1E);
const WIDGET: Color32 = Color32::from_rgb(0x1E, 0x23, 0x28);
const WIDGET_HOVER: Color32 = Color32::from_rgb(0x2A, 0x30, 0x38);
const GOLD: Color32 = Color32::from_rgb(0xC8, 0xAA, 0x6E);
const TEXT: Color32 = Color32::from_rgb(0xF0, 0xE6, 0xD2);
const MUTED: Color32 = Color32::from_rgb(0xA0, 0x9B, 0x8C);
const TEAL: Color32 = Color32::from_rgb(0x0A, 0xC8, 0xB9);
const TEAL_DARK: Color32 = Color32::from_rgb(0x0A, 0x32, 0x3C);
const RED: Color32 = Color32::from_rgb(0xE8, 0x40, 0x57);

const GROUPS: &[&str] = &["Summoner's Rift", "ARAM", "Arena", "Featured", "Teamfight Tactics", "Co-op vs AI"];

pub fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|st| {
        let mut v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = Color32::from_rgb(0x05, 0x0E, 0x1A);
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = TEAL_DARK;
        v.selection.stroke = Stroke::new(1.0, TEAL);
        let r = CornerRadius::same(3);
        for (w, fill, stroke) in [
            (&mut v.widgets.inactive, WIDGET, BORDER),
            (&mut v.widgets.hovered, WIDGET_HOVER, GOLD),
            (&mut v.widgets.active, WIDGET_HOVER, TEXT),
            (&mut v.widgets.open, WIDGET_HOVER, GOLD),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0, stroke);
            w.corner_radius = r;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        st.visuals = v;
        st.spacing.item_spacing = vec2(8.0, 6.0);
        st.spacing.button_padding = vec2(10.0, 5.0);
        for (ts, size) in [(TextStyle::Heading, 20.0), (TextStyle::Body, 14.0), (TextStyle::Button, 14.0), (TextStyle::Small, 11.0)] {
            st.text_styles.insert(ts, FontId::proportional(size));
        }
    });
}

fn card(ui: &mut Ui, title: &str, accent: Color32, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(RichText::new(title).size(12.0).color(accent).strong());
                ui.add_space(4.0);
            }
            add(ui);
        });
    ui.add_space(8.0);
}

fn big_button(ui: &mut Ui, text: &str, fill: Color32, enabled: bool) -> bool {
    let b = Button::new(RichText::new(text).size(16.0).strong().color(BG)).fill(fill).min_size(vec2(ui.available_width(), 42.0));
    ui.add_enabled(enabled, b).clicked()
}

fn outline_button(ui: &mut Ui, text: &str, color: Color32) -> bool {
    ui.add(Button::new(RichText::new(text).color(color)).fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, color))).clicked()
}

fn pill(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.15))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 1))
        .show(ui, |ui| ui.label(RichText::new(text).size(12.0).color(color)));
}

fn phase_label(g: &Shared) -> (String, Color32) {
    match g.phase.as_str() {
        "" => (g.status.clone(), MUTED),
        "None" => ("Idle".into(), MUTED),
        "Lobby" => ("In lobby".into(), GOLD),
        "Matchmaking" => ("In queue".into(), TEAL),
        "ReadyCheck" => ("Match found".into(), TEAL),
        "ChampSelect" => ("Champ select".into(), GOLD),
        "GameStart" | "InProgress" => ("In game".into(), RED),
        p => (p.to_string(), MUTED),
    }
}

impl App {
    pub(crate) fn draw_fancy(&mut self, ui: &mut Ui) {
        let s = self.s.clone();
        let mut g = s.lock().unwrap();

        egui::Panel::top("header").frame(Frame::new().fill(HEADER).inner_margin(Margin::symmetric(12, 8))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("LOLITE").size(16.0).strong().color(GOLD));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.options_menu(ui);
                    if outline_button(ui, "Kill Riot", RED) {
                        self.kill_riot(&mut g);
                    }
                });
            });
            ui.horizontal(|ui| {
                let name = if g.summoner.is_empty() { "Not connected" } else { g.summoner.as_str() };
                ui.label(RichText::new(name).size(15.0).color(TEXT));
                let (label, color) = phase_label(&g);
                pill(ui, &label, color);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if g.lcu.is_some() {
                        if ui.small_button(if g.want_ux { "Hide full client" } else { "Full client" }).clicked() {
                            self.toggle_full_client(&mut g);
                        }
                    } else if !g.auto_launch && ui.small_button("Start League").clicked() {
                        g.auto_launch = true;
                    }
                });
            });
        });

        egui::Panel::bottom("footer").frame(Frame::new().fill(HEADER).inner_margin(Margin::symmetric(12, 6))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut g.auto_accept, "Auto-accept match");
                if !g.error.is_empty() {
                    ui.label(RichText::new(&g.error).size(11.0).color(RED));
                }
            });
        });

        egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(Margin::same(12))).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| self.fancy_body(ui, &mut g));
        });
    }

    fn fancy_body(&mut self, ui: &mut Ui, g: &mut Shared) {
        for inv in g.invites.clone() {
            let id = inv["invitationId"].as_str().unwrap_or("").to_string();
            card(ui, "PARTY INVITE", GOLD, |ui| {
                let from = inv["fromSummonerName"].as_str().filter(|n| !n.is_empty()).unwrap_or("A friend");
                ui.label(RichText::new(from).size(16.0).strong());
                ui.label(RichText::new(queue_name(g, inv["gameConfig"]["queueId"].as_i64().unwrap_or(0))).color(MUTED));
                ui.horizontal(|ui| {
                    let accept = Button::new(RichText::new("Accept").strong().color(BG)).fill(TEAL).min_size(vec2(120.0, 32.0));
                    if ui.add_enabled(inv["canAcceptInvitation"] != false, accept).clicked() {
                        act(&self.s, "POST", format!("/lol-lobby/v2/received-invitations/{id}/accept"), None);
                    }
                    if outline_button(ui, "Decline", MUTED) {
                        act(&self.s, "POST", format!("/lol-lobby/v2/received-invitations/{id}/decline"), None);
                    }
                });
            });
        }

        if g.lcu.is_none() {
            card(ui, "LEAGUE CLIENT", GOLD, |ui| {
                ui.label(RichText::new(&g.status).size(16.0));
                if g.need_login {
                    ui.label(RichText::new("Log in once with \"Stay signed in\" ticked; LoLite handles the rest.").color(MUTED));
                    if big_button(ui, "LOG IN", GOLD, true) {
                        Self::log_in();
                    }
                } else if !g.auto_launch && big_button(ui, "START LEAGUE", TEAL, true) {
                    g.auto_launch = true;
                }
            });
            return;
        }

        match g.phase.as_str() {
            "None" => self.fancy_play(ui, g, "PLAY"),
            "Lobby" => {
                self.fancy_lobby(ui, g);
                self.fancy_play(ui, g, "CHANGE MODE");
            }
            "Matchmaking" => {
                card(ui, "IN QUEUE", TEAL, |ui| {
                    let t = g.search["timeInQueue"].as_f64().unwrap_or(0.0);
                    let est = g.search["estimatedQueueTime"].as_f64().unwrap_or(0.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(mmss(t)).size(40.0).strong());
                        ui.label(RichText::new(format!("Estimated {}", mmss(est))).color(MUTED));
                    });
                    if est > 0.0 {
                        ui.add(ProgressBar::new((t / est).min(1.0) as f32).desired_height(4.0).fill(TEAL));
                    }
                    ui.add_space(4.0);
                    if outline_button(ui, "Cancel queue", RED) {
                        act(&self.s, "DELETE", "/lol-lobby/v2/lobby/matchmaking/search".into(), None);
                    }
                });
                self.fancy_lobby(ui, g);
            }
            "ReadyCheck" => card(ui, "MATCH FOUND", TEAL, |ui| {
                let left = 1.0 - g.ready["timer"].as_f64().unwrap_or(0.0) / 12.0;
                ui.add(ProgressBar::new(left.clamp(0.0, 1.0) as f32).desired_height(4.0).fill(TEAL));
                ui.add_space(6.0);
                match g.ready["playerResponse"].as_str().unwrap_or("") {
                    "Accepted" => {
                        ui.label(RichText::new("Accepted — waiting for other players").size(16.0).color(TEAL));
                    }
                    "Declined" => {
                        ui.label(RichText::new("Declined").size(16.0).color(RED));
                    }
                    _ => {
                        let accept = Button::new(RichText::new("ACCEPT").size(20.0).strong().color(BG)).fill(TEAL).min_size(vec2(ui.available_width(), 56.0));
                        if ui.add(accept).clicked() {
                            act(&self.s, "POST", "/lol-matchmaking/v1/ready-check/accept".into(), None);
                        }
                        if outline_button(ui, "Decline", MUTED) {
                            act(&self.s, "POST", "/lol-matchmaking/v1/ready-check/decline".into(), None);
                        }
                    }
                }
            }),
            "ChampSelect" => self.fancy_champ_select(ui, g),
            "GameStart" | "InProgress" => card(ui, "IN GAME", RED, |ui| {
                ui.label(RichText::new("Good luck, have fun.").size(16.0));
                ui.label(RichText::new("LoLite stays idle while you play.").color(MUTED));
            }),
            "Reconnect" => card(ui, "DISCONNECTED", RED, |ui| {
                if big_button(ui, "RECONNECT", TEAL, true) {
                    act(&self.s, "POST", "/lol-gameflow/v1/reconnect".into(), None);
                }
            }),
            "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => card(ui, "GAME OVER", GOLD, |ui| {
                if big_button(ui, "PLAY AGAIN", TEAL, true) {
                    act(&self.s, "POST", "/lol-lobby/v2/play-again".into(), None);
                }
            }),
            p => card(ui, "", MUTED, |ui| {
                ui.label(p);
            }),
        }
    }

    fn fancy_play(&mut self, ui: &mut Ui, g: &Shared, title: &str) {
        let current = g.lobby["gameConfig"]["queueId"].as_i64().unwrap_or(0);
        card(ui, title, GOLD, |ui| {
            for group in GROUPS {
                let queues: Vec<_> = g.queues.iter().filter(|q| q.2 == *group).collect();
                if queues.is_empty() {
                    continue;
                }
                ui.label(RichText::new(*group).size(12.0).color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    for (id, name, _) in queues {
                        let label = name.strip_prefix("Co-op vs AI: ").unwrap_or(name);
                        let tile = Button::new(label).min_size(vec2(126.0, 34.0)).selected(*id == current);
                        if ui.add(tile).clicked() && *id != current {
                            act(&self.s, "POST", "/lol-lobby/v2/lobby".into(), Some(json!({ "queueId": id })));
                        }
                    }
                });
                ui.add_space(6.0);
            }
        });
    }

    fn fancy_lobby(&mut self, ui: &mut Ui, g: &Shared) {
        let l = &g.lobby;
        card(ui, "LOBBY", GOLD, |ui| {
            ui.label(RichText::new(queue_name(g, l["gameConfig"]["queueId"].as_i64().unwrap_or(0))).size(18.0).strong());
            ui.add_space(4.0);
            for m in l["members"].as_array().into_iter().flatten() {
                ui.horizontal(|ui| {
                    let leader = if m["isLeader"] == true { "★ " } else { "" };
                    ui.label(RichText::new(format!("{leader}{}", member_name(m))).size(15.0));
                    for k in ["firstPositionPreference", "secondPositionPreference"] {
                        if let Some(r) = m[k].as_str().filter(|r| !r.is_empty() && *r != "UNSELECTED") {
                            pill(ui, r, MUTED);
                        }
                    }
                });
            }
            if g.phase == "Lobby" {
                if l["gameConfig"]["showPositionSelector"] == true {
                    ui.add_space(4.0);
                    ui.label(RichText::new("Your roles").size(12.0).color(MUTED));
                    self.role_pickers(ui, g);
                }
                ui.add_space(6.0);
                if big_button(ui, "FIND MATCH", TEAL, l["canStartActivity"] == true) {
                    act(&self.s, "POST", "/lol-lobby/v2/lobby/matchmaking/search".into(), None);
                }
                if outline_button(ui, "Leave lobby", MUTED) {
                    act(&self.s, "DELETE", "/lol-lobby/v2/lobby".into(), None);
                }
            }
        });
    }

    fn fancy_champ_select(&mut self, ui: &mut Ui, g: &Shared) {
        let sess = &g.session;
        let timer = &sess["timer"];
        let left = timer["adjustedTimeLeftInPhase"].as_f64().unwrap_or(0.0);
        let total = timer["totalTimeInPhase"].as_f64().unwrap_or(0.0);
        let phase = match timer["phase"].as_str().unwrap_or("") {
            "PLANNING" => "Planning",
            "BAN_PICK" => "Ban & pick",
            "FINALIZATION" => "Finalizing",
            "GAME_STARTING" => "Game starting",
            p => p,
        };
        card(ui, "CHAMPION SELECT", GOLD, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(phase).size(18.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{}s", (left / 1000.0) as i64)).size(22.0).strong().color(GOLD));
                });
            });
            if total > 0.0 {
                ui.add(ProgressBar::new((left / total).clamp(0.0, 1.0) as f32).desired_height(4.0).fill(GOLD));
            }
            ui.add_space(6.0);
            let me = sess["localPlayerCellId"].as_i64();
            for p in sess["myTeam"].as_array().into_iter().flatten() {
                let id = p["championId"].as_i64().filter(|i| *i != 0).or(p["championPickIntent"].as_i64()).unwrap_or(0);
                let you = p["cellId"].as_i64() == me;
                let name = p["gameName"].as_str().filter(|n| !n.is_empty()).unwrap_or("Teammate");
                ui.horizontal(|ui| {
                    let pos = p["assignedPosition"].as_str().unwrap_or("");
                    if !pos.is_empty() {
                        ui.label(RichText::new(pos).size(11.0).color(MUTED));
                    }
                    ui.label(RichText::new(name).color(if you { GOLD } else { TEXT }).strong());
                    ui.label(RichText::new(champ_name(g, id)).color(MUTED));
                });
            }
        });

        let t = match my_turn(g) {
            Ok(t) => t,
            Err(locked) => {
                card(ui, "YOUR PICK", TEAL, |ui| {
                    let text = locked.map_or("Waiting for your turn…".into(), |id| format!("Locked in: {}", champ_name(g, id)));
                    ui.label(RichText::new(text).size(18.0).strong());
                });
                return;
            }
        };
        let (title, accent) = match (t.is_ban, t.active) {
            (true, _) => ("BAN A CHAMPION", RED),
            (false, true) => ("PICK YOUR CHAMPION", TEAL),
            (false, false) => ("PRE-PICK (DECLARE INTENT)", GOLD),
        };
        card(ui, title, accent, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search champion…").desired_width(f32::INFINITY));
            ui.add_space(4.0);
            let verb = if t.is_ban { "BAN" } else { "LOCK IN" };
            let label = if t.current == 0 { verb.to_string() } else { format!("{verb} {}", champ_name(g, t.current).to_uppercase()) };
            if big_button(ui, &label, accent, t.active && t.current != 0) {
                lock_in(&self.s, &t);
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (id, name) in champ_options(g, &t, &self.search) {
                    let tile = Button::new(RichText::new(&name).size(13.0)).min_size(vec2(98.0, 30.0)).selected(id == t.current);
                    if ui.add(tile).clicked() {
                        hover(&self.s, &t, id);
                    }
                }
            });
        });
    }
}
