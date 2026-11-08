//! Blob strains after tg `/datum/blobstrain/reagent` (`blobstrains/*.dm`): the reagent a blob
//! injects when it attacks (`attack_living` -> `/datum/reagent/blob/expose_mob`) and the passive
//! reactions of its structures (`damage_reaction`, `expand_reaction`).
//!
//! Eight strains are ported: reactive spines, blazing oil, cryogenic poison, energized jelly,
//! networked fibers, regenerative materia, replicating foam, shifting fragments. The pure parts
//! live here ([`exposure`], [`reac_volume`], [`damage_taken`]); the reactions that move or make
//! structures are in `super::structures`.
// shortcut: debris devourer, distributed neurons, electromagnetic web, explosive lattice,
// multiplex, pressurized slime and synchronous mesh are not ported (a new overmind never rolls
// them); the blob reagents' own metabolism (cryogenic poison's and regenerative materia's damage
// over time, the fake healthy HUD), blazing oil's water weakness and flame bursts, energized
// jelly's tesla immunity, EMP weakness, sparks and `losebreath`, regenerative materia's
// drugginess are not ported either.

/// tg `BLOB_REAGENTATK_VOL`.
pub const REAGENT_ATTACK_VOLUME: f32 = 25.0;
/// tg `BLOBMOB_BLOBBERNAUT_REAGENTATK_VOL`.
pub const BLOBBERNAUT_ATTACK_VOLUME: f32 = 20.0;
/// tg `BLOB_BASE_POINT_RATE`, `BLOB_CORE_HP_REGEN`.
pub const BASE_POINT_RATE: f32 = 2.0;
pub const BASE_CORE_REGEN: f32 = 2.0;
/// `reactive_spines` `retaliate_cooldown`, seconds.
pub const RETALIATE_COOLDOWN: f64 = 2.5;

/// A ported `/datum/blobstrain/reagent` subtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Strain {
    ReactiveSpines,
    BlazingOil,
    CryogenicPoison,
    EnergizedJelly,
    NetworkedFibers,
    RegenerativeMateria,
    ReplicatingFoam,
    ShiftingFragments,
}

/// A strain's vars.
#[derive(Clone, Copy, Debug)]
pub struct StrainDef {
    pub name: &'static str,
    pub description: &'static str,
    pub effectdesc: Option<&'static str>,
    pub analyzer_damage: &'static str,
    pub analyzer_effect: Option<&'static str>,
    pub color: [u8; 3],
    pub complementary: [u8; 3],
    pub blobbernaut_message: &'static str,
    pub message: &'static str,
    pub message_living: Option<&'static str>,
    pub core_regen_bonus: f32,
    pub point_rate_bonus: f32,
}

const fn def(name: &'static str, color: [u8; 3], complementary: [u8; 3]) -> StrainDef {
    StrainDef {
        name,
        description: "",
        effectdesc: None,
        analyzer_damage: "",
        analyzer_effect: None,
        color,
        complementary,
        blobbernaut_message: "slams",
        message: "The blob strikes you",
        message_living: None,
        core_regen_bonus: 0.0,
        point_rate_bonus: 0.0,
    }
}

const REACTIVE_SPINES: StrainDef = StrainDef {
    description: "will do high brute damage through armor and bio resistance.",
    effectdesc: Some(
        "will also react when attacked with burn or brute damage, attacking everything in melee range.",
    ),
    analyzer_damage: "Does high brute damage, ignoring armor and bio resistance.",
    analyzer_effect: Some(
        "When attacked with burn or brute damage it violently lashes out, attacking everything nearby.",
    ),
    blobbernaut_message: "stabs",
    message: "The blob stabs you",
    ..def("Reactive Spines", [0x9A, 0xCD, 0x32], [0xFF, 0xA5, 0x00])
};

