//! Friends list and direct-message chat; shared by both layouts, so colors come from the active theme.
use crate::i18n::tr;
use crate::*;
use eframe::egui::{Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

const ONLINE: Color32 = Color32::from_rgb(0x3D, 0xD6, 0x8C);
const AWAY: Color32 = Color32::from_rgb(0xF5, 0xB8, 0x41);
const BUSY: Color32 = Color32::from_rgb(0xF2, 0x55, 0x5A);
const OTHER_GAME: Color32 = Color32::from_rgb(0x8A, 0xA4, 0xFF);
const OFFLINE: Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
const BADGE: Color32 = Color32::from_rgb(0xF2, 0x55, 0x5A);
/// Muted hues for avatar initials, picked by name so each friend keeps a color.
const AVATARS: &[Color32] = &[
    Color32::from_rgb(0x4C, 0x5F, 0xB8),
    Color32::from_rgb(0x2F, 0x8A, 0x7A),
    Color32::from_rgb(0x8E, 0x5A, 0xB5),
    Color32::from_rgb(0xB0, 0x6A, 0x3B),
    Color32::from_rgb(0x3E, 0x7F, 0xB0),
    Color32::from_rgb(0xA8, 0x4D, 0x6B),
    Color32::from_rgb(0x5E, 0x8A, 0x3A),
    Color32::from_rgb(0x6A, 0x6F, 0x7E),
];

fn online(f: &Value) -> bool {
    !matches!(f["availability"].as_str().unwrap_or("offline"), "offline" | "mobile")
}

fn friend_name(f: &Value) -> String {
    match (f["gameName"].as_str().filter(|n| !n.is_empty()), f["gameTag"].as_str()) {
        (Some(n), Some(t)) if !t.is_empty() => format!("{n}#{t}"),
        (Some(n), _) => n.to_string(),
        _ => f["name"].as_str().unwrap_or("?").to_string(),
    }
}

/// (status text, dot color)
fn presence(f: &Value) -> (String, Color32) {
    let avail = f["availability"].as_str().unwrap_or("offline");
    if !online(f) {
        let text = if avail == "mobile" { tr("Mobile", "Celular") } else { tr("Offline", "Offline") };
        return (text.into(), OFFLINE);
    }
    if f["product"].as_str().is_some_and(|p| !p.is_empty() && p != "league_of_legends") {
        return (f["productName"].as_str().unwrap_or("").to_string(), OTHER_GAME);
    }
    let game = f["lol"]["gameStatus"].as_str().unwrap_or("");
    let text = match game {
        "inQueue" => tr("In queue", "Na fila"),
        "championSelect" => tr("Champ select", "Seleção de campeões"),
        "inGame" => tr("In game", "Em jogo"),
        "spectating" => tr("Spectating", "Assistindo"),
        g if g.starts_with("hosting") => tr("In lobby", "No saguão"),
        _ if avail == "away" => tr("Away", "Ausente"),
        _ => tr("Online", "Online"),
    };
    let color = match (avail, game) {
        ("away", _) => AWAY,
        ("dnd", _) | (_, "inGame" | "championSelect" | "inQueue") => BUSY,
        _ => ONLINE,
    };
    (text.into(), color)
}

pub(crate) fn unread_total(g: &Shared) -> i64 {
    g.unread.values().map(|u| u.1).sum()
}

fn avatar_color(name: &str) -> Color32 {
    AVATARS[name.bytes().fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize)) % AVATARS.len()]
}

fn paint_avatar(ui: &Ui, center: egui::Pos2, radius: f32, name: &str, color: Color32) {
    let initial = name.chars().find(|c| c.is_alphanumeric()).map_or('?', |c| c.to_uppercase().next().unwrap_or(c));
    ui.painter().circle_filled(center, radius, color);
    ui.painter().text(center, Align2::CENTER_CENTER, initial, FontId::proportional(radius * 0.95), Color32::WHITE);
}

/// Circle with the name's initial.
pub(crate) fn avatar(ui: &mut Ui, name: &str, radius: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(radius * 2.0, radius * 2.0), Sense::hover());
    paint_avatar(ui, rect.center(), radius, name, color);
}

