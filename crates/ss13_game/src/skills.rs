//! Skills (server), after tg `/datum/skill` (`code/datums/skills`) and the mind's skill procs
//! (`code/datums/mind/skills.dm`).
//!
//! - A mob's [`Skills`] keep the experience and level of every ported skill; a mob without the
//!   component has every skill at `SKILL_LEVEL_NONE` with no experience.
//! - [`adjust_experience`] moves the experience (never below 0) and the level follows
//!   `SKILL_EXP_LIST`; a level up or down tells the mob its skill's level message.
//! - [`modifier`] is `get_skill_modifier`: the skill's modifier list at the current level. The
//!   callers apply it as tg does: cleaning speed (mop, soap, `crate::janitor`), mining speed
//!   (`crate::mining::rocks`), athletics climbing time (`crate::tables`).
//! - Experience: cleaning a dirty tile (`round(beauty / -15)` per decal, `round(1.5)` per
//!   wash), mining a wall with a tool or a kinetic blast (the ore's `mine_experience` × amount,
//!   4 for plain rock), climbing a table or rack (`round(5 / fitness)`), the fishing minigame
//!   ([`fishing_experience`], `crate::fishing::server`).
// shortcut: skills belong to the body, not a mind (they do not follow a mind transfer); no
// `experience_multiplier_reasons` (the mind-expanding drug), skill rewards (the skill tracker
// PDA app sends them), admin skill panel, `TRAIT_EXAMINE_FITNESS`, Journeyman mining scan pulses
// or gaming experience (no arcades); the athletics experience of
// boxing, exercise and fireman carries waits for those mechanics. The level messages use the
// notice span (no `nicegreen` class in the chat protocol).

use bevy::prelude::*;
use ss13_shared::chat::Span;

use crate::server::PlayerMob;

/// `SKILL_LEVEL_NONE` .. `SKILL_LEVEL_LEGENDARY`.
pub const LEVEL_NONE: u8 = 1;
pub const LEVEL_NOVICE: u8 = 2;
pub const LEVEL_APPRENTICE: u8 = 3;
pub const LEVEL_JOURNEYMAN: u8 = 4;
pub const LEVEL_MASTER: u8 = 6;
pub const LEVEL_LEGENDARY: u8 = 7;
/// `SKILL_EXP_LIST`: the experience each level starts at.
pub const EXP_LIST: [f32; 7] = [0.0, 100.0, 250.0, 500.0, 900.0, 1500.0, 2500.0];
/// `SSskills.level_names`.
pub const LEVEL_NAMES: [&str; 7] = [
    "None",
    "Novice",
    "Apprentice",
    "Journeyman",
    "Expert",
    "Master",
    "Legendary",
];
/// `CLEAN_SKILL_BEAUTY_ADJUSTMENT`, `CLEAN_SKILL_GENERIC_WASH_XP`.
const CLEAN_BEAUTY_ADJUSTMENT: f32 = -15.0;
const CLEAN_GENERIC_WASH_XP: f32 = 1.5;
/// `ATHLETICS_SKILL_MISC_EXP`.
pub const ATHLETICS_MISC_EXP: f32 = 5.0;

/// The ported tg skill types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Skill {
    Cleaning,
    Mining,
    Athletics,
    Gaming,
    Fishing,
}

/// `SKILL_*_MODIFIER`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Speed,
    Probs,
    Rands,
    Value,
}

/// The static part of a skill (tg vars); a modifier list per level, `None` when the skill has
/// none of that kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillDef {
    pub skill: Skill,
    pub path: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub speed: Option<[f32; 7]>,
    pub probs: Option<[f32; 7]>,
    pub rands: Option<[f32; 7]>,
    /// `SKILL_VALUE_MODIFIER` (athletics: the `exercised` duration cap, seconds).
    pub value: Option<[f32; 7]>,
}

