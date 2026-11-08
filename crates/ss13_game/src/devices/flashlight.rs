//! Flashlights, lamps and hard hat headlamps (server), after tg `/obj/item/flashlight`
//! (`code/game/objects/items/devices/flashlight.dm`) and `/obj/item/clothing/head/utility/hardhat`
//! (`code/modules/clothing/head/hardhat.dm`).
//!
//! Every switchable `/obj/item/flashlight` gets a [`Flashlight`]. Used in hand (`attack_self`)
//! or right-clicked with an empty hand (`attack_hand_secondary`) it toggles (`toggle_light`):
//! the click sound, then the light, the `-on` icon and in-hand states. The light follows the
//! item the way tg's overlay light does: on its holder while carried, on itself on the floor,
//! nowhere inside a container. An EMP (`emp_act` -> `on_saboteur`) switches it off and keeps it
//! from switching on for `1 minute / severity` ("сбой!").
// shortcut: the light is omnidirectional (tg `OVERLAY_LIGHT_DIRECTIONAL`: a cone along the
// holder's facing) and gives way to any other light already on the holder (one `Optics` light
// per entity, as the PDA's); no item action button, no eye/mouth examine with a penlight, no
// medical holosign, no paint colour, no flares, glowsticks, candles, torches or the heretic
// lantern (their own fuel and lighting rules).
// Cost: one pass over the flashlights whose state, holder or place changed this tick.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::light::{DmLight, source};
use ss13_mapload::{DmAtom, DmItem};
use ss13_shared::chat::Span;
use ss13_shared::{LightSource, Optics, TilePos};

use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;

const FLASHLIGHT: &str = "/obj/item/flashlight";
pub const HARDHAT: &str = "/obj/item/clothing/head/utility/hardhat";
/// Subtypes with their own `toggle_light`/`attack_self` or light rules (see the shortcut).
const NOT_SWITCHES: [&str; 7] = [
    "/obj/item/flashlight/flare",
    "/obj/item/flashlight/glowstick",
    "/obj/item/flashlight/spotlight",
    "/obj/item/flashlight/flashdark",
    "/obj/item/flashlight/eyelight",
    "/obj/item/flashlight/lamp/space_bubble",
    "/obj/item/flashlight/lantern/heretic",
];
/// `sound_on`, `sound_off`.
const SOUND_ON: &str = "sound/items/weapons/magin.ogg";
const SOUND_OFF: &str = "sound/items/weapons/magout.ogg";
/// `on_saboteur(src, 1 MINUTES / severity)`.
const EMP_DISABLE_SECS: f64 = 60.0;

/// `/obj/item/flashlight`: `light_on`, `disabled_time`, and the light it gives when on.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Flashlight {
    pub on: bool,
    /// Game seconds until it may switch on again (`COOLDOWN_START(disabled_time)`).
    pub disabled_until: f64,
    /// `None` for a light with no range (nothing to draw).
    pub light: Option<LightSource>,
    /// `initial(icon_state)`, `initial(inhand_icon_state)` (empty: none).
    base_state: String,
    base_inhand: String,
    ready: bool,
    /// A hard hat: `hardhat[on]_[hat_type]` sprites, no switch sound.
    hardhat: bool,
}

/// On a flashlight whose light this module put on `.0` (its holder or itself), with that light.
#[derive(Component, Clone, Copy, Debug)]
struct LitOn(Entity, LightSource);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // tg `attack_self` -> `toggle_light`.
    app.add_click_handler(ClickPhase::AttackSelf, 1710, |w, c| {
        toggle_by(w, c.client, c.mob, c.target)
    });
    // tg `attack_hand_secondary`: a right click with an empty hand toggles it where it lies.
    app.add_click_handler(ClickPhase::EmptyHand, 3456, |w, c| {
        c.modifiers.right
            && w.get::<Flashlight>(c.target).is_some_and(|f| !f.hardhat)
            && toggle_by(w, c.client, c.mob, c.target)
    });
    app.add_observer(add_flashlight)
        .add_observer(unlight)
        .add_systems(
            Update,
            (
                (init_flashlights, sync_lights).chain(),
                flashlight_emps.in_set(crate::emp::EmpSystems::Effects),
            ),
        );
}

/// True for the flashlight types this module switches.
pub fn is_switch(path: &str) -> bool {
    is_under(path, HARDHAT)
        || (is_under(path, FLASHLIGHT) && !NOT_SWITCHES.iter().any(|root| is_under(path, root)))
}

fn add_flashlight(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items.get(add.entity).is_ok_and(|i| is_switch(&i.path)) {
        commands
            .entity(add.entity)
            .try_insert_if_new(Flashlight::default());
    }
}

