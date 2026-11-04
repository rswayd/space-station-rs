//! The gibber (server): tg `/obj/machinery/gibber` (`food_and_drinks/machinery/gibber.dm`).
//!
//! - An empty-hand click while pulling a human (not buckled, nothing equipped unless a tier 2+
//!   servo lets clothes through) stuffs it in after `gibtime` deciseconds; the pulled mob must
//!   still be pulled, unbuckled and the gibber empty at the end. Without a pulled mob the click
//!   starts the gibber.
//! - Gibbing: `active_power_usage`, the occupant's limbs' butcher drops (`get_butcher_drops`: the
//!   chest 2 human meat and a human skin, the head a skin, every arm and leg one meat) with tg's
//!   carried `drop_chance` over `efficiency`, the victim's reagents split over the results that
//!   hold reagents, plus `nutrition / fat nutriment_factor` fat each; the victim dies and is
//!   gone (its player a ghost). `gibtime` later the results fly out up to 3 tiles away, gibs
//!   land on free neighbouring tiles and the gibber may get bloody (50%).
//! - A right click (tg's "Empty gibber" verb) or a step of the occupant lets it out.
//! - `RefreshParts`: `efficiency = 0.25 + 0.25 × matter bin tiers`, `gibtime = 40 - 5 × servo
//!   tiers`, a servo of tier 2+ ignores clothing.
// shortcut: no sounds, shaking, emissive overlays, blood DNA color (the blood overlay is red),
// meat diseases or `subjectname`, no autogibber, no hologram occupants; non-human mobs cannot
// be put in (tg takes only carbons and monkeys are not ported); the victim's equipped items
// (with a tier 2+ servo) drop on the gibber's tile instead of being deleted with it.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmObj, DmPower, IconIndex};
use ss13_reagents::is_under;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Overlay, TilePos};

use crate::chem::{Chem, MobReagents, Reagents, name_of};
use crate::closets::{ClosetRelayMove, InCloset};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, Health, Stat};
use crate::inventory::Inventory;
use crate::pull::Pulling;
use crate::server::PlayerMob;
use crate::stock_parts::PartKind;
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::{Actor, tell};
use crate::zone::BodyZone;

pub const GIBBER_ROOT: &str = "/obj/machinery/gibber";
const HUMAN_MEAT: &str = "/obj/item/food/meat/slab/human";
const HUMAN_SKIN: &str = "/obj/item/stack/sheet/animalhide/carbon/human";
const FAT: &str = "/datum/reagent/consumable/nutriment/fat";
/// `/obj/effect/gibspawner/human` `gibtypes` (its keys: `pick` on an assoc list).
const HUMAN_GIBS: [&str; 6] = [
    "/obj/effect/decal/cleanable/blood/gibs/up",
    "/obj/effect/decal/cleanable/blood/gibs/down",
    "/obj/effect/decal/cleanable/blood/gibs",
    "/obj/effect/decal/cleanable/blood/gibs/body",
    "/obj/effect/decal/cleanable/blood/gibs/limb",
    "/obj/effect/decal/cleanable/blood/gibs/core",
];
/// `initial(efficiency)`, `initial(gibtime)` (deciseconds).
pub const BASE_EFFICIENCY: f32 = 0.25;
const BASE_GIBTIME: f32 = 40.0;
/// `RANGE_TURFS(3, our_turf)`.
const THROW_RANGE: i32 = 3;
/// `BLOOD_COLOR_RED`.
const BLOOD_RED: [u8; 4] = [0xC8, 0x00, 0x00, 0xFF];

/// A gibber's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Gibber {
    /// `operating`: when the running gib finishes (game seconds).
    pub finishes_at: Option<f64>,
    pub dirty: bool,
}

/// The gibber's `do_after` (stuffing the pulled mob in).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StuffJob {
    pub victim: Entity,
}

pub struct GibberServerPlugin;

impl Plugin for GibberServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_gibbers)
            .add_systems(
                Update,
                (
                    finish_gibbing.run_if(any_with_component::<Gibber>),
                    relay_moves.after(ss13_world::WorldSystems::Movement),
                ),
            )
            .add_observer(release_on_despawn);
    }
}

fn setup_gibbers(world: &mut World) {
    let mut q = world.query::<(Entity, &DmPower)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| is_under(&p.path, GIBBER_ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e, true);
    }
}

