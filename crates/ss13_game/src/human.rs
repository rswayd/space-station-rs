//! How a human mob looks: tg draws `/mob/living/carbon/human` with an empty icon and builds it
//! from standing overlays (body parts, eyes, worn items, hair) in fixed layer slots
//! (`human_update_icons.dm`, `carbon_update_icons.dm`, `/obj/item/bodypart/proc/get_limb_icon`).
//! Body parts and hair are greyscale sprites multiplied by a color, like tg.
//!
//! [`HumanAppearances`] turns a [`HumanLook`] into an [`Appearance`] once per distinct look and
//! keeps it: spawning a player is a hash lookup plus a clone.
//!
//! The limbs are the species' (`crate::species::Limbs`): their icon, `limb_id`, dimorphism and
//! color source (skin tone, mutant color, a fixed color or none).
//!
//! BandaStation (`modular_bandastation`) adds hair styles and other species but does not change
//! how a human is drawn; everything here follows upstream tg.

use std::collections::{HashMap, HashSet};
use std::fmt;

use bevy::prelude::*;
use ss13_assets::manifest::AssetManifest;
use ss13_shared::{Appearance, Dir, IconStateId, Overlay};

use crate::species::{LimbColor, SpeciesId};

/// tg standing overlay layers (`code/__DEFINES/mobs.dm`). They are applied as `-layer`, so a
/// higher number draws further below.
pub mod layers {
    /// The far leg when facing east or west (`handle_masking`).
    pub const BODYPARTS_LOW_LAYER: f32 = 23.1;
    pub const BODYPARTS_LAYER: f32 = 23.0;
    pub const EYES_LAYER: f32 = 21.0;
    pub const UNIFORM_LAYER: f32 = 19.0;
    pub const ID_LAYER: f32 = 18.0;
    pub const GLOVES_LAYER: f32 = 17.0;
    /// Hands: above the uniform's sleeves.
    pub const BODYPARTS_HIGH_LAYER: f32 = 17.5;
    pub const SHOES_LAYER: f32 = 16.0;
    pub const EARS_LAYER: f32 = 15.0;
    pub const SUIT_LAYER: f32 = 14.0;
    pub const GLASSES_LAYER: f32 = 13.0;
    pub const BELT_LAYER: f32 = 12.0;
    pub const SUIT_STORE_LAYER: f32 = 11.0;
    pub const NECK_LAYER: f32 = 10.0;
    pub const BACK_LAYER: f32 = 9.0;
    pub const HAIR_LAYER: f32 = 8.0;
    pub const FACEMASK_LAYER: f32 = 7.0;
    pub const HEAD_LAYER: f32 = 6.0;
    /// Held items.
    pub const HANDS_LAYER: f32 = 3.0;
}
use layers::*;

/// tg `icon_state = ""` on humans draws nothing; this state of the same file is empty too.
const BASE: (&str, &str) = ("icons/mob/human/human.dmi", "blank");
/// `DEFAULT_BODYPART_ICON_ORGANIC`.
const BODYPARTS_DMI: &str = "icons/mob/human/bodyparts_greyscale.dmi";
/// `/obj/item/organ/eyes::eye_icon`.
const EYES_DMI: &str = "icons/mob/human/human_eyes.dmi";
/// `/datum/sprite_accessory/hair::icon`.
const HAIR_DMI: &str = "icons/mob/human/human_face.dmi";
/// `/obj/item/bodypart::icon_husk`: the husk's blood over each limb (`[husk_type]_husk_[zone]`).
const HUSK_DMI: &str = "icons/mob/human/bodyparts.dmi";
/// tg `/obj/item/bodypart/*/robot` `icon_static` (also in `crate::species::LIMB_DMIS`).
pub const ROBOTIC_LIMB_DMI: &str = "icons/mob/augmentation/augments.dmi";
/// A husked limb: tg `ColorTone(HUSK_COLOR_TONE)` (96, 88, 80) of the greyscale sprite, then
/// `husk_color` `#A6A6A6`; the greyscale skin is near white, so the product of the two.
const HUSK_SKIN: [u8; 3] = [62, 57, 52];
/// `BLOOD_COLOR_RED` of the husk blood overlays.
const HUSK_BLOOD: [u8; 3] = [0xa1, 0x08, 0x08];

/// tg `TRAIT_HUSK`: a husked body (a changeling's drained victim). Its look draws husked limbs
/// without hair or eyes ([`HumanLook::husk`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Husk;