pub const SKILLS: [SkillDef; 5] = [
    SkillDef {
        skill: Skill::Cleaning,
        path: "/datum/skill/cleaning",
        name: "Cleaning",
        title: "Cleaner",
        desc: "It’s not who I am underneath, but what I mop up that defines me.",
        speed: Some([1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.36]),
        probs: None,
        rands: None,
        value: None,
    },
    SkillDef {
        skill: Skill::Mining,
        path: "/datum/skill/mining",
        name: "Mining",
        title: "Miner",
        desc: "A dwarf's biggest skill, after drinking.",
        speed: Some([1.0, 0.95, 0.9, 0.85, 0.75, 0.6, 0.5]),
        probs: Some([10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0]),
        rands: None,
        value: None,
    },
    SkillDef {
        skill: Skill::Athletics,
        path: "/datum/skill/athletics",
        name: "Athletics",
        title: "Athlete",
        desc: "Twinkle twinkle little star, hit the gym and lift the bar.",
        speed: None,
        probs: None,
        rands: Some([0.0, 5.0, 10.0, 15.0, 20.0, 30.0, 50.0]),
        value: Some([60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 300.0]),
    },
    SkillDef {
        skill: Skill::Gaming,
        path: "/datum/skill/gaming",
        name: "Gaming",
        title: "Gamer",
        desc: "My proficiency as a gamer. This helps me beat bosses with ease, powergame in \
               Orion Trail, and makes me wanna slam some gamer fuel.",
        speed: None,
        probs: Some([0.0, 5.0, 10.0, 15.0, 15.0, 20.0, 25.0]),
        rands: Some([0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 7.0]),
        value: None,
    },
    SkillDef {
        skill: Skill::Fishing,
        path: "/datum/skill/fishing",
        name: "Fishing",
        title: "Angler",
        desc: "How empty and alone you are on this barren Earth.",
        speed: None,
        probs: None,
        rands: None,
        // Added to the minigame's difficulty (negative helps).
        value: Some([1.0, 0.0, -1.0, -3.0, -5.0, -7.0, -10.0]),
    },
];

impl Skill {
    pub const ALL: [Self; 5] = [
        Self::Cleaning,
        Self::Mining,
        Self::Athletics,
        Self::Gaming,
        Self::Fishing,
    ];

    fn index(self) -> usize {
        self as usize
    }

