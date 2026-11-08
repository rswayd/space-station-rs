//! The sparring god (server), after tg `/datum/religion_sect/spar`,
//! `code/modules/religion/sparring` (`sparring_contract.dm`, `sparring_datum.dm`,
//! `ceremonial_gear.dm`), the sparring rites of `rites.dm` and `/datum/component/unbreakable`.
//!
//! - "Summon Sparring Contract" turns blank paper on the altar into the sect's one contract
//!   (a new one dusts the old). The contract (used in hand; list prompts instead of tgui's
//!   `SparringContract`) holds the terms: weapons, arena (the sect's arenas) and stakes; two
//!   humans sign it, a change of terms clears the signatures. "FIGHT!" needs both signed, both
//!   in the arena, a holy role on one side for a holy match, and starts the match.
//! - The match ([`SparMatch`] on the chaplain's side, [`Sparring`] on both): a fighter who drops
//!   (soft crit, unconscious, hard crit) loses; leaving the arena is a violation, the second one
//!   disqualifies; a death or a vanished fighter flubs the match (no winner). The stakes: a holy
//!   match won by the chaplain is 1 favor and the loser can't be challenged to one again; lost by
//!   the chaplain it counts, the third loss smites and excommunicates them; a money match moves
//!   all the loser's money to the winner.
//! - "Forge Ceremonial Gear" turns five sheets of a rigid material into a ceremonial blade;
//!   "Declare Arena" adds an area to the contract's choices; "Become Unbreakable" makes the
//!   priest heal 15 brute and burn and get ephedrine and epinephrine on falling into crit, once a
//!   minute.
// shortcut: no attack, gun or grenade hooks yet: weapon violations, outsiders' interference
// (omen, lightning, brand) and the ceremonial blade's tenfold force against sparrers are not
// ported; leaving the arena does not throw the fighter back; a soul match only says so (no
// soulstone); no outline filters; rigid sheets are a path list (no material datums), the blade
// takes no material colour. tg awards the holy match's favor even when "not entertained" by a
// violation victory and lets a past loser sign a holy match after the warning: here neither.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::PromptAnswer;
use ss13_world::SpatialIndex;

use super::religion::{
    Excommunicated, HolyRole, PerformingRite, Religion, ReligionAsk, Rite, ask_list, glob_deity,
    holy_role, is_human, religion, religion_mut, rename, tell_mob,
};
use super::server::{item_path, sound_at};
use crate::chem::name_of;
use crate::health::{Health, Stat};

const CONTRACT: &str = "/obj/item/sparring_contract";
const BLADE: &str = "/obj/item/ceremonial_blade";
const PAPER: &str = "/obj/item/paper";
const CHAPEL: &str = "/area/station/service/chapel";
const RECREATION: &str = "/area/station/commons/fitness/recreation";
const PRAY: &str = "sound/effects/pray.ogg";
const LIGHTNING: &str = "sound/effects/magic/lightningbolt.ogg";
/// `LIGHTNING_BOLT_DAMAGE`.
const LIGHTNING_BOLT_DAMAGE: f32 = 75.0;
/// `/datum/component/unbreakable` `surge_cooldown`.
const SURGE_COOLDOWN_SECS: f64 = 60.0;
const EPHEDRINE: &str = "/datum/reagent/medicine/ephedrine";
const EPINEPHRINE: &str = "/datum/reagent/medicine/epinephrine";
/// The sheets of `MATERIAL_CLASS_RIGID` materials.
const RIGID_SHEETS: [&str; 22] = [
    "/obj/item/stack/sheet/iron",
    "/obj/item/stack/sheet/glass",
    "/obj/item/stack/sheet/plasteel",
    "/obj/item/stack/sheet/plasmaglass",
    "/obj/item/stack/sheet/titaniumglass",
    "/obj/item/stack/sheet/plastitaniumglass",
    "/obj/item/stack/sheet/mineral/silver",
    "/obj/item/stack/sheet/mineral/gold",
    "/obj/item/stack/sheet/mineral/diamond",
    "/obj/item/stack/sheet/mineral/uranium",
    "/obj/item/stack/sheet/mineral/plasma",
    "/obj/item/stack/sheet/mineral/titanium",
    "/obj/item/stack/sheet/mineral/plastitanium",
    "/obj/item/stack/sheet/mineral/bananium",
    "/obj/item/stack/sheet/mineral/wood",
    "/obj/item/stack/sheet/mineral/bamboo",
    "/obj/item/stack/sheet/mineral/adamantine",
    "/obj/item/stack/sheet/mineral/abductor",
    "/obj/item/stack/sheet/bluespace_crystal",
    "/obj/item/stack/sheet/plastic",
    "/obj/item/stack/sheet/bronze",
    "/obj/item/stack/sheet/bone",
];

