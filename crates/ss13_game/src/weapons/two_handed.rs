//! tg `/datum/component/two_handed` (`code/datums/components/twohanded.dm`): an item wielded
//! in both hands (`attack_self` with the other hand free) takes its wielded force, its `[base]1`
//! sprite in hand and on the map, plays its wield sound, and an offhand
//! (`/obj/item/offhand`, `TRAIT_NODROP`) fills the other hand, which cannot be selected while
//! the item is wielded (`on_swapping_hands`). It lets go when used in hand again, or when the
//! item or the offhand leaves the hands (`on_drop`, `on_moved`). A `require_twohands` item
//! (chainsaws, kirby plants) is wielded as it enters a hand and falls to the floor when it
//! cannot be or when it is unwielded.
//!
//! Which items are two-handed, and with what, is each item's `Initialize` in tg: [`SPECS`]
//! names them; a `force_unwielded`/`force_wielded` (or `two_hand_force`) var the item passes
//! comes from its [`DmMelee`]. Other modules call [`is_wielded`] (tg `TRAIT_WIELDED`).
// shortcut: no " (Wielded)" name suffix, no cyborg module wording, no `TRAIT_NO_TWOHANDING`,
// no fantasy or material bonuses; the push broom keeps its own wield in `crate::janitor`.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmMelee, DmName};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::tools::tell;

/// `/obj/item/offhand`.
pub const OFFHAND: &str = "/obj/item/offhand";

/// The forces an item passes to `AddComponent(/datum/component/two_handed, ...)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Force {
    /// Neither: the force does not change.
    Same,
    /// Its own `force_unwielded`/`force_wielded` vars.
    Vars,
    /// `force_unwielded = force, force_wielded = two_hand_force`.
    TwoHandForce,
    Fixed(f32, f32),
    Multiplier(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    pub root: &'static str,
    pub require_twohands: bool,
    pub force: Force,
    /// `update_icon_state`: `[base]0`/`[base]1` on the map and in hand.
    pub icon: bool,
    pub wieldsound: &'static str,
    pub unwieldsound: &'static str,
}

const fn spec(root: &'static str, force: Force, icon: bool) -> Spec {
    Spec {
        root,
        require_twohands: false,
        force,
        icon,
        wieldsound: "",
        unwieldsound: "",
    }
}

const fn required(root: &'static str, force: Force) -> Spec {
    Spec {
        root,
        require_twohands: true,
        force,
        icon: false,
        wieldsound: "",
        unwieldsound: "",
    }
}

/// The two-handed items and their `AddComponent` arguments (the longest matching root wins).
pub const SPECS: &[Spec] = &[
    // `weaponry/melee/fireaxe.dm`, `spear.dm`, `chainsaw.dm`, `misc.dm`, `high_frequency_blade.dm`.
    spec("/obj/item/fireaxe", Force::Vars, true),
    spec("/obj/item/spear", Force::Vars, true),
    required("/obj/item/chainsaw", Force::Same),
    spec("/obj/item/bambostaff", Force::Fixed(10.0, 14.0), true),
    spec("/obj/item/highfrequencyblade", Force::Same, true),
    // `weaponry/melee/dualsaber.dm`: its look and weight are `crate::weapons::energy`'s.
    Spec {
        root: "/obj/item/dualsaber",
        require_twohands: false,
        force: Force::TwoHandForce,
        icon: false,
        wieldsound: "sound/items/weapons/saberon.ogg",
        unwieldsound: "sound/items/weapons/saberoff.ogg",
    },
    // `modular_bandastation/weapon/code/melee/sledgehammer.dm`.
    spec("/obj/item/sledgehammer", Force::Vars, true),
    // `items/pitchfork.dm`, `pillow.dm`, `tools/engineering/crowbar.dm`.
    spec("/obj/item/pitchfork", Force::Fixed(7.0, 15.0), true),
    spec("/obj/item/pillow", Force::Fixed(5.0, 10.0), false),
    spec(
        "/obj/item/crowbar/mechremoval",
        Force::Fixed(5.0, 19.0),
        true,
    ),
    // `kirby_plants/kirbyplants.dm`, `v8_engine.dm`, `flatpacks.dm` (BandaStation addition).
    required("/obj/item/kirbyplants", Force::Fixed(10.0, 10.0)),
    required("/obj/item/v8_engine", Force::Fixed(5.0, 5.0)),
    required("/obj/item/flatpack", Force::Same),
];