/// `Initialize`: 5% of mapped and built gibbers are "meat grinders", dirty from the start.
pub(crate) fn setup(world: &mut World, e: Entity, _mapload: bool) {
    let dirty = world.resource_mut::<CombatRng>().chance(5.0);
    if dirty {
        world
            .entity_mut(e)
            .insert(Name::new("мясорубка".to_owned()));
    }
    world.entity_mut(e).insert(Gibber {
        finishes_at: None,
        dirty,
    });
    refresh_look(world, e);
}

/// tg `gibber/RefreshParts`: (`efficiency`, `gibtime` deciseconds, `ignore_clothing`).
pub fn ratings(world: &World, e: Entity) -> (f32, f32, bool) {
    let r = crate::stock_parts::ratings(world, e);
    let efficiency = BASE_EFFICIENCY + r.sum(PartKind::MatterBin) as f32 * 0.25;
    let gibtime = BASE_GIBTIME - 5.0 * r.sum(PartKind::Servo) as f32;
    let ignore_clothing = r.tiers(PartKind::Servo).any(|t| t >= 2);
    (efficiency, gibtime, ignore_clothing)
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// The mob inside `gibber` (`occupant`).
pub fn occupant(world: &mut World, gibber: Entity) -> Option<Entity> {
    let mut q = world.query::<(Entity, &InCloset)>();
    q.iter(world)
        .find(|(_, c)| c.closet == gibber)
        .map(|(e, _)| e)
}

fn anchored(world: &World, e: Entity) -> bool {
    world
        .get::<crate::obj_damage::Anchored>(e)
        .map(|a| a.0)
        .or_else(|| world.get::<DmObj>(e).map(|o| o.anchored))
        .unwrap_or(true)
}

fn show(world: &mut World, at: Entity, span: Span, text: String) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (span, text),
            &[],
        );
    }
}

/// `attack_hand` (left) and the "Empty gibber" verb (right). True when the click was the
/// gibber's.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    let Some(gibber) = world.get::<Gibber>(e).cloned() else {
        return false;
    };
    if right {
        // `eject`: `IS_UNCONSCIOUS_OR_CRIT(usr) || HAS_TRAIT(usr, TRAIT_HANDS_BLOCKED)`.
        if world.get::<Stat>(mob) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, mob).hands_blocked
        {
            go_out(world, e);
        }
        return true;
    }
    if world.get::<crate::power::Unpowered>(e).is_some()
        || world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken)
    {
        return true;
    }
    let name = name_of(world, e);
    if gibber.finishes_at.is_some() {
        tell(
            world,
            client,
            Span::Danger,
            "Машина работает, потому доступ заблокирован.",
        );
        return true;
    }
    if !anchored(world, e) {
        tell(
            world,
            client,
            Span::Warning,
            format!("Нельзя использовать {name}, пока она не прикручена к полу!"),
        );
        return true;
    }
    let pulled = world
        .get::<Pulling>(mob)
        .map(|p| p.0)
        .filter(|&p| world.get::<Health>(p).is_some());
    let Some(victim) = pulled else {
        start_gibbing(world, e);
        return true;
    };
    // `iscarbon(L)`: humans are the only carbons here.
    if world.get::<crate::medical::wounds::Limbs>(victim).is_none() {
        tell(
            world,
            client,
            Span::Warning,
            format!("Этот предмет не подходит для {name}!"),
        );
        return true;
    }
    if world
        .get::<crate::medical::surgery::Buckled>(victim)
        .is_some()
    {
        let who = name_of(world, victim);
        tell(
            world,
            client,
            Span::Warning,
            format!("Тело {who} к чему-то пристегнуто!"),
        );
        return true;
    }
    let (_, gibtime, ignore_clothing) = ratings(world, e);
    if !ignore_clothing && has_abiotic(world, victim) {
        tell(
            world,
            client,
            Span::Warning,
            "На объекте не должно быть абиотических предметов!",
        );
        return true;
    }
    let (who, what) = (name_of(world, mob), name_of(world, victim));
    show(
        world,
        e,
        Span::Danger,
        format!("{} начинает запихивать {what} в {name}!", capitalize(&who)),
    );
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        e,
        gibtime,
        0,
        crate::construction::Job::Gibber(StuffJob { victim }),
    );
    true
}

/// `get_equipped_items(INCLUDE_HELD)` without `TRAIT_NODROP`.
fn has_abiotic(world: &World, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return false;
    };
    crate::inventory::ALL_SLOTS
        .iter()
        .filter_map(|&s| inventory.slot(s))
        .any(|i| !crate::inventory::no_drop(world, i))
}

