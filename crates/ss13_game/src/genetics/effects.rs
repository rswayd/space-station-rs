//! What the mutations beyond the starter set do: the Hulk (`hulk.dm`), the power actions
//! (`/datum/mutation::power_path`: the conjuring kineses of `cold.dm`/`hot.dm`), the speech
//! mutations (`speech.dm`, the text in `ss13_chat::accents`), the monkey (`/datum/mutation/race`)
//! and the mood auras (`aoe_moodlet.dm`).
// shortcut: of the powers the conjuring kineses, the cryo/pyro beams (pointed, aimed like the
// pointed spells: the button arms a `ClickIntercept`) and fire breath; telekinesis,
// telepathy, laser eyes, webbing and the rest come with the systems they need. No chromosome
// energy or power coefficients.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::accents::{Accent, Accents};
use ss13_net::server::IntentReceived;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::server::UiActed;
use ss13_world::SpatialIndex;

use super::{Dna, Effect, GeneticsRng, MUTATIONS, SOURCE_ACTIVATED, STANDARD_SOURCES, tell};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::combat::CombatMode;
use crate::health::{DamageType, HEALTH_THRESHOLD_CRIT, Health, Stat, apply_damage_zone};
use crate::inventory::Inventory;
use crate::mood::{ActiveMood, MoodEvent};
use crate::server::PlayerMob;
use crate::species::SpeciesId;
use crate::zone::BodyZone;

/// The `TRAIT_STUNIMMUNE` source of the Hulk.
const HULK_SOURCE: &str = "hulk";
/// `hulk/scream_delay`.
const SCREAM_DELAY: f64 = 5.0;
/// `CLICK_CD_MELEE`.
const CLICK_CD_MELEE: f64 = 0.8;
/// `/datum/mood_event/hulk`.
const HULK_MOOD: MoodEvent = MoodEvent {
    id: "hulk",
    description: "ХАЛК ЛОМАТЬ!",
    mood_change: -4.0,
    timeout: 0.0,
    hidden: false,
};
const PUNCH: &str = "sound/items/weapons/punch1.ogg";
const METEOR_IMPACT: &str = "sound/effects/meteorimpact.ogg";
const BANG: &str = "sound/effects/bang.ogg";
/// What the conjuring powers make (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
pub const SNOW: &str = "/obj/item/stack/sheet/mineral/snow";
pub const ASH: &str = "/obj/effect/decal/cleanable/ash";
/// The `trigger` id prefix of the power buttons.
const BUTTON: &str = "mutation:";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_genetic.dmi";

/// A mutation's `power_path` action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    /// `/datum/action/cooldown/spell/conjure_item/snow`.
    Geladikinesis,
    /// `/datum/action/cooldown/spell/conjure_item/ash`.
    Cindikinesis,
    /// `/datum/action/cooldown/spell/pointed/projectile/cryo`.
    Cryokinesis,
    /// `/datum/action/cooldown/spell/pointed/projectile/pyro`.
    Pyrokinesis,
    /// `/datum/action/cooldown/spell/cone/staggered/fire_breath`.
    FireBreath,
}

impl Power {
    fn name(self) -> &'static str {
        match self {
            Self::Geladikinesis => "Create Snow",
            Self::Cindikinesis => "Create Ash",
            Self::Cryokinesis => "Cryobeam",
            Self::Pyrokinesis => "Pyrobeam",
            Self::FireBreath => "Fire Breath",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Geladikinesis => "snow",
            Self::Cindikinesis => "ash",
            Self::Cryokinesis => "icebeam",
            Self::Pyrokinesis => "firebeam",
            Self::FireBreath => "fireball0",
        }
    }

    /// `cooldown_time`.
    pub fn cooldown(self) -> f64 {
        match self {
            Self::Geladikinesis | Self::Cindikinesis => 5.0,
            Self::Cryokinesis => 16.0,
            Self::Pyrokinesis => 30.0,
            Self::FireBreath => 40.0,
        }
    }

    /// A pointed spell (`cast_range`): the button arms it, the next click casts.
    pub fn cast_range(self) -> Option<u32> {
        matches!(self, Self::Cryokinesis | Self::Pyrokinesis).then_some(9)
    }

    /// `/obj/projectile/temp/<kind>::temperature` of a beam.
    fn beam(self) -> Option<(&'static str, f64)> {
        match self {
            Self::Cryokinesis => Some((CRYO_BEAM, -350.0)),
            Self::Pyrokinesis => Some((PYRO_BEAM, 350.0)),
            _ => None,
        }
    }
}

