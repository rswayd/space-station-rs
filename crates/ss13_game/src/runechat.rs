//! tg's runechat, server half (`/mob/proc/create_chat_message`, `code/datums/chatmessage.dm`): the
//! say handler reports what each client heard ([`Overhead`]); here the hearer's state is checked,
//! the text is clipped and colored by the speaker's name (`colorize_string`), and it goes to that
//! client alone as [`Animation::Runechat`]. `crate::runechat_client` draws it.
// shortcut: the `enable_runechat`, `see_chat_non_mob`, `see_rc_emotes` and `max_chat_length`
// preferences need the preferences menu (`docs/parity/ui/preferences.md`); no language or radio
// icon prefixes, no `apply_message_emphasis` (|i|, +b+, _u_), and radio messages have no bubble
// over the speaker.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::language::ScrambleSeed;
use ss13_chat::server::{Overhead, OverheadKind, SaySystems};
use ss13_chat::speech::SpeakerStat;
use ss13_net::animation::server::PlayAnimationFor;
use ss13_net::server::ControlledEntity;
use ss13_shared::animation::{Animation, MAX_RUNECHAT_CHARS, RunechatStyle};

/// `CM_COLOR_SAT_MIN`, `CM_COLOR_SAT_MAX`, `CM_COLOR_LUM_MIN`, `CM_COLOR_LUM_MAX`.
const SAT_MIN: f32 = 0.6;
const SAT_MAX: f32 = 0.7;
const LUM_MIN: f32 = 0.65;
const LUM_MAX: f32 = 0.75;
/// `colorize_string(name, 0.85, 0.85)`: `chat_color_darkened`, for italics.
const DARKENED: f32 = 0.85;

pub struct RunechatServerPlugin;

impl Plugin for RunechatServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, send_overheads.after(SaySystems));
    }
}

/// tg `colorize_string`: a color from the name and the round (here its scramble seed).
pub fn chat_color(name: &str, seed: u64, shift: f32) -> [u8; 3] {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^= hash >> 32;
    hash = hash.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let [a, b, c, ..] = hash.to_le_bytes();
    let h = f32::from(a) * (360.0 / 255.0);
    let s = f32::from(b >> 2) * ((SAT_MAX - SAT_MIN) / 63.0) + SAT_MIN;
    let l = f32::from(c >> 2) * ((LUM_MAX - LUM_MIN) / 63.0) + LUM_MIN;
    let (s, l) = ((s * shift).clamp(0.0, 1.0), (l * shift).clamp(0.0, 1.0));
    let sector = ((h / 60.0).floor() as u8).min(5);
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = chroma * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - chroma * 0.5;
    let to_u8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    let (c8, x8, m8) = (to_u8(chroma), to_u8(x), to_u8(0.0));
    match sector {
        0 => [c8, x8, m8],
        1 => [x8, c8, m8],
        2 => [m8, c8, x8],
        3 => [m8, x8, c8],
        4 => [x8, m8, c8],
        _ => [c8, m8, x8],
    }
}

/// `/datum/chatmessage/New` and `generate_image`: `+` (BandaStation's TTS stress marks) goes,
/// long texts are cut; `None` when nothing is left.
pub fn runechat_text(raw: &str) -> Option<String> {
    let text: String = raw.chars().filter(|&c| c != '+').collect();
    if text.trim().is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_RUNECHAT_CHARS {
        return Some(text);
    }
    let mut cut: String = text.chars().take(MAX_RUNECHAT_CHARS).collect();
    cut.push_str("...");
    Some(cut)
}

/// Cost: per message heard, a few component reads per hearer.
fn send_overheads(world: &mut World, mut cursor: Local<MessageCursor<Overhead>>) {
    // Servers without chat (some test apps) have no say handler to report anything.
    if !world.contains_resource::<Messages<Overhead>>() {
        return;
    }
    let pending: Vec<Overhead> = cursor
        .read(world.resource::<Messages<Overhead>>())
        .cloned()
        .collect();
    if pending.is_empty() || !world.contains_resource::<Messages<PlayAnimationFor>>() {
        return;
    }
    let seed = world.get_resource::<ScrambleSeed>().map_or(0, |s| s.0);
    for said in pending {
        let Some(body) = world.get::<ControlledEntity>(said.client).map(|c| c.0) else {
            continue;
        };
        // `IS_UNCONSCIOUS_AND_ALIVE`; an emote is seen, a blind mob sees none.
        if world.get::<SpeakerStat>(body) == Some(&SpeakerStat::HardCrit)
            || (said.kind == OverheadKind::Emote && crate::vision::is_blind(world, body))
        {
            continue;
        }
        let Some(text) = runechat_text(&said.text) else {
            continue;
        };
        let Some(entity) = crate::balloon::atom_on_turf(world, said.speaker) else {
            continue;
        };
        let italics = matches!(said.kind, OverheadKind::Whisper | OverheadKind::Emote);
        let style = RunechatStyle {
            italics,
            small: world.get::<crate::health::Stat>(said.speaker).is_none(),
            yell: text.ends_with("!!"),
            looc: said.kind == OverheadKind::Looc,
        };
        let shift = if italics { DARKENED } else { 1.0 };
        let color = chat_color(&said.name, seed, shift);
        world.write_message(PlayAnimationFor {
            client: said.client,
            entity,
            animation: Animation::Runechat { text, color, style },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_depend_on_the_name_and_stay_in_tg_bounds() {
        let a = chat_color("Боб", 1, 1.0);
        assert_eq!(a, chat_color("Боб", 1, 1.0));
        assert_ne!(a, chat_color("Алиса", 1, 1.0));
        let dark = chat_color("Боб", 1, DARKENED);
        assert!(
            dark.iter().map(|&v| u32::from(v)).sum::<u32>()
                <= a.iter().map(|&v| u32::from(v)).sum()
        );
        // Lightness around 0.65..0.75 with saturation 0.6..0.7: never black, never white.
        for name in ["a", "b", "ccc", "Гоша", "Ivan Ivanov"] {
            let [r, g, b] = chat_color(name, 7, 1.0);
            let lightness = (r.max(g).max(b) as f32 + r.min(g).min(b) as f32) / 510.0;
            assert!((0.6..=0.8).contains(&lightness), "{name}: {lightness}");
        }
    }

    #[test]
    fn long_texts_are_cut_and_empty_ones_dropped() {
        assert_eq!(runechat_text("при+вет").as_deref(), Some("привет"));
        assert_eq!(runechat_text("  +  "), None);
        let long = "я".repeat(MAX_RUNECHAT_CHARS + 1);
        let cut = runechat_text(&long).unwrap();
        assert_eq!(cut.chars().count(), MAX_RUNECHAT_CHARS + 3);
        assert!(cut.ends_with("..."));
        assert_eq!(
            runechat_text(&"я".repeat(MAX_RUNECHAT_CHARS))
                .unwrap()
                .chars()
                .count(),
            MAX_RUNECHAT_CHARS
        );
    }
}