const BANISHED: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "banished",
    description: "Меня отлучили от церкви!",
    mood_change: -10.0,
    timeout: 600.0,
    hidden: false,
};

/// `CONDITION_*`: the weapons of a match.
pub const WEAPONS: [&str; 4] = [
    "Fist Fight",
    "Ceremonial Weapons",
    "Melee Only",
    "Any Weapons",
];
const WEAPON_BLURBS: [&str; 4] = [
    "You will fight with your fists only. Any weapons will be considered a violation.",
    "You can only fight with ceremonial weapons. You will be at a severe disadvantage without one!",
    "You can fight with weapons, or fists if you have none. Ranged weapons are a violation.",
    "You can fight with any and all weapons as you please. Try not to kill them, okay?",
];
/// `STAKES_*`.
pub const STAKES: [&str; 4] = ["No Stakes", "Holy Match", "Money Match", "Your Soul"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stakes {
    None,
    HolyMatch,
    MoneyMatch,
    YourSoul,
}

impl Stakes {
    const ALL: [Stakes; 4] = [
        Stakes::None,
        Stakes::HolyMatch,
        Stakes::MoneyMatch,
        Stakes::YourSoul,
    ];

    fn name(self) -> &'static str {
        STAKES[self as usize]
    }
}

/// The sparring god's state (tg `/datum/religion_sect/spar` vars).
#[derive(Clone, Debug)]
pub struct SparState {
    /// `existing_contract`.
    pub contract: Option<Entity>,
    /// `arenas`: name and area type.
    pub arenas: Vec<(String, String)>,
    pub matches_lost: u32,
    pub past_opponents: Vec<Entity>,
}

impl Default for SparState {
    fn default() -> Self {
        Self {
            contract: None,
            arenas: vec![
                ("Recreation Area".to_owned(), RECREATION.to_owned()),
                ("Chapel".to_owned(), CHAPEL.to_owned()),
            ],
            matches_lost: 0,
            past_opponents: Vec::new(),
        }
    }
}

/// `/obj/item/sparring_contract`: the terms and `signed_by`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SparringContract {
    /// Index into [`WEAPONS`] (`CONDITION_* - 1`).
    pub weapons: usize,
    pub arena: String,
    pub stakes: Stakes,
    pub signed: [Option<Entity>; 2],
}

impl Default for SparringContract {
    fn default() -> Self {
        Self {
            weapons: 2,
            arena: CHAPEL.to_owned(),
            stakes: Stakes::None,
            signed: [None, None],
        }
    }
}

/// `/datum/sparring_match`, on the chaplain's side.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SparMatch {
    pub opponent: Entity,
    pub weapons: usize,
    pub arena: String,
    pub stakes: Stakes,
    pub violations: [u8; 2],
    /// Where each fighter (chaplain, opponent) stood when last checked.
    pub last_pos: [Option<TilePos>; 2],
}

/// `TRAIT_SPARRING`: in the match of `chaplain`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sparring {
    pub chaplain: Entity,
}

/// `/datum/component/unbreakable`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Unbreakable {
    pub ready_at: f64,
    pub last_stat: Stat,
}

