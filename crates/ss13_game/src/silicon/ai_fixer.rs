//! The AI system integrity restorer (server), after tg `/obj/machinery/computer/aifixer`
//! (`code/game/machinery/computer/aifixer.dm`) and its `AiRestorer` window.
//!
//! - A loaded intelliCard on a powered restorer installs its AI (tg `transfer_ai`
//!   `AI_TRANS_FROM_CARD`): the AI is inside the terminal (`AiInside::Terminal`, its eye on the
//!   terminal like a carded AI's), its wireless and radio off. An empty card takes the AI back
//!   while no reconstruction runs.
//! - The window (empty hand, or a silicon's `attack_ai`) shows the AI's name, integrity
//!   (`(health + 100) / 2`), whether it is functional, its laws; "Begin Reconstruction" starts
//!   restoring an AI below 100 health.
//! - While restoring and powered, every machine tick (2 s) heals 5 oxygen, burn and brute; a dead
//!   AI at 0 health or more is revived and its radio switched back on; at full health the
//!   restoring stops (tg `Fix`).
//! - Screen overlays: `ai-fixer-on` while restoring, `ai-fixer-empty`, `ai-fixer-404` (dead),
//!   `ai-fixer-full` (conscious).
//! - A deleted restorer kills its AI and ghosts its player (tg `QDEL_NULL(occupier)`).
// shortcut: `Fix` does not draw `active_power_usage` (an unpowered restorer just pauses, as tg
// `process` returns), and the screwdriver lock of an occupied restorer is not there (computers
// here are not deconstructed by screwdriver yet). No NTOS `AI Restorer` app.
// Deviation: tg installs a second AI over the first (`occupier = AI` without a check); here an
// occupied restorer refuses the card.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmName, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::ai::{AiCore, AiInside};
use super::aicard::IntelliCard;
use super::chassis::{name_of, tell_ai};
use super::laws::Laws;
use super::ui::{AiRestorerData, AiRestorerUi, BEGIN_RECONSTRUCTION};
use crate::click::{ClickResolved, ResolveClicks};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::power::{MachineTicks, Unpowered};
use crate::tools::tell;

/// `/obj/machinery/computer/aifixer`.
pub const AI_FIXER: &str = "/obj/machinery/computer/aifixer";
/// `Fix`: `adjust_*_loss(-5)` per machine tick.
pub const FIX_PER_TICK: f32 = 5.0;
const SCREENS: [&str; 4] = [
    "ai-fixer-on",
    "ai-fixer-empty",
    "ai-fixer-404",
    "ai-fixer-full",
];

/// tg `occupier`, `restoring`; made on first use.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiFixer {
    pub occupier: Option<Entity>,
    pub restoring: bool,
}

pub struct AiFixerPlugin;

impl Plugin for AiFixerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AiRestorerUi>()
            .add_systems(
                Update,
                (
                    fixer_clicks.after(ResolveClicks).before(UiSystems::Open),
                    fixer_acts.in_set(UiSystems::Handle),
                    process_fixers,
                    fixer_screens,
                ),
            )
            .add_observer(fixer_removed);
    }
}

/// The restorer (tg type), its state made when first used.
pub fn is_fixer(world: &World, e: Entity) -> bool {
    world.get::<AiFixer>(e).is_some()
        || world
            .get::<DmObj>(e)
            .is_some_and(|o| ss13_mapload::fidelity::is_under(&o.path, AI_FIXER))
}

fn state(world: &mut World, fixer: Entity) -> AiFixer {
    if world.get::<AiFixer>(fixer).is_none() {
        world.entity_mut(fixer).insert(AiFixer::default());
    }
    world.get::<AiFixer>(fixer).copied().unwrap_or_default()
}

/// tg `ui_interact` (also `attack_ai` / `attack_robot`).
pub fn open(world: &mut World, client: Entity, user: Entity, fixer: Entity) {
    if !is_fixer(world, fixer) {
        return;
    }
    state(world, fixer);
    let title = world
        .get::<DmName>(fixer)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<AiRestorerUi>(client, user, fixer, title));
}

/// An empty hand of a non-silicon on the restorer (silicons come through `attack_ai`).
fn fixer_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        if click.modifiers.right
            || !is_fixer(world, click.target)
            || super::interact::has_silicon_access(world, click.mob)
            || world
                .get::<Inventory>(click.mob)
                .is_none_or(|i| i.active_item().is_some())
        {
            continue;
        }
        open(world, click.client, click.mob, click.target);
    }
}

fn exe_number(world: &mut World) -> u32 {
    world
        .get_resource_mut::<crate::combat::CombatRng>()
        .map_or(1000, |mut r| r.range(1000, 9999))
}

