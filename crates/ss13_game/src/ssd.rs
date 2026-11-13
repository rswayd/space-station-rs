//! BandaStation SSD indicator (`modular_bandastation/ssd_indicator`): a living body its player
//! left (disconnected, ghosted out, moved to another body: tg `Logout`) gets
//! `/datum/element/ssd`. While it is not dead it shows the `zzz_glow` overlay
//! (`ssd_indicator.dmi`, `FLY_LAYER`), and every `Life()` it is put to sleep for 7 s
//! (`handle_ssd`: `SetSleeping(7 SECONDS)`). A player taking the body again (tg `Login`) removes
//! both. The examine line of a body without its player is `crate::examine`'s.
// shortcut: the AI core is not marked (its player controls the eye entity, the core never has a
// `PlayerMob`).

use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::NetServerSystems;
use ss13_shared::{Appearance, Overlay};

use crate::breath::MobsClock;
use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::server::PlayerMob;
use crate::status_effects::{Incap, StatusSystems};

pub const SSD_DMI: &str = "modular_bandastation/ssd_indicator/icons/ssd_indicator.dmi";
pub const SSD_STATE: &str = "zzz_glow";
/// `handle_ssd`: `SetSleeping(7 SECONDS)` every `Life()`.
pub const SSD_SLEEP_SECS: f32 = 7.0;

/// tg `/datum/element/ssd` with `player_logged = FALSE`: the body's player left it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ssd;

/// BandaStation `/mob/Logout`'s `logout_time` (game seconds), for `crate::cryosleep`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LoggedOutAt(pub f64);

pub struct SsdPlugin;

impl Plugin for SsdPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_logout)
            .add_observer(on_login)
            .add_systems(Update, ssd_sleep.before(StatusSystems))
            .add_systems(PostUpdate, ssd_overlay.before(NetServerSystems::Send));
    }
}

/// `/mob/living/Logout` -> `AddElement(/datum/element/ssd)`: only living bodies, not ghosts.
fn on_logout(
    remove: On<Remove<PlayerMob>>,
    living: Query<(), (With<Health>, Without<Ghost>)>,
    time: Res<Time>,
    mut commands: Commands,
) {
    if living.contains(remove.entity)
        && let Ok(mut mob) = commands.get_entity(remove.entity)
    {
        mob.try_insert((Ssd, LoggedOutAt(time.elapsed_secs_f64())));
    }
}

/// `on_mob_login` -> `handle_detach`.
fn on_login(add: On<Add<PlayerMob>>, mut commands: Commands) {
    if let Ok(mut mob) = commands.get_entity(add.entity) {
        mob.try_remove::<Ssd>();
    }
}

/// `handle_ssd` from `Life()` (`stat != DEAD`), once per `SSmobs` fire.
// Cost: every 2 s one pass over SSD bodies (left players only).
fn ssd_sleep(world: &mut World, mut seen: Local<u64>) {
    let fired = world.resource::<MobsClock>().times_fired;
    if fired == *seen {
        return;
    }
    *seen = fired;
    let bodies: Vec<Entity> = world
        .query_filtered::<(Entity, Option<&Stat>), (With<Ssd>, Without<crate::medical::stasis::InStasis>)>()
        .iter(world)
        .filter(|(_, stat)| stat.is_none_or(|s| *s != Stat::Dead))
        .map(|(e, _)| e)
        .collect();
    for mob in bodies {
        crate::status_effects::set(world, mob, Incap::Sleeping, SSD_SLEEP_SECS);
    }
}

