//! `/datum/hallucination/delusion` (`code/modules/hallucination/delusions.dm`): humans look like
//! something else to the hallucinator alone for a while. Each delusion is an alternate
//! appearance of every affected human (`crate::alt_appearance`) that only the hallucinator's
//! mob sees, as tg's override images in `hallucinator.client.images`.
// shortcut: no examine and screentip name override (`delusion_name`: examine has no
// per-examiner name hook), no ghost floating animation, no lighting underlays; the dynamic human
// presets (syndies, seccies, heretic, heretic gate: `get_dynamic_human_appearance` needs an
// outfit renderer), the custom admin delusion and BandaStation's vampire hysteria are not ported,
// so the syndies' weight is missing from the rare pool.

use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_shared::Appearance;
use ss13_shared::chat::Span;

use super::kinds::{Delusion, unconscious};
use super::{client_of, humans_in_view, later, play_local, pos_of, tell};
use crate::alt_appearance::{Seers, add_alt_appearance, remove_alt_appearance};
use crate::human::HumanLook;

/// `ABOVE_GAME_PLANE`.
const ABOVE_GAME_PLANE: i16 = -3;
const WABBAJACK: &str = "sound/effects/magic/staff_change.ogg";
const LIVE_AGAIN: &str = "sound/mobs/non-humanoids/cyborg/liveagain.ogg";
/// The files delusions play (for the asset converter's list).
pub const SOUNDS: [&str; 2] = [WABBAJACK, LIVE_AGAIN];

/// The `New()` arguments of a delusion (Mass Hallucination overrides the preset's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Args {
    /// Seconds; 0 or less: for good.
    pub duration: f64,
    pub affects_us: bool,
    /// Every human.
    pub affects_others: bool,
    /// Not the humans in view of the hallucinator.
    pub skip_nearby: bool,
    pub play_wabbajack: bool,
}

impl Delusion {
    /// `delusion_icon_file` and `delusion_icon_state`.
    pub fn icon(self) -> (&'static str, &'static str) {
        match self {
            Self::Nothing => ("icons/effects/effects.dmi", "nothing"),
            Self::Curse => (
                "icons/mob/simple/lavaland/lavaland_monsters.dmi",
                "curseblob",
            ),
            Self::Monkey => ("icons/mob/human/human.dmi", "monkey"),
            Self::Corgi => ("icons/mob/simple/pets.dmi", "corgi"),
            Self::Carp => ("icons/mob/simple/carp.dmi", "carp"),
            Self::Skeleton => ("icons/mob/human/human.dmi", "skeleton"),
            Self::Zombie => ("icons/mob/human/human.dmi", "zombie"),
            Self::Demon => ("icons/mob/simple/demon.dmi", "slaughter_demon"),
            Self::Cyborg => ("icons/mob/silicon/robots.dmi", "robot"),
            Self::Ghost => ("icons/mob/simple/mob.dmi", "ghost"),
            Self::Mare => ("icons/obj/clothing/masks.dmi", "horsehead"),
            Self::Moon => ("icons/mob/nonhuman-player/eldritch_mobs.dmi", "moon_mass"),
        }
    }

    /// The preset's own vars.
    pub fn args(self) -> Args {
        let mut args = Args {
            duration: 30.0,
            affects_us: true,
            affects_others: false,
            skip_nearby: false,
            play_wabbajack: false,
        };
        match self {
            Self::Cyborg => args.play_wabbajack = true,
            Self::Ghost => args.affects_others = true,
            Self::Mare => {
                args.affects_us = false;
                args.affects_others = true;
            }
            Self::Moon => {
                args.duration = 15.0;
                args.affects_others = true;
            }
            _ => {}
        }
        args
    }
}

/// Delusions started so far: each one's alternate appearance key.
#[derive(Resource, Default)]
struct Started(u64);

/// `delusion.start()`: false without a client, unconscious, or without the preset's icon.
pub fn start(world: &mut World, mob: Entity, preset: Delusion, args: Args) -> bool {
    if client_of(world, mob).is_none() || unconscious(world, mob) {
        return false;
    }
    let (dmi, state) = preset.icon();
    let Some(icon) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(dmi, state))
    else {
        return false;
    };
    let key = {
        let mut started = world.get_resource_or_init::<Started>();
        started.0 += 1;
        format!("delusion {}", started.0)
    };
    let mut shown = Vec::new();
    for target in targets(world, mob, args) {
        let Some(own) = world.get::<Appearance>(target) else {
            continue;
        };
        // `image(file, over_who, state)` with `override`: the whole look replaced, in the
        // mob's layer, on `ABOVE_GAME_PLANE`; still clicked as the mob.
        let look = Appearance {
            icon_state: icon,
            layer: own.layer,
            plane: ABOVE_GAME_PLANE,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: own.mouse_opacity,
            glide: 0,
            overlays: Vec::new(),
        };
        if !add_alt_appearance(world, target, key.clone(), Seers::mob(mob), look) {
            continue;
        }
        shown.push(target);
        if preset == Delusion::Cyborg {
            let at = pos_of(world, target);
            play_local(world, mob, at, LIVE_AGAIN, 75.0, true);
        }
    }
    if args.play_wabbajack {
        tell(world, mob, Span::Italics, "...wabbajack...wabbajack...");
        let at = pos_of(world, mob);
        play_local(world, mob, at, WABBAJACK, 50.0, true);
    }
    if args.duration > 0.0 {
        // `QDEL_IN(src, duration)`: `Destroy()` takes the images back. A deleted target took
        // its alternate appearance with it.
        later(world, args.duration, move |world| {
            for target in shown {
                remove_alt_appearance(world, target, &key);
            }
        });
    }
    true
}