    pub fn def(self) -> &'static SkillDef {
        &SKILLS[self.index()]
    }

    /// `get_skill_modifier(modifier, level)`; a skill without that list gives 1 (speed) or 0.
    pub fn modifier(self, modifier: Modifier, level: u8) -> f32 {
        let def = self.def();
        let list = match modifier {
            Modifier::Speed => def.speed,
            Modifier::Probs => def.probs,
            Modifier::Rands => def.rands,
            Modifier::Value => def.value,
        };
        let i = usize::from(level.clamp(LEVEL_NONE, LEVEL_LEGENDARY) - 1);
        list.map_or(
            if modifier == Modifier::Speed {
                1.0
            } else {
                0.0
            },
            |l| l[i],
        )
    }

    /// `levelUpMessages[level]`.
    pub fn level_up_message(self, level: u8) -> String {
        let (name, title) = (self.def().name, self.def().title);
        match (self, level) {
            (Self::Athletics, LEVEL_NOVICE) => format!(
                "I am just getting started on my {name} journey! I think I should be able to \
                 identify other people who are working to improve their body by sight."
            ),
            (Self::Gaming, 1) => {
                "I'm starting to get a hang of the controls of these games...".to_owned()
            }
            (Self::Gaming, 4) => "I'm starting to pick up the meta of these arcade games. If I \
                                  were to minmax the optimal strat and accentuate my playstyle \
                                  around well-refined tech..."
                .to_owned(),
            (Self::Gaming, 6) => format!(
                "Through incredible determination and effort, I've reached the peak of my {name} \
                 abilities. I wonder how I can become any more powerful... Maybe gamer fuel \
                 would actually help me play better..?"
            ),
            (Self::Fishing, LEVEL_NOVICE) => format!(
                "I'm starting to figure out what {name} really is! I can guess a fish size and \
                 weight at a glance."
            ),
            (Self::Fishing, 3) => format!(
                "I'm getting a little better at {name}! I can tell if a fish is hungry, dying and \
                 otherwise."
            ),
            (Self::Fishing, LEVEL_JOURNEYMAN) => format!(
                "I feel like I've become quite proficient at {name}! I can tell what fishes I can \
                 catch at any given fishing spot."
            ),
            (Self::Fishing, LEVEL_MASTER) => format!(
                "I've begun to truly understand the surprising depth behind {name}. As a master \
                 {title}, I can guess what I'm going to catch now!"
            ),
            (_, 2) => format!("I'm starting to figure out what {name} really is!"),
            (_, 3) => format!("I'm getting a little better at {name}!"),
            (_, 4) => format!("I'm getting much better at {name}!"),
            (_, 5) => format!("I feel like I've become quite proficient at {name}!"),
            (_, 6) => format!(
                "After lots of practice, I've begun to truly understand the intricacies and \
                 surprising depth behind {name}. I now consider myself a master {title}."
            ),
            (_, 7) => format!(
                "Through incredible determination and effort, I've reached the peak of my {name} \
                 abilities. I'm finally able to consider myself a legendary {title}!"
            ),
            _ => format!("What the hell is {name}? Tell an admin if you see this message."),
        }
    }

    /// `levelDownMessages[level]` (indexed by the old level).
    pub fn level_down_message(self, level: u8) -> String {
        let name = self.def().name;
        match level {
            2 => format!(
                "I'm starting to forget what {name} really even is. I need more practice..."
            ),
            3 => format!(
                "I'm getting a little worse at {name}. I'll need to keep practicing to get better \
                 at it..."
            ),
            4 => format!("I'm getting a little worse at {name}..."),
            5 => format!("I'm losing my {name} expertise ...."),
            6 => format!("I feel like I'm losing my mastery of {name}."),
            7 => format!(
                "I feel as though my legendary {name} skills have deteriorated. I'll need more \
                 intense training to recover my lost skills."
            ),
            _ => format!(
                "I have somehow completely lost all understanding of {name}. Please tell an admin \
                 if you see this."
            ),
        }
    }
}

/// `update_skill_level`: the level `exp` reaches.
pub fn level_for(exp: f32) -> u8 {
    EXP_LIST
        .iter()
        .filter(|&&threshold| exp >= threshold)
        .count() as u8
}

/// The mind's `known_skills`: experience and level per skill.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Skills {
    exp: [f32; Skill::ALL.len()],
    level: [u8; Skill::ALL.len()],
}

impl Default for Skills {
    /// `init_known_skills`.
    fn default() -> Self {
        Self {
            exp: [0.0; Skill::ALL.len()],
            level: [LEVEL_NONE; Skill::ALL.len()],
        }
    }
}

impl Skills {
    pub fn level(&self, skill: Skill) -> u8 {
        self.level[skill.index()]
    }

    pub fn exp(&self, skill: Skill) -> f32 {
        self.exp[skill.index()]
    }

    pub fn modifier(&self, skill: Skill, modifier: Modifier) -> f32 {
        skill.modifier(modifier, self.level(skill))
    }

    /// `exp_needed_to_level_up`: 0 at the top level.
    pub fn exp_needed_to_level_up(&self, skill: Skill) -> f32 {
        let level = usize::from(self.level(skill));
        EXP_LIST
            .get(level)
            .map_or(0.0, |next| next - self.exp(skill))
    }