/// The contract's menus ([`ReligionAsk::Spar`]).
#[derive(Clone, Debug)]
pub enum SparAsk {
    Menu {
        contract: Entity,
    },
    Weapons {
        contract: Entity,
    },
    Arena {
        contract: Entity,
        arenas: Vec<String>,
    },
    Stakes {
        contract: Entity,
    },
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The contract's menu (`ui_interact`), before the generic service items.
    app.add_click_handler(ClickPhase::AttackSelf, 2999, |w, c| {
        open_contract(w, c.client, c.mob, c.target)
    });
    app.add_systems(
        Update,
        (
            referee.run_if(any_with_component::<SparMatch>),
            unbreakable.run_if(any_with_component::<Unbreakable>),
            forget_contract,
        ),
    );
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

/// `tool_examine`.
pub(super) fn tool_examine(station: &Religion, deity: &str) -> String {
    format!(
        "You have {} sparring matches won in {deity}'s name to redeem. You have lost {} holy \
         matches. You will be excommunicated after losing three matches.",
        station.favor.floor(),
        station.spar.matches_lost
    )
}

fn area_at(world: &World, mob: Entity) -> Option<String> {
    let pos = crate::chem::map_pos(world, mob)?;
    crate::events::area_type_at(world, pos).map(str::to_owned)
}

fn arena_name(station: &Religion, path: &str) -> String {
    station
        .spar
        .arenas
        .iter()
        .find(|(_, p)| p == path)
        .map_or_else(|| path.to_owned(), |(n, _)| n.clone())
}

/// `declare_arena/perform_rite`'s list: every area outside CentCom, one per type, by name.
// shortcut: `HIDDEN_AREA` is not converted: every non-CentCom area is offered.
pub(super) fn declarable_areas(world: &World) -> Vec<Entity> {
    let Some(areas) = world.get_resource::<ss13_mapload::Areas>() else {
        return Vec::new();
    };
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<(String, Entity)> = Vec::new();
    for &area in areas.entities() {
        let Some(path) = world
            .get::<ss13_mapload::DmPower>(area)
            .map(|p| p.path.clone())
        else {
            continue;
        };
        if crate::obj_damage::is_under(&path, "/area/centcom") || seen.contains(&path) {
            continue;
        }
        seen.push(path);
        out.push((crate::shuttle::area_name(world, area), area));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, a)| a).collect()
}

/// The sparring rites' `perform_rite` checks and targets.
pub(super) fn rite_target(
    world: &mut World,
    user: Entity,
    altar: Entity,
    rite: Rite,
) -> Option<Option<Entity>> {
    let on_altar: Vec<Entity> = world
        .get::<TilePos>(altar)
        .map(|&pos| world.resource::<SpatialIndex>().on_tile(pos).to_vec())
        .unwrap_or_default();
    let what = name_of(world, altar);
    match rite {
        Rite::SparringContract => {
            let paper = on_altar.iter().copied().find(|&e| {
                crate::obj_damage::is_under(&item_path(world, e), PAPER)
                    && world
                        .get::<super::Paper>(e)
                        .is_some_and(|p| p.total_length() == 0)
            });
            if paper.is_none() {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("You need to place blank paper on {what} to do this!"),
                );
                return None;
            }
            Some(paper)
        }
        Rite::CeremonialWeapon => {
            let mut not_rigid = None;
            for &e in &on_altar {
                let path = item_path(world, e);
                if !crate::obj_damage::is_under(&path, "/obj/item/stack/sheet") {
                    continue;
                }
                if !is_rigid(&path) {
                    not_rigid = Some(name_of(world, e));
                    continue;
                }
                if crate::tools::stack_amount(world, e) < 5 {
                    continue;
                }
                return Some(Some(e));
            }
            let text = match not_rigid {
                Some(name) => format!("{name} is not suitable for being made into gear!"),
                None => "You need at least five sheets of a rigid material to make gear!".into(),
            };
            tell_mob(world, user, Span::Warning, text);
            None
        }
        Rite::Unbreakable => {
            if !is_human(world, user) {
                return None;
            }
            if world.get::<Unbreakable>(user).is_some() {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "Your spirit is already unbreakable!",
                );
                return None;
            }
            Some(None)
        }
        _ => Some(None),
    }
}

/// `MATERIAL_CLASS_RIGID` of a sheet.
pub fn is_rigid(path: &str) -> bool {
    RIGID_SHEETS
        .iter()
        .any(|root| crate::obj_damage::is_under(path, root))
}