/// Single-line text cut with "…" to fit `width`.
fn fit(ui: &Ui, text: &str, size: f32, color: Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), FontId::proportional(size), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ui.painter().layout_job(job)
}

/// Clickable friend/conversation row: avatar with presence dot, name, status, unread badge.
fn row(ui: &mut Ui, name: &str, status: &str, dot: Color32, unread: i64) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let v = ui.visuals();
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), v.widgets.hovered.bg_fill);
    }
    let c = pos2(rect.left() + 22.0, rect.center().y);
    paint_avatar(ui, c, 14.0, name, avatar_color(name));
    // Presence dot cut out of the avatar with the panel color.
    let d = pos2(c.x + 10.0, c.y + 10.0);
    ui.painter().circle_filled(d, 5.5, v.panel_fill);
    ui.painter().circle_filled(d, 4.0, dot);
    let badge_w = if unread > 0 { 30.0 } else { 0.0 };
    let text_w = rect.width() - 46.0 - badge_w - 6.0;
    let (strong, weak) = (v.strong_text_color(), v.weak_text_color());
    let top = fit(ui, name, 14.0, strong, text_w);
    let bottom = fit(ui, status, 12.0, weak, text_w);
    ui.painter().galley(pos2(rect.left() + 44.0, rect.top() + 5.0), top, strong);
    ui.painter().galley(pos2(rect.left() + 44.0, rect.top() + 23.0), bottom, weak);
    if unread > 0 {
        let b = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(if unread > 9 { 26.0 } else { 20.0 }, 18.0));
        ui.painter().rect_filled(b, CornerRadius::same(9), BADGE);
        ui.painter().text(b.center(), Align2::CENTER_CENTER, unread.to_string(), FontId::proportional(11.0), Color32::WHITE);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn group_label(ui: &mut Ui, text: &str, count: usize) {
    ui.add_space(6.0);
    ui.label(RichText::new(format!("{text}  {count}")).size(12.0).color(ui.visuals().weak_text_color()));
}

/// Sets the open conversation; the poller fetches its messages.
fn open_chat(s: &S, g: &mut Shared, pid: String, name: String) {
    g.chat_with = pid.clone();
    g.chat_name = name;
    g.messages.clear();
    poke(g, "/lol-chat/v1/conversations");
    let s = s.clone();
    std::thread::spawn(move || {
        let Some(api) = s.lock().unwrap().lcu.clone() else { return };
        // The official UI opens the conversation before messages can be posted, and marks it active to clear unread.
        let _ = api.send("POST", "/lol-chat/v1/conversations", Some(json!({ "id": pid, "type": "chat" })));
        let _ = api.send("PUT", "/lol-chat/v1/conversations/active", Some(json!({ "id": pid })));
    });
}

