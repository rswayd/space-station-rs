//! Cult structures (`cult_structures.dm`, `cult_structure_altar.dm`, `_forge.dm`,
//! `_archives.dm`, `_pylon.dm`) and the runed metal they are built from (`runed_metal.dm`).
//!
//! - Runed metal: only a cultist can work it (`interact`), and only on a station or mining level
//!   in a `CULT_PERMITTED` area; then the stack's recipes (`crate::stack_recipes`) build the
//!   pylon, altar, archives, daemon forge, runed door and girder.
//! - Altar, daemon forge and archives (`item_dispenser`): a cultist's empty hand on an anchored
//!   one lists its items; the pick is checked again (adjacent, conscious cultist, anchored, off
//!   cooldown), the item appears on the structure's tile and the structure rests 5 minutes.
//!   Non-cultists cannot touch them.
//! - Pylon (`SSfastprocess`, anchored): cultists within 5 tiles heal 0.4 brute, 0.4 burn and
//!   0.4 blood per second (`aura_healing`); every 5 s one floor in range turns into engraved
//!   cult flooring (10 s when there is nothing to convert).
// shortcut: lists instead of radial menus; no heretic-sacrifice extras (proteon orb, cursed
// blade, crimson medallion: `unlocked_heretic_items` is not ported); no examine stability or
// cooldown lines, concealing, rust conversion or dagger unanchoring; the pylon's range is a
// circle without line of sight and it gives no healing visuals or wound clotting.

use super::*;

/// `use_cooldown_duration`.
pub const DISPENSER_COOLDOWN_SECS: f64 = 5.0 * 60.0;
/// `corruption_cooldown_duration`.
pub const PYLON_CORRUPTION_SECS: f64 = 5.0;
/// `aura_healing` `range`, `brute_heal`, `burn_heal`, `blood_heal`.
const PYLON_RANGE: i32 = 5;
const PYLON_HEAL: f32 = 0.4;
pub const CULT_FLOOR: &str = "/turf/open/floor/engine/cult";
const PYLON: &str = "/obj/structure/destructible/cult/pylon";

/// An `item_dispenser` subtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dispenser {
    Altar,
    Forge,
    Archives,
}

impl Dispenser {
    pub fn of(path: &str) -> Option<Self> {
        const ROOT: &str = "/obj/structure/destructible/cult/item_dispenser";
        let rest = path.strip_prefix(ROOT)?;
        if rest.starts_with("/altar") {
            Some(Self::Altar)
        } else if rest.starts_with("/forge") {
            Some(Self::Forge)
        } else if rest.starts_with("/archives") {
            Some(Self::Archives)
        } else {
            None
        }
    }

    /// `setup_options()`: the choice names and what they make.
    pub fn options(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Altar => &[
                ("Eldritch Whetstone", "/obj/item/sharpener/cult"),
                ("Construct Shell", "/obj/structure/constructshell"),
                (
                    "Flask of Unholy Water",
                    "/obj/item/reagent_containers/cup/beaker/unholywater",
                ),
            ],
            Self::Forge => &[
                (
                    "Nar'Sien Hardened Armor",
                    "/obj/item/clothing/suit/hooded/cultrobes/hardened",
                ),
                ("Eldritch Longsword", "/obj/item/melee/cultblade"),
            ],
            Self::Archives => &[
                (
                    "Zealot's Blindfold",
                    "/obj/item/clothing/glasses/hud/health/night/cultblind",
                ),
                ("Shuttle Curse", "/obj/item/shuttle_curse"),
                ("Veil Walker", "/obj/item/cult_shift"),
            ],
        }
    }

    /// `succcess_message`.
    fn success(self, structure: &str, item: &str) -> String {
        match self {
            Self::Altar => {
                format!("You kneel before {structure} and your faith is rewarded with {item}!")
            }
            Self::Forge => format!(
                "You work {structure} as dark knowledge guides your hands, creating {item}!"
            ),
            Self::Archives => format!("You summon {item} from {structure}!"),
        }
    }
}

/// `COOLDOWN_DECLARE(use_cooldown)`: usable again at this time.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct DispenserCooldown(pub f64);

/// `COOLDOWN_DECLARE(corruption_cooldown)` of a pylon.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct PylonCorruption(pub f64);

/// A dispenser's list, on its prompt entity.
#[derive(Component, Clone, Copy, Debug)]
struct DispenserPrompt {
    mob: Entity,
    structure: Entity,
    kind: Dispenser,
}