/// `/proc/skintone2hex` for the tones in `GLOB.skin_tones`.
pub const SKIN_TONES: [(&str, [u8; 3]); 16] = [
    ("albino", [0xff, 0xf4, 0xe6]),
    ("caucasian1", [0xff, 0xe0, 0xd1]),
    ("caucasian2", [0xfc, 0xcc, 0xb3]),
    ("caucasian3", [0xe8, 0xb5, 0x9b]),
    ("latino", [0xd9, 0xae, 0x96]),
    ("mediterranean", [0xc7, 0x9b, 0x8b]),
    ("asian1", [0xff, 0xde, 0xb3]),
    ("asian2", [0xe3, 0xba, 0x84]),
    ("arab", [0xc4, 0x91, 0x5e]),
    ("indian", [0xb8, 0x78, 0x40]),
    ("mixed1", [0xa5, 0x7a, 0x66]),
    ("mixed2", [0x87, 0x56, 0x3d]),
    ("mixed3", [0x72, 0x55, 0x47]),
    ("mixed4", [0x86, 0x6e, 0x63]),
    ("african1", [0x75, 0x45, 0x23]),
    ("african2", [0x47, 0x1c, 0x18]),
];

/// The names of [`HAIR_STYLES`] in the preferences menu.
pub const HAIR_STYLE_NAMES: [&str; 6] = [
    "Короткая",
    "Деловая",
    "Ёжик",
    "Растрёпанная",
    "Взъерошенная",
    "Хвост",
];

/// `icon_state`s of a few common `/datum/sprite_accessory/hair` styles (short, business, buzzcut,
/// messy, bedhead, ponytail).
pub const HAIR_STYLES: [&str; 6] = [
    "hair_a",
    "hair_business",
    "hair_buzzcut",
    "hair_messy",
    "hair_bedhead",
    "hair_ponytail",
];

/// `natural_hair_colors` of `/proc/random_hair_color`.
pub const HAIR_COLORS: [u32; 36] = [
    0x111111, 0x362925, 0x3B3831, 0x41250C, 0x412922, 0x544C49, 0x583322, 0x593029, 0x703b30,
    0x714721, 0x744729, 0x74482a, 0x7b746e, 0x855832, 0x863019, 0x8c4734, 0x9F550E, 0xA29A96,
    0xA4381C, 0xB17B41, 0xC0BAB7, 0xEFE5E4, 0xF7F3F1, 0xFFF2D6, 0xa15537, 0xa17e61, 0xb38b67,
    0xba673c, 0xc89f73, 0xd9b380, 0xdbc9b8, 0xe1621d, 0xe17d17, 0xe1af93, 0xf1cc8f, 0xfbe7a1,
];

/// Mutant colors (`features["mcolor"]`) offered for species with `TRAIT_MUTANT_COLORS`.
// shortcut: a palette instead of tg's free color picker (`/datum/preference/color/mutant_color`).
pub const MUTANT_COLORS: [u32; 12] = [
    0x77dd44, 0x44aa33, 0x2f7a3a, 0xa9e25d, 0x6699cc, 0x3355aa, 0xcc6644, 0xaa3333, 0xddbb55,
    0x886644, 0x9966cc, 0xdddddd,
];

/// The fixed colors of `/proc/random_eye_color` (brown, hazel, a grey, blue, green, amber).
pub const EYE_COLORS: [u32; 6] = [0x663300, 0x554422, 0x888888, 0x3366cc, 0x006600, 0xffcc00];

/// One sprite of a worn item: `(dmi, state, multiply color)`.
type WornSprite = (&'static str, &'static str, Option<[u8; 3]>);

/// Item type -> (standing layer, worn sprites bottom-up). The sprites are tg's GAGS worn configs
/// unrolled: an `icon_state` layer with `color_ids` is that state multiplied by the color,
/// `blend_mode: overlay` draws it over the previous one.
// shortcut: two hand-copied items; generate this from `worn_icon`/`greyscale_config_worn`/
// `greyscale_colors` in `ss13-mapload convert` once players can wear more than the assistant kit.
const WORN: [(&str, f32, &[WornSprite]); 2] = [
    (
        // `greyscale_colors = "#b3b3b3"`, `/datum/greyscale_config/jumpsuit/worn`.
        "/obj/item/clothing/under/color/grey",
        UNIFORM_LAYER,
        &[
            (
                "icons/mob/clothing/under/color.dmi",
                "jumpsuit",
                Some([0xb3, 0xb3, 0xb3]),
            ),
            (
                "icons/mob/clothing/under/color.dmi",
                "jumpsuit_accessories",
                None,
            ),
        ],
    ),
    (
        // Inherits `greyscale_colors = "#2d2d33#ffffff"`, `/datum/greyscale_config/sneakers/worn`.
        "/obj/item/clothing/shoes/sneakers/black",
        SHOES_LAYER,
        &[
            (
                "icons/mob/clothing/feet.dmi",
                "sneakers_back",
                Some([0x2d, 0x2d, 0x33]),
            ),
            (
                "icons/mob/clothing/feet.dmi",
                "sneakers_front",
                Some([0xff, 0xff, 0xff]),
            ),
        ],
    ),
];