/// The spec of an item type.
pub fn spec_of(path: &str) -> Option<&'static Spec> {
    SPECS
        .iter()
        .filter(|s| is_under(path, s.root))
        .max_by_key(|s| s.root.len())
}

/// A two-handed item's resolved component vars (made on first use).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TwoHanded {
    pub spec: Spec,
    pub force_unwielded: f32,
    pub force_wielded: f32,
    pub force_multiplier: f32,
    /// `sharpened_increase` (`on_sharpen`).
    pub sharpened_increase: f32,
    /// `BASE_OR_INITIAL_ICON_STATE`.
    pub base_icon: String,
}

/// tg `TRAIT_WIELDED`: wielded by `mob`, `offhand` in its other hand.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Wielded {
    pub mob: Entity,
    pub offhand: Entity,
}

/// `/obj/item/offhand` holding `main`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Offhand {
    pub main: Entity,
}

/// tg `HAS_TRAIT(item, TRAIT_WIELDED)`.
pub fn is_wielded(world: &World, item: Entity) -> bool {
    world.get::<Wielded>(item).is_some()
}

/// The item's [`TwoHanded`], made from its spec and vars the first time.
pub fn two_handed(world: &mut World, item: Entity) -> Option<TwoHanded> {
    if let Some(th) = world.get::<TwoHanded>(item) {
        return Some(th.clone());
    }
    let dm = world.get::<DmItem>(item)?.clone();
    let spec = *spec_of(&dm.path)?;
    let melee = world.get::<DmMelee>(item).cloned().unwrap_or_default();
    let (force_unwielded, force_wielded, force_multiplier) = match spec.force {
        Force::Same => (dm.force, dm.force, 1.0),
        Force::Vars => (melee.force_unwielded, melee.force_wielded, 1.0),
        Force::TwoHandForce => (dm.force, melee.two_hand_force, 1.0),
        Force::Fixed(u, w) => (u, w, 1.0),
        Force::Multiplier(m) => (dm.force, dm.force, m),
    };
    let base_icon = if melee.base_icon_state.is_empty() {
        world
            .get::<DmAtom>(item)
            .map(|a| a.icon_state.clone())
            .unwrap_or_default()
    } else {
        melee.base_icon_state
    };
    let th = TwoHanded {
        spec,
        force_unwielded,
        force_wielded,
        force_multiplier,
        sharpened_increase: 0.0,
        base_icon,
    };
    world.entity_mut(item).insert(th.clone());
    Some(th)
}

fn item_name(world: &World, item: Entity) -> String {
    world
        .get::<DmName>(item)
        .map_or_else(|| "предмет".to_owned(), |n| n.name.clone())
}

fn hand_of(world: &World, mob: Entity, item: Entity) -> Option<usize> {
    world
        .get::<Inventory>(mob)?
        .hands
        .iter()
        .position(|h| *h == Some(item))
}

/// The item's live force for its wield state (`wield`/`unwield` force lines).
pub fn force_for(th: &TwoHanded, base: f32, wielded: bool) -> f32 {
    if !wielded {
        return if th.force_multiplier != 1.0 {
            base
        } else if th.force_unwielded != th.force_wielded {
            th.force_unwielded
        } else {
            base
        };
    }
    let force = if th.force_multiplier != 1.0 {
        base * th.force_multiplier
    } else if th.force_wielded != th.force_unwielded {
        th.force_wielded
    } else {
        base
    };
    force + th.sharpened_increase
}

/// tg `two_handed/on_attack_self`: wields or lets go. False for other items and for
/// `require_twohands` ones (their `attack_self` is their own).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(th) = two_handed(world, item) else {
        return false;
    };
    if th.spec.require_twohands {
        return false;
    }
    if is_wielded(world, item) {
        unwield(world, item, Some(client), true);
    } else if hand_of(world, mob, item).is_some() {
        wield(world, Some(client), mob, item);
    }
    true
}