/// The beams (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
pub const CRYO_BEAM: &str = "/obj/projectile/temp/cryo";
pub const PYRO_BEAM: &str = "/obj/projectile/temp/pyro";
/// `fire_breath::sound`.
const DEMON_DIES: &str = "sound/effects/magic/demon_dies.ogg";

/// The pointed power `mob` aims (tg `is_action_active` of a pointed spell).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerAim(pub Power);

/// On a flying beam: its `temperature` (read by `crate::projectile` on hit, [`beam_hit`]).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TempBeam {
    pub temperature: f64,
}

/// When a body's Hulk last screamed (`last_scream`) and the original species and name of a
/// monkified body (`race/original_species`, `original_name`).
#[derive(Component, Clone, Debug, Default)]
pub struct MutationState {
    pub last_scream: f64,
    pub original: Option<(SpeciesId, String)>,
}

/// `on_acquiring` checks of the mutation itself (`species_allowed`, `health_req`).
pub fn can_acquire(world: &World, mob: Entity, effect: Effect) -> bool {
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return false;
    }
    match effect {
        // `species_allowed = list(SPECIES_HUMAN)`, `health_req = 25`.
        Effect::Hulk => {
            crate::species::species_of(world, mob) == SpeciesId::Human
                && world.get::<Health>(mob).is_none_or(|h| h.health() >= 25.0)
        }
        _ => true,
    }
}

use crate::tools::now;

/// The effect's own part of `on_acquiring`.
pub fn on_gain(world: &mut World, mob: Entity, effect: Effect) {
    match effect {
        Effect::Hulk => {
            crate::status_effects::add_stun_immunity(world, mob, HULK_SOURCE);
            set_hulk_look(world, mob, true);
            crate::mood::add_event(world, mob, "hulk", &HULK_MOOD);
            set_accent(world, mob, Accent::Hulk, true);
        }
        // `heckacious` and the speechmods (`ss13_chat::accents`).
        Effect::Accent(accent) => set_accent(world, mob, accent, true),
        Effect::Unintelligible => crate::medical::traumas::refresh(world, mob),
        Effect::Power(_) => refresh_buttons(world, mob),
        Effect::Race => monkeyize(world, mob),
        Effect::Stoner => set_stoner(world, mob, true),
        _ => {}
    }
}

/// `stoner`: Beach Bum granted and every other language blocked (`LANGUAGE_STONER`), or both
/// taken back.
fn set_stoner(world: &mut World, mob: Entity, on: bool) {
    use ss13_chat::language::{ALL, LanguageId, LanguageSource, edit_holder};
    edit_holder(world, mob, |holder| {
        if on {
            holder.grant(LanguageId::Beachbum, ALL, LanguageSource::Stoner);
        } else {
            holder.remove(LanguageId::Beachbum, ALL, Some(LanguageSource::Stoner));
        }
        for language in LanguageId::ALL {
            if language == LanguageId::Beachbum {
                continue;
            }
            if on {
                holder.block(language, ALL, LanguageSource::Stoner);
            } else {
                holder.unblock(language, ALL, Some(LanguageSource::Stoner));
            }
        }
    });
}

/// The effect's own part of `on_losing`.
pub fn on_lose(world: &mut World, mob: Entity, effect: Effect) {
    match effect {
        Effect::Hulk => {
            crate::status_effects::remove_stun_immunity(world, mob, HULK_SOURCE);
            set_hulk_look(world, mob, false);
            crate::mood::clear_event(world, mob, "hulk");
            set_accent(world, mob, Accent::Hulk, false);
        }
        Effect::Accent(accent) => set_accent(world, mob, accent, false),
        Effect::Unintelligible => crate::medical::traumas::refresh(world, mob),
        // An armed pointed power disarms with its button gone.
        Effect::Power(power) if world.get::<PowerAim>(mob) == Some(&PowerAim(power)) => {
            set_aim(world, mob, None);
        }
        Effect::Power(_) => refresh_buttons(world, mob),
        Effect::Race => humanize(world, mob),
        Effect::Stoner => set_stoner(world, mob, false),
        _ => {}
    }
}