/// `item` has hand-resolved worn sprites in [`WORN`]; other items are drawn from their own
/// `worn_icon` (`crate::inventory`).
pub fn has_worn_sprites(item: &str) -> bool {
    WORN.iter().any(|(ty, ..)| *ty == item)
}

pub const ASSISTANT_UNIFORM: &str = "/obj/item/clothing/under/color/grey";
pub const ASSISTANT_SHOES: &str = "/obj/item/clothing/shoes/sneakers/black";

/// What a human looks like: the inputs of tg's icon update, not the sprites.
/// Species is always human. On a player mob it is the body; `uniform` and `shoes` there are
/// overwritten from its inventory (`crate::inventory`).
// shortcut: a female body wears the uniform uncropped; tg multiplies it by the `female_full` mask
// (`masking_helpers.dmi`, `wear_female_version`), a pixel mask overlays cannot express.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HumanLook {
    /// The species whose limbs are drawn (`crate::species::set_species` keeps it in step with
    /// the mob's `Species`).
    pub species: SpeciesId,
    /// tg `physique == FEMALE`: the dimorphic parts (head, chest) use their `_f` states.
    pub female: bool,
    /// A name from [`SKIN_TONES`].
    pub skin_tone: String,
    /// A hair `icon_state` in `human_face.dmi`; `None` = bald.
    pub hair_style: Option<String>,
    pub hair_color: [u8; 3],
    pub eye_color: [u8; 3],
    /// `features["mcolor"]`: the limbs' color for `LimbColor::Mutant` species.
    pub mutant_color: [u8; 3],
    /// Item types, see [`WORN`].
    pub uniform: Option<String>,
    pub shoes: Option<String>,
    /// tg `TRAIT_HUSK` (`HUSKED_BURN`): limbs husked (`huskify_image`), no hair
    /// (`head_hair_and_lips.dm`) and no eyes (`update_body_parts`).
    pub husk: bool,
    /// Amputated bodyparts, bits by [`crate::zone::BodyZone::index`] (tg draws no limb there).
    pub missing: u8,
    /// Robotic prosthetics, bits by [`crate::zone::BodyZone::index`]: drawn from
    /// [`ROBOTIC_LIMB_DMI`] (tg `/obj/item/bodypart/*/robot`, `limb_id = "robotic"`).
    pub robotic: u8,
    /// No eyes organ (tg `update_body_parts` draws eyes only from the organ).
    pub eyeless: bool,
}

impl HumanLook {
    /// An assistant with skin, hair and eyes picked from tg's random tables by `seed`.
    pub fn random_assistant(seed: u64) -> Self {
        Self {
            uniform: Some(ASSISTANT_UNIFORM.into()),
            shoes: Some(ASSISTANT_SHOES.into()),
            ..AppearancePrefs::random(seed).body()
        }
    }
}

/// A player's character appearance (tg's character preferences: body type, skin tone, hair style
/// and color, eye color) as indexes into [`SKIN_TONES`], [`HAIR_STYLES`], [`HAIR_COLORS`] and
/// [`EYE_COLORS`]; on the client entity, read when a body spawns.
// shortcut: kept for the connection only (no preferences database); tg's color pickers are the
// palettes of its random tables.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppearancePrefs {
    /// Index into [`roundstart_species`].
    pub species: usize,
    pub female: bool,
    pub skin_tone: usize,
    /// `None` = bald (tg "Bald").
    pub hair_style: Option<usize>,
    pub hair_color: usize,
    pub eye_color: usize,
    /// Index into [`MUTANT_COLORS`].
    pub mutant_color: usize,
}

/// The species a character may be (`get_selectable_species`), in preferences order.
pub fn roundstart_species() -> Vec<SpeciesId> {
    SpeciesId::roundstart().collect()
}

impl AppearancePrefs {
    /// tg `randomize_human_appearance` from the random tables, picked by `seed`.
    pub fn random(seed: u64) -> Self {
        let mut state = seed;
        let mut pick = |n: usize| (splitmix64(&mut state) % n as u64) as usize;
        // The species stays human (tg randomizes the species only when asked to).
        Self {
            species: 0,
            female: pick(2) == 1,
            skin_tone: pick(SKIN_TONES.len()),
            hair_style: Some(pick(HAIR_STYLES.len())),
            hair_color: pick(HAIR_COLORS.len()),
            eye_color: pick(EYE_COLORS.len()),
            mutant_color: pick(MUTANT_COLORS.len()),
        }
    }

    /// Every index inside its table (player input is checked with this before it is stored).
    pub fn is_valid(&self) -> bool {
        self.species < roundstart_species().len()
            && self.mutant_color < MUTANT_COLORS.len()
            && self.skin_tone < SKIN_TONES.len()
            && self.hair_style.is_none_or(|h| h < HAIR_STYLES.len())
            && self.hair_color < HAIR_COLORS.len()
            && self.eye_color < EYE_COLORS.len()
    }