/// `runed_metal/interact`: true when it refused (the stack's menu does not open).
pub fn runed_metal_refused(world: &mut World, client: Entity, mob: Entity, stack: Entity) -> bool {
    if !path_of(world, stack).is_some_and(|p| crate::obj_damage::is_under(&p, RUNED_METAL)) {
        return false;
    }
    if !is_cultist(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "Only one with forbidden knowledge could hope to work this metal...",
        );
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return true;
    };
    let valid_z = crate::events::controller::station_levels(world).contains(&pos.z)
        || ss13_mapload::mining_z(world) == Some(pos.z);
    let permitted = area_flags(world, pos).is_some_and(|f| f & CULT_PERMITTED != 0);
    if !valid_z || !permitted {
        tell(
            world,
            client,
            Span::Warning,
            "The veil is not weak enough here.",
        );
        return true;
    }
    false
}

fn time_left(world: &World, structure: Entity) -> f64 {
    world
        .get::<DispenserCooldown>(structure)
        .map_or(0.0, |c| c.0 - now(world))
}

/// `item_dispenser/attack_hand`.
pub fn dispenser_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(kind) = path_of(world, target).and_then(|p| Dispenser::of(&p)) else {
        return false;
    };
    if !is_cultist(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "You're pretty sure you know exactly what this is used for and you can't seem to \
             touch it.",
        );
        return true;
    }
    let name = name_of(world, target);
    if !crate::obj_damage::anchored(world, target) {
        tell(
            world,
            client,
            Span::CultItalic,
            format!("You need to anchor {name} to the floor first."),
        );
        return true;
    }
    let left = time_left(world, target);
    if left > 0.0 {
        let text = crate::jobs::display_time_text(left);
        tell(
            world,
            client,
            Span::CultItalic,
            format!("The magic in {name} is too weak, it will be ready to use again in {text}."),
        );
        return true;
    }
    let items = kind
        .options()
        .iter()
        .map(|(n, _)| (*n).to_owned())
        .collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            name,
            "",
            prompt::PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(DispenserPrompt {
        mob,
        structure: target,
        kind,
    });
    true
}

/// The pick: `check_menu`, adjacency, anchoring and the cooldown again; then the item.
pub(super) fn dispenser_picked(
    world: &mut World,
    mut answers: Local<MessageCursor<PromptAnswered>>,
) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(asked) = world.get::<DispenserPrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&(_, path)) = asked.kind.options().get(i) else {
            continue;
        };
        let (mob, structure) = (asked.mob, asked.structure);
        let adjacent = match (world.get::<TilePos>(mob), world.get::<TilePos>(structure)) {
            (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
            _ => false,
        };
        let able = world.get::<Stat>(mob) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, mob).incapacitated;
        if !adjacent
            || !able
            || !is_cultist(world, mob)
            || !crate::obj_damage::anchored(world, structure)
            || time_left(world, structure) > 0.0
        {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(structure) else {
            continue;
        };
        let t = now(world);
        world
            .entity_mut(structure)
            .insert(DispenserCooldown(t + DISPENSER_COOLDOWN_SECS));
        let Some(item) = crate::construction::spawn_obj(world, path, pos) else {
            warn!("no {path} prototype: the cult dispenser made nothing");
            continue;
        };
        let (structure_name, item_name) = (name_of(world, structure), name_of(world, item));
        chat_to(
            world,
            mob,
            Span::CultItalic,
            asked.kind.success(&structure_name, &item_name),
        );
        info!("{mob} made {path} at a cult {:?}", asked.kind);
    }
}

/// The turfs a pylon may corrupt and the cult floors already there (`circle_view_turfs`).
fn pylon_turfs(world: &World, center: TilePos) -> (Vec<TilePos>, usize) {
    use crate::obj_damage::is_under;
    let mut valid = Vec::new();
    let mut cult = 0;
    for dx in -PYLON_RANGE..=PYLON_RANGE {
        for dy in -PYLON_RANGE..=PYLON_RANGE {
            if dx * dx + dy * dy > PYLON_RANGE * PYLON_RANGE {
                continue;
            }
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            let Some(path) =
                crate::construction::turf_at(world, pos).and_then(|t| path_of(world, t))
            else {
                continue;
            };
            if is_under(&path, CULT_FLOOR) {
                cult += 1;
                continue;
            }
            // `isgroundlessturf` and the blacklist.
            let blocked = [
                "/turf/closed",
                "/turf/open/misc/asteroid",
                "/turf/open/space",
                "/turf/open/openspace",
                "/turf/open/chasm",
                "/turf/open/lava",
            ];
            if blocked.iter().any(|b| is_under(&path, b)) {
                continue;
            }
            valid.push(pos);
        }
    }
    (valid, cult)
}

