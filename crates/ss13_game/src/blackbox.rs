//! The blackbox recorder (server): tg `/obj/machinery/blackbox_recorder` and `/obj/item/blackbox`
//! (`message_server.dm`), the traitor theft target next to the message server.
//!
//! A mapped recorder starts with a blackbox inside. An empty hand takes it out (into the hands,
//! "The tapes stop spinning", `blackbox_b`); the blackbox clicked on an empty recorder goes back
//! in with a click (`blackbox`).
// shortcut: SSblackbox itself (the round's feedback and SQL persistence) is not ported, so the
// recorder only holds the item; the `Destroy` drop with an oil splat waits for a machine teardown
// hook.

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::inventory::Inventory;
use crate::storage::StoredIn;
use crate::tools::tell;

pub const RECORDER_PATH: &str = "/obj/machinery/blackbox_recorder";
pub const BLACKBOX_PATH: &str = "/obj/item/blackbox";
const CLICK: &str = "sound/machines/click.ogg";

/// A recorder and the blackbox it holds (`stored`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlackboxRecorder {
    pub stored: Option<Entity>,
}

pub struct BlackboxPlugin;

impl Plugin for BlackboxPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_observer(add_recorder)
            .add_systems(ss13_world::MapInit, fill_recorders);
        app.add_click_handler(ClickPhase::EmptyHand, 3507, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1587, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, h, c.target))
        });
    }
}

fn add_recorder(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, RECORDER_PATH))
    {
        commands
            .entity(add.entity)
            .try_insert(BlackboxRecorder::default());
    }
}

/// `Initialize`: `stored = new /obj/item/blackbox(src)`.
fn fill_recorders(world: &mut World) {
    let empty: Vec<Entity> = world
        .query::<(Entity, &BlackboxRecorder)>()
        .iter(world)
        .filter(|(_, r)| r.stored.is_none())
        .map(|(e, _)| e)
        .collect();
    for recorder in empty {
        if let Some(item) = crate::construction::spawn_detached(world, BLACKBOX_PATH) {
            store(world, recorder, item);
        }
    }
}

/// The blackbox, if it is still inside.
pub fn stored(world: &World, recorder: Entity) -> Option<Entity> {
    world
        .get::<BlackboxRecorder>(recorder)?
        .stored
        .filter(|&s| world.get::<StoredIn>(s).is_some_and(|i| i.0 == recorder))
}

fn store(world: &mut World, recorder: Entity, item: Entity) {
    crate::storage::put_inside(world, item, recorder);
    world
        .entity_mut(recorder)
        .insert(BlackboxRecorder { stored: Some(item) });
    crate::obj_damage::set_icon_state(world, recorder, "blackbox");
}

fn is_blackbox(world: &World, item: Entity) -> bool {
    world
        .get::<ss13_mapload::DmItem>(item)
        .is_some_and(|i| is_under(&i.path, BLACKBOX_PATH))
}

/// `attack_hand`: the blackbox into the user's hands.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, recorder: Entity) -> bool {
    if world.get::<BlackboxRecorder>(recorder).is_none() {
        return false;
    }
    let Some(item) = stored(world, recorder) else {
        tell(
            world,
            client,
            Span::Warning,
            "Кажется, чёрный ящик отсутствует...",
        );
        return true;
    };
    world
        .entity_mut(recorder)
        .insert(BlackboxRecorder { stored: None });
    match world.get::<Inventory>(mob).cloned() {
        Some(inventory) => {
            world.entity_mut(item).insert(ss13_net::server::Networked);
            crate::storage::put_in_hands(world, mob, &inventory, item);
        }
        None => {
            if let Some(&pos) = world.get::<TilePos>(recorder) {
                crate::storage::take_out(world, item, pos);
            }
        }
    }
    crate::obj_damage::set_icon_state(world, recorder, "blackbox_b");
    tell(
        world,
        client,
        Span::Notice,
        "Вы извлекаете чёрный ящик. Плёнки перестают вращаться.",
    );
    true
}

/// `item_interaction`: the blackbox back into an empty recorder.
fn item_interaction(world: &mut World, client: Entity, held: Entity, recorder: Entity) -> bool {
    if world.get::<BlackboxRecorder>(recorder).is_none() || !is_blackbox(world, held) {
        return false;
    }
    if stored(world, recorder).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "В регистраторе уже есть чёрный ящик.",
        );
        return true;
    }
    if crate::inventory::no_drop(world, held) {
        tell(world, client, Span::Warning, "Чёрный ящик прилип к руке!");
        return true;
    }
    store(world, recorder, held);
    tell(
        world,
        client,
        Span::Notice,
        "Вы вставляете чёрный ящик в регистратор, и он встаёт на место. Плёнки снова вращаются.",
    );
    if let Some(&pos) = world.get::<TilePos>(recorder) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, CLICK, 50.0, true));
    }
    true
}

#[cfg(test)]
mod tests {
    use ss13_net::chat::server::SendChat;

    use super::*;

    #[test]
    fn the_blackbox_comes_out_into_the_hands_and_goes_back() {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        let client = world.spawn_empty().id();
        let pos = TilePos::new(2, 2, 1);
        let recorder = world.spawn((BlackboxRecorder::default(), pos)).id();
        let item = world
            .spawn(ss13_mapload::DmItem {
                path: BLACKBOX_PATH.into(),
                ..default()
            })
            .id();
        store(&mut world, recorder, item);
        let mob = world.spawn((Inventory::default(), pos)).id();
        assert!(attack_hand(&mut world, client, mob, recorder));
        assert_eq!(stored(&world, recorder), None);
        let inventory = world.get::<Inventory>(mob).unwrap();
        assert_eq!(inventory.active_item(), Some(item));
        // Empty now: nothing more to take.
        assert!(attack_hand(&mut world, client, mob, recorder));
        assert_eq!(
            world.get::<Inventory>(mob).unwrap().active_item(),
            Some(item)
        );
        // Back in; a second one does not fit.
        assert!(item_interaction(&mut world, client, item, recorder));
        assert_eq!(stored(&world, recorder), Some(item));
        let other = world
            .spawn(ss13_mapload::DmItem {
                path: BLACKBOX_PATH.into(),
                ..default()
            })
            .id();
        assert!(item_interaction(&mut world, client, other, recorder));
        assert_eq!(stored(&world, recorder), Some(item));
        // Other items are not its business.
        let wrench = world
            .spawn(ss13_mapload::DmItem {
                path: "/obj/item/wrench".into(),
                ..default()
            })
            .id();
        assert!(!item_interaction(&mut world, client, wrench, recorder));
    }
}