    /// The naked body with this appearance.
    pub fn body(&self) -> HumanLook {
        let species = roundstart_species()
            .get(self.species)
            .copied()
            .unwrap_or_default();
        HumanLook {
            species,
            // `sexes = FALSE`: never female.
            female: self.female && species.def().sexes,
            skin_tone: SKIN_TONES[self.skin_tone].0.into(),
            hair_style: self.hair_style.map(|h| HAIR_STYLES[h].into()),
            hair_color: hex_rgb(HAIR_COLORS[self.hair_color]),
            eye_color: hex_rgb(EYE_COLORS[self.eye_color]),
            mutant_color: hex_rgb(MUTANT_COLORS[self.mutant_color]),
            uniform: None,
            shoes: None,
            husk: false,
            missing: 0,
            robotic: 0,
            eyeless: false,
        }
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// `0xRRGGBB` as bytes.
pub fn hex_rgb(hex: u32) -> [u8; 3] {
    let [_, r, g, b] = hex.to_be_bytes();
    [r, g, b]
}

/// One overlay before icon resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// tg standing layer (see [`layers`]).
    pub layer: f32,
    pub dmi: &'static str,
    pub state: String,
    pub color: Option<[u8; 3]>,
    /// [`Overlay::only_dirs`].
    pub only_dirs: u8,
}

/// Why a look cannot be drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookError(pub Vec<String>);

impl fmt::Display for LookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("; "))
    }
}

