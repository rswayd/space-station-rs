//! BandaStation's OOC badges and donor name effects (`modular_bandastation/chat`): the icon of
//! the subscription tier and of the staff role before the key, the donor's key in their OOC color
//! with the effect of their tier. The pure rules are here; `ss13_admin::badges` builds the OOC
//! name from a connection.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_shared::IconStateId;
use ss13_shared::chat::{ChatSegment, NameShine, Span};

/// `CHAT_BADGES_DMI`.
pub const BADGES_DMI: &str = "modular_bandastation/chat/icons/chatbadges.dmi";

/// `MAX_DONATOR_LEVEL` (`DONATOR_TIER_5`).
pub const MAX_DONATOR_LEVEL: u8 = 5;

/// `/datum/preference/color/ooc_color` default `#c43b23`.
pub const DEFAULT_OOC_COLOR: [u8; 3] = [0xc4, 0x3b, 0x23];

/// The icon states of [`BADGES_DMI`] by name; filled by the game from the icon index. Empty:
/// no badge icons are drawn (the key and its color still are).
#[derive(Resource, Clone, Debug, Default)]
pub struct ChatBadges(pub HashMap<String, IconStateId>);

impl ChatBadges {
    /// A badge as a chat segment: the icon, `:State:` for logs and clients without it.
    pub fn segment(&self, state: &str) -> Option<ChatSegment> {
        let &id = self.0.get(state)?;
        Some(ChatSegment::new(Span::Emoji(id), format!(":{state}:")))
    }
}

/// `GLOB.donor_chat_effects` (`/datum/preference/choiced/donor_chat_effect`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DonorChatEffect {
    #[default]
    Auto,
    Metal,
    Glowing,
    Off,
}

impl DonorChatEffect {
    /// The choices as the preference lists them.
    pub const CHOICES: [(&'static str, DonorChatEffect); 4] = [
        ("Автоматически", DonorChatEffect::Auto),
        ("Металлик", DonorChatEffect::Metal),
        ("Светящийся", DonorChatEffect::Glowing),
        ("Выключить", DonorChatEffect::Off),
    ];
}

/// A player's chat preferences of the modpack, on the connection.
// shortcut: defaults for everyone until the preferences menu (`docs/parity/ui/preferences.md`)
// saves `donor_public`, `donor_chat_effect` and `ooccolor`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DonorChatPrefs {
    /// `donor_public`.
    pub public: bool,
    pub effect: DonorChatEffect,
    /// `ooc_color`.
    pub ooc_color: [u8; 3],
}

impl Default for DonorChatPrefs {
    fn default() -> Self {
        Self {
            public: true,
            effect: DonorChatEffect::Auto,
            ooc_color: DEFAULT_OOC_COLOR,
        }
    }
}

/// `get_worker_badge`'s `rank_badge_map`: the badge of the holder's first rank.
pub fn worker_badge(rank: &str) -> Option<&'static str> {
    Some(match rank {
        "Максон" => "Wycc",
        "Банда" | "Друг Банды" => "Streamer",
        "Хост" | "Ведущий Разработчик" => "Host",
        "Старший Разработчик" => "HeadDeveloper",
        "Разработчик" => "Developer",
        "Начальный Разработчик" => "MiniDeveloper",
        "Ведущий Маппер" => "HeadMapper",
        "Маппер" => "Mapper",
        "Спрайтер" => "Spriceter",
        "Ведущий редактор Вики" => "HeadWiki",
        "Редактор Вики" => "WikiLore",
        "Главный Администратор" => "HeadAdmin",
        "Администратор" => "GameAdmin",
        "Ивентмейкер" => "Eventmaker",
        "Триал Администратор" => "TrialAdmin",
        "Тестовый Администратор" => "TestAdmin",
        "Ментор" => "Mentor",
        _ => return None,
    })
}

/// `get_donator_badge` without the BYOND membership (there is none here).
pub fn donator_badge(tier: u8, prefs: &DonorChatPrefs) -> Option<String> {
    (tier > 0 && prefs.public).then(|| format!("Tier_{}", tier.min(MAX_DONATOR_LEVEL)))
}