const BLAZING_OIL: StrainDef = StrainDef {
    description: "will do medium-high burn damage ignoring armor, and set targets on fire.",
    effectdesc: Some("will also release bursts of flame when burnt, but takes damage from water."),
    analyzer_damage: "Does medium-high burn damage and sets targets on fire.",
    analyzer_effect: Some(
        "Releases fire when burnt, but takes damage from water and other extinguishing liquids.",
    ),
    blobbernaut_message: "splashes",
    message: "The blob splashes you with burning oil",
    message_living: Some(", and you feel your skin char and melt"),
    ..def("Blazing Oil", [0xB6, 0x8D, 0x00], [0xBE, 0x55, 0x32])
};

const CRYOGENIC_POISON: StrainDef = StrainDef {
    description: "will inject targets with a freezing poison, applying little impact damage but dealing high damage over time.",
    analyzer_damage: "Injects targets with a freezing poison that will gradually solidify the target's internal organs.",
    blobbernaut_message: "injects",
    message: "The blob stabs you",
    message_living: Some(", and you feel like your insides are solidifying"),
    ..def("Cryogenic Poison", [0x8B, 0xA6, 0xE9], [0x7D, 0x6E, 0xB4])
};

const ENERGIZED_JELLY: StrainDef = StrainDef {
    description: "will cause high stamina and medium oxygen damage, and cause targets to be unable to breathe.",
    effectdesc: Some("will also conduct electricity, but takes damage from EMPs."),
    analyzer_damage: "Does high stamina damage, medium oxygen damage, and prevents targets from breathing.",
    analyzer_effect: Some(
        "Is immune to electricity and will easily conduct it, but is weak to EMPs.",
    ),
    ..def("Energized Jelly", [0xEF, 0xD6, 0x5A], [0x00, 0xE5, 0xB1])
};

const NETWORKED_FIBERS: StrainDef = StrainDef {
    description: "will do a high mix of brute and burn damage, and will generate resources quicker, but can only expand manually using the core or nodes.",
    effectdesc: Some("will move your core or node when manually expanding near it."),
    analyzer_damage: "Does a high mix of brute and burn damage.",
    analyzer_effect: Some("Is mobile and generates resources rapidly."),
    core_regen_bonus: 3.0,
    ..def("Networked Fibers", [0x4F, 0x44, 0x41], [0x41, 0x4C, 0x4F])
};

const REGENERATIVE_MATERIA: StrainDef = StrainDef {
    description: "will do medium initial toxin damage, injecting a poison which does more toxin damage and makes targets believe they are fully healed. The core regenerates much faster.",
    analyzer_damage: "Does medium initial toxin damage, injecting a poison which does more toxin damage and makes targets believe they are fully healed. Core regenerates much faster.",
    message_living: Some(", and you feel <i>alive</i>"),
    core_regen_bonus: 18.0,
    point_rate_bonus: 1.0,
    ..def(
        "Regenerative Materia",
        [0xA8, 0x8F, 0xB7],
        [0xAF, 0x7B, 0x8D],
    )
};

const REPLICATING_FOAM: StrainDef = StrainDef {
    description: "will do medium brute damage and occasionally expand again when expanding.",
    effectdesc: Some(
        "will also expand when attacked with burn damage, but takes more brute damage.",
    ),
    analyzer_damage: "Does medium brute damage.",
    analyzer_effect: Some(
        "Expands when attacked with burn damage, will occasionally expand again when expanding, and is fragile to brute damage.",
    ),
    ..def("Replicating Foam", [0x7B, 0x5A, 0x57], [0x57, 0x78, 0x7B])
};

const SHIFTING_FRAGMENTS: StrainDef = StrainDef {
    description: "will do medium brute damage.",
    effectdesc: Some("will also cause blob parts to shift away when attacked."),
    analyzer_damage: "Does medium brute damage.",
    analyzer_effect: Some("When attacked, may shift away from the attacker."),
    ..def("Shifting Fragments", [0xC8, 0x96, 0x3C], [0x3C, 0x6E, 0xC8])
};

impl Strain {
    /// `GLOB.valid_blobstrains` of the ported strains.
    pub const ALL: [Strain; 8] = [
        Strain::ReactiveSpines,
        Strain::BlazingOil,
        Strain::CryogenicPoison,
        Strain::EnergizedJelly,
        Strain::NetworkedFibers,
        Strain::RegenerativeMateria,
        Strain::ReplicatingFoam,
        Strain::ShiftingFragments,
    ];