/// tg `wield`: true when the item is now wielded. A `require_twohands` item that cannot be
/// falls to the floor.
pub fn wield(world: &mut World, client: Option<Entity>, mob: Entity, item: Entity) -> bool {
    if is_wielded(world, item) {
        return true;
    }
    let Some(th) = two_handed(world, item) else {
        return false;
    };
    let Some(hand) = hand_of(world, mob, item) else {
        return false;
    };
    let other = 1 - hand;
    let busy = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands[other].is_some());
    let hands = (0..2)
        .filter(|&h| crate::medical::bodyparts::has_hand(world, mob, h))
        .count();
    let refusal = if busy {
        Some(if th.spec.require_twohands {
            "нельзя взять в одну руку!"
        } else {
            "что-то держим в другой руке!"
        })
    } else if hands < 2 {
        Some("не хватает рук!")
    } else {
        None
    };
    if let Some(text) = refusal {
        if let Some(client) = client {
            tell(world, client, Span::Warning, text);
        }
        if th.spec.require_twohands {
            drop_to_ground(world, mob, item);
        }
        return false;
    }
    let offhand = spawn_offhand(world, item);
    world.entity_mut(offhand).remove::<TilePos>().insert((
        CarriedBy(mob),
        NoDrop,
        Offhand { main: item },
    ));
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.hands[other] = Some(offhand);
    }
    world.entity_mut(item).insert(Wielded { mob, offhand });
    refresh(world, item, &th, true);
    crate::weapons::energy::on_wield(world, item, true);
    if let Some(client) = client {
        let name = item_name(world, item);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы хватаете {name} двумя руками."),
        );
    }
    play(world, mob, th.spec.wieldsound);
    true
}

/// tg `unwield(user, show_message, can_drop)`.
pub fn unwield(world: &mut World, item: Entity, client: Option<Entity>, can_drop: bool) {
    let Some(wielded) = world.get::<Wielded>(item).copied() else {
        return;
    };
    world.entity_mut(item).remove::<Wielded>();
    let th = two_handed(world, item);
    if let Some(th) = &th {
        refresh(world, item, th, false);
    }
    crate::weapons::energy::on_wield(world, item, false);
    if world.get_entity(wielded.offhand).is_ok() {
        world.despawn(wielded.offhand);
    }
    let Some(th) = th else {
        return;
    };
    let mob = wielded.mob;
    let require = th.spec.require_twohands;
    if require && can_drop && hand_of(world, mob, item).is_some() {
        drop_to_ground(world, mob, item);
    }
    if let Some(client) = client {
        let name = item_name(world, item);
        let text = if require {
            format!("Вы роняете {name}.")
        } else {
            format!("Вы теперь держите {name} одной рукой.")
        };
        tell(world, client, Span::Notice, text);
    }
    play(world, mob, th.spec.unwieldsound);
    crate::inventory::refresh_look(world, mob);
}

fn spawn_offhand(world: &mut World, main: Entity) -> Entity {
    // Without the prototype (bare test worlds) the offhand still fills the hand.
    let offhand = crate::construction::spawn_detached(world, OFFHAND).unwrap_or_else(|| {
        world
            .spawn(DmItem {
                path: OFFHAND.into(),
                ..default()
            })
            .id()
    });
    // `offhand_item.name = "[parent_item.name] - offhand"`, its `desc`.
    let main_name = item_name(world, main);
    world.entity_mut(offhand).insert(DmName {
        name: format!("{main_name} - offhand"),
        desc: format!("Ваша вторая рука держит {main_name}."),
    });
    offhand
}

fn drop_to_ground(world: &mut World, mob: Entity, item: Entity) {
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::inventory::put_down(world, item, pos);
        crate::construction::tile_changed(world, pos);
    }
}

fn play(world: &mut World, mob: Entity, sound: &'static str) {
    if sound.is_empty() {
        return;
    }
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 50.0, true));
    }
}

/// The force and, with `update_icon_state`, the sprites of the wield state.
fn refresh(world: &mut World, item: Entity, th: &TwoHanded, wielded: bool) {
    let force = world.get::<DmItem>(item).map_or(0.0, |d| d.force);
    // Unwielding takes back the sharpening and the multiplier the wield added.
    let base = if wielded {
        force
    } else if th.force_multiplier != 1.0 {
        (force - th.sharpened_increase) / th.force_multiplier
    } else {
        force - th.sharpened_increase
    };
    let force = force_for(th, base, wielded);
    let state = format!("{}{}", th.base_icon, u8::from(wielded));
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.force = force;
        if th.spec.icon {
            dm.inhand_icon_state = state.clone();
        }
    }
    if th.spec.icon {
        crate::obj_damage::set_icon_state(world, item, &state);
    }
    if let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// Keeps wields true to the hands, after tg `on_drop`/`on_moved`/`on_equip`: a wielded item