/// `Initialize`: the light it gives when on, and whether it starts on (`start_on`; the map
/// loader already lit those, `ss13_mapload::light`). Runs once the prototype is complete.
fn init_flashlights(
    mut new: Query<
        (
            Entity,
            &mut Flashlight,
            &DmItem,
            Option<&DmLight>,
            Option<&DmAtom>,
            Option<&Optics>,
        ),
        Added<Flashlight>,
    >,
    mut commands: Commands,
) {
    for (e, mut flashlight, item, dm_light, atom, optics) in &mut new {
        if flashlight.ready {
            continue;
        }
        let dir = atom.map_or(0, |a| a.dir);
        let light = dm_light.and_then(|l| {
            let on = DmLight {
                start_on: true,
                // A hard hat's light, not a flashlight's, goes by its `light_on`.
                light_on: true,
                ..l.clone()
            };
            source(&item.path, &on, dir, [0, 0])
        });
        flashlight.hardhat = is_under(&item.path, HARDHAT);
        flashlight.light = light;
        flashlight.base_state = atom.map(|a| a.icon_state.clone()).unwrap_or_default();
        flashlight.base_inhand = item.inhand_icon_state.clone();
        flashlight.on = dm_light.is_some_and(|l| {
            if flashlight.hardhat {
                l.light_on
            } else {
                l.start_on
            }
        }) && light.is_some();
        flashlight.ready = true;
        // The map loader's light on the item itself is this module's from now on.
        if flashlight.on
            && let Some(lit) = optics.and_then(|o| o.light)
        {
            commands.entity(e).insert(LitOn(e, lit));
        }
    }
}

/// `toggle_light(user)` from a click; true when `item` is a switchable flashlight.
fn toggle_by(world: &mut World, client: Entity, _mob: Entity, item: Entity) -> bool {
    if world.get::<Flashlight>(item).is_none() {
        return false;
    }
    toggle_light(world, Some(client), item);
    true
}

/// tg `toggle_light(user)`: the sound (always), then the switch unless disrupted. Whether the
/// light changed.
pub fn toggle_light(world: &mut World, client: Option<Entity>, item: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(flashlight) = world.get::<Flashlight>(item).cloned() else {
        return false;
    };
    if !flashlight.hardhat
        && let Some(pos) = crate::modpc::pda::turf_of(world, item)
    {
        let sound = if flashlight.on { SOUND_OFF } else { SOUND_ON };
        playsound(world, PlaySound::new(pos, sound, 40.0, true));
    }
    if now < flashlight.disabled_until {
        if let Some(client) = client {
            crate::tools::tell(world, client, Span::Warning, "сбой!");
        }
        return false;
    }
    set_on(world, item, !flashlight.on);
    true
}

/// `set_light_on` + `update_brightness`: the state, the icon and in-hand states.
pub fn set_on(world: &mut World, item: Entity, on: bool) {
    let Some(mut flashlight) = world.get_mut::<Flashlight>(item) else {
        return;
    };
    if flashlight.on == on {
        return;
    }
    flashlight.on = on;
    let hardhat = flashlight.hardhat;
    let (base_state, base_inhand) = (
        flashlight.base_state.clone(),
        flashlight.base_inhand.clone(),
    );
    if hardhat {
        // `update_icon_state`: `hardhat[on]_[hat_type]`, the in-hand state as well.
        if let Some((_, hat_type)) = base_state.split_once('_') {
            let state = format!("hardhat{}_{hat_type}", u8::from(on));
            if let Some(mut dm) = world.get_mut::<DmItem>(item) {
                dm.inhand_icon_state = state.clone();
            }
            crate::clothing::toggles::set_states(world, item, &state, state.clone());
        }
        return;
    }
    // `update_icon_state`: `"[initial(icon_state)]-on"`, the same for the in-hand state.
    let suffix = if on { "-on" } else { "" };
    if !base_state.is_empty() {
        crate::obj_damage::set_icon_state(world, item, &format!("{base_state}{suffix}"));
    }
    if !base_inhand.is_empty() {
        let inhand = format!("{base_inhand}{suffix}");
        if let Some(mut dm) = world.get_mut::<DmItem>(item)
            && dm.inhand_icon_state != inhand
        {
            dm.inhand_icon_state = inhand;
        }
        // The holder's in-hand sprites are drawn from its inventory.
        if let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0)
            && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
        {
            inventory.set_changed();
        }
    }
}