/// End of the stuffing `do_after`, revalidated (`C && user.pulling == C && !C.buckled &&
/// !occupant`).
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: StuffJob) {
    let (e, victim) = (action.target, job.victim);
    if world.get::<Gibber>(e).is_none()
        || world.get_entity(victim).is_err()
        || world.get::<Pulling>(mob).map(|p| p.0) != Some(victim)
        || world
            .get::<crate::medical::surgery::Buckled>(victim)
            .is_some()
        || world.get::<InCloset>(victim).is_some()
        || occupant(world, e).is_some()
    {
        return;
    }
    let (who, what, name) = (
        name_of(world, mob),
        name_of(world, victim),
        name_of(world, e),
    );
    show(
        world,
        e,
        Span::Danger,
        format!("{} запихивает {what} в {name}!", capitalize(&who)),
    );
    crate::closets::put_mob_inside(world, victim, e);
    if let Some(&pos) = world.get::<TilePos>(e) {
        world.entity_mut(victim).insert(pos);
    }
    refresh_look(world, e);
}

/// `go_out` -> `dump_inventory_contents`.
fn go_out(world: &mut World, e: Entity) {
    if let Some(mob) = occupant(world, e) {
        crate::closets::let_out(world, mob);
    }
    refresh_look(world, e);
}

/// The limbs' butcher drops in `get_bodyparts()` order (chest, head, left arm, right arm, right
/// leg, left leg), each with its amount (`get_butcher_drops`).
fn butcher_drops(world: &World, victim: Entity) -> Vec<(&'static str, u32)> {
    let limbs = world.get::<crate::medical::wounds::Limbs>(victim);
    let present = |z: BodyZone| limbs.is_none_or(|l| !l.limb(z).missing);
    let mut out = Vec::new();
    for zone in [
        BodyZone::Chest,
        BodyZone::Head,
        BodyZone::LArm,
        BodyZone::RArm,
        BodyZone::RLeg,
        BodyZone::LLeg,
    ] {
        if !present(zone) {
            continue;
        }
        match zone {
            BodyZone::Chest => out.extend([(HUMAN_MEAT, 2), (HUMAN_SKIN, 1)]),
            // `base_meat_amount = 0`: only the skin.
            BodyZone::Head => out.push((HUMAN_SKIN, 1)),
            _ => out.push((HUMAN_MEAT, 1)),
        }
    }
    out
}

/// tg's loop over the butcher drops: `drop_chance` carries over the drops that did not happen.
/// Returns (path, amount) of what drops.
pub fn roll_drops(
    drops: &[(&'static str, u32)],
    efficiency: f32,
    mut prob: impl FnMut(f32) -> bool,
) -> Vec<(&'static str, u32)> {
    let mut out = Vec::new();
    let mut drop_chance = 0.0;
    for &(path, amount) in drops {
        drop_chance += amount.max(1) as f32 * efficiency;
        let amount = if drop_chance > 1.0 {
            // `prob(drop_chance * 100)` is over 100 here: always one more.
            let mut n = drop_chance.floor() as u32;
            if prob(drop_chance * 100.0) {
                n += 1;
            }
            n
        } else {
            if !prob(drop_chance * 100.0) {
                continue;
            }
            1
        };
        drop_chance = 0.0;
        out.push((path, amount));
    }
    out
}

/// `start_gibbing`.
fn start_gibbing(world: &mut World, e: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let Some(victim) = occupant(world, e) else {
        show(
            world,
            e,
            Span::Plain,
            "Вы слышите громкий металлический скрежет.".to_owned(),
        );
        return;
    };
    let joules = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, joules);
    show(
        world,
        e,
        Span::Plain,
        "Вы слышите громкий хлюпающий скрежещущий звук.".to_owned(),
    );
    let (efficiency, gibtime, _) = ratings(world, e);
    let drops = butcher_drops(world, victim);
    let rolled = {
        let mut rng = world.resource_mut::<CombatRng>();
        roll_drops(&drops, efficiency, |p| rng.chance(p))
    };
    let victim_name = name_of(world, victim);
    let mut results = Vec::new();
    for (path, amount) in rolled {
        if path == HUMAN_SKIN {
            if let Some(stack) = crate::construction::spawn_stack(world, path, pos, amount) {
                results.push(stack);
            }
            continue;
        }
        for _ in 0..amount {
            let Some(meat) = crate::construction::spawn_obj(world, path, pos) else {
                continue;
            };
            // `meat.name = "[victim.real_name]'s [meat.name]"`.
            let named = format!("{} ({victim_name})", name_of(world, meat));
            world.entity_mut(meat).insert(Name::new(named));
            results.push(meat);
        }
    }
    process_results(world, victim, &results);
    for &r in &results {
        put_inside(world, r, e);
    }
    kill(world, victim, pos);
    let finishes_at = now(world) + f64::from(gibtime.max(0.0)) / 10.0;
    if let Some(mut g) = world.get_mut::<Gibber>(e) {
        g.finishes_at = Some(finishes_at);
    }
    refresh_look(world, e);
}

/// `process_results`: the victim's reagents split over the results with reagents, and fat for
/// its nutrition.
fn process_results(world: &mut World, victim: Entity, results: &[Entity]) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let holders: Vec<Entity> = results
        .iter()
        .copied()
        .filter(|&r| world.get::<Reagents>(r).is_some())
        .collect();
    if holders.is_empty() {
        return;
    }
    let n = holders.len() as f32;
    let nutrition = world
        .get::<crate::hunger::Hunger>(victim)
        .map_or(0.0, |h| h.nutrition);
    let fat = chem.id(FAT);
    let fat_factor = fat.map_or(0.0, |f| chem.def(f).nutriment_factor);
    let mut blood = world.get::<MobReagents>(victim).map(|m| m.blood.clone());
    let share = blood.as_ref().map_or(0.0, |b| b.total_volume / n);
    for r in holders {
        let Some(mut target) = world.get::<Reagents>(r).map(|r| r.0.clone()) else {
            continue;
        };
        if let Some(blood) = blood.as_mut() {
            blood.trans_to(&chem, &mut target, share, 1.0, None, false, false);
        }
        if let Some(fat) = fat
            && fat_factor > 0.0
        {
            target.add_reagent_list(&chem, &[(fat, nutrition / fat_factor / n)], None, true);
        }
        world.entity_mut(r).insert(Reagents(target));
    }
}