/// The overlays of `look` in draw order (bottom first).
///
/// Inside one layer the order is tg's: limbs in `/datum/species::bodypart_overrides` order
/// (left arm, right arm, head, left leg, right leg, chest), each item's GAGS layers bottom-up.
/// Legs are split like tg's `/obj/item/bodypart/leg/handle_masking`: `leg_masks.dmi` keeps a leg
/// whole or empty per direction, so each leg is two overlays shown for some facings only, the far
/// leg of an east or west facing on [`BODYPARTS_LOW_LAYER`].
pub fn parts(look: &HumanLook) -> Result<Vec<Part>, LookError> {
    let mut errors = Vec::new();
    let limbs = look.species.def().limbs;
    let mut skin = match limbs.color {
        LimbColor::Skin => {
            let skin = SKIN_TONES
                .iter()
                .find(|(name, _)| *name == look.skin_tone)
                .map(|&(_, color)| color);
            if skin.is_none() {
                errors.push(format!("unknown skin tone {:?}", look.skin_tone));
            }
            skin
        }
        LimbColor::Mutant => Some(look.mutant_color),
        LimbColor::Fixed(color) => Some(color),
        LimbColor::Static => None,
    };
    if look.husk {
        skin = Some(HUSK_SKIN);
    }
    let limb = |layer, state: &str, only_dirs: u8| Part {
        layer,
        dmi: limbs.dmi,
        state: format!("{}_{state}", limbs.id),
        color: skin,
        only_dirs,
    };
    // `digitigrade_*_leg` of the same icon (`limb_id = BODYPART_ID_DIGITIGRADE`).
    let leg = |layer, side: &str, only_dirs: u8| Part {
        state: if limbs.digitigrade {
            format!("digitigrade_{side}_leg")
        } else {
            format!("{}_{side}_leg", limbs.id)
        },
        ..limb(layer, "", only_dirs)
    };
    let gender = if look.female { "f" } else { "m" };
    let dimorphic = |part: &str, dimorphic: bool| {
        if dimorphic {
            format!("{part}_{gender}")
        } else {
            part.to_owned()
        }
    };
    let (n, s, e, w) = (
        Dir::North as u8,
        Dir::South as u8,
        Dir::East as u8,
        Dir::West as u8,
    );
    use crate::zone::BodyZone;
    let has = |zone: BodyZone| look.missing & (1 << zone.index()) == 0;
    let mut limb_parts = vec![
        (BodyZone::LArm, limb(BODYPARTS_LAYER, "l_arm", 0)),
        (BodyZone::LArm, limb(BODYPARTS_HIGH_LAYER, "l_hand", 0)),
        (BodyZone::RArm, limb(BODYPARTS_LAYER, "r_arm", 0)),
        (BodyZone::RArm, limb(BODYPARTS_HIGH_LAYER, "r_hand", 0)),
        (
            BodyZone::Head,
            limb(BODYPARTS_LAYER, &dimorphic("head", limbs.head_dimorphic), 0),
        ),
        // `left_leg` / `left_leg_lower` masks.
        (BodyZone::LLeg, leg(BODYPARTS_LAYER, "l", s | n | w)),
        (BodyZone::LLeg, leg(BODYPARTS_LOW_LAYER, "l", e)),
        // `right_leg` / `right_leg_lower`.
        (BodyZone::RLeg, leg(BODYPARTS_LAYER, "r", s | n | e)),
        (BodyZone::RLeg, leg(BODYPARTS_LOW_LAYER, "r", w)),
        (
            BodyZone::Chest,
            limb(
                BODYPARTS_LAYER,
                &dimorphic("chest", limbs.chest_dimorphic),
                0,
            ),
        ),
    ];
    limb_parts.retain(|(zone, part)| {
        let hand = part.state.ends_with("_hand");
        let leg = matches!(zone, BodyZone::LLeg | BodyZone::RLeg);
        (!hand || limbs.hands) && (!leg || limbs.legs)
    });
    let robotic = |zone: BodyZone| look.robotic & (1 << zone.index()) != 0;
    let mut parts: Vec<Part> = limb_parts
        .into_iter()
        .filter(|(zone, _)| has(*zone))
        .map(|(zone, part)| {
            if !robotic(zone) {
                return part;
            }
            // `robotic_l_arm`, `robotic_l_hand`, `robotic_head`, `robotic_l_leg`: not
            // greyscale, not dimorphic.
            let name = match zone {
                BodyZone::LArm if part.state.ends_with("_hand") => "l_hand",
                BodyZone::LArm => "l_arm",
                BodyZone::RArm if part.state.ends_with("_hand") => "r_hand",
                BodyZone::RArm => "r_arm",
                BodyZone::Head => "head",
                BodyZone::LLeg => "l_leg",
                BodyZone::RLeg => "r_leg",
                BodyZone::Chest => "chest",
            };
            Part {
                dmi: ROBOTIC_LIMB_DMI,
                state: format!("robotic_{name}"),
                color: None,
                ..part
            }
        })
        .collect();
    if look.husk {
        // Blend-inset in tg (masked by the limb); these sprites stay inside their limbs.
        for (zone, name) in [
            (BodyZone::LArm, "l_arm"),
            (BodyZone::RArm, "r_arm"),
            (BodyZone::Head, "head"),
            (BodyZone::LLeg, "l_leg"),
            (BodyZone::RLeg, "r_leg"),
            (BodyZone::Chest, "chest"),
        ] {
            // `BODYPART_UNHUSKABLE` robotic limbs.
            if !has(zone)
                || robotic(zone)
                || (!limbs.legs && matches!(zone, BodyZone::LLeg | BodyZone::RLeg))
            {
                continue;
            }
            parts.push(Part {
                layer: BODYPARTS_LAYER,
                dmi: HUSK_DMI,
                state: format!("humanoid_husk_{name}"),
                color: Some(HUSK_BLOOD),
                only_dirs: 0,
            });
        }
    } else if !look.eyeless
        && has(BodyZone::Head)
        && let Some(eyes) = limbs.eyes
    {
        for side in ["l", "r"] {
            parts.push(Part {
                layer: EYES_LAYER,
                dmi: EYES_DMI,
                state: format!("{eyes}_{side}"),
                color: Some(look.eye_color),
                only_dirs: 0,
            });
        }
    }
    for item in [&look.uniform, &look.shoes].into_iter().flatten() {
        let Some((_, layer, sprites)) = WORN.iter().find(|(ty, ..)| ty == item) else {
            errors.push(format!("no worn sprites for {item}"));
            continue;
        };
        parts.extend(sprites.iter().map(|&(dmi, state, color)| Part {
            layer: *layer,
            dmi,
            state: state.into(),
            color,
            only_dirs: 0,
        }));
    }
    if let Some(style) = look
        .hair_style
        .as_ref()
        .filter(|_| !look.husk && limbs.hair && has(BodyZone::Head))
    {
        parts.push(Part {
            layer: HAIR_LAYER,
            dmi: HAIR_DMI,
            state: style.clone(),
            color: Some(look.hair_color),
            only_dirs: 0,
        });
    }
    if !errors.is_empty() {
        return Err(LookError(errors));
    }
    // Stable: equal layers keep the order above.
    parts.sort_by(|a, b| b.layer.total_cmp(&a.layer));
    Ok(parts)
}

/// Built human appearances by look. Holds the icon ids of the few `.dmi` files humans use, so
/// a new look never touches the full manifest.
#[derive(Resource, Debug)]
pub struct HumanAppearances {
    /// Layer, plane and offset of the mob type; icon and overlays are replaced.
    base: Appearance,
    ids: HashMap<(String, String), IconStateId>,
    /// The appearance and the tg layer of each of its overlays.
    cache: HashMap<HumanLook, (Appearance, Vec<f32>)>,
}

impl HumanAppearances {
    pub fn new(manifest: &AssetManifest, base: Appearance) -> Self {
        let mut dmis: HashSet<&str> = [BASE.0, BODYPARTS_DMI, EYES_DMI, HAIR_DMI, HUSK_DMI].into();
        dmis.extend(crate::species::LIMB_DMIS);
        dmis.extend(
            WORN.iter()
                .flat_map(|(_, _, s)| s.iter().map(|(dmi, ..)| *dmi)),
        );
        let ids = manifest
            .index()
            .into_iter()
            .filter(|((dmi, _), _)| dmis.contains(dmi))
            .map(|((dmi, state), id)| ((dmi.to_owned(), state.to_owned()), id))
            .collect();
        Self {
            base,
            ids,
            cache: HashMap::new(),
        }
    }