    /// `adjust_experience` without the messages: `(old level, new level)`.
    pub fn adjust(&mut self, skill: Skill, amount: f32) -> (u8, u8) {
        let i = skill.index();
        let old = self.level[i];
        self.exp[i] = (self.exp[i] + amount).max(0.0);
        self.level[i] = level_for(self.exp[i]);
        (old, self.level[i])
    }

    /// `set_level` without the messages: the experience moves by the thresholds' difference.
    pub fn set_level(&mut self, skill: Skill, level: u8) -> (u8, u8) {
        let level = level.clamp(LEVEL_NONE, LEVEL_LEGENDARY);
        let old = self.level(skill);
        let difference = EXP_LIST[usize::from(level) - 1] - EXP_LIST[usize::from(old) - 1];
        self.adjust(skill, difference)
    }
}

/// `get_skill_level` of `mob` (`SKILL_LEVEL_NONE` without skills).
pub fn level(world: &World, mob: Entity, skill: Skill) -> u8 {
    world
        .get::<Skills>(mob)
        .map_or(LEVEL_NONE, |s| s.level(skill))
}

/// `get_skill_modifier` of `mob` (the `SKILL_LEVEL_NONE` modifier without skills).
pub fn modifier(world: &World, mob: Entity, skill: Skill, modifier: Modifier) -> f32 {
    skill.modifier(modifier, level(world, mob, skill))
}

/// tg `adjust_experience(skill, amount, silent)`, then `level_gained`/`level_lost`'s message.
pub fn adjust_experience(world: &mut World, mob: Entity, skill: Skill, amount: f32, silent: bool) {
    if amount == 0.0 || world.get_entity(mob).is_err() {
        return;
    }
    let (old, new) = match world.get_mut::<Skills>(mob) {
        Some(mut skills) => skills.adjust(skill, amount),
        None => {
            let mut skills = Skills::default();
            let change = skills.adjust(skill, amount);
            world.entity_mut(mob).insert(skills);
            change
        }
    };
    announce(world, mob, skill, old, new, silent);
}

/// tg `set_level(skill, level, silent)`.
pub fn set_level(world: &mut World, mob: Entity, skill: Skill, new_level: u8, silent: bool) {
    if world.get_entity(mob).is_err() {
        return;
    }
    let mut skills = world.get::<Skills>(mob).cloned().unwrap_or_default();
    let (old, new) = skills.set_level(skill, new_level);
    world.entity_mut(mob).insert(skills);
    announce(world, mob, skill, old, new, silent);
}

fn announce(world: &mut World, mob: Entity, skill: Skill, old: u8, new: u8, silent: bool) {
    if silent || old == new {
        return;
    }
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    let text = if new > old {
        skill.level_up_message(new)
    } else {
        skill.level_down_message(old)
    };
    crate::tools::tell(world, client, Span::Notice, text);
}

/// `/obj/effect/decal/cleanable::beauty` of the cleanable types that set it (longest path
/// prefix wins; 0 for the others).
pub fn cleanable_beauty(path: &str) -> f32 {
    const BEAUTY: [(&str, f32); 21] = [
        ("/obj/effect/decal/cleanable/ants", -150.0),
        ("/obj/effect/decal/cleanable/blood", -100.0),
        ("/obj/effect/decal/cleanable/blood/tracks", -50.0),
        ("/obj/effect/decal/cleanable/blood/trail_holder", -50.0),
        ("/obj/effect/decal/cleanable/blood/trail", -50.0),
        ("/obj/effect/decal/cleanable/food", -100.0),
        ("/obj/effect/decal/cleanable/fuel_pool", -50.0),
        ("/obj/effect/decal/cleanable/generic", -50.0),
        ("/obj/effect/decal/cleanable/ash", -50.0),
        ("/obj/effect/decal/cleanable/ash/large", -100.0),
        ("/obj/effect/decal/cleanable/glass", -100.0),
        ("/obj/effect/decal/cleanable/dirt", -75.0),
        ("/obj/effect/decal/cleanable/greenglow", -300.0),
        ("/obj/effect/decal/cleanable/cobweb", -100.0),
        ("/obj/effect/decal/cleanable/molten_object", -150.0),
        ("/obj/effect/decal/cleanable/molten_object/large", -300.0),
        ("/obj/effect/decal/cleanable/vomit", -150.0),
        ("/obj/effect/decal/cleanable/vomit/nebula", 10.0),
        ("/obj/effect/decal/cleanable/garbage", -150.0),
        ("/obj/effect/decal/cleanable/rubble", -10.0),
        ("/obj/effect/decal/cleanable/concrete_dust", -20.0),
    ];
    BEAUTY
        .iter()
        .filter(|(p, _)| ss13_mapload::fidelity::is_under(path, p))
        .max_by_key(|(p, _)| p.len())
        .map_or(0.0, |&(_, beauty)| beauty)
}