/// `invoke_effect` of the sparring rites.
pub(super) fn invoke_effect(
    world: &mut World,
    user: Entity,
    state: PerformingRite,
    pos: TilePos,
) -> bool {
    let deity = glob_deity(world);
    let left = || "Your target left the altar!";
    match state.rite {
        Rite::SparringContract => {
            let Some(paper) = state
                .target
                .filter(|&p| world.get::<TilePos>(p) == Some(&pos))
            else {
                tell_mob(world, user, Span::Warning, left());
                return false;
            };
            let name = name_of(world, paper);
            super::server::say_around(
                world,
                paper,
                Span::Notice,
                format!("words magically form on {name}!"),
                &[],
            );
            sound_at(world, state.altar, PRAY, 50.0, true);
            if let Some(old) = religion(world)
                .spar
                .contract
                .filter(|&c| world.get_entity(c).is_ok())
            {
                let name = name_of(world, old);
                super::server::say_around(
                    world,
                    old,
                    Span::Warning,
                    format!("{name} fizzles into nothing!"),
                    &[],
                );
                world.despawn(old);
            }
            let contract = crate::construction::spawn_obj(world, CONTRACT, pos);
            if let Some(contract) = contract {
                rename(world, contract, &format!("{deity}'s sparring contract"));
                world
                    .entity_mut(contract)
                    .insert(SparringContract::default());
            }
            religion_mut(world).spar.contract = contract;
            world.despawn(paper);
            true
        }
        Rite::CeremonialWeapon => {
            let Some(sheets) = state.target.filter(|&s| {
                world.get::<TilePos>(s) == Some(&pos) && crate::tools::stack_amount(world, s) >= 5
            }) else {
                tell_mob(world, user, Span::Warning, left());
                return false;
            };
            let name = name_of(world, sheets);
            tell_mob(
                world,
                user,
                Span::Warning,
                format!("{name} reshapes into a ceremonial blade!"),
            );
            if !crate::tools::use_stack(world, sheets, 5) {
                return false;
            }
            crate::construction::spawn_obj(world, BLADE, pos);
            true
        }
        Rite::DeclareArena => {
            let Some(area) = state.target else {
                return false;
            };
            let Some(path) = world
                .get::<ss13_mapload::DmPower>(area)
                .map(|p| p.path.clone())
            else {
                return false;
            };
            let name = crate::shuttle::area_name(world, area);
            {
                let mut station = religion_mut(world);
                station.spar.arenas.retain(|(n, _)| *n != name);
                station.spar.arenas.push((name.clone(), path));
            }
            tell_mob(
                world,
                user,
                Span::Warning,
                format!("{name} is a now an option to select on sparring contracts."),
            );
            true
        }
        Rite::Unbreakable => {
            tell_mob(
                world,
                user,
                Span::Notice,
                format!("You feel {deity}'s will to keep fighting pouring into you!"),
            );
            let last_stat = world.get::<Stat>(user).copied().unwrap_or_default();
            world.entity_mut(user).insert(Unbreakable {
                ready_at: 0.0,
                last_stat,
            });
            true
        }
        _ => false,
    }
}

/// The contract is gone: the sect may summon another (`sparring_contract/Destroy`).
fn forget_contract(world: &mut World) {
    let Some(contract) = world
        .get_resource::<Religion>()
        .and_then(|r| r.spar.contract)
    else {
        return;
    };
    if world.get_entity(contract).is_err() {
        religion_mut(world).spar.contract = None;
    }
}

// ---------------------------------------------------------------------------------------------
// The contract

fn contract_of(world: &World, e: Entity) -> Option<SparringContract> {
    world.get::<SparringContract>(e).cloned()
}

/// The contract in hand or next to `mob`.
fn contract_in_reach(world: &World, mob: Entity, contract: Entity) -> bool {
    world.get::<SparringContract>(contract).is_some()
        && (super::religion::holding(world, mob, contract)
            || crate::storage::in_reach(world, mob, contract))
}

/// `ui_interact` as a menu: the terms, the signatures and the buttons.
fn open_contract(world: &mut World, client: Entity, mob: Entity, contract: Entity) -> bool {
    let Some(terms) = contract_of(world, contract) else {
        return false;
    };
    let station = religion(world);
    let signer = |e: Option<Entity>| e.map_or_else(|| "none".to_owned(), |e| name_of(world, e));
    let items = vec![
        format!("Sign left (now: {})", signer(terms.signed[0])),
        format!("Sign right (now: {})", signer(terms.signed[1])),
        format!("Weapons: {}", WEAPONS[terms.weapons]),
        format!("Arena: {}", arena_name(&station, &terms.arena)),
        format!("Stakes: {}", terms.stakes.name()),
        "FIGHT!".to_owned(),
        "Clear".to_owned(),
    ];
    let message = format!(
        "{}\nBoth sparring partners must agree with the terms to begin.",
        WEAPON_BLURBS[terms.weapons]
    );
    let title = name_of(world, contract);
    ask_list(
        world,
        client,
        mob,
        &title,
        &message,
        items,
        ReligionAsk::Spar(SparAsk::Menu { contract }),
    );
    true
}