/// tg `/obj/machinery/computer/aifixer/transfer_ai`, from `super::aicard` after its flush
/// check: a loaded card (`card_ai`) installs, an empty one takes the occupier.
pub fn transfer(
    world: &mut World,
    client: Entity,
    card: Entity,
    card_ai: Option<Entity>,
    fixer: Entity,
) {
    let fixer_state = state(world, fixer);
    let fixer_name = name_of(world, fixer);
    match card_ai {
        Some(ai) => {
            if world.get::<AiInside>(ai) != Some(&AiInside::Card(card)) {
                return;
            }
            if world.get::<Unpowered>(fixer).is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{fixer_name} is offline and cannot take an AI at this time."),
                );
                return;
            }
            if fixer_state.occupier.is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{fixer_name} already holds an AI."),
                );
                return;
            }
            super::ai::house(world, ai, AiInside::Terminal(fixer));
            if let Some(mut f) = world.get_mut::<AiFixer>(fixer) {
                f.occupier = Some(ai);
            }
            super::aicard::set_wireless(world, ai, false);
            super::aicard::set_radio(world, ai, false);
            if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
                c.ai = None;
            }
            super::aicard::refresh_eye(world, ai, true);
            super::aicard::refresh_look(world, card);
            tell_ai(
                world,
                ai,
                Span::Warning,
                "You have been uploaded to a stationary terminal. Sadly, there is no remote \
                 access from here.",
            );
            let name = name_of(world, ai);
            let n = exe_number(world);
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "Transfer successful: {name} ({n}.exe) installed and executed successfully. \
                     Local copy has been removed."
                ),
            );
            info!("{client} installed the AI {ai} into the restorer {fixer}");
        }
        None => match fixer_state.occupier {
            _ if fixer_state.restoring => tell(
                world,
                client,
                Span::Warning,
                "ERROR: Reconstruction in progress.",
            ),
            None => tell(
                world,
                client,
                Span::Warning,
                "ERROR: Unable to locate artificial intelligence.",
            ),
            Some(ai) => {
                if world.get::<AiInside>(ai) != Some(&AiInside::Terminal(fixer)) {
                    return;
                }
                tell_ai(
                    world,
                    ai,
                    Span::Notice,
                    "You have been downloaded to a mobile storage device. Still no remote access.",
                );
                let name = name_of(world, ai);
                let n = exe_number(world);
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!(
                        "Transfer successful: {name} ({n}.exe) removed from host terminal and \
                         stored within local memory."
                    ),
                );
                super::ai::house(world, ai, AiInside::Card(card));
                if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
                    c.ai = Some(ai);
                }
                if let Some(mut f) = world.get_mut::<AiFixer>(fixer) {
                    f.occupier = None;
                }
                super::aicard::refresh_eye(world, ai, true);
                super::aicard::refresh_look(world, card);
                info!("{client} took the AI {ai} out of the restorer {fixer}");
            }
        },
    }
}

/// The occupier while it is still inside.
fn occupier(world: &World, fixer: Entity) -> Option<Entity> {
    let ai = world.get::<AiFixer>(fixer)?.occupier?;
    (world.get::<AiInside>(ai) == Some(&AiInside::Terminal(fixer))).then_some(ai)
}

impl ServerUi for AiRestorerUi {
    type Source = &'static AiFixer;
    type UserParam = (
        Query<
            'static,
            'static,
            (
                &'static Health,
                &'static Stat,
                Option<&'static Laws>,
                Option<&'static Name>,
            ),
        >,
        Query<'static, 'static, &'static AiInside>,
    );
    type Act = ();

    fn static_data(_: &AiFixer) {}

    fn data(
        fixer: &AiFixer,
        _: Entity,
        (ais, inside): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> AiRestorerData {
        let present = fixer
            .occupier
            .filter(|&ai| matches!(inside.get(ai), Ok(AiInside::Terminal(_))))
            .and_then(|ai| ais.get(ai).ok());
        let Some((health, stat, laws, name)) = present else {
            return AiRestorerData {
                error: Some("Please transfer an AI unit.".to_owned()),
                ..default()
            };
        };
        AiRestorerData {
            ai_present: true,
            error: None,
            name: name.map(|n| n.as_str().to_owned()).unwrap_or_default(),
            restoring: fixer.restoring,
            health: (health.health() + 100.0) / 2.0,
            is_dead: *stat == Stat::Dead,
            laws: laws.map(|l| l.law_list(true)).unwrap_or_default(),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
        if action == BEGIN_RECONSTRUCTION {
            Ok(())
        } else {
            Err(UiReject::UnknownAction)
        }
    }
}

/// tg `ui_act("PRG_beginReconstruction")`: an occupier below 100 health starts restoring.
pub fn begin_reconstruction(world: &mut World, client: Entity, fixer: Entity) {
    let Some(ai) = occupier(world, fixer) else {
        if let Some(mut f) = world.get_mut::<AiFixer>(fixer) {
            f.restoring = false;
        }
        return;
    };
    if world.get::<Health>(ai).is_none_or(|h| h.health() >= h.max) {
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        "Reconstruction in progress. This will take several minutes.",
    );
    if let Some(&pos) = world.get::<TilePos>(fixer) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(
                pos,
                "sound/machines/terminal/terminal_prompt_confirm.ogg",
                25.0,
                false,
            ),
        );
    }
    if let Some(mut f) = world.get_mut::<AiFixer>(fixer) {
        f.restoring = true;
    }
    // tg `notify_revival`: the AI's player stays on its eye here.
    tell_ai(
        world,
        ai,
        Span::Notice,
        "Your core files are being restored!",
    );
}