/// The cleaner's experience for a clean that washed: `round(beauty / -15)` per cleanable
/// `decals` held (tg counts those before the wash), and `round(1.5)` when the wash did
/// something. DM's one-argument `round` floors.
pub fn cleaning_experience(decal_paths: &[String], washed: bool) -> f32 {
    let decals: f32 = decal_paths
        .iter()
        .map(|p| (cleanable_beauty(p) / CLEAN_BEAUTY_ADJUSTMENT).floor())
        .sum();
    decals
        + if washed {
            CLEAN_GENERIC_WASH_XP.floor()
        } else {
            0.0
        }
}

/// `/obj/item/stack/ore::mine_experience` of an ore type.
pub fn mine_experience(ore: &str) -> f32 {
    const ORES: [(&str, f32); 12] = [
        ("/obj/item/stack/ore/uranium", 6.0),
        ("/obj/item/stack/ore/iron", 1.0),
        ("/obj/item/stack/ore/glass", 0.0),
        ("/obj/item/stack/ore/plasma", 5.0),
        ("/obj/item/stack/ore/silver", 3.0),
        ("/obj/item/stack/ore/gold", 5.0),
        ("/obj/item/stack/ore/diamond", 10.0),
        ("/obj/item/stack/ore/bananium", 15.0),
        ("/obj/item/stack/ore/titanium", 3.0),
        ("/obj/item/stack/ore/glass/basalt", 0.0),
        ("/obj/item/stack/ore/glass/siderite", 0.0),
        ("/obj/item/stack/ore/glass/concrete_powder", 0.0),
    ];
    ORES.iter()
        .filter(|(p, _)| ss13_mapload::fidelity::is_under(ore, p))
        .max_by_key(|(p, _)| p.len())
        .map_or(5.0, |&(_, xp)| xp)
}

/// `gets_drilled(user, exp_multiplier)`'s experience for a human `user` mining `wall`:
/// `mine_experience × mineral_amt`, or 4 for a wall without ore, times `exp_multiplier`. Call
/// before the wall is drilled away (gibtonite gives none).
pub fn mining_experience(world: &mut World, user: Entity, wall: Entity, exp_multiplier: f32) {
    if exp_multiplier == 0.0 || world.get::<crate::human::HumanLook>(user).is_none() {
        return;
    }
    // The gibtonite wall's own `gets_drilled` gives none.
    let Some(mineral) = world
        .get::<ss13_mapload::DmMineral>(wall)
        .filter(|m| !m.gibtonite)
    else {
        return;
    };
    let xp = if !mineral.mineral_type.is_empty() && mineral.mineral_amt > 0 {
        mine_experience(&mineral.mineral_type) * mineral.mineral_amt as f32
    } else {
        4.0
    };
    adjust_experience(world, user, Skill::Mining, xp * exp_multiplier, false);
}