/// The SSD body is drawn with `zzz_glow` unless dead (`on_mob_death` cuts it, `on_mob_revive`
/// adds it back), on top of whatever built the appearance (idempotent, like
/// `crate::fire`'s burning overlays).
// Cost: one pass over living mobs per tick, writing only on a change.
fn ssd_overlay(
    mut mobs: Query<(Has<Ssd>, Ref<Stat>, &mut Appearance), With<Health>>,
    mut removed: RemovedComponents<Ssd>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(zzz) = icons.as_deref().and_then(|i| i.get(SSD_DMI, SSD_STATE)) else {
        return;
    };
    let removed: Vec<Entity> = removed.read().collect();
    for (ssd, stat, mut appearance) in &mut mobs {
        if !appearance.is_changed() && !stat.is_changed() && !(ssd && is_new_ssd(&appearance, zzz))
        {
            continue;
        }
        sync(&mut appearance, zzz, ssd && *stat != Stat::Dead);
    }
    for mob in removed {
        if let Ok((false, _, mut appearance)) = mobs.get_mut(mob) {
            sync(&mut appearance, zzz, false);
        }
    }
}

/// An SSD body without the overlay yet: it may have just been marked (no appearance change).
fn is_new_ssd(appearance: &Appearance, zzz: ss13_shared::IconStateId) -> bool {
    !appearance.overlays.iter().any(|o| o.icon_state == zzz)
}

fn sync(appearance: &mut Mut<Appearance>, zzz: ss13_shared::IconStateId, wanted: bool) {
    let has = appearance.overlays.iter().any(|o| o.icon_state == zzz);
    if has == wanted {
        return;
    }
    if wanted {
        appearance.overlays.push(Overlay::new(zzz));
    } else {
        appearance.overlays.retain(|o| o.icon_state != zzz);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::{IconStateId, Layer, MouseOpacity};

    const ZZZ: IconStateId = IconStateId(7);

    fn app() -> App {
        let mut app = App::new();
        let mut icons = IconIndex::default();
        icons
            .0
            .insert((SSD_DMI.to_owned(), SSD_STATE.to_owned()), ZZZ);
        app.insert_resource(icons)
            .init_resource::<Time>()
            .add_observer(on_logout)
            .add_observer(on_login)
            .add_systems(Update, ssd_overlay);
        app
    }

    fn look() -> Appearance {
        Appearance {
            icon_state: IconStateId(1),
            layer: Layer(4.0),
            plane: 0,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            overlays: vec![],
            mouse_opacity: MouseOpacity::Icon,
            glide: 0,
        }
    }

    fn zzz(app: &App, mob: Entity) -> bool {
        app.world()
            .get::<Appearance>(mob)
            .unwrap()
            .overlays
            .iter()
            .any(|o| o.icon_state == ZZZ)
    }

    #[test]
    fn a_left_body_sleeps_with_zzz_until_its_player_returns() {
        let mut app = app();
        let client = app.world_mut().spawn_empty().id();
        let mob = app
            .world_mut()
            .spawn((
                PlayerMob { client },
                Health::default(),
                Stat::Conscious,
                look(),
            ))
            .id();
        let ghost = app
            .world_mut()
            .spawn((
                PlayerMob { client },
                Health::default(),
                Ghost { body: None },
                look(),
            ))
            .id();
        app.update();
        assert!(!zzz(&app, mob));

        app.world_mut().entity_mut(mob).remove::<PlayerMob>();
        app.world_mut().entity_mut(ghost).remove::<PlayerMob>();
        app.update();
        assert!(app.world().get::<Ssd>(mob).is_some());
        assert!(
            app.world().get::<Ssd>(ghost).is_none(),
            "a ghost is no SSD body"
        );
        assert!(zzz(&app, mob));

        // Dead: no overlay; revived: back.
        app.world_mut().entity_mut(mob).insert(Stat::Dead);
        app.update();
        assert!(!zzz(&app, mob));
        app.world_mut().entity_mut(mob).insert(Stat::Conscious);
        app.update();
        assert!(zzz(&app, mob));

        app.world_mut().entity_mut(mob).insert(PlayerMob { client });
        app.update();
        assert!(app.world().get::<Ssd>(mob).is_none());
        assert!(!zzz(&app, mob));
    }
}