/// A contract menu answered (`ui_act`).
pub(super) fn answer(world: &mut World, actor: Entity, ask: SparAsk, answer: Option<PromptAnswer>) {
    let Some(PromptAnswer::Item(item)) = answer else {
        return;
    };
    let contract = match &ask {
        SparAsk::Menu { contract }
        | SparAsk::Weapons { contract }
        | SparAsk::Arena { contract, .. }
        | SparAsk::Stakes { contract } => *contract,
    };
    if !contract_in_reach(world, actor, contract) {
        return;
    }
    if !is_human(world, actor) {
        tell_mob(
            world,
            actor,
            Span::Warning,
            "This contract refuses to be signed by a lesser creature such as yourself.",
        );
        return;
    }
    let Some(client) = super::server::client_of(world, actor) else {
        return;
    };
    match ask {
        SparAsk::Menu { .. } => match item {
            0 | 1 => sign(world, actor, contract, item),
            2 => ask_list(
                world,
                client,
                actor,
                "Weapons",
                "What weapons will be allowed?",
                WEAPONS.iter().map(|w| (*w).to_owned()).collect(),
                ReligionAsk::Spar(SparAsk::Weapons { contract }),
            ),
            3 => {
                let arenas = religion(world).spar.arenas;
                ask_list(
                    world,
                    client,
                    actor,
                    "Arena",
                    "Where will the match take place?",
                    arenas.iter().map(|(n, _)| n.clone()).collect(),
                    ReligionAsk::Spar(SparAsk::Arena {
                        contract,
                        arenas: arenas.into_iter().map(|(_, p)| p).collect(),
                    }),
                );
            }
            4 => ask_list(
                world,
                client,
                actor,
                "Stakes",
                "What will the stakes be?",
                STAKES.iter().map(|s| (*s).to_owned()).collect(),
                ReligionAsk::Spar(SparAsk::Stakes { contract }),
            ),
            5 => fight(world, actor, contract),
            6 => {
                if let Some(mut terms) = world.get_mut::<SparringContract>(contract) {
                    terms.signed = [None, None];
                }
            }
            _ => {}
        },
        SparAsk::Weapons { .. } => {
            if item < WEAPONS.len() {
                change_terms(world, actor, contract, |t| t.weapons = item);
            }
        }
        SparAsk::Arena { arenas, .. } => {
            if let Some(path) = arenas.get(item).cloned() {
                change_terms(world, actor, contract, |t| t.arena = path);
            }
        }
        SparAsk::Stakes { .. } => {
            if let Some(&stakes) = Stakes::ALL.get(item) {
                change_terms(world, actor, contract, |t| t.stakes = stakes);
            }
        }
    }
}

/// A term changed: a signed contract has to be signed again.
fn change_terms(
    world: &mut World,
    actor: Entity,
    contract: Entity,
    change: impl FnOnce(&mut SparringContract),
) {
    let Some(mut terms) = contract_of(world, contract) else {
        return;
    };
    let before = terms.clone();
    change(&mut terms);
    if terms == before {
        return;
    }
    if terms.signed.iter().any(Option::is_some) {
        terms.signed = [None, None];
        tell_mob(
            world,
            actor,
            Span::Warning,
            "You will need to get your sparring partner to sign again under these new terms \
             you've set.",
        );
    }
    world.entity_mut(contract).insert(terms);
}

/// `ui_act("sign")`.
fn sign(world: &mut World, actor: Entity, contract: Entity, side: usize) {
    let Some(mut terms) = contract_of(world, contract) else {
        return;
    };
    if terms.signed.contains(&Some(actor)) {
        tell_mob(
            world,
            actor,
            Span::Warning,
            "You've already signed one side of the contract.",
        );
        return;
    }
    if terms.stakes == Stakes::HolyMatch && religion(world).spar.past_opponents.contains(&actor) {
        tell_mob(
            world,
            actor,
            Span::Warning,
            "This contract refuses to allow a holy match with a previous holy match loser. Pick \
             a different stake!",
        );
        return;
    }
    terms.signed[side] = Some(actor);
    world.entity_mut(contract).insert(terms);
}

/// Can `terms` start a match now: (chaplain, opponent), or why not.
pub fn ready_to_fight(
    world: &World,
    terms: &SparringContract,
) -> Result<(Entity, Entity), &'static str> {
    let [Some(left), Some(right)] = terms.signed else {
        return Err("Both sparring partners must sign first.");
    };
    if [left, right]
        .iter()
        .any(|&m| world.get_entity(m).is_err() || world.get::<crate::mind::MindData>(m).is_none())
    {
        return Err("Both sparring partners must sign first.");
    }
    if world.get::<Sparring>(left).is_some() || world.get::<Sparring>(right).is_some() {
        return Err("One participant is already sparring!");
    }
    let in_arena = |m: Entity| area_at(world, m).as_deref() == Some(terms.arena.as_str());
    if !in_arena(left) || !in_arena(right) {
        return Err("Both sparring partners must be in the arena.");
    }
    let holy = |m: Entity| holy_role(world, m).is_some();
    if terms.stakes == Stakes::HolyMatch && !holy(left) && !holy(right) {
        return Err("A holy match needs a holy fighter.");
    }
    Ok(if holy(left) {
        (left, right)
    } else {
        (right, left)
    })
}