    /// The appearance of `look`, built on first use.
    pub fn get(&mut self, look: &HumanLook) -> Result<&Appearance, LookError> {
        Ok(&self.cached(look)?.0)
    }

    /// `look` with item sprites `(tg layer, state)` (held items, ID) added at their layers.
    /// Items are not part of the cache key: the body is built once, items are merged per call.
    pub fn dressed(
        &mut self,
        look: &HumanLook,
        items: &[(f32, IconStateId)],
    ) -> Result<Appearance, LookError> {
        let (base, layers) = self.cached(look)?;
        let mut overlays: Vec<(f32, Overlay)> = layers
            .iter()
            .copied()
            .zip(base.overlays.iter().cloned())
            .collect();
        overlays.extend(
            items
                .iter()
                .map(|&(layer, icon_state)| (layer, Overlay::new(icon_state))),
        );
        // Stable: items go above body parts of the same layer.
        overlays.sort_by(|a, b| b.0.total_cmp(&a.0));
        Ok(Appearance {
            overlays: overlays.into_iter().map(|(_, o)| o).collect(),
            ..base.clone()
        })
    }

    fn cached(&mut self, look: &HumanLook) -> Result<&(Appearance, Vec<f32>), LookError> {
        if !self.cache.contains_key(look) {
            let built = self.build(look)?;
            self.cache.insert(look.clone(), built);
        }
        Ok(&self.cache[look])
    }

    /// Distinct looks built so far.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    fn build(&self, look: &HumanLook) -> Result<(Appearance, Vec<f32>), LookError> {
        let parts = parts(look)?;
        let mut missing = Vec::new();
        let mut id = |dmi: &str, state: &str| {
            let id = self.ids.get(&(dmi.to_owned(), state.to_owned())).copied();
            if id.is_none() {
                missing.push(format!("missing icon state {dmi}:{state}"));
            }
            id.unwrap_or(IconStateId(0))
        };
        let base = id(BASE.0, BASE.1);
        let overlays = parts
            .iter()
            .map(|p| Overlay {
                // Turning is a `Facing` change only; every part has 4 dirs.
                // `None` inherits the mob's color, which is untinted.
                color: p.color.map(|[r, g, b]| [r, g, b, 255]),
                only_dirs: p.only_dirs,
                ..Overlay::new(id(p.dmi, &p.state))
            })
            .collect();
        if !missing.is_empty() {
            return Err(LookError(missing));
        }
        let appearance = Appearance {
            icon_state: base,
            overlays,
            ..self.base.clone()
        };
        Ok((appearance, parts.iter().map(|p| p.layer).collect()))
    }
}