fn fixer_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<AiRestorerUi>>>) {
    let acts: Vec<(Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AiRestorerUi>>>())
        .map(|a| (a.client, a.source))
        .collect();
    for (client, fixer) in acts {
        begin_reconstruction(world, client, fixer);
    }
}

/// tg `Fix`: one machine tick of restoring; whether it goes on (health still below 100).
pub fn fix(world: &mut World, ai: Entity) -> bool {
    let (now, max) = {
        let Some(mut health) = world.get_mut::<Health>(ai) else {
            return false;
        };
        for kind in [DamageType::Oxy, DamageType::Burn, DamageType::Brute] {
            health.adjust(kind, -FIX_PER_TICK);
        }
        (health.health(), health.max)
    };
    if now >= 0.0 && world.get::<Stat>(ai) == Some(&Stat::Dead) {
        // tg `revive()`: silicons have no crit, so conscious.
        world.entity_mut(ai).insert(Stat::Conscious);
        if !super::aicard::radio_on(world, ai) {
            super::aicard::set_radio(world, ai, true);
            tell_ai(
                world,
                ai,
                Span::Warning,
                "Your Subspace Transceiver has been enabled!",
            );
        }
        info!("the AI {ai} was restored");
    }
    now < max
}

/// tg `process`: a powered restorer restores its occupier every machine tick.
// Cost: the few restorers of a map, per machine tick (2 s).
fn process_fixers(world: &mut World, mut last: Local<Option<u64>>) {
    let Some(ticks) = world.get_resource::<MachineTicks>().map(|t| t.0) else {
        return;
    };
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let mut q = world.query_filtered::<(Entity, &AiFixer), Without<Unpowered>>();
        let restoring: Vec<Entity> = q
            .iter(world)
            .filter(|(_, f)| f.restoring)
            .map(|(e, _)| e)
            .collect();
        for fixer in restoring {
            let going = occupier(world, fixer).is_some_and(|ai| fix(world, ai));
            if let Some(mut f) = world.get_mut::<AiFixer>(fixer) {
                f.restoring = going;
            }
        }
    }
}

/// The screen overlays tg `update_overlays` wants.
pub fn screens(powered: bool, restoring: bool, occupier: Option<Stat>) -> Vec<&'static str> {
    if !powered {
        return Vec::new();
    }
    let mut out = Vec::new();
    if restoring {
        out.push("ai-fixer-on");
    }
    match occupier {
        None => out.push("ai-fixer-empty"),
        // tg `IS_UNCONSCIOUS`: a dead mob is knocked out too.
        Some(Stat::Conscious) => out.push("ai-fixer-full"),
        Some(_) => out.push("ai-fixer-404"),
    }
    out
}

fn fixer_screens(world: &mut World) {
    let mut q = world.query::<(Entity, &AiFixer, &DmAtom, Has<Unpowered>)>();
    let fixers: Vec<(Entity, AiFixer, String, bool)> = q
        .iter(world)
        .map(|(e, f, a, unpowered)| (e, *f, a.icon.clone(), !unpowered))
        .collect();
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let mut updates = Vec::new();
    for (fixer, state, icon, powered) in fixers {
        let stat = occupier(world, fixer)
            .map(|ai| world.get::<Stat>(ai).copied().unwrap_or(Stat::Conscious));
        let ours: Vec<_> = SCREENS.iter().filter_map(|s| icons.get(&icon, s)).collect();
        let wanted: Vec<_> = screens(powered, state.restoring, stat)
            .into_iter()
            .filter_map(|s| icons.get(&icon, s))
            .collect();
        updates.push((fixer, ours, wanted));
    }
    for (fixer, ours, wanted) in updates {
        let Some(mut look) = world.get_mut::<Appearance>(fixer) else {
            continue;
        };
        let mut overlays = look.overlays.clone();
        overlays.retain(|o| !ours.contains(&o.icon_state));
        overlays.extend(wanted.into_iter().map(Overlay::new));
        if look.overlays != overlays {
            look.overlays = overlays;
        }
    }
}

