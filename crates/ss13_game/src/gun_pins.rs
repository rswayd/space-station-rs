//! Firing pins (server), after tg `code/modules/projectiles/pins.dm` and the pin tools of
//! `gun.dm`.
//!
//! - A pin used on a gun without one goes in; a clown's pin (`force_replace`) pushes any pin
//!   out into the user's hands, and any pin replaces a test-range one (`pin_hot_swappable`).
//! - A screwdriver, wirecutters or a welder on a gun in the other hand takes the pin out in 5
//!   seconds, destroying it.
//! - What each pin allows ([`special_auth`]): the test-range pin only in the firing range, the
//!   outback pin only off the station levels, the DNA pin its first user (or the carbon it was
//!   keyed on), the monkey pin simians, the hilarious pin nobody, the ultra hilarious one the
//!   clumsy (and their guns never botch). A failed check honks or screeches; a
//!   `selfdestruct` pin blows the gun up. An emagged pin allows everyone.
// shortcut: no paywall pin (it needs bank accounts tied to a pin and its payment prompt): it
// answers like a standard pin. The abductor and the two pistol pins that are not
// `pin_removable` come out like the rest.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmGun, DmItem};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::construction::Job;
use crate::guns::Gun;
use crate::inventory::Inventory;
use crate::security::name_of;
use crate::tools::{Actor, DoAfter, tell};

pub const PIN: &str = "/obj/item/firing_pin";
const TEST_RANGE: &str = "/obj/item/firing_pin/test_range";
const CLOWN: &str = "/obj/item/firing_pin/clown";
const ULTRA: &str = "/obj/item/firing_pin/clown/ultra";
const ULTRA_SELFDESTRUCT: &str = "/obj/item/firing_pin/clown/ultra/selfdestruct";
const DNA: &str = "/obj/item/firing_pin/dna";
const DREDD: &str = "/obj/item/firing_pin/dna/dredd";
const EXPLORER: &str = "/obj/item/firing_pin/explorer";
const MONKEY: &str = "/obj/item/firing_pin/monkey";
const RANGE_AREA: &str = "/area/station/security/range";
/// `FIRING_PIN_REMOVAL_DELAY`, deciseconds.
const REMOVAL_DS: f32 = 50.0;
const HONK: &str = "sound/items/bikehorn.ogg";

/// What a pin carries besides its type, on the pin item and on the gun it is in: `EMAGGED`
/// and the DNA pin's `unique_enzymes`.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct PinState {
    pub emagged: bool,
    pub dna: Option<String>,
}

/// `fail_message` of the pins this module checks.
fn fail_message(pin: &str) -> &'static str {
    if is_under(pin, TEST_RANGE) {
        "проверка полигона не пройдена!"
    } else if is_under(pin, CLOWN) {
        "хонк!"
    } else if is_under(pin, DNA) {
        "проверка ДНК не пройдена!"
    } else if is_under(pin, EXPLORER) {
        "вы на территории станции, дружок!"
    } else if is_under(pin, MONKEY) {
        "вы не мартышка!"
    } else {
        "недопустимый пользователь!"
    }
}

fn selfdestructs(pin: &str) -> bool {
    is_under(pin, ULTRA_SELFDESTRUCT) || is_under(pin, DREDD)
}

/// `clumsy_check = FALSE` of a gun with an ultra hilarious pin.
pub fn no_clumsy_check(pin: &str) -> bool {
    is_under(pin, ULTRA)
}

fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.clone())
}