/// `victim.death(TRUE)`, `ghostize()`, `qdel(victim)`.
fn kill(world: &mut World, victim: Entity, pos: TilePos) {
    crate::closets::let_out(world, victim);
    world.entity_mut(victim).insert(Stat::Dead);
    if let Some(client) = world.get::<PlayerMob>(victim).map(|p| p.client)
        && let Some(ghost) = crate::ghost::ghostize_client(world, client)
        && let Some(mut g) = world.get_mut::<crate::ghost::Ghost>(ghost)
    {
        g.body = None;
    }
    if let Some(inventory) = world.get::<Inventory>(victim).cloned() {
        let items: Vec<Entity> = crate::inventory::ALL_SLOTS
            .iter()
            .filter_map(|&s| inventory.slot(s))
            .collect();
        for item in items {
            if crate::inventory::no_drop(world, item) {
                world.despawn(item);
            } else {
                crate::inventory::put_down(world, item, pos);
            }
        }
    }
    world.despawn(victim);
}

/// `finish_gibbing` of every gibber whose time has come.
fn finish_gibbing(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Gibber)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, g)| g.finishes_at.is_some_and(|at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        finish(world, e);
    }
}

fn finish(world: &mut World, e: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let bloody = world.resource_mut::<CombatRng>().chance(50.0);
    if let Some(mut g) = world.get_mut::<Gibber>(e) {
        g.finishes_at = None;
        g.dirty |= bloody;
    }
    let results: Vec<Entity> = world
        .get::<Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let count = results.len() as u32;
    for item in &results {
        take_out(world, *item, pos);
        let (dx, dy) = {
            let mut rng = world.resource_mut::<CombatRng>();
            loop {
                let dx = rng.range(0, 2 * THROW_RANGE as u32) as i32 - THROW_RANGE;
                let dy = rng.range(0, 2 * THROW_RANGE as u32) as i32 - THROW_RANGE;
                if (dx, dy) != (0, 0) {
                    break (dx, dy);
                }
            }
        };
        let range = world.resource_mut::<CombatRng>().range(1, count.max(1));
        let target = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
        crate::throw::throw_at(world, *item, target, range, 1.0);
    }
    // `new gibs_type()`: one gib per result on a free neighbouring tile.
    for _ in 0..count {
        let (dir, gib) = {
            let mut rng = world.resource_mut::<CombatRng>();
            let dir = Dir::DMI_ORDER[rng.range(0, 7) as usize];
            (
                dir,
                HUMAN_GIBS[rng.range(0, HUMAN_GIBS.len() as u32 - 1) as usize],
            )
        };
        let at = pos.step(dir);
        if crate::construction::tile_blocked(world, at) {
            continue;
        }
        crate::construction::spawn_obj(world, gib, at);
    }
    refresh_look(world, e);
}