fn set_accent(world: &mut World, mob: Entity, accent: Accent, on: bool) {
    let mut accents = world.get::<Accents>(mob).cloned().unwrap_or_default();
    accents.0.retain(|&a| a != accent);
    if on {
        accents.0.push(accent);
    }
    let mut entity = world.entity_mut(mob);
    if accents.0.is_empty() {
        entity.remove::<Accents>();
    } else {
        entity.insert(accents);
    }
}

/// `add_color_override(COLOR_DARK_LIME, LIMB_COLOR_HULK)` on the limbs, `update_body_parts`.
fn set_hulk_look(world: &mut World, mob: Entity, hulk: bool) {
    if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob)
        && look.hulk != hulk
    {
        look.hulk = hulk;
    }
    crate::inventory::refresh_look(world, mob);
}

/// Whether `mob` is a Hulk (`TRAIT_HULK`, `TRAIT_CHUNKYFINGERS`).
pub fn is_hulk(world: &World, mob: Entity) -> bool {
    super::has_effect(world, mob, Effect::Hulk)
}

/// `can_use_guns` with `TRAIT_CHUNKYFINGERS`: true (and the alert) when the fingers are too
/// big for a normal trigger guard.
// shortcut: every gun has a normal trigger guard here.
pub fn chunky_fingers(world: &mut World, mob: Entity) -> bool {
    if !is_hulk(world, mob) {
        return false;
    }
    crate::balloon::balloon_alert(world, mob, mob, "слишком большие пальцы!");
    true
}

/// Every `SSmobs` fire (`on_life`), from `super::mutations_life`.
pub fn on_life(world: &mut World, mob: Entity, effect: Effect, secs: f32) {
    let rng = &mut world.get_resource_or_init::<GeneticsRng>().0;
    match effect {
        // `hulk/on_life`: below the crit threshold the mutation goes.
        Effect::Hulk => {
            if world
                .get::<Health>(mob)
                .is_some_and(|h| h.health() < HEALTH_THRESHOLD_CRIT)
            {
                let id = super::mutation_id("/datum/mutation/hulk").expect("hulk");
                super::remove_mutation(world, mob, id, STANDARD_SOURCES);
                tell(
                    world,
                    mob,
                    Span::Danger,
                    "Вы вдруг чувствуете сильную слабость.",
                );
            }
        }
        // `nervousness/on_life`: `SPT_PROB(5)`, `set_stutter_if_lower(20 SECONDS)`.
        Effect::Nervousness if super::spt_prob(rng, 5.0, secs) => {
            crate::status_effects::set_timed(
                world,
                mob,
                crate::status_effects::Timed::Stutter,
                20.0,
                true,
            );
        }
        // `elvis/on_life`: one of two dance lines, `SPT_PROB(7.5)`.
        Effect::Accent(Accent::Elvis) if super::spt_prob(rng, 7.5, secs) => {
            const DANCES: [&str; 10] = [
                "swinging",
                "fancy",
                "stylish",
                "20'th century",
                "jivin'",
                "rock and roller",
                "cool",
                "salacious",
                "bashing",
                "smashing",
            ];
            const MOVES: [&str; 7] = [
                "jiggles their hips",
                "rotates their hips",
                "gyrates their hips",
                "taps their foot",
                "dances to an imaginary song",
                "jiggles their legs",
                "snaps their fingers",
            ];
            let first = rng.prob(50.0);
            let line = if first {
                let dance = DANCES[rng.range(0, DANCES.len() as i64 - 1) as usize];
                format!("busts out some {dance} moves!")
            } else {
                format!("{}!", MOVES[rng.range(0, MOVES.len() as i64 - 1) as usize])
            };
            let name = crate::medical::name_of(world, mob);
            let text = format!("{name} {line}");
            crate::chem::visible(world, mob, &text, &text, Span::Emote);
        }
        Effect::Aura { positive } => aura(world, mob, positive),
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Hulk

/// `hulk/on_attack_hand` (`COMSIG_LIVING_EARLY_UNARMED_ATTACK`): an empty-handed click in combat
/// mode on a living mob or a wall, before melee.
pub fn hulk_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::ClickResolved>>,
) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if click.modifiers.right
            || mob == target
            || world
                .resource::<crate::security::MeleeHandled>()
                .0
                .contains(&mob)
            || !is_hulk(world, mob)
        {
            continue;
        }
        let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
        let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
        if !combat || held.is_some() || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        let now = now(world);
        if world
            .get::<crate::combat::NextMove>(mob)
            .is_some_and(|n| now < n.0)
        {
            continue;
        }
        let handled = if world.get::<Health>(target).is_some() {
            hulk_mob(world, mob, target)
        } else if let Some(build) = world.get::<ss13_mapload::DmTurfBuild>(target)
            && build.path.starts_with("/turf/closed/wall")
        {
            let path = build.path.clone();
            hulk_wall(world, mob, target, click.target_pos, &path);
            true
        } else {
            false
        };
        if !handled {
            continue;
        }
        world
            .resource_mut::<crate::security::MeleeHandled>()
            .0
            .insert(mob);
        // tg `ClickOn` -> `face_atom`.
        let (dx, dy) = (
            (click.target_pos.x - click.mob_pos.x).signum(),
            (click.target_pos.y - click.mob_pos.y).signum(),
        );
        if let Some(dir) = Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy)) {
            world.entity_mut(mob).insert(Facing(dir));
        }
        crate::animation::do_attack_animation(world, mob, target);
        if let Some(mut next) = world.get_mut::<crate::combat::NextMove>(mob) {
            next.0 = now + CLICK_CD_MELEE;
        }
        // `scream_attack`: "WAAAAAAAAAAAAAAGH!" at most every `scream_delay`.
        let state = world.get::<MutationState>(mob).cloned().unwrap_or_default();
        if now > state.last_scream + SCREAM_DELAY {
            world.entity_mut(mob).insert(MutationState {
                last_scream: now,
                ..state
            });
            world.write_message(IntentReceived {
                client: click.client,
                intent: ClientIntent::Say("WAAAAAAAAAAAAAAGH!".into()),
            });
        }
    }
}