/// tg `Destroy` / `on_deconstruction`: the occupier is deleted with the terminal.
fn fixer_removed(remove: On<Remove<AiFixer>>, fixers: Query<&AiFixer>, mut commands: Commands) {
    let Some(ai) = fixers.get(remove.entity).ok().and_then(|f| f.occupier) else {
        return;
    };
    let fixer = remove.entity;
    commands.queue(move |world: &mut World| {
        if world.get::<AiInside>(ai) != Some(&AiInside::Terminal(fixer)) {
            return;
        }
        if let Some(mut stat) = world.get_mut::<Stat>(ai) {
            *stat = Stat::Dead;
        }
        let client = world
            .get::<AiCore>(ai)
            .and_then(|c| world.get::<crate::server::PlayerMob>(c.eye))
            .map(|p| p.client);
        if let Some(client) = client {
            crate::ghost::ghostize_client(world, client);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_follow_tg() {
        assert!(screens(false, true, Some(Stat::Dead)).is_empty());
        assert_eq!(screens(true, false, None), ["ai-fixer-empty"]);
        assert_eq!(
            screens(true, true, Some(Stat::Dead)),
            ["ai-fixer-on", "ai-fixer-404"]
        );
        assert_eq!(
            screens(true, false, Some(Stat::Conscious)),
            ["ai-fixer-full"]
        );
    }

    /// A loaded card installs its AI, not into an unpowered restorer; an empty card takes it
    /// back, not while restoring.
    #[test]
    fn cards_install_and_take_the_ai() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let client = world.spawn_empty().id();
        let card = world
            .spawn((
                ss13_mapload::DmItem {
                    path: super::super::aicard::AICARD.into(),
                    ..default()
                },
                IntelliCard::default(),
            ))
            .id();
        let fixer = world.spawn((AiFixer::default(), Unpowered)).id();
        let ai = world
            .spawn((Health::default(), Stat::Dead, AiInside::Card(card)))
            .id();
        world.get_mut::<IntelliCard>(card).unwrap().ai = Some(ai);
        transfer(&mut world, client, card, Some(ai), fixer);
        assert_eq!(world.get::<AiInside>(ai), Some(&AiInside::Card(card)));
        world.entity_mut(fixer).remove::<Unpowered>();
        transfer(&mut world, client, card, Some(ai), fixer);
        assert_eq!(world.get::<AiInside>(ai), Some(&AiInside::Terminal(fixer)));
        assert_eq!(world.get::<AiFixer>(fixer).unwrap().occupier, Some(ai));
        assert_eq!(world.get::<IntelliCard>(card).unwrap().ai, None);
        world.get_mut::<AiFixer>(fixer).unwrap().restoring = true;
        transfer(&mut world, client, card, None, fixer);
        assert_eq!(world.get::<AiInside>(ai), Some(&AiInside::Terminal(fixer)));
        world.get_mut::<AiFixer>(fixer).unwrap().restoring = false;
        transfer(&mut world, client, card, None, fixer);
        assert_eq!(world.get::<AiInside>(ai), Some(&AiInside::Card(card)));
        assert_eq!(world.get::<IntelliCard>(card).unwrap().ai, Some(ai));
        assert_eq!(world.get::<AiFixer>(fixer).unwrap().occupier, None);
    }

    /// A dead AI in the restorer: 5 of each damage healed per tick, revived at 0 health, the
    /// restoring stops at full health.
    #[test]
    fn fix_heals_revives_and_stops_at_full_health() {
        let mut world = World::new();
        let fixer = world.spawn(AiFixer::default()).id();
        let mut health = Health::default();
        health.adjust(DamageType::Brute, 120.0);
        health.adjust(DamageType::Oxy, 90.0);
        let ai = world
            .spawn((health, Stat::Dead, AiInside::Terminal(fixer)))
            .id();
        world.entity_mut(fixer).insert(AiFixer {
            occupier: Some(ai),
            restoring: true,
        });
        assert_eq!(occupier(&world, fixer), Some(ai));
        let before = world.get::<Health>(ai).unwrap().health();
        assert!(before < -100.0, "{before}");
        // One tick heals 10 (oxy 5, brute 5; no burn to heal).
        assert!(fix(&mut world, ai));
        let h = world.get::<Health>(ai).unwrap().health();
        assert!((h - before - 10.0).abs() < 0.01, "{before} -> {h}");
        assert_eq!(world.get::<Stat>(ai), Some(&Stat::Dead));
        let mut ticks = 1;
        while fix(&mut world, ai) {
            ticks += 1;
            if world.get::<Health>(ai).unwrap().health() >= 0.0 {
                assert_eq!(world.get::<Stat>(ai), Some(&Stat::Conscious));
            }
            assert!(ticks < 100);
        }
        assert_eq!(world.get::<Health>(ai).unwrap().health(), 100.0);
        assert_eq!(world.get::<Stat>(ai), Some(&Stat::Conscious));
    }
}