/// `ui_act("fight")`: `new /datum/sparring_match`, the contract is spent.
fn fight(world: &mut World, actor: Entity, contract: Entity) {
    let Some(terms) = contract_of(world, contract) else {
        return;
    };
    let (chaplain, opponent) = match ready_to_fight(world, &terms) {
        Ok(fighters) => fighters,
        Err(why) => {
            tell_mob(world, actor, Span::Warning, why);
            return;
        }
    };
    let last_pos = [
        crate::chem::map_pos(world, chaplain),
        crate::chem::map_pos(world, opponent),
    ];
    world.entity_mut(chaplain).insert(SparMatch {
        opponent,
        weapons: terms.weapons,
        arena: terms.arena,
        stakes: terms.stakes,
        violations: [2, 2],
        last_pos,
    });
    for mob in [chaplain, opponent] {
        world.entity_mut(mob).insert(Sparring { chaplain });
    }
    world.despawn(contract);
}

// ---------------------------------------------------------------------------------------------
// The match

/// What the referee saw this check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Continue,
    /// (winner index, violation victory); 0 is the chaplain.
    Won(usize, bool),
    Flub,
}

/// `check_for_victory`, `death_flub`: the stats of the fighters (chaplain, opponent).
pub fn call_stats(stats: [Option<Stat>; 2], down: [bool; 2]) -> Call {
    if stats.iter().any(|s| s.is_none() || *s == Some(Stat::Dead)) {
        return Call::Flub;
    }
    match down.iter().position(|&d| d) {
        Some(i) => Call::Won(1 - i, false),
        None => Call::Continue,
    }
}

/// The sparring datum's signals, polled: deaths and vanishings flub, a fighter down loses,
/// leaving the arena is a violation. Cost: one or two matches, a few reads each.
fn referee(world: &mut World) {
    let matches: Vec<(Entity, SparMatch)> = world
        .query::<(Entity, &SparMatch)>()
        .iter(world)
        .map(|(e, m)| (e, m.clone()))
        .collect();
    for (chaplain, mut state) in matches {
        let fighters = [chaplain, state.opponent];
        let alive = |m: Entity| world.get_entity(m).is_ok();
        let stats = fighters.map(|m| alive(m).then(|| world.get::<Stat>(m).copied()).flatten());
        // `new_stat != STABLE`: soft crit, unconscious, hard crit.
        let down = fighters.map(|m| {
            alive(m)
                && (world.get::<Stat>(m).is_some_and(|&s| s >= Stat::SoftCrit)
                    || crate::status_effects::knocked_out(world, m))
        });
        match call_stats(stats, down) {
            Call::Flub => {
                flubbed_match(world, chaplain, &state);
                continue;
            }
            Call::Won(winner, violation) => {
                end_match(world, chaplain, &state, winner, violation);
                continue;
            }
            Call::Continue => {}
        }
        // `arena_violation` on every move out of the arena.
        let mut ended = false;
        for (i, &mob) in fighters.iter().enumerate() {
            let pos = crate::chem::map_pos(world, mob);
            if pos == state.last_pos[i] {
                continue;
            }
            state.last_pos[i] = pos;
            if area_at(world, mob).as_deref() == Some(state.arena.as_str()) {
                continue;
            }
            tell_mob(world, mob, Span::Danger, "Violation! No leaving the arena!");
            state.violations[i] = state.violations[i].saturating_sub(1);
            if state.violations[i] == 0 {
                end_match(world, chaplain, &state, 1 - i, true);
                ended = true;
                break;
            }
        }
        if !ended && let Ok(mut e) = world.get_entity_mut(chaplain) {
            e.insert(state);
        }
    }
}

/// `cleanup_sparring_match`.
fn cleanup(world: &mut World, chaplain: Entity, state: &SparMatch) {
    for mob in [chaplain, state.opponent] {
        if let Ok(mut e) = world.get_entity_mut(mob) {
            e.remove::<Sparring>();
        }
    }
    if let Ok(mut e) = world.get_entity_mut(chaplain) {
        e.remove::<SparMatch>();
    }
}