/// `/mob/living/carbon/human/attack_hulk`: 15 brute; a pacifist refuses (`living/attack_hulk`).
fn hulk_mob(world: &mut World, hulk: Entity, target: Entity) -> bool {
    let victim = crate::medical::name_of(world, target);
    if world
        .get::<crate::medical::traumas::Pacifist>(hulk)
        .is_some()
    {
        tell(
            world,
            hulk,
            Span::Warning,
            &format!("Вы не хотите причинить вред {victim}!"),
        );
        return true;
    }
    let verb = if world.get_resource_or_init::<GeneticsRng>().0.prob(50.0) {
        "крушит"
    } else {
        "избивает"
    };
    let user = crate::medical::name_of(world, hulk);
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, PUNCH, 25.0, true).extrarange(-1.0),
        );
    }
    crate::chem::visible(
        world,
        hulk,
        &format!("Вы {verb}е {victim}!"),
        &format!("{user} {verb} {victim}!"),
        Span::Danger,
    );
    // `apply_damage(15, BRUTE)` without a zone: the chest.
    if let Ok(mut e) = world.get_entity_mut(target)
        && let Ok((mut health, mut stat)) = e.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        let mut new = *stat;
        apply_damage_zone(
            &mut health,
            &mut new,
            BodyZone::Chest,
            DamageType::Brute,
            15.0,
        );
        stat.set_if_neq(new);
    }
    true
}

/// `/turf/closed/wall::hardness` (reinforced walls 10).
// shortcut: by path, not the dumped var (mineral walls' own hardness is not read).
pub fn wall_hardness(path: &str) -> f32 {
    if path.starts_with("/turf/closed/wall/r_wall") {
        10.0
    } else {
        40.0
    }
}