/// `FISHING_SKILL_EXP_PER_SECOND`: `SKILL_EXP_LEGENDARY` over 15 minutes of minigames.
const FISHING_EXP_PER_SECOND: f32 = 2500.0 / 900.0;
/// `FISHING_SKILL_DIFFIULTY_EXP_MULT`.
const FISHING_DIFFICULTY_EXP_MULT: f32 = 0.015;
/// `EXPERIENCE_MALUS_MULT` of `/datum/fishing_challenge/complete`.
const FISHING_EXP_MALUS_MULT: f32 = 0.08;

/// The mind traits `/datum/skill/fishing/level_gained` grants, as the level they come at:
/// `TRAIT_EXAMINE_FISH` (size and weight), `TRAIT_EXAMINE_DEEPER_FISH` (health warnings),
/// `TRAIT_EXAMINE_FISHING_SPOT` (a spot's fish) and `TRAIT_REVEAL_FISH` (what bites).
pub const EXAMINE_FISH: u8 = LEVEL_NOVICE;
pub const EXAMINE_DEEPER_FISH: u8 = LEVEL_APPRENTICE;
pub const EXAMINE_FISHING_SPOT: u8 = LEVEL_JOURNEYMAN;
pub const REVEAL_FISH: u8 = LEVEL_MASTER;

/// `HAS_MIND_TRAIT(mob, <fishing trait>)` for one of the levels above: the skill, or the active
/// angler skillchip (`crate::skillchips`), which grants all four.
// shortcut: no fish-infused organs.
pub fn fishing_insight(world: &World, mob: Entity, trait_level: u8) -> bool {
    let chip_trait = match trait_level {
        EXAMINE_FISH => Some(crate::skillchips::EXAMINE_FISH),
        EXAMINE_DEEPER_FISH => Some(crate::skillchips::EXAMINE_DEEPER_FISH),
        EXAMINE_FISHING_SPOT => Some(crate::skillchips::EXAMINE_FISHING_SPOT),
        REVEAL_FISH => Some(crate::skillchips::REVEAL_FISH),
        _ => None,
    };
    level(world, mob, Skill::Fishing) >= trait_level
        || chip_trait.is_some_and(|t| crate::skillchips::has_trait(world, mob, t))
}

/// The experience of a minigame (`/datum/fishing_challenge/complete`): `seconds` spent in it at
/// `difficulty` by a fisher at `level`; `mult` is the challenge's (halved when interrupted or
/// stopped). A fisher far above the difficulty learns less.
pub fn fishing_experience(seconds: f32, difficulty: f32, level: u8, mult: f32) -> f32 {
    let mut mult = mult * (1.0 + difficulty * FISHING_DIFFICULTY_EXP_MULT);
    let malus = f32::from(level) - difficulty * 0.1;
    if malus > 0.0 {
        mult /= 1.0 + malus * FISHING_EXP_MALUS_MULT;
    }
    (seconds * FISHING_EXP_PER_SECOND * mult).round().max(0.0)
}