/// The classes `get_ooc_badged_name` puts on the key, as `chat-dark.scss` draws them: from tier
/// 3 with an effect chosen, metallic at 3, glowing at 4, glowing with the halo at 5 (`auto` and
/// `glowing`), always metallic for `metal`.
pub fn name_shine(tier: u8, effect: DonorChatEffect) -> Option<NameShine> {
    let tier = tier.min(MAX_DONATOR_LEVEL);
    match (effect, tier) {
        (DonorChatEffect::Off, _) | (_, 0..=2) => None,
        (DonorChatEffect::Metal, _) | (_, 3) => Some(NameShine::Metallic),
        (_, 4) => Some(NameShine::Glowing),
        _ => Some(NameShine::GlowingShine),
    }
}

/// `get_ooc_badged_name`: the donor badge, the staff badge (of `rank`, the holder's first rank),
/// then the key, in the donor's color and effect when the tier is shown, else in `span`.
pub fn ooc_badged_name(
    key: &str,
    tier: u8,
    rank: Option<&str>,
    prefs: &DonorChatPrefs,
    badges: Option<&ChatBadges>,
    span: Span,
) -> Vec<ChatSegment> {
    let mut out = Vec::new();
    if let Some(badges) = badges {
        let donor = donator_badge(tier, prefs);
        let worker = rank.and_then(worker_badge);
        out.extend(donor.as_deref().and_then(|s| badges.segment(s)));
        out.extend(worker.and_then(|s| badges.segment(s)));
    }
    let key_span = if tier > 0 && prefs.public {
        Span::DonorKey {
            rgb: prefs.ooc_color,
            shine: name_shine(tier, prefs.effect),
        }
    } else {
        span
    };
    out.push(ChatSegment::new(key_span, key));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_follow_the_tier() {
        use DonorChatEffect::*;
        assert_eq!(name_shine(2, Auto), None);
        assert_eq!(name_shine(3, Auto), Some(NameShine::Metallic));
        assert_eq!(name_shine(4, Glowing), Some(NameShine::Glowing));
        assert_eq!(name_shine(5, Auto), Some(NameShine::GlowingShine));
        assert_eq!(name_shine(5, Metal), Some(NameShine::Metallic));
        assert_eq!(name_shine(5, Off), None);
    }

    #[test]
    fn badges_and_key_of_a_donor_admin() {
        let badges = ChatBadges(HashMap::from([
            ("Tier_5".to_owned(), IconStateId(1)),
            ("GameAdmin".to_owned(), IconStateId(2)),
        ]));
        let prefs = DonorChatPrefs::default();
        let got = ooc_badged_name(
            "bob",
            7,
            Some("Администратор"),
            &prefs,
            Some(&badges),
            Span::Ooc,
        );
        assert_eq!(
            got,
            vec![
                ChatSegment::new(Span::Emoji(IconStateId(1)), ":Tier_5:"),
                ChatSegment::new(Span::Emoji(IconStateId(2)), ":GameAdmin:"),
                ChatSegment::new(
                    Span::DonorKey {
                        rgb: DEFAULT_OOC_COLOR,
                        shine: Some(NameShine::GlowingShine)
                    },
                    "bob"
                ),
            ]
        );
        // Hidden tier: no donor badge, the plain key; a rank without a badge adds nothing.
        let hidden = DonorChatPrefs {
            public: false,
            ..prefs
        };
        let got = ooc_badged_name(
            "bob",
            5,
            Some("Moderator"),
            &hidden,
            Some(&badges),
            Span::Ooc,
        );
        assert_eq!(got, vec![ChatSegment::new(Span::Ooc, "bob")]);
        // Tier 1: colored key, no effect.
        let got = ooc_badged_name("bob", 1, None, &prefs, None, Span::Ooc);
        assert_eq!(
            got[0].span,
            Span::DonorKey {
                rgb: DEFAULT_OOC_COLOR,
                shine: None
            }
        );
    }
}
