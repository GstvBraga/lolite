use std::sync::atomic::{AtomicBool, Ordering};

static PT: AtomicBool = AtomicBool::new(false);

pub fn pt() -> bool {
    PT.load(Ordering::Relaxed)
}

pub fn set_pt(on: bool) {
    PT.store(on, Ordering::Relaxed);
}

/// UI text in the chosen language (English or Brazilian Portuguese).
pub fn tr(en: &'static str, pt: &'static str) -> &'static str {
    if self::pt() { pt } else { en }
}

/// Display name for a lobby/champ-select position value.
pub fn role(r: &str) -> &str {
    match r {
        "TOP" => tr("Top", "Topo"),
        "JUNGLE" => tr("Jungle", "Selva"),
        "MIDDLE" => tr("Mid", "Meio"),
        "BOTTOM" => tr("Bot", "Atirador"),
        "UTILITY" => tr("Support", "Suporte"),
        "FILL" => tr("Fill", "Preencher"),
        r => r,
    }
}

/// Names for common queues. LCU sends names in the League client's language, which may differ from LoLite's.
pub fn queue(id: i64) -> Option<&'static str> {
    Some(match id {
        400 => tr("Draft Pick", "Escolha Alternada"),
        420 => tr("Ranked Solo/Duo", "Ranqueada Solo/Duo"),
        430 => tr("Blind Pick", "Escolha às Cegas"),
        440 => tr("Ranked Flex", "Ranqueada Flexível"),
        450 => "ARAM",
        480 => tr("Swiftplay", "Partida Rápida"),
        490 => tr("Quickplay", "Jogo Rápido"),
        700 => "Clash",
        870 => tr("Intro", "Introdução"),
        880 => tr("Beginner", "Iniciante"),
        890 => tr("Intermediate", "Intermediário"),
        1020 => tr("One for All", "Um Por Todos"),
        1090 => "Normal",
        1100 => tr("Ranked", "Ranqueada"),
        1700 => "Arena",
        _ => return None,
    })
}

/// Display text for a gameflow phase.
pub fn phase(p: &str) -> &str {
    match p {
        "None" => tr("Idle", "Ocioso"),
        "Lobby" => tr("In lobby", "No saguão"),
        "Matchmaking" => tr("In queue", "Na fila"),
        "ReadyCheck" => tr("Match found", "Partida encontrada"),
        "ChampSelect" => tr("Champ select", "Seleção de campeões"),
        "GameStart" | "InProgress" => tr("In game", "Em jogo"),
        "Reconnect" => tr("Disconnected", "Desconectado"),
        "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => tr("Game over", "Fim de jogo"),
        p => p,
    }
}