    pub fn def(self) -> &'static StrainDef {
        match self {
            Strain::ReactiveSpines => &REACTIVE_SPINES,
            Strain::BlazingOil => &BLAZING_OIL,
            Strain::CryogenicPoison => &CRYOGENIC_POISON,
            Strain::EnergizedJelly => &ENERGIZED_JELLY,
            Strain::NetworkedFibers => &NETWORKED_FIBERS,
            Strain::RegenerativeMateria => &REGENERATIVE_MATERIA,
            Strain::ReplicatingFoam => &REPLICATING_FOAM,
            Strain::ShiftingFragments => &SHIFTING_FRAGMENTS,
        }
    }

    pub fn by_name(name: &str) -> Option<Strain> {
        Self::ALL.into_iter().find(|s| s.def().name == name)
    }

    /// `color` as an RGBA multiply.
    pub fn color(self) -> [u8; 4] {
        let [r, g, b] = self.def().color;
        [r, g, b, 255]
    }

    /// `complementary_color` as an RGBA multiply.
    pub fn complementary(self) -> [u8; 4] {
        let [r, g, b] = self.def().complementary;
        [r, g, b, 255]
    }

    /// `send_message`: "The blob strikes you!" (`message_living` too unless the mob is a
    /// silicon).
    pub fn attack_message(self, silicon: bool) -> String {
        let d = self.def();
        let living = if silicon {
            ""
        } else {
            d.message_living.unwrap_or("")
        };
        format!("{}{living}!", d.message)
    }
}

/// `/datum/reagent/blob/return_mob_expose_reac_volume`: nothing on the dead and on blob allies;
/// full bio protection halves it (any protection below 0.5 changes nothing). Reactive spines
/// ignore the protection.
pub fn reac_volume(strain: Strain, volume: f32, bio_protection: f32, dead_or_ally: bool) -> f32 {
    if dead_or_ally {
        return 0.0;
    }
    if strain == Strain::ReactiveSpines {
        return volume;
    }
    let v = volume * (1.5 - bio_protection).min(1.0);
    (v * 10.0).round() / 10.0
}

/// What one exposure to a strain's reagent does to a mob.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Exposure {
    pub brute: f32,
    pub burn: f32,
    pub tox: f32,
    pub oxy: f32,
    pub stamina: f32,
    pub fire_stacks: f32,
    pub ignite: bool,
    /// Reagents added to the blood: (type, units).
    pub reagents: Vec<(&'static str, f32)>,
}

pub const FROSTOIL: &str = "/datum/reagent/consumable/frostoil";
pub const ICE: &str = "/datum/reagent/consumable/ice";
pub const SPORE_TOXIN: &str = "/datum/reagent/toxin/spore";

/// The strain reagent's `expose_mob` for `volume` (already through [`reac_volume`]).
pub fn exposure(strain: Strain, volume: f32) -> Exposure {
    let v = volume;
    match strain {
        Strain::ReactiveSpines => Exposure {
            brute: v,
            ..Default::default()
        },
        Strain::BlazingOil => Exposure {
            burn: 0.8 * v,
            fire_stacks: (v / 10.0).round(),
            ignite: true,
            ..Default::default()
        },
        Strain::CryogenicPoison => Exposure {
            brute: 0.2 * v,
            reagents: vec![(FROSTOIL, 0.3 * v), (ICE, 0.3 * v)],
            ..Default::default()
        },
        Strain::EnergizedJelly => Exposure {
            stamina: 1.2 * v,
            oxy: 0.6 * v,
            ..Default::default()
        },
        Strain::NetworkedFibers => Exposure {
            brute: 0.6 * v,
            burn: 0.6 * v,
            ..Default::default()
        },
        Strain::RegenerativeMateria => Exposure {
            tox: 0.7 * v,
            reagents: vec![(SPORE_TOXIN, 0.2 * v)],
            ..Default::default()
        },
        Strain::ReplicatingFoam | Strain::ShiftingFragments => Exposure {
            brute: 0.7 * v,
            ..Default::default()
        },
    }
}