impl App {
    /// Friends side panel: list, or the open chat.
    pub(crate) fn social(&mut self, ui: &mut Ui, g: &mut Shared) {
        ui.add_space(6.0);
        if g.chat_with.is_empty() || g.lcu.is_none() {
            let unread = unread_total(g);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Friends", "Amigos")).size(17.0).strong());
                if unread > 0 {
                    Frame::new().fill(BADGE).corner_radius(CornerRadius::same(9)).inner_margin(Margin::symmetric(7, 1)).show(ui, |ui| {
                        ui.label(RichText::new(unread.to_string()).size(12.0).color(Color32::WHITE));
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(egui::Button::new("»").frame(false)).on_hover_text(tr("Collapse", "Recolher")).clicked() {
                        self.friends_open = false;
                    }
                });
            });
        }
        if g.lcu.is_none() {
            ui.weak(tr("League client not connected", "Cliente do League não conectado"));
            return;
        }
        if g.chat_with.is_empty() {
            self.friend_list(ui, g);
        } else {
            self.chat(ui, g);
        }
    }

    fn friend_list(&mut self, ui: &mut Ui, g: &mut Shared) {
        let mut friends: Vec<&Value> = g.friends.iter().collect();
        friends.sort_by_key(|f| (!online(f), friend_name(f).to_lowercase()));
        let total_online = friends.iter().filter(|f| online(f)).count();
        ui.checkbox(&mut self.show_offline, tr("Show offline", "Mostrar offline"));
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            // Every unread conversation, including offline friends and non-friends the list hides.
            if !g.unread.is_empty() {
                group_label(ui, tr("Unread", "Não lidas"), g.unread.len());
                let mut convs: Vec<_> = g.unread.iter().collect();
                convs.sort_by_key(|c| c.1.0.to_lowercase());
                for (pid, (name, n)) in convs {
                    let text = if *n == 1 { tr("New message", "Nova mensagem").to_string() } else { format!("{n} {}", tr("new messages", "novas mensagens")) };
                    if row(ui, name, &text, BADGE, *n) {
                        open = Some((pid.clone(), name.clone()));
                    }
                }
            }
            group_label(ui, tr("Online", "Online"), total_online);
            if total_online == 0 {
                ui.weak(tr("No friends online", "Nenhum amigo online"));
            }
            for (i, f) in friends.iter().enumerate() {
                if !online(f) {
                    if !self.show_offline {
                        break;
                    }
                    if i == total_online {
                        group_label(ui, tr("Offline", "Offline"), friends.len() - total_online);
                    }
                }
                let pid = f["pid"].as_str().unwrap_or("").to_string();
                let (status, color) = presence(f);
                let unread = g.unread.get(&pid).map_or(0, |u| u.1);
                if row(ui, &friend_name(f), &status, color, unread) {
                    open = Some((pid, friend_name(f)));
                }
            }
        });
        if let Some((pid, name)) = open {
            open_chat(&self.s, g, pid, name);
        }
    }

    fn chat(&mut self, ui: &mut Ui, g: &mut Shared) {
        let pid = g.chat_with.clone();
        let friend = g.friends.iter().find(|f| f["pid"] == pid.as_str());
        let name = friend.map_or(g.chat_name.clone(), friend_name);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("‹").size(20.0)).frame(false)).on_hover_text(tr("Back", "Voltar")).clicked() {
                g.chat_with.clear();
            }
            avatar(ui, &name, 15.0, avatar_color(&name));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(&name).strong());
                if let Some(f) = friend {
                    let (status, color) = presence(f);
                    ui.label(RichText::new(status).size(12.0).color(color));
                }
            });
        });
        if g.chat_with.is_empty() {
            return;
        }
        ui.separator();
        let v = ui.visuals().clone();
        let height = (ui.available_height() - 44.0).max(120.0);
        egui::ScrollArea::vertical().max_height(height).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            let max = ui.available_width() * 0.8;
            for m in &g.messages {
                let body = m["body"].as_str().unwrap_or("");
                if m["type"] != "chat" {
                    ui.vertical_centered(|ui| ui.label(RichText::new(body).size(12.0).color(v.weak_text_color())));
                    continue;
                }
                let mine = m["fromPid"] == g.my_pid.as_str();
                let layout = if mine { Layout::right_to_left(Align::TOP) } else { Layout::left_to_right(Align::TOP) };
                ui.with_layout(layout, |ui| {
                    Frame::new()
                        .fill(if mine { v.selection.bg_fill } else { v.widgets.inactive.bg_fill })
                        .stroke(if mine { Stroke::new(1.0, v.selection.stroke.color.gamma_multiply(0.4)) } else { Stroke::NONE })
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_max_width(max);
                            ui.add(egui::Label::new(body).wrap());
                        });
                });
            }
        });
        let send = ui
            .horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.chat_input)
                        .hint_text(tr("Message…", "Mensagem…"))
                        .margin(Margin::symmetric(10, 7))
                        .desired_width(ui.available_width() - 64.0),
                );
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked = ui.add(egui::Button::new(tr("Send", "Enviar")).min_size(vec2(56.0, 30.0))).clicked();
                if enter {
                    edit.request_focus();
                }
                enter || clicked
            })
            .inner;
        if send && !self.chat_input.trim().is_empty() {
            let body = std::mem::take(&mut self.chat_input);
            act(&self.s, "POST", format!("/lol-chat/v1/conversations/{pid}/messages"), Some(json!({ "body": body.trim(), "type": "chat" })));
        }
    }
}