/// `pin_auth` of the pins of this module for `user` pulling `gun`'s trigger: `None` for the
/// other pins. An emagged pin allows everyone (`handle_pins`).
pub fn special_auth(
    world: &World,
    gun: Entity,
    pin: &str,
    user: Entity,
) -> Option<Result<(), String>> {
    let state = world.get::<PinState>(gun);
    if state.is_some_and(|s| s.emagged) {
        return Some(Ok(()));
    }
    let ok = if is_under(pin, TEST_RANGE) {
        world
            .get::<TilePos>(user)
            .and_then(|&pos| area_path(world, pos))
            .is_some_and(|area| is_under(&area, RANGE_AREA))
    } else if is_under(pin, ULTRA) {
        crate::genetics::is_clumsy(world, user)
    } else if is_under(pin, CLOWN) {
        false
    } else if is_under(pin, DNA) {
        let own = world
            .get::<crate::genetics::Dna>(user)
            .map(|d| d.unique_enzymes.clone());
        own.is_some() && own == state.and_then(|s| s.dna.clone())
    } else if is_under(pin, EXPLORER) {
        world
            .get::<TilePos>(user)
            .is_some_and(|&pos| !crate::comms_server::on_station_level(world, pos))
    } else if is_under(pin, MONKEY) {
        crate::transform::is_monkey(world, user)
    } else {
        return None;
    };
    Some(if ok {
        Ok(())
    } else {
        Err(fail_message(pin).to_owned())
    })
}

/// What a trigger pull does besides allowing or refusing (`pin_auth`'s sounds, `auth_fail`):
/// `allowed` is the check's answer. True when the failure was not one after all or the gun is
/// gone: the caller says nothing more.
pub fn after_auth(
    world: &mut World,
    client: Entity,
    user: Entity,
    gun: Entity,
    allowed: bool,
) -> bool {
    let Some(pin) = world.get::<Gun>(gun).map(|g| g.pin.clone()) else {
        return false;
    };
    let at = crate::modpc::pda::turf_of(world, gun);
    if is_under(&pin, CLOWN)
        && let Some(at) = at
    {
        crate::sound::playsound(world, crate::sound::PlaySound::new(at, HONK, 50.0, true));
    }
    if allowed {
        return false;
    }
    if is_under(&pin, MONKEY)
        && let Some(at) = at
    {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, "screech", 75.0, true),
        );
    }
    // `/obj/item/firing_pin/dna/auth_fail`: an unkeyed pin takes its first user's DNA.
    if is_under(&pin, DNA) {
        let state = world.get::<PinState>(gun).cloned().unwrap_or_default();
        let own = world
            .get::<crate::genetics::Dna>(user)
            .map(|d| d.unique_enzymes.clone());
        if state.dna.is_none() && !state.emagged {
            if let Some(dna) = own {
                world.entity_mut(gun).insert(PinState {
                    dna: Some(dna),
                    ..state
                });
                tell(world, client, Span::Notice, "ДНК-замок установлен");
            }
            return true;
        }
    }
    if selfdestructs(&pin) {
        tell(world, client, Span::Danger, "САМОУНИЧТОЖЕНИЕ...");
        let name = ss13_chat::capitalize(&name_of(world, gun));
        tell(world, client, Span::Danger, format!("{name} взрывается!"));
        if let Some(at) = at {
            crate::explosions::explosion(
                world,
                at,
                crate::explosions::Explosion {
                    flash: Some(3.0),
                    cause: "firing pin".to_owned(),
                    ..crate::explosions::Explosion::new(-1, 0, 2)
                },
            );
        }
        if world.get_entity(gun).is_ok() {
            drop_from_hands(world, gun);
            world.entity_mut(gun).despawn();
        }
        return true;
    }
    false
}