/// `flubbed_match`: nobody wins or loses anything.
fn flubbed_match(world: &mut World, chaplain: Entity, state: &SparMatch) {
    cleanup(world, chaplain, state);
    if world.get_entity(chaplain).is_ok() {
        tell_mob(
            world,
            chaplain,
            Span::BoldWarning,
            "The match was flub'd! No winners, no losers. You may restart the match with another \
             contract.",
        );
    }
    if world.get_entity(state.opponent).is_ok() {
        tell_mob(
            world,
            state.opponent,
            Span::BoldWarning,
            "The match was flub'd! No winners, no losers.",
        );
    }
}

/// `end_match`, `win` and `lose`.
fn end_match(
    world: &mut World,
    chaplain: Entity,
    state: &SparMatch,
    winner: usize,
    violation: bool,
) {
    cleanup(world, chaplain, state);
    let fighters = [chaplain, state.opponent];
    let (winner, loser) = (fighters[winner], fighters[1 - winner]);
    let (winner_name, loser_name) = (name_of(world, winner), name_of(world, loser));
    let line = if violation {
        format!("{loser_name} DISQUALIFIED!  {winner_name} HAS WON!")
    } else {
        format!("  {winner_name} HAS WON!")
    };
    for mob in fighters {
        tell_mob(world, mob, Span::BoldWarning, line.clone());
    }
    let deity = glob_deity(world);
    match state.stakes {
        Stakes::HolyMatch => {
            if winner == chaplain {
                if violation {
                    tell_mob(
                        world,
                        winner,
                        Span::Warning,
                        format!(
                            "{deity} is not entertained from a matched decided by violations. No \
                             favor awarded..."
                        ),
                    );
                } else {
                    tell_mob(
                        world,
                        winner,
                        Span::Notice,
                        format!("You've won favor with {deity}!"),
                    );
                    religion_mut(world).adjust_favor(1.0);
                }
                religion_mut(world).spar.past_opponents.push(loser);
            } else {
                lose_holy_match(world, loser, &deity);
            }
        }
        Stakes::MoneyMatch => {
            tell_mob(
                world,
                winner,
                Span::Notice,
                format!("You've won all of {loser_name}'s money!"),
            );
            tell_mob(
                world,
                loser,
                Span::Danger,
                format!("You've lost all your money to {winner_name}!"),
            );
            transfer_all_money(world, loser, winner);
        }
        Stakes::YourSoul => {
            tell_mob(
                world,
                winner,
                Span::Notice,
                format!("You've won {loser_name}'s SOUL!"),
            );
            tell_mob(
                world,
                loser,
                Span::Danger,
                format!("You've lost ownership over your soul to {winner_name}!"),
            );
        }
        Stakes::None => {}
    }
    if state.stakes != Stakes::YourSoul {
        let mut text = "You may want to heal up the loser now.".to_owned();
        if winner == chaplain {
            text.push_str(" Your bible will heal the loser for awhile.");
        }
        tell_mob(world, winner, Span::Notice, text);
    }
}

/// `lose` of a holy match by the chaplain: the third loss excommunicates.
fn lose_holy_match(world: &mut World, chaplain: Entity, deity: &str) {
    let lost = {
        let mut station = religion_mut(world);
        station.spar.matches_lost += 1;
        station.spar.matches_lost
    };
    if lost < 3 {
        tell_mob(
            world,
            chaplain,
            Span::Danger,
            format!("{deity} is angry you lost in their name!"),
        );
        return;
    }
    tell_mob(
        world,
        chaplain,
        Span::Danger,
        format!("{deity} is enraged by your lackluster sparring record!"),
    );
    lightningbolt(world, chaplain);
    crate::mood::add_event(world, chaplain, "sparring", &BANISHED);
    world
        .entity_mut(chaplain)
        .remove::<HolyRole>()
        .insert(Excommunicated);
    tell_mob(
        world,
        chaplain,
        Span::Danger,
        "You have been excommunicated! You are no longer holy!",
    );
}

/// `lightningbolt`: 75 burn and the thunder (no beam).
fn lightningbolt(world: &mut World, mob: Entity) {
    if let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) {
        let before = stat;
        crate::health::apply_damage(
            &mut health,
            &mut stat,
            crate::health::DamageType::Burn,
            LIGHTNING_BOLT_DAMAGE,
        );
        world.entity_mut(mob).insert(health);
        if stat != before {
            world.entity_mut(mob).insert(stat);
        }
    }
    sound_at(world, mob, LIGHTNING, 50.0, true);
}