/// or its offhand out of the hands lets go; a `require_twohands` item entering a hand is
/// wielded (or falls). Cost: one check per wielded item and per hand of a changed inventory.
pub(crate) fn maintain(
    wielded: Query<(Entity, &Wielded)>,
    offhands: Query<(Entity, &Offhand)>,
    changed: Query<(Entity, &Inventory), Changed<Inventory>>,
    items: Query<&DmItem>,
    mut commands: Commands,
) {
    let changed: Vec<(Entity, Entity)> = changed
        .iter()
        .flat_map(|(mob, inv)| inv.hands.iter().flatten().map(move |&h| (mob, h)))
        .filter(|&(_, h)| {
            items
                .get(h)
                .ok()
                .and_then(|d| spec_of(&d.path))
                .is_some_and(|s| s.require_twohands)
        })
        .collect();
    let wielded: Vec<(Entity, Wielded)> = wielded.iter().map(|(e, w)| (e, *w)).collect();
    let orphans: Vec<(Entity, Entity)> = offhands.iter().map(|(e, o)| (e, o.main)).collect();
    if changed.is_empty() && wielded.is_empty() && orphans.is_empty() {
        return;
    }
    commands.queue(move |world: &mut World| {
        for (item, w) in wielded {
            let (main_hand, off_hand) = (
                hand_of(world, w.mob, item),
                hand_of(world, w.mob, w.offhand),
            );
            let intact = matches!((main_hand, off_hand), (Some(a), Some(b)) if a != b);
            if !intact {
                let client = world
                    .get::<crate::server::PlayerMob>(w.mob)
                    .map(|p| p.client);
                // tg `on_moved`: `unwield(user, can_drop = FALSE)`; a lost offhand drops a
                // `require_twohands` item still in hand (`on_drop`).
                unwield(world, item, client, main_hand.is_some());
            }
        }
        for (offhand, main) in orphans {
            let live = world
                .get::<Wielded>(main)
                .is_some_and(|w| w.offhand == offhand);
            if !live && world.get_entity(offhand).is_ok() {
                world.despawn(offhand);
            }
        }
        for (mob, item) in changed {
            if !is_wielded(world, item) && hand_of(world, mob, item).is_some() {
                let client = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client);
                wield(world, client, mob, item);
            }
        }
    });
}

/// tg `on_swapping_hands`: the hand holding an offhand cannot be selected.
pub fn blocks_swap(world: &World, mob: Entity, hand: usize) -> bool {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.hands.get(hand).copied().flatten())
        .is_some_and(|h| world.get::<Offhand>(h).is_some())
}

/// tg `two_handed/on_sharpen`: what a whetstone adds to a two-handed item (only its wielded
/// force, `COMPONENT_BLOCK_SHARPEN_APPLIED`), or why it refuses.
// tg compares `force * force_multiplier` whenever `force_multiplier` is set, which it always is
// (default 1), so `force_wielded` items were judged by their unwielded force; the intended
// wielded force is compared here.
pub fn on_sharpen(world: &mut World, item: Entity, amount: f32, max: f32) -> Result<(), Sharpen> {
    let Some(mut th) = two_handed(world, item) else {
        return Err(Sharpen::NotTwoHanded);
    };
    if th.sharpened_increase > 0.0 {
        return Err(Sharpen::Already);
    }
    let wielded = is_wielded(world, item);
    let force = world.get::<DmItem>(item).map_or(0.0, |d| d.force);
    let wielded_val = if th.force_multiplier != 1.0 {
        if wielded {
            force
        } else {
            force * th.force_multiplier
        }
    } else if th.force_wielded != th.force_unwielded {
        th.force_wielded
    } else {
        force
    };
    if wielded_val > max {
        return Err(Sharpen::Maxed);
    }
    th.sharpened_increase = amount.min(max - wielded_val);
    if wielded && let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.force += th.sharpened_increase;
    }
    world.entity_mut(item).insert(th);
    Ok(())
}