/// `/turf/closed/wall/attack_hulk`: `prob(hardness)` the wall comes down (no recoil for the
/// standard Hulk), else a dent and a bang.
fn hulk_wall(world: &mut World, hulk: Entity, wall: Entity, pos: TilePos, path: &str) {
    let smash = world
        .get_resource_or_init::<GeneticsRng>()
        .0
        .prob(wall_hardness(path));
    if smash {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, METEOR_IMPACT, 100.0, true),
        );
        crate::construction::dismantle_wall(world, wall, pos);
        return;
    }
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, BANG, 50.0, true));
    let user = crate::medical::name_of(world, hulk);
    crate::chem::visible(
        world,
        hulk,
        "Вы крушите стену!",
        &format!("{user} крушит стену!"),
        Span::Danger,
    );
}

// ---------------------------------------------------------------------------------------------
// Powers

/// The power of each of `mob`'s mutations.
fn powers(world: &World, mob: Entity) -> Vec<Power> {
    world
        .get::<Dna>(mob)
        .map(|d| {
            d.mutations
                .iter()
                .filter_map(|m| match MUTATIONS[m.id].effect {
                    Effect::Power(p) => Some(p),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn button_id(power: Power) -> String {
    format!("{BUTTON}{power:?}")
}

/// `grant_power` / the action's `Remove`: the mob's power buttons after its others.
fn refresh_buttons(world: &mut World, mob: Entity) {
    let old: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| b.0.clone())
        .unwrap_or_default();
    let mut buttons: Vec<(ActionButton, f64)> = old
        .iter()
        .filter(|(b, _)| !b.id.starts_with(BUTTON))
        .cloned()
        .collect();
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<ss13_mapload::IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let aiming = world.get::<PowerAim>(mob).map(|a| a.0);
    for power in powers(world, mob) {
        let id = button_id(power);
        let active = aiming == Some(power);
        let ready_at = old
            .iter()
            .find(|(b, _)| b.id == id)
            .map_or(0.0, |(_, r)| *r);
        buttons.push((
            ActionButton {
                id,
                name: power.name().to_owned(),
                icon: icon(world, ACTIONS_DMI, power.icon()),
                background: icon(world, "icons/mob/actions/backgrounds.dmi", "bg_spell"),
                border: icon(
                    world,
                    "icons/mob/actions/backgrounds.dmi",
                    if active {
                        "bg_spell_border_active_blue"
                    } else {
                        "bg_spell_border"
                    },
                ),
                available: true,
                cooldown: 0.0,
                active,
                maptext: None,
            },
            ready_at,
        ));
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a power's button (`Trigger` -> `cast`), checked at the moment of effect: the
/// client controls the mob, the mob still has the mutation, the cooldown is over and the mob is
/// conscious (`spell_requirements = NONE`).
pub fn trigger_powers(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        let Some(power) = powers(world, actor)
            .into_iter()
            .find(|&p| button_id(p) == id)
        else {
            continue;
        };
        let now = now(world);
        let ready = world
            .get::<ActionButtons>(actor)
            .and_then(|b| b.0.iter().find(|(b, _)| b.id == id).map(|(_, r)| *r))
            .unwrap_or(0.0);
        if now < ready || world.get::<Stat>(actor) != Some(&Stat::Conscious) {
            continue;
        }
        // A pointed power arms (or, armed, disarms) instead (`pointed/Trigger`).
        if power.cast_range().is_some() {
            let armed = world.get::<PowerAim>(actor) == Some(&PowerAim(power));
            set_aim(world, actor, (!armed).then_some(power));
            continue;
        }
        cast(world, actor, power);
        start_cooldown(world, actor, power);
    }
}

fn start_cooldown(world: &mut World, mob: Entity, power: Power) {
    let (now, id) = (now(world), button_id(power));
    if let Some(mut buttons) = world.get_mut::<ActionButtons>(mob)
        && let Some(entry) = buttons.0.iter_mut().find(|(b, _)| b.id == id)
    {
        entry.1 = now + power.cooldown();
    }
}

/// `set_click_ability` / `unset_click_ability` of a pointed power, with `active_msg` and
/// `deactive_msg`.
fn set_aim(world: &mut World, mob: Entity, power: Option<Power>) {
    let spell_aiming = world
        .get::<crate::spells::Spells>(mob)
        .is_some_and(|s| s.aiming.is_some());
    match power {
        Some(power) => {
            let msg = match power {
                Power::Cryokinesis => "You focus your cryokinesis!",
                _ => "You focus your pyrokinesis!",
            };
            tell(world, mob, Span::Notice, msg);
            world
                .entity_mut(mob)
                .insert((PowerAim(power), crate::click::ClickIntercept));
        }
        None => {
            if world.get::<PowerAim>(mob).is_some() {
                tell(world, mob, Span::Notice, "You relax.");
            }
            let mut entity = world.entity_mut(mob);
            entity.remove::<PowerAim>();
            // A pointed spell aimed at the same time keeps the intercept.
            if !spell_aiming {
                entity.remove::<crate::click::ClickIntercept>();
            }
        }
    }
    refresh_buttons(world, mob);
}

/// The click of a mob aiming a pointed power (`InterceptClickOn` -> `cast`): in range, the
/// power still there and off cooldown, the caster conscious; the beam flies and the aim ends.
pub fn pointed_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::InterceptedClick>>,
) {
    let clicks: Vec<crate::click::InterceptedClick> = clicks
        .read(world.resource::<Messages<crate::click::InterceptedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(PowerAim(power)) = world.get::<PowerAim>(click.mob).copied() else {
            continue;
        };
        if world.get::<PlayerMob>(click.mob).map(|p| p.client) != Some(click.client) {
            continue;
        }
        if !powers(world, click.mob).contains(&power) {
            set_aim(world, click.mob, None);
            continue;
        }
        let id = button_id(power);
        let ready = world
            .get::<ActionButtons>(click.mob)
            .and_then(|b| b.0.iter().find(|(b, _)| b.id == id).map(|(_, r)| *r))
            .unwrap_or(0.0);
        if now(world) < ready || world.get::<Stat>(click.mob) != Some(&Stat::Conscious) {
            continue;
        }
        let range = power.cast_range().unwrap_or(0);
        if click
            .mob_pos
            .chebyshev(click.target_pos)
            .is_none_or(|d| d > range)
        {
            tell(world, click.mob, Span::Warning, "too far away!");
            continue;
        }
        let target = (click.target != click.mob).then_some(click.target);
        fire_beam(world, click.mob, power, target, click.target_pos);
        start_cooldown(world, click.mob, power);
        set_aim(world, click.mob, None);
    }
}

/// `pointed/projectile/cast` -> `fire_projectile`: the beam at `at`.
pub fn fire_beam(
    world: &mut World,
    mob: Entity,
    power: Power,
    target: Option<Entity>,
    at: TilePos,
) -> Option<Entity> {
    let (path, temperature) = power.beam()?;
    let from = *world.get::<TilePos>(mob)?;
    // `/obj/projectile/temp`: no damage, burn against energy armour; `range = 9`.
    let data = ss13_mapload::ProjectileData {
        path: path.to_owned(),
        damage: 0.0,
        damage_type: "burn".to_owned(),
        armor_flag: "energy".to_owned(),
        speed: 1.25,
        range: 9.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    };
    crate::projectile::fire_with(
        world,
        crate::projectile::Shot {
            data: &data,
            firer: mob,
            from,
            target,
            target_pos: at,
            zone: BodyZone::Chest,
            hit_prone: false,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: None,
        },
        TempBeam { temperature },
    )
}

/// `/obj/projectile/temp/on_hit` on a carbon: `adjust_bodytemperature((thermal_protection *
/// temperature) + temperature)`, `thermal_protection = 1 - insulation`.
// shortcut: worn insulation is not read (no `get_insulation_protection`): an unprotected body;
// no `freezing_blast` slowdown, frozen turfs or `fire_act` on objects.
pub fn beam_hit(world: &mut World, projectile: Entity, target: Entity) {
    let Some(beam) = world.get::<TempBeam>(projectile).copied() else {
        return;
    };
    if let Some(mut body) = world.get_mut::<crate::breath::Respiration>(target) {
        body.bodytemperature = (body.bodytemperature + 2.0 * beam.temperature).max(0.0);
    }
}

/// `conjure_item/cast`: the active hand's item to the floor, the conjured thing into the hands
/// (an effect onto the floor).
pub fn cast(world: &mut World, mob: Entity, power: Power) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    match power {
        Power::Geladikinesis => {
            if let Some(held) = world.get::<Inventory>(mob).and_then(Inventory::active_item) {
                crate::inventory::put_down(world, held, pos);
            }
            if let Some(snow) = crate::construction::spawn_stack(world, SNOW, pos, 1)
                && let Some(inventory) = world.get::<Inventory>(mob).cloned()
            {
                crate::storage::put_in_hands(world, mob, &inventory, snow);
            }
        }
        Power::Cindikinesis => {
            crate::construction::spawn_obj(world, ASH, pos);
        }
        Power::FireBreath => fire_breath(world, mob, pos),
        Power::Cryokinesis | Power::Pyrokinesis => {}
    }
}

/// The tiles of fire breath's cone: `cone_levels` 3 ahead of `pos` facing `dir`, 3 wide
/// (`calculate_cone_shape`), with the level of each.
pub fn breath_cone(pos: TilePos, dir: Dir) -> Vec<(TilePos, i32)> {
    let (dx, dy) = dir.offset();
    let mut tiles = Vec::new();
    for level in 1..=3 {
        for side in -1..=1 {
            let (sx, sy) = (dy * side, dx * side);
            tiles.push((
                TilePos::new(pos.x + dx * level + sx, pos.y + dy * level + sy, pos.z),
                level,
            ));
        }
    }
    tiles
}

/// `fire_breath`: a covered mouth sets the breather alight; each cone turf gets a hotspot, each
/// mob there `max(10, 40 - 5 * level)` burn and `max(2, 5 - level)` fire stacks, ignited.
// shortcut: `respect_density` is the cone tile's line of sight from the breather; no self throw
// and no `fire_act` on objects.
fn fire_breath(world: &mut World, mob: Entity, pos: TilePos) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, DEMON_DIES, 50.0, true),
    );
    if crate::clothing::cover::mouth_covered(world, mob).is_some() {
        crate::fire::adjust_fire_stacks(world, mob, 3.0);
        crate::fire::ignite_mob(world, mob);
        tell(
            world,
            mob,
            Span::Warning,
            "Something in front of your mouth catches fire!",
        );
        return;
    }
    let dir = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    for (tile, level) in breath_cone(pos, dir) {
        if !crate::mobs::ai::behaviors::can_see(world, pos, tile, 3) {
            continue;
        }
        let level_f = f64::from(level);
        if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            atmos.hotspot_expose(
                tile,
                (900.0 - 100.0 * level_f).max(500.0),
                (200.0 - 50.0 * level_f).max(50.0),
                true,
            );
        }
        let mobs: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(tile)
            .iter()
            .copied()
            .filter(|&e| e != mob && world.get::<Health>(e).is_some())
            .collect();
        let level = level as f32;
        for victim in mobs {
            if let Ok(mut e) = world.get_entity_mut(victim)
                && let Ok((mut health, mut stat)) =
                    e.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                let mut new = *stat;
                crate::health::apply_damage(
                    &mut health,
                    &mut new,
                    DamageType::Burn,
                    (40.0 - 5.0 * level).max(10.0),
                );
                stat.set_if_neq(new);
            }
            crate::fire::adjust_fire_stacks(world, victim, (5.0 - level).max(2.0));
            crate::fire::ignite_mob(world, victim);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Monkey

/// `race/on_acquiring` -> `crate::transform::monkeyize`: the species and name are kept to come
/// back to.
// shortcut: instant (tg `monkeyize()` animates for 2.2 s); use `monkeyize(world, mob, false)` once
// the mutation can wait for it.
fn monkeyize(world: &mut World, mob: Entity) {
    let species = crate::species::species_of(world, mob);
    if species == SpeciesId::Monkey {
        return;
    }
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let mut state = world.get::<MutationState>(mob).cloned().unwrap_or_default();
    state.original = Some((species, name));
    world.entity_mut(mob).insert(state);
    crate::transform::monkeyize(world, mob, true);
}

/// `race/on_losing` -> `crate::transform::humanize` and the name back; a dead monkey stays.
fn humanize(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    let original = world
        .get_mut::<MutationState>(mob)
        .and_then(|mut s| s.original.take());
    if crate::species::species_of(world, mob) != SpeciesId::Monkey {
        return;
    }
    let (species, name) = original.unwrap_or((SpeciesId::Human, String::new()));
    crate::transform::humanize(world, mob, species, true);
    if !name.is_empty() {
        world.entity_mut(mob).insert(Name::new(name));
    }
}

/// The race mutation's id (the monkey's block).
pub fn race_id() -> usize {
    super::mutation_id("/datum/mutation/race").expect("race")
}

/// `monkeyize()` from outside genetics (tg `dna.add_mutation(/datum/mutation/race)`).
pub fn make_monkey(world: &mut World, mob: Entity) -> bool {
    super::add_mutation(world, mob, race_id(), SOURCE_ACTIVATED)
}

// ---------------------------------------------------------------------------------------------
// Mood auras

/// `aoe_moodlet/on_life`: everyone living within `viewers(3)` gets the moodlet (weak when the
/// face is hidden).
// shortcut: no `TRAIT_FACE_COVERED` (newspapers), mirrors or personalities.
// Cost: every 2 s per aura carrier, a 3-tile radius lookup and a sight check per mob there.
fn aura(world: &mut World, mob: Entity, positive: bool) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let nearby: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 3)
        .filter(|&e| e != mob)
        .collect();
    let weak = crate::clothing::cover::face_obscured(world, mob);
    let name = crate::medical::name_of(world, mob);
    let (description, change) = aura_moodlet(positive, weak, &name);
    let category = if positive {
        "/datum/mutation/aoe_moodlet/positive"
    } else {
        "/datum/mutation/aoe_moodlet"
    };
    let now = now(world);
    for other in nearby {
        let Some(&other_pos) = world.get::<TilePos>(other) else {
            continue;
        };
        if world.get::<crate::mood::Mood>(other).is_none()
            || world.get::<Stat>(other) == Some(&Stat::Dead)
            || !crate::mobs::ai::behaviors::can_see(world, other_pos, pos, 3)
        {
            continue;
        }
        if let Some(mut mood) = world.get_mut::<crate::mood::Mood>(other) {
            mood.clear_event(category);
            mood.add_instance(
                now,
                ActiveMood {
                    category: category.to_owned(),
                    id: if positive {
                        "seen_pretty_mutation"
                    } else {
                        "seen_ugly_mutation"
                    },
                    description: description.clone(),
                    mood_change: change,
                    timeout: 120.0,
                    expires: None,
                    hidden: false,
                    stacks: false,
                },
            );
        }
    }
}