/// The light follows the flashlight: on its holder while carried, on itself on the floor,
/// nowhere in a container. Runs for flashlights whose state, holder or position changed.
#[allow(clippy::type_complexity)]
fn sync_lights(
    changed: Query<
        Entity,
        (
            With<Flashlight>,
            Or<(
                Changed<Flashlight>,
                Changed<CarriedBy>,
                Added<TilePos>,
                Added<StoredIn>,
            )>,
        ),
    >,
    mut dropped: RemovedComponents<CarriedBy>,
    all: Query<(
        Entity,
        &Flashlight,
        Option<&CarriedBy>,
        Has<TilePos>,
        Option<&LitOn>,
    )>,
    mut optics: Query<&mut Optics>,
    mut commands: Commands,
) {
    let mut entities: Vec<Entity> = changed.iter().chain(dropped.read()).collect();
    entities.sort_unstable();
    entities.dedup();
    for (entity, flashlight, carried, on_map, lit) in
        entities.into_iter().filter_map(|e| all.get(e).ok())
    {
        if !flashlight.ready {
            continue;
        }
        let target = match flashlight.light {
            Some(_) if flashlight.on => carried.map(|c| c.0).or(on_map.then_some(entity)),
            _ => None,
        };
        if lit.map(|l| l.0) == target {
            continue;
        }
        if lit.is_some() {
            commands.entity(entity).remove::<LitOn>();
        }
        let (Some(target), Some(light)) = (target, flashlight.light) else {
            continue;
        };
        match optics.get_mut(target) {
            Ok(mut o) if o.light.is_none() => o.light = Some(light),
            Ok(_) => continue,
            Err(_) => {
                commands.entity(target).insert(Optics {
                    light: Some(light),
                    ..default()
                });
            }
        }
        commands.entity(entity).insert(LitOn(target, light));
    }
}

/// The light goes out where [`sync_lights`] put it (moved, switched off, flashlight gone).
fn unlight(
    remove: On<Remove<LitOn>>,
    lit: Query<&LitOn>,
    mut optics: Query<&mut Optics>,
    mut commands: Commands,
) {
    let Ok(&LitOn(target, light)) = lit.get(remove.entity) else {
        return;
    };
    if let Ok(mut o) = optics.get_mut(target)
        && o.light == Some(light)
    {
        if !o.opaque && o.tile.is_none() {
            commands.entity(target).try_remove::<Optics>();
        } else {
            o.light = None;
        }
    }
}

/// `emp_act` -> `on_saboteur(src, 1 MINUTES / severity)`: a lit one goes out (with its sound),
/// and none switches on until the time is up. (`/obj/item/flashlight/emp` is
/// `EMP_PROTECT_SELF`, `crate::emp`.)
fn flashlight_emps(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::emp::EmpAct>>,
) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        if !ev.hits_self() || world.get::<Flashlight>(ev.target).is_none() {
            continue;
        }
        if world.get::<Flashlight>(ev.target).is_some_and(|f| f.on) {
            toggle_light(world, None, ev.target);
        }
        let until = world.resource::<Time>().elapsed_secs_f64() + ev.by_severity(EMP_DISABLE_SECS);
        if let Some(mut f) = world.get_mut::<Flashlight>(ev.target) {
            f.disabled_until = until;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_are_the_plain_flashlights() {
        assert!(is_switch("/obj/item/flashlight"));
        assert!(is_switch("/obj/item/flashlight/seclite"));
        assert!(is_switch("/obj/item/flashlight/lamp/green"));
        assert!(is_switch("/obj/item/flashlight/emp"));
        assert!(!is_switch("/obj/item/flashlight/flare/candle"));
        assert!(!is_switch("/obj/item/flashlight/glowstick/red"));
        assert!(!is_switch("/obj/item/flashlight/lamp/space_bubble"));
        assert!(!is_switch("/obj/item/flashlightx"));
    }

    #[test]
    fn the_light_follows_the_flashlight() {
        let mut app = App::new();
        app.add_systems(Update, sync_lights).add_observer(unlight);
        let light = LightSource {
            range: 4.0,
            power: 1.0,
            color: [255, 200, 100],
            height: 1.0,
            angle: 360,
            dir: ss13_shared::Dir::South,
            offset: [0, 0],
        };
        let mob = app.world_mut().spawn(TilePos::new(1, 1, 1)).id();
        let item = app
            .world_mut()
            .spawn((
                Flashlight {
                    on: true,
                    light: Some(light),
                    ready: true,
                    ..default()
                },
                CarriedBy(mob),
            ))
            .id();
        app.update();
        let lit = |app: &App, e| app.world().get::<Optics>(e).and_then(|o| o.light);
        assert_eq!(lit(&app, mob), Some(light), "the holder carries the light");
        app.world_mut().get_mut::<Flashlight>(item).unwrap().on = false;
        app.update();
        assert_eq!(lit(&app, mob), None, "switched off");
        // Dropped and on: it lights its own tile.
        app.world_mut().entity_mut(item).remove::<CarriedBy>();
        app.world_mut()
            .entity_mut(item)
            .insert(TilePos::new(2, 1, 1));
        app.world_mut().get_mut::<Flashlight>(item).unwrap().on = true;
        app.update();
        assert_eq!(lit(&app, item), Some(light));
        assert_eq!(lit(&app, mob), None);
    }
}