/// The damage kinds `damage_reaction` sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hurt {
    Brute,
    Burn,
}

/// The damage flag of a hit (`MELEE`, `BULLET`, `LASER`, `BOMB`, `FIRE`, `ENERGY`, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Melee,
    Bullet,
    Laser,
    Energy,
    Bomb,
    Fire,
    Acid,
}

impl Flag {
    /// A projectile's `armor_flag`.
    pub fn from_dm(flag: &str) -> Flag {
        match flag {
            "bullet" => Flag::Bullet,
            "laser" => Flag::Laser,
            "energy" => Flag::Energy,
            "bomb" => Flag::Bomb,
            "fire" => Flag::Fire,
            "acid" => Flag::Acid,
            _ => Flag::Melee,
        }
    }
}

/// The numeric half of `damage_reaction`: replicating foam takes double brute, blazing oil
/// nothing from fire.
pub fn damage_taken(strain: Strain, damage: f32, kind: Hurt, flag: Flag) -> f32 {
    match strain {
        Strain::ReplicatingFoam if kind == Hurt::Brute => damage * 2.0,
        Strain::BlazingOil if flag == Flag::Fire => 0.0,
        _ => damage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protection_halves_at_most_and_spines_ignore_it() {
        let v = REAGENT_ATTACK_VOLUME;
        assert_eq!(reac_volume(Strain::ReplicatingFoam, v, 0.0, false), 25.0);
        assert_eq!(reac_volume(Strain::ReplicatingFoam, v, 0.4, false), 25.0);
        assert_eq!(reac_volume(Strain::ReplicatingFoam, v, 1.0, false), 12.5);
        assert_eq!(reac_volume(Strain::ReplicatingFoam, v, 0.75, false), 18.8);
        assert_eq!(reac_volume(Strain::ReactiveSpines, v, 1.0, false), 25.0);
        assert_eq!(reac_volume(Strain::ReactiveSpines, v, 0.0, true), 0.0);
    }

    #[test]
    fn exposures_follow_the_reagents() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let e = exposure(Strain::BlazingOil, 25.0);
        assert!(near(e.burn, 20.0) && e.fire_stacks == 3.0 && e.ignite);
        let e = exposure(Strain::NetworkedFibers, 25.0);
        assert!(near(e.brute, 15.0) && near(e.burn, 15.0));
        let e = exposure(Strain::CryogenicPoison, 20.0);
        assert!(near(e.brute, 4.0));
        assert_eq!(e.reagents.len(), 2);
        assert!(e.reagents[0].0 == FROSTOIL && near(e.reagents[0].1, 6.0));
        let e = exposure(Strain::EnergizedJelly, 25.0);
        assert!(near(e.stamina, 30.0) && near(e.oxy, 15.0));
        assert!(near(exposure(Strain::RegenerativeMateria, 25.0).tox, 17.5));
    }

    #[test]
    fn reactions_scale_the_damage() {
        assert_eq!(
            damage_taken(Strain::ReplicatingFoam, 10.0, Hurt::Brute, Flag::Melee),
            20.0
        );
        assert_eq!(
            damage_taken(Strain::ReplicatingFoam, 10.0, Hurt::Burn, Flag::Laser),
            10.0
        );
        assert_eq!(
            damage_taken(Strain::BlazingOil, 10.0, Hurt::Burn, Flag::Fire),
            0.0
        );
        assert_eq!(
            Strain::BlazingOil.attack_message(false),
            "The blob splashes you with burning oil, and you feel your skin char and melt!"
        );
        assert_eq!(
            Strain::BlazingOil.attack_message(true),
            "The blob splashes you with burning oil!"
        );
        assert_eq!(
            Strain::by_name("Networked Fibers"),
            Some(Strain::NetworkedFibers)
        );
        assert_eq!(Strain::ALL.len(), 8);
    }
}