/// `winner_account.transfer_money(loser_account, loser_account.account_balance)`.
fn transfer_all_money(world: &mut World, from: Entity, to: Entity) {
    let (Some(a), Some(b)) = (
        super::religion::account_of_mob(world, from),
        super::religion::account_of_mob(world, to),
    ) else {
        return;
    };
    if a == b {
        return;
    }
    let Some(mut economy) = world.get_resource_mut::<crate::cargo::economy::Economy>() else {
        return;
    };
    let Some(amount) = economy.account(a).map(|acc| acc.balance) else {
        return;
    };
    if amount <= 0 || economy.account(b).is_none() {
        return;
    }
    if let Some(acc) = economy.account_mut(a) {
        acc.adjust_money(-amount);
    }
    if let Some(acc) = economy.account_mut(b) {
        acc.adjust_money(amount);
    }
}

/// `unbreakable/surge` on falling into crit, once a minute.
fn unbreakable(world: &mut World) {
    let now = now(world);
    let mobs: Vec<(Entity, Unbreakable)> = world
        .query::<(Entity, &Unbreakable)>()
        .iter(world)
        .map(|(e, u)| (e, *u))
        .collect();
    for (mob, mut state) in mobs {
        let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
        let changed = stat != state.last_stat;
        state.last_stat = stat;
        let surge = changed && stat >= Stat::SoftCrit && stat < Stat::Dead && now >= state.ready_at;
        if surge {
            state.ready_at = now + SURGE_COOLDOWN_SECS;
        }
        world.entity_mut(mob).insert(state);
        if !surge {
            continue;
        }
        tell_mob(world, mob, Span::BoldNotice, "you refuse to give up!");
        if let Some(mut health) = world.get::<Health>(mob).cloned() {
            health.adjust(crate::health::DamageType::Brute, -15.0);
            health.adjust(crate::health::DamageType::Burn, -15.0);
            let mut stat = stat;
            crate::health::update_stat(&health, &mut stat);
            world.entity_mut(mob).insert((health, stat));
        }
        for reagent in [EPHEDRINE, EPINEPHRINE] {
            if crate::chem::blood_amount(world, mob, reagent) < 20.0 {
                add_blood(world, mob, reagent, 10.0);
            }
        }
    }
}

fn add_blood(world: &mut World, mob: Entity, path: &str, amount: f32) {
    if !world.contains_resource::<crate::chem::Chem>() {
        return;
    }
    world.resource_scope(|world, chem: Mut<crate::chem::Chem>| {
        let Some(id) = chem.0.id(path) else {
            return;
        };
        if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
            reagents
                .blood
                .add_reagent(&chem.0, id, amount, 300.0, None, None, false, false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_referee_calls_downs_and_deaths() {
        let up = Some(Stat::Conscious);
        assert_eq!(call_stats([up, up], [false, false]), Call::Continue);
        assert_eq!(call_stats([up, up], [true, false]), Call::Won(1, false));
        assert_eq!(call_stats([up, up], [false, true]), Call::Won(0, false));
        assert_eq!(
            call_stats([Some(Stat::Dead), up], [true, false]),
            Call::Flub
        );
        assert_eq!(call_stats([up, None], [false, false]), Call::Flub);
    }

    #[test]
    fn rigid_sheets_make_blades() {
        assert!(is_rigid("/obj/item/stack/sheet/iron"));
        assert!(is_rigid("/obj/item/stack/sheet/mineral/wood"));
        assert!(!is_rigid("/obj/item/stack/sheet/cloth"));
        assert!(!is_rigid("/obj/item/stack/sheet/cardboard"));
    }

    #[test]
    fn a_contract_needs_signatures_and_a_holy_fighter_for_a_holy_match() {
        let mut world = World::new();
        let terms = SparringContract::default();
        assert!(ready_to_fight(&world, &terms).is_err());
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let signed = SparringContract {
            signed: [Some(a), Some(b)],
            ..Default::default()
        };
        // No minds yet.
        assert!(ready_to_fight(&world, &signed).is_err());
        assert_eq!(SparState::default().arenas.len(), 2);
    }

    #[test]
    fn losing_three_holy_matches_excommunicates() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(Religion::default());
        let chaplain = world.spawn(HolyRole::HighPriest).id();
        lose_holy_match(&mut world, chaplain, "Bob");
        lose_holy_match(&mut world, chaplain, "Bob");
        assert_eq!(holy_role(&world, chaplain), Some(HolyRole::HighPriest));
        lose_holy_match(&mut world, chaplain, "Bob");
        assert_eq!(holy_role(&world, chaplain), None);
        assert_eq!(religion(&world).spar.matches_lost, 3);
    }
}