/// Why [`on_sharpen`] did not apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sharpen {
    NotTwoHanded,
    Already,
    Maxed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_net::chat::server::SendChat;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        world
    }

    fn axe(world: &mut World) -> Entity {
        world
            .spawn((
                DmItem {
                    path: "/obj/item/fireaxe".into(),
                    force: 5.0,
                    ..default()
                },
                DmMelee {
                    force_unwielded: 5.0,
                    force_wielded: 24.0,
                    base_icon_state: "fireaxe".into(),
                    ..default()
                },
            ))
            .id()
    }

    fn holder(world: &mut World, hands: [Option<Entity>; 2]) -> Entity {
        let mob = world
            .spawn((Inventory { hands, ..default() }, TilePos::new(1, 1, 1)))
            .id();
        for item in hands.into_iter().flatten() {
            world.entity_mut(item).insert(CarriedBy(mob));
        }
        mob
    }

    #[test]
    fn specs_pick_the_longest_root() {
        assert_eq!(
            spec_of("/obj/item/fireaxe/boardingaxe").map(|s| s.root),
            Some("/obj/item/fireaxe")
        );
        assert!(spec_of("/obj/item/chainsaw").is_some_and(|s| s.require_twohands));
        assert!(spec_of("/obj/item/fireaxes").is_none());
    }

    /// The fireaxe hits for 24 wielded, 5 in one hand; the offhand fills and frees the other
    /// hand, which cannot be selected meanwhile.
    #[test]
    fn wielding_a_fireaxe_sets_force_and_offhand() {
        let mut world = world();
        let axe = axe(&mut world);
        let mob = holder(&mut world, [Some(axe), None]);
        let client = world.spawn_empty().id();
        assert!(attack_self(&mut world, client, mob, axe));
        assert_eq!(world.get::<DmItem>(axe).unwrap().force, 24.0);
        assert_eq!(
            world.get::<DmItem>(axe).unwrap().inhand_icon_state,
            "fireaxe1"
        );
        let offhand = world.get::<Wielded>(axe).unwrap().offhand;
        assert_eq!(world.get::<Inventory>(mob).unwrap().hands[1], Some(offhand));
        assert!(world.get::<NoDrop>(offhand).is_some());
        assert!(blocks_swap(&world, mob, 1) && !blocks_swap(&world, mob, 0));
        assert!(attack_self(&mut world, client, mob, axe));
        assert_eq!(world.get::<DmItem>(axe).unwrap().force, 5.0);
        assert!(world.get_entity(offhand).is_err());
        assert!(!is_wielded(&world, axe));
    }

    /// A busy other hand refuses the wield; a whetstone adds to the wielded force only.
    #[test]
    fn busy_hand_refuses_and_sharpening_adds_wielded_force() {
        let mut world = world();
        let axe = axe(&mut world);
        let cup = world.spawn(DmItem::default()).id();
        let mob = holder(&mut world, [Some(axe), Some(cup)]);
        assert!(!wield(&mut world, None, mob, axe));
        assert_eq!(world.get::<DmItem>(axe).unwrap().force, 5.0);
        assert_eq!(on_sharpen(&mut world, axe, 4.0, 30.0), Ok(()));
        assert_eq!(
            on_sharpen(&mut world, axe, 4.0, 30.0),
            Err(Sharpen::Already)
        );
        world.entity_mut(cup).remove::<CarriedBy>();
        world.get_mut::<Inventory>(mob).unwrap().hands[1] = None;
        assert!(wield(&mut world, None, mob, axe));
        assert_eq!(world.get::<DmItem>(axe).unwrap().force, 28.0);
        unwield(&mut world, axe, None, true);
        assert_eq!(world.get::<DmItem>(axe).unwrap().force, 5.0);
    }

    /// A `require_twohands` item that cannot be wielded falls; `force_for` multiplies.
    #[test]
    fn required_items_fall_and_multipliers_scale() {
        let mut world = world();
        let saw = world
            .spawn(DmItem {
                path: "/obj/item/chainsaw".into(),
                force: 13.0,
                ..default()
            })
            .id();
        let cup = world.spawn(DmItem::default()).id();
        let mob = holder(&mut world, [Some(saw), Some(cup)]);
        assert!(!wield(&mut world, None, mob, saw));
        assert_eq!(world.get::<TilePos>(saw), Some(&TilePos::new(1, 1, 1)));
        let th = TwoHanded {
            spec: spec("/x", Force::Multiplier(4.0), false),
            force_unwielded: 5.0,
            force_wielded: 5.0,
            force_multiplier: 4.0,
            sharpened_increase: 0.0,
            base_icon: String::new(),
        };
        assert_eq!(force_for(&th, 5.0, true), 20.0);
        assert_eq!(force_for(&th, 5.0, false), 5.0);
    }
}