fn drop_from_hands(world: &mut World, item: Entity) {
    let Some(holder) = world.get::<crate::inventory::CarriedBy>(item).map(|c| c.0) else {
        return;
    };
    if let Some(mut inventory) = world.get_mut::<Inventory>(holder) {
        for hand in &mut inventory.hands {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
}

fn is_pin(world: &World, item: Entity) -> Option<String> {
    world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .filter(|p| is_under(p, PIN))
}

/// `/obj/item/firing_pin/interact_with_atom`: `held` pin on a gun (or the DNA pin on a
/// carbon). True when the click is taken.
pub fn pin_interaction(
    world: &mut World,
    client: Entity,
    user: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(pin) = is_pin(world, held) else {
        return false;
    };
    // `/obj/item/firing_pin/dna/interact_with_atom`.
    if is_under(&pin, DNA)
        && crate::throw::is_carbon(world, target)
        && let Some(dna) = world
            .get::<crate::genetics::Dna>(target)
            .map(|d| d.unique_enzymes.clone())
    {
        let state = world.get::<PinState>(held).cloned().unwrap_or_default();
        world.entity_mut(held).insert(PinState {
            dna: Some(dna),
            ..state
        });
        tell(world, client, Span::Notice, "ДНК-замок установлен");
        return true;
    }
    let (Some(dm), Some(mut gun)) = (
        world.get::<DmGun>(target).cloned(),
        world.get::<Gun>(target).cloned(),
    ) else {
        return false;
    };
    if dm.pinless {
        return false;
    }
    let mut replaced = false;
    if !gun.pin.is_empty() && (is_under(&pin, CLOWN) || is_under(&gun.pin, TEST_RANGE)) {
        // The old pin comes out whole, into the user's hands.
        let old = std::mem::take(&mut gun.pin);
        let old_state = world.entity_mut(target).take::<PinState>();
        if let Some(&at) = world.get::<TilePos>(user)
            && let Some(item) = crate::construction::spawn_obj(world, &old, at)
        {
            if let Some(state) = old_state {
                world.entity_mut(item).insert(state);
            }
            if let Some(inventory) = world.get::<Inventory>(user).cloned() {
                crate::storage::put_in_hands(world, user, &inventory, item);
            }
        }
        replaced = true;
    }
    if !gun.pin.is_empty() {
        tell(
            world,
            client,
            Span::Notice,
            "Это огнестрельное оружие уже имеет установленный ударник.",
        );
        return true;
    }
    gun.pin = pin;
    let state = world.entity_mut(held).take::<PinState>();
    drop_from_hands(world, held);
    world.entity_mut(held).despawn();
    let mut entity = world.entity_mut(target);
    entity.insert(gun);
    match state {
        Some(state) => entity.insert(state),
        None => entity.remove::<PinState>(),
    };
    tell(
        world,
        client,
        Span::Notice,
        if replaced {
            "ударник заменен"
        } else {
            "ударник вставлен"
        },
    );
    true
}

/// tg `screwdriver_act`, `welder_act`, `wirecutter_act` of a gun in the user's other hand:
/// the pin comes out in 5 seconds and is destroyed.
pub fn tool_interaction(
    world: &mut World,
    client: Entity,
    user: Entity,
    tool: Entity,
    gun: Entity,
) -> bool {
    use crate::tools::ToolKind;
    let Some((kind, _)) = crate::tools::tool_of(world, tool) else {
        return false;
    };
    if !matches!(
        kind,
        ToolKind::Screwdriver | ToolKind::Welder | ToolKind::Wirecutters
    ) {
        return false;
    }
    let held = world
        .get::<Inventory>(user)
        .is_some_and(|i| i.hands.contains(&Some(gun)));
    let Some(pin) = world
        .get::<Gun>(gun)
        .map(|g| g.pin.clone())
        .filter(|p| !p.is_empty() && held)
    else {
        return false;
    };
    if world.get::<DoAfter>(user).is_some() {
        return true;
    }
    let (who, what, from, with) = (
        name_of(world, user),
        pin_name(&pin),
        name_of(world, gun),
        name_of(world, tool),
    );
    crate::chem::visible(
        world,
        user,
        &format!("Вы пытаетесь снять {what} с {from}. (Займет примерно 5 секунд.)"),
        &format!("{who} пытается снять {what} с {from} с помощью {with}."),
        Span::Warning,
    );
    let actor = Actor {
        client,
        mob: user,
        item: Some(tool),
    };
    crate::tools::use_tool(world, actor, user, REMOVAL_DS, 0, Job::RemovePin(gun));
    true
}

fn pin_name(pin: &str) -> &'static str {
    if is_under(pin, "/obj/item/firing_pin/magic") {
        "магический осколок"
    } else {
        "ударник"
    }
}

/// The end of [`tool_interaction`]'s `use_tool`.
pub fn complete_removal(world: &mut World, user: Entity, gun: Entity) {
    let Some(mut state) = world.get::<Gun>(gun).cloned() else {
        return;
    };
    let held = world
        .get::<Inventory>(user)
        .is_some_and(|i| i.hands.contains(&Some(gun)));
    // "check to see if the pin is still there".
    if state.pin.is_empty() || !held {
        return;
    }
    let (who, what, from) = (
        name_of(world, user),
        pin_name(&state.pin),
        name_of(world, gun),
    );
    state.pin.clear();
    world.entity_mut(gun).insert(state).remove::<PinState>();
    crate::chem::visible(
        world,
        user,
        &format!("Вы снимаете {what} с {from}, уничтожая ударник в процессе."),
        &format!("{who} снимает {what} с {from}, уничтожая ударник в процессе."),
        Span::Warning,
    );
}

/// `/obj/item/firing_pin/emag_act`.
pub fn emag_act(world: &mut World, client: Entity, pin: Entity) -> bool {
    if is_pin(world, pin).is_none() {
        return false;
    }
    let state = world.get::<PinState>(pin).cloned().unwrap_or_default();
    if state.emagged {
        return false;
    }
    world.entity_mut(pin).insert(PinState {
        emagged: true,
        ..state
    });
    tell(
        world,
        client,
        Span::Notice,
        "проверка аутентификации переписана",
    );
    true
}

pub struct GunPinsPlugin;

impl Plugin for GunPinsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 1487, |w, c| {
            c.held
                .is_some_and(|h| pin_interaction(w, c.client, c.mob, h, c.target))
        })
        .add_click_handler(ClickPhase::OtherHand, 1487, |w, c| {
            c.held.is_some_and(|h| {
                pin_interaction(w, c.client, c.mob, h, c.target)
                    || tool_interaction(w, c.client, c.mob, h, c.target)
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gun(world: &mut World, pin: &str) -> Entity {
        world
            .spawn(Gun {
                pin: pin.to_owned(),
                ..default()
            })
            .id()
    }

    #[test]
    fn pins_check_their_user() {
        let mut world = World::new();
        let user = world.spawn_empty().id();
        // A standard pin is not this module's.
        let plain = gun(&mut world, PIN);
        assert_eq!(special_auth(&world, plain, PIN, user), None);
        // The hilarious pin never fires, until it is emagged.
        let honk = gun(&mut world, CLOWN);
        assert_eq!(
            special_auth(&world, honk, CLOWN, user),
            Some(Err("хонк!".to_owned()))
        );
        world.entity_mut(honk).insert(PinState {
            emagged: true,
            dna: None,
        });
        assert_eq!(special_auth(&world, honk, CLOWN, user), Some(Ok(())));
        // A DNA pin fires for the enzymes it holds only.
        let keyed = gun(&mut world, DNA);
        let dna = |enzymes: &str| crate::genetics::Dna {
            unique_enzymes: enzymes.to_owned(),
            unique_identity: String::new(),
            mutation_index: Vec::new(),
            default_mutation_genes: Vec::new(),
            mutations: Vec::new(),
            stability: 100,
            melt_until: None,
        };
        world.entity_mut(user).insert(dna("abc"));
        assert!(special_auth(&world, keyed, DNA, user).unwrap().is_err());
        world.entity_mut(keyed).insert(PinState {
            emagged: false,
            dna: Some("abc".to_owned()),
        });
        assert_eq!(special_auth(&world, keyed, DNA, user), Some(Ok(())));
        world.entity_mut(user).insert(dna("xyz"));
        assert_eq!(
            special_auth(&world, keyed, DREDD, user),
            Some(Err("проверка ДНК не пройдена!".to_owned()))
        );
        assert!(selfdestructs(DREDD) && !selfdestructs(DNA));
        assert!(no_clumsy_check(ULTRA_SELFDESTRUCT) && !no_clumsy_check(CLOWN));
    }
}