/// A `/obj/structure/destructible/cult/pylon`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Pylon;

/// New objects that are pylons get [`Pylon`] (`START_PROCESSING` in `Initialize`).
// Cost: per tick, the objects spawned since the last run.
pub(super) fn tag_pylons(
    new: Query<(Entity, &ss13_mapload::DmObj), Added<ss13_mapload::DmObj>>,
    mut commands: Commands,
) {
    for (e, obj) in &new {
        if crate::obj_damage::is_under(&obj.path, PYLON) {
            commands.entity(e).insert(Pylon);
        }
    }
}

/// `pylon/process` and its `aura_healing`.
// Cost: per tick, every pylon (a handful) scans the cultists; every 5 s it reads ~80 turfs.
pub(super) fn pylons(world: &mut World) {
    let t = now(world);
    let secs = world.resource::<Time>().delta_secs();
    let pylons: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), With<Pylon>>()
        .iter(world)
        .map(|(e, &p)| (e, p))
        .collect();
    let cultists: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), With<Cultist>>()
        .iter(world)
        .map(|(e, &p)| (e, p))
        .collect();
    for (pylon, pos) in pylons {
        if !crate::obj_damage::anchored(world, pylon) {
            continue;
        }
        for &(mob, at) in &cultists {
            let near = at.z == pos.z && {
                let (dx, dy) = (at.x - pos.x, at.y - pos.y);
                dx * dx + dy * dy <= PYLON_RANGE * PYLON_RANGE
            };
            if near {
                heal(world, mob, PYLON_HEAL * secs);
            }
        }
        let ready = world.get::<PylonCorruption>(pylon).is_none_or(|c| t >= c.0);
        if !ready {
            continue;
        }
        let (valid, cult) = pylon_turfs(world, pos);
        let wait = if !valid.is_empty() {
            let i = world
                .get_resource_mut::<crate::combat::CombatRng>()
                .map_or(0, |mut r| r.range(0, valid.len() as u32 - 1)) as usize;
            corrupt(world, valid[i]);
            PYLON_CORRUPTION_SECS
        } else if cult > 0 {
            PYLON_CORRUPTION_SECS
        } else {
            PYLON_CORRUPTION_SECS * 2.0
        };
        world.entity_mut(pylon).insert(PylonCorruption(t + wait));
    }
}

/// Plating gets the cult floor on top; any other floor becomes it (`CHANGETURF_INHERIT_AIR`).
fn corrupt(world: &mut World, pos: TilePos) {
    let Some(turf) = crate::construction::turf_at(world, pos) else {
        return;
    };
    let plating = path_of(world, turf)
        .is_some_and(|p| crate::obj_damage::is_under(&p, "/turf/open/floor/plating"));
    if plating {
        crate::construction::place_on_top(world, pos, CULT_FLOOR, true);
    } else {
        let stack = crate::construction::baseturfs_of(world, turf);
        crate::construction::change_turf(world, pos, CULT_FLOOR, stack, true);
    }
}

/// `aura_healing/process` on one cultist: brute, burn and blood.
fn heal(world: &mut World, mob: Entity, amount: f32) {
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) {
        let before = health.clone();
        apply_damage(&mut health, &mut stat, DamageType::Brute, -amount);
        apply_damage(&mut health, &mut stat, DamageType::Burn, -amount);
        if health != before {
            world.entity_mut(mob).insert((health, stat));
        }
    }
    if let Some(mut blood) = world.get_mut::<Blood>(mob)
        && blood.volume < crate::medical::blood::BLOOD_VOLUME_NORMAL
    {
        blood.volume = (blood.volume + amount).min(crate::medical::blood::BLOOD_VOLUME_NORMAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispensers_know_their_items() {
        let altar = Dispenser::of("/obj/structure/destructible/cult/item_dispenser/altar");
        assert_eq!(altar, Some(Dispenser::Altar));
        assert_eq!(
            Dispenser::of("/obj/structure/destructible/cult/item_dispenser/archives/library"),
            Some(Dispenser::Archives)
        );
        assert_eq!(
            Dispenser::of("/obj/structure/destructible/cult/pylon"),
            None
        );
        assert_eq!(Dispenser::Forge.options().len(), 2);
        assert_eq!(
            Dispenser::Altar.options()[1].1,
            "/obj/structure/constructshell"
        );
    }
}