/// `update_icon_state`, `update_overlays` (without emissives).
fn refresh_look(world: &mut World, e: Entity) {
    let Some(gibber) = world.get::<Gibber>(e).cloned() else {
        return;
    };
    let panel = world
        .get::<crate::construction::PanelOpen>(e)
        .is_some_and(|p| p.0);
    let occupied = occupant(world, e).is_some();
    let icon = |world: &World, state: &str| {
        world
            .get::<DmAtom>(e)
            .zip(world.get_resource::<IconIndex>())
            .and_then(|(atom, icons)| icons.get(&atom.icon, state))
    };
    let base = icon(world, if panel { "grinder_open" } else { "grinder" });
    let mut overlays = Vec::new();
    if gibber.dirty
        && let Some(id) = icon(world, "grinder_bloody")
    {
        let mut o = Overlay::new(id);
        o.color = Some(BLOOD_RED);
        overlays.push(o);
    }
    let unpowered = world.get::<crate::power::Unpowered>(e).is_some();
    if !unpowered && !panel {
        let states: &[&str] = if !occupied && gibber.finishes_at.is_none() {
            &["grinder_empty"]
        } else if gibber.finishes_at.is_some() {
            &["grinder_active", "grinder_jaws_active"]
        } else {
            &["grinder_loaded"]
        };
        overlays.extend(
            states
                .iter()
                .filter_map(|s| icon(world, s))
                .map(Overlay::new),
        );
    }
    let Some(mut look) = world.get_mut::<Appearance>(e) else {
        return;
    };
    if let Some(id) = base
        && look.icon_state != id
    {
        look.icon_state = id;
    }
    if look.overlays != overlays {
        look.overlays = overlays;
    }
}

/// `relaymove` / `container_resist_act`: a step of the occupant lets it out.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        let Some(container) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if world.get::<Gibber>(container).is_some() {
            go_out(world, container);
        }
    }
}

/// A removed gibber lets its occupant out where it stood.
fn release_on_despawn(
    remove: On<Remove<Gibber>>,
    inside: Query<(Entity, &InCloset)>,
    mut commands: Commands,
) {
    for (mob, c) in &inside {
        if c.closet == remove.entity {
            commands.queue(move |world: &mut World| crate::closets::let_out(world, mob));
        }
    }
}

/// Examine: tg's status display line.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<Gibber>(e).is_none() {
        return;
    }
    let (efficiency, gibtime, ignore) = ratings(world, e);
    lines.push((
        Span::Notice,
        format!(
            "На дисплее состояния отображается: Вывод {efficiency} мясных кусков через {} секунд \
         работы.",
            gibtime * 0.1
        ),
    ));
    if ignore {
        lines.push((
            Span::Notice,
            "Машина была модернизирована для обработки неорганических материалов.".into(),
        ));
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_carry_their_chance() {
        let human = [
            (HUMAN_MEAT, 2),
            (HUMAN_SKIN, 1),
            (HUMAN_SKIN, 1),
            (HUMAN_MEAT, 1),
            (HUMAN_MEAT, 1),
            (HUMAN_MEAT, 1),
            (HUMAN_MEAT, 1),
        ];
        // One tier 1 bin: 0.5. The chest's 2 meat make 1.0 (one drop, `prob(100)`), the skin
        // 0.5: a failed roll carries it to the head's skin, which then drops at 1.0.
        let mut rolls = vec![];
        let out = roll_drops(&human, 0.5, |p| {
            rolls.push(p);
            p >= 100.0
        });
        assert_eq!(rolls, [100.0, 50.0, 100.0, 50.0, 100.0, 50.0, 100.0]);
        assert_eq!(
            out,
            [
                (HUMAN_MEAT, 1),
                (HUMAN_SKIN, 1),
                (HUMAN_MEAT, 1),
                (HUMAN_MEAT, 1)
            ]
        );
        // Four tier 4 bins: 4.25 per amount, so floor + 1 every time.
        let out = roll_drops(&human[..2], 4.25, |_| true);
        assert_eq!(out, [(HUMAN_MEAT, 9), (HUMAN_SKIN, 5)]);
    }
}