/// The body of a new player's mob from its character preferences (a random look without them);
/// clothes come from its inventory.
pub(crate) fn player_body(client: Entity, prefs: Option<&AppearancePrefs>) -> HumanLook {
    prefs
        .copied()
        .unwrap_or_else(|| AppearancePrefs::random(client.to_bits()))
        .body()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ss13_assets::manifest::IconStateInfo;
    use ss13_shared::Layer;

    use super::*;

    #[test]
    fn appearance_prefs_check_their_indexes() {
        let mut prefs = AppearancePrefs::random(7);
        assert!(prefs.is_valid());
        prefs.hair_style = None;
        assert!(prefs.is_valid() && prefs.body().hair_style.is_none());
        prefs.skin_tone = SKIN_TONES.len();
        assert!(!prefs.is_valid());
        prefs.skin_tone = 0;
        prefs.hair_style = Some(HAIR_STYLES.len());
        assert!(!prefs.is_valid());
        // The random assistant is the random appearance plus the assistant's clothes.
        let body = AppearancePrefs::random(7).body();
        assert_eq!(
            HumanLook {
                uniform: None,
                shoes: None,
                ..HumanLook::random_assistant(7)
            },
            body
        );
    }

    /// Male, so the tests can name the dimorphic states.
    fn look(seed: u64) -> HumanLook {
        HumanLook {
            female: false,
            ..HumanLook::random_assistant(seed)
        }
    }

    fn base() -> Appearance {
        Appearance {
            icon_state: IconStateId(0),
            layer: Layer(4.0),
            plane: -1,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: Default::default(),
            overlays: vec![],
        }
    }

    /// A manifest with exactly the states `looks` need, plus the base.
    fn manifest_for(looks: &[HumanLook]) -> AssetManifest {
        let mut names = vec![(BASE.0.to_owned(), BASE.1.to_owned())];
        for l in looks {
            for p in parts(l).unwrap() {
                names.push((p.dmi.to_owned(), p.state));
            }
        }
        names.dedup();
        AssetManifest {
            atlases: vec![],
            states: names
                .into_iter()
                .map(|(dmi, state)| IconStateInfo {
                    dmi,
                    state,
                    atlas: 0,
                    width: 32,
                    height: 32,
                    dirs: 4,
                    rects: vec![[0, 0, 32, 32]; 4],
                    delays: vec![],
                    loop_count: 0,
                    rewind: false,
                    movement: false,
                })
                .collect(),
        }
    }

    #[test]
    fn parts_follow_tg_layer_order() {
        let parts = parts(&look(1)).unwrap();
        assert!(parts.windows(2).all(|w| w[0].layer >= w[1].layer));
        let at = |state: &str| parts.iter().position(|p| p.state == state).unwrap();
        // Under the uniform: limbs, eyes. Over it: hands, shoes, then hair on top.
        assert!(at("human_chest_m") < at("eyes_l"));
        assert!(at("eyes_r") < at("jumpsuit"));
        assert!(at("jumpsuit") < at("jumpsuit_accessories"));
        assert!(at("jumpsuit_accessories") < at("human_l_hand"));
        assert!(at("human_r_hand") < at("sneakers_back"));
        assert!(at("sneakers_front") < parts.len() - 1);
        assert_eq!(parts.last().unwrap().layer, HAIR_LAYER);
        // tg limb order inside BODYPARTS_LAYER: the chest is drawn last.
        assert!(at("human_head_m") < at("human_chest_m"));
        assert!(at("human_r_leg") < at("human_chest_m"));
    }

    #[test]
    fn female_parts_and_legs_split_per_facing() {
        let mut l = look(5);
        l.female = true;
        let parts = parts(&l).unwrap();
        let states: Vec<&str> = parts.iter().map(|p| p.state.as_str()).collect();
        assert!(states.contains(&"human_chest_f") && states.contains(&"human_head_f"));
        assert!(!states.contains(&"human_chest_m"));
        let legs: Vec<&Part> = parts.iter().filter(|p| p.state.ends_with("_leg")).collect();
        assert_eq!(legs.len(), 4);
        // Facing east the left leg is the far one: drawn first (lowest), and only then.
        let low = legs[0];
        assert_eq!(
            (low.layer, low.state.as_str()),
            (BODYPARTS_LOW_LAYER, "human_l_leg")
        );
        for dir in Dir::CARDINALS {
            for side in ["human_l_leg", "human_r_leg"] {
                let shown = legs
                    .iter()
                    .filter(|p| p.state == side)
                    .filter(|p| {
                        Overlay {
                            only_dirs: p.only_dirs,
                            ..Overlay::new(IconStateId(0))
                        }
                        .shown_for(dir)
                    })
                    .count();
                assert_eq!(shown, 1, "{side} facing {dir:?}");
            }
        }
    }

    #[test]
    fn colors_are_tg_values() {
        let mut l = look(2);
        l.skin_tone = "african2".into();
        l.eye_color = [0x33, 0x66, 0xcc];
        let parts = parts(&l).unwrap();
        let color = |state: &str| parts.iter().find(|p| p.state == state).unwrap().color;
        assert_eq!(color("human_chest_m"), Some([0x47, 0x1c, 0x18]));
        assert_eq!(color("human_r_hand"), Some([0x47, 0x1c, 0x18]));
        assert_eq!(color("eyes_l"), Some([0x33, 0x66, 0xcc]));
        assert_eq!(color("jumpsuit"), Some([0xb3, 0xb3, 0xb3]));
        assert_eq!(color("jumpsuit_accessories"), None);
    }

    #[test]
    fn unknown_inputs_are_errors() {
        let mut l = look(3);
        l.skin_tone = "green-ish".into();
        l.shoes = Some("/obj/item/clothing/shoes/clown_shoes".into());
        assert_eq!(parts(&l).unwrap_err().0.len(), 2);
    }

    #[test]
    fn looks_are_built_once_and_cached() {
        let (a, b) = (look(10), look(11));
        assert_ne!(a, b, "seeds give different looks");
        let mut humans = HumanAppearances::new(&manifest_for(&[a.clone(), b.clone()]), base());
        let first = humans.get(&a).unwrap().clone();
        assert_eq!(humans.len(), 1);
        assert_eq!(humans.get(&a).unwrap(), &first);
        assert_eq!(humans.len(), 1);
        humans.get(&b).unwrap();
        assert_eq!(humans.len(), 2);
        assert_eq!(first.overlays.len(), parts(&a).unwrap().len());
        assert!(first.overlays.iter().all(|o| o.dir.is_none()));
        assert_eq!(first.layer, Layer(4.0));
    }

    #[test]
    fn items_merge_at_their_layers() {
        let a = look(12);
        let mut humans = HumanAppearances::new(&manifest_for(std::slice::from_ref(&a)), base());
        let body = humans.get(&a).unwrap().clone();
        let (held, id) = (IconStateId(900), IconStateId(901));
        let dressed = humans
            .dressed(&a, &[(HANDS_LAYER, held), (ID_LAYER, id)])
            .unwrap();
        assert_eq!(humans.len(), 1, "items do not grow the cache");
        let states: Vec<_> = dressed.overlays.iter().map(|o| o.icon_state).collect();
        assert_eq!(states.len(), body.overlays.len() + 2);
        // Held items on top of everything (hair included); the ID between uniform and hands.
        assert_eq!(states.last(), Some(&held));
        let parts = parts(&a).unwrap();
        let id_at = states.iter().position(|&s| s == id).unwrap();
        assert_eq!(parts[id_at].layer, BODYPARTS_HIGH_LAYER);
        assert!(parts[id_at - 1].layer >= UNIFORM_LAYER);
        assert_eq!(humans.dressed(&a, &[]).unwrap(), body);
    }

    #[test]
    fn missing_states_are_reported_and_not_cached() {
        let mut humans = HumanAppearances::new(&AssetManifest::default(), base());
        let err = humans.get(&look(4)).unwrap_err();
        assert!(
            err.0
                .iter()
                .any(|e| e.contains("bodyparts_greyscale.dmi:human_chest_m"))
        );
        assert!(humans.is_empty());
    }

    fn source() -> PathBuf {
        std::env::var("SS13_SOURCE")
            .unwrap_or_else(|_| env!("SS13_SOURCE").into())
            .into()
    }

    /// Needs the BYOND checkout (`SS13_SOURCE`).
    #[test]
    #[ignore]
    fn layers_match_tg_defines() {
        let path = source().join("code/__DEFINES/mobs.dm");
        let text = std::fs::read_to_string(&path).unwrap();
        let define = |name: &str| -> f32 {
            text.lines()
                .find_map(|l| {
                    let mut words = l.split_whitespace();
                    (words.next() == Some("#define") && words.next() == Some(name))
                        .then(|| words.next().unwrap().parse().unwrap())
                })
                .unwrap_or_else(|| panic!("{name} not in {}", path.display()))
        };
        for (name, value) in [
            ("BODYPARTS_LAYER", BODYPARTS_LAYER),
            ("EYES_LAYER", EYES_LAYER),
            ("UNIFORM_LAYER", UNIFORM_LAYER),
            ("BODYPARTS_HIGH_LAYER", BODYPARTS_HIGH_LAYER),
            ("SHOES_LAYER", SHOES_LAYER),
            ("HAIR_LAYER", HAIR_LAYER),
            ("ID_LAYER", ID_LAYER),
            ("GLOVES_LAYER", GLOVES_LAYER),
            ("EARS_LAYER", EARS_LAYER),
            ("SUIT_LAYER", SUIT_LAYER),
            ("GLASSES_LAYER", GLASSES_LAYER),
            ("BELT_LAYER", BELT_LAYER),
            ("SUIT_STORE_LAYER", SUIT_STORE_LAYER),
            ("NECK_LAYER", NECK_LAYER),
            ("BACK_LAYER", BACK_LAYER),
            ("FACEMASK_LAYER", FACEMASK_LAYER),
            ("HEAD_LAYER", HEAD_LAYER),
            ("HANDS_LAYER", HANDS_LAYER),
        ] {
            assert_eq!(define(name), value, "{name}");
        }
    }

    /// Needs generated assets (`SS13_ASSETS`, see docs/RUNNING.md).
    #[test]
    #[ignore]
    fn every_look_resolves_against_generated_assets() {
        let assets: PathBuf = std::env::var("SS13_ASSETS")
            .unwrap_or_else(|_| "assets/generated".into())
            .into();
        let manifest = ss13_mapload::read_manifest(&assets).unwrap();
        let mut humans = HumanAppearances::new(&manifest, base());
        let mut looks = Vec::new();
        for (i, (tone, _)) in SKIN_TONES.iter().enumerate() {
            let mut l = look(0);
            l.skin_tone = (*tone).into();
            l.hair_style = HAIR_STYLES.get(i).map(|s| (*s).into());
            looks.push(l);
        }
        for l in &looks {
            let appearance = humans.get(l).unwrap_or_else(|e| panic!("{e}")).clone();
            let base = manifest.get(appearance.icon_state).unwrap();
            assert_eq!((base.dmi.as_str(), base.state.as_str()), BASE);
            for o in &appearance.overlays {
                let info = manifest.get(o.icon_state).unwrap();
                assert_eq!(info.dirs, 4, "{}:{} must have 4 dirs", info.dmi, info.state);
                assert!(!info.movement);
            }
        }
        assert_eq!(humans.len(), looks.len());
    }
}