/// `seen_pretty_mutation` / `seen_ugly_mutation` (and `/weak`) with `add_effects(mutant)`.
pub fn aura_moodlet(positive: bool, weak: bool, name: &str) -> (String, f32) {
    match (positive, weak) {
        (true, false) => (
            format!("{name} looks pretty good - must be a new haircut or something."),
            3.0,
        ),
        (true, true) => (
            format!("{name} looks alright - it must be the lighting in here."),
            1.0,
        ),
        (false, false) => (format!("Something is seriously wrong with {name}!"), -3.0),
        (false, true) => (format!("Something looks wrong about {name}."), -1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_and_moodlets() {
        assert_eq!(wall_hardness("/turf/closed/wall"), 40.0);
        assert_eq!(wall_hardness("/turf/closed/wall/r_wall"), 10.0);
        assert_eq!(aura_moodlet(true, false, "Bob").1, 3.0);
        assert_eq!(aura_moodlet(false, true, "Bob").1, -1.0);
        assert!(aura_moodlet(false, false, "Bob").0.contains("Bob"));
        // Facing north from (5,5): three rows of three, the nearest first.
        let cone = breath_cone(TilePos::new(5, 5, 1), Dir::North);
        assert_eq!(cone.len(), 9);
        assert!(cone.contains(&(TilePos::new(5, 6, 1), 1)));
        assert!(cone.contains(&(TilePos::new(4, 8, 1), 3)));
        assert_eq!(Power::Cryokinesis.cast_range(), Some(9));
        assert_eq!(Power::FireBreath.cast_range(), None);
    }
}