/// `get_delusion_targets()` (BandaStation's split of tg's `start()`).
fn targets(world: &mut World, mob: Entity, args: Args) -> Vec<Entity> {
    let mut out: Vec<Entity> = if args.affects_others {
        let mut humans = world.query_filtered::<Entity, With<HumanLook>>();
        humans.iter(world).filter(|&e| e != mob).collect()
    } else {
        Vec::new()
    };
    if args.affects_us && world.get::<HumanLook>(mob).is_some() {
        out.push(mob);
    }
    if args.skip_nearby {
        // `view(hallucinator)`: `world.view` 7.
        let nearby: Vec<Entity> = (humans_in_view(world, mob, 7).into_iter())
            .map(|(e, _)| e)
            .collect();
        out.retain(|e| !nearby.contains(e));
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use ss13_net::server::AltAppearances;
    use ss13_shared::{IconStateId, Layer};

    use super::*;
    use crate::health::Stat;
    use crate::server::PlayerMob;

    fn look(icon: u32) -> Appearance {
        Appearance {
            icon_state: IconStateId(icon),
            layer: Layer(4.0),
            plane: -4,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: Default::default(),
            glide: 0,
            overlays: Vec::new(),
        }
    }

    fn alts(world: &World, e: Entity) -> Vec<(Vec<Entity>, u32)> {
        world.get::<AltAppearances>(e).map_or(Vec::new(), |a| {
            (a.0.iter())
                .map(|a| (a.seers.mobs.clone(), a.appearance.icon_state.0))
                .collect()
        })
    }

    #[test]
    fn delusions_show_only_to_the_hallucinator_and_end() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let mut icons = IconIndex::default();
        let (dmi, state) = Delusion::Ghost.icon();
        icons.0.insert((dmi.into(), state.into()), IconStateId(9));
        world.insert_resource(icons);
        let human = |world: &mut World, icon: u32| {
            world
                .spawn((HumanLook::random_assistant(1), Stat::Conscious, look(icon)))
                .id()
        };
        let mob = human(&mut world, 1);
        let client = world.spawn_empty().id();
        world.entity_mut(mob).insert(PlayerMob { client });
        let other = human(&mut world, 2);
        let lookless = world.spawn(HumanLook::random_assistant(2)).id();

        // The ghost preset: everyone, the hallucinator too, seen by it alone.
        assert!(start(
            &mut world,
            mob,
            Delusion::Ghost,
            Delusion::Ghost.args()
        ));
        for target in [mob, other] {
            assert_eq!(alts(&world, target), [(vec![mob], 9)]);
        }
        let alt = &world.get::<AltAppearances>(other).unwrap().0[0];
        assert_eq!(
            (alt.appearance.plane, alt.appearance.layer),
            (ABOVE_GAME_PLANE, Layer(4.0))
        );
        assert!(alts(&world, lookless).is_empty());

        // Mass Hallucination's "everyone but us": the second hallucinator's own look stays.
        let mob2 = human(&mut world, 3);
        let client2 = world.spawn_empty().id();
        world.entity_mut(mob2).insert(PlayerMob { client: client2 });
        let others = Args {
            affects_us: false,
            affects_others: true,
            ..Delusion::Ghost.args()
        };
        assert!(start(&mut world, mob2, Delusion::Ghost, others));
        assert!(alts(&world, mob2).is_empty());
        assert_eq!(alts(&world, other), [(vec![mob], 9), (vec![mob2], 9)]);

        // Without a client or the icon nothing starts.
        assert!(!start(&mut world, other, Delusion::Ghost, others));
        assert!(!start(
            &mut world,
            mob,
            Delusion::Carp,
            Delusion::Carp.args()
        ));

        // Both end after their 30 s.
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(31));
        super::super::run_timers(&mut world);
        for target in [mob, other, mob2] {
            assert!(world.get::<AltAppearances>(target).is_none());
        }
    }
}