/// `/datum/element/climbable/climb_structure`'s fitness: the athletics level above `None`.
pub fn fitness_level(world: &World, mob: Entity) -> f32 {
    f32::from(level(world, mob, Skill::Athletics) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_experience_thresholds() {
        assert_eq!(level_for(0.0), LEVEL_NONE);
        assert_eq!(level_for(99.9), LEVEL_NONE);
        assert_eq!(level_for(100.0), LEVEL_NOVICE);
        assert_eq!(level_for(2499.0), 6);
        assert_eq!(level_for(1e9), LEVEL_LEGENDARY);
        let mut skills = Skills::default();
        assert_eq!(skills.exp_needed_to_level_up(Skill::Cleaning), 100.0);
        assert_eq!(skills.adjust(Skill::Cleaning, 260.0), (1, 3));
        assert_eq!(skills.exp_needed_to_level_up(Skill::Cleaning), 240.0);
        // Never below 0; the level falls with it.
        assert_eq!(skills.adjust(Skill::Cleaning, -1000.0), (3, 1));
        assert_eq!(skills.exp(Skill::Cleaning), 0.0);
        // `set_level` moves by the thresholds' difference.
        assert_eq!(skills.set_level(Skill::Mining, LEVEL_LEGENDARY), (1, 7));
        assert_eq!(skills.exp_needed_to_level_up(Skill::Mining), 0.0);
        assert_eq!(skills.level(Skill::Athletics), LEVEL_NONE);
    }

    #[test]
    fn modifiers_by_level() {
        assert_eq!(Skill::Cleaning.modifier(Modifier::Speed, LEVEL_NONE), 1.0);
        assert_eq!(
            Skill::Cleaning.modifier(Modifier::Speed, LEVEL_LEGENDARY),
            0.36
        );
        assert_eq!(
            Skill::Mining.modifier(Modifier::Probs, LEVEL_JOURNEYMAN),
            25.0
        );
        assert_eq!(
            Skill::Athletics.modifier(Modifier::Value, LEVEL_LEGENDARY),
            300.0
        );
        // No such list: speed 1, the rest 0.
        assert_eq!(Skill::Gaming.modifier(Modifier::Speed, 5), 1.0);
        assert_eq!(Skill::Cleaning.modifier(Modifier::Rands, 5), 0.0);
        let mut skills = Skills::default();
        skills.adjust(Skill::Mining, 900.0);
        assert_eq!(skills.modifier(Skill::Mining, Modifier::Speed), 0.75);
    }

    #[test]
    fn cleaning_and_mining_experience() {
        assert_eq!(
            cleanable_beauty("/obj/effect/decal/cleanable/blood/drip"),
            -100.0
        );
        assert_eq!(
            cleanable_beauty("/obj/effect/decal/cleanable/blood/tracks"),
            -50.0
        );
        assert_eq!(cleanable_beauty("/obj/effect/decal/cleanable/crayon"), 0.0);
        // Blood floor(100/15) = 6, dirt 5, the wash floor(1.5) = 1.
        let decals = [
            "/obj/effect/decal/cleanable/blood".to_owned(),
            "/obj/effect/decal/cleanable/dirt".to_owned(),
        ];
        assert_eq!(cleaning_experience(&decals, true), 12.0);
        assert_eq!(cleaning_experience(&[], false), 0.0);
        assert_eq!(mine_experience("/obj/item/stack/ore/diamond"), 10.0);
        assert_eq!(mine_experience("/obj/item/stack/ore/glass/basalt"), 0.0);
        assert_eq!(
            mine_experience("/obj/item/stack/ore/bluespace_crystal"),
            5.0
        );
    }

    #[test]
    fn fishing_experience_and_insight() {
        // 60 s at difficulty 20, level None: 60 * 2500/900 * 1.3 (no malus: 1 - 2 < 0).
        assert_eq!(fishing_experience(60.0, 20.0, LEVEL_NONE, 1.0), 217.0);
        // Interrupted: half.
        assert_eq!(fishing_experience(60.0, 20.0, LEVEL_NONE, 0.5), 108.0);
        // A Legendary fisher on an easy fish: 7 - 1 = 6 over, / 1.48.
        assert_eq!(fishing_experience(60.0, 10.0, LEVEL_LEGENDARY, 1.0), 130.0);
        assert_eq!(
            Skill::Fishing.modifier(Modifier::Value, LEVEL_LEGENDARY),
            -10.0
        );
        assert!(
            Skill::Fishing
                .level_up_message(LEVEL_MASTER)
                .contains("master Angler")
        );
    }

    #[test]
    fn level_messages() {
        assert!(
            Skill::Cleaning
                .level_up_message(LEVEL_LEGENDARY)
                .contains("legendary Cleaner")
        );
        assert!(
            Skill::Athletics
                .level_up_message(LEVEL_NOVICE)
                .contains("Athletics journey")
        );
        assert!(
            Skill::Mining
                .level_down_message(4)
                .contains("worse at Mining")
        );
    }
}
