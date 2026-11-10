//! Embedding (server), after tg `/datum/embedding` (`code/datums/embedding.dm`).
//!
//! - An item with an `embed_type` ([`DmEmbed`]) thrown into a carbon at `throw_speed` 4 or more
//!   (or with `ignore_throwspeed_threshold`) rolls `embed_chance` (plus 10 per point of speed
//!   above its own, minus half the better of the bullet and bomb armor of the hit zone) and
//!   lodges in the hit bodypart instead of bouncing off ([`try_embed_thrown`]). A projectile
//!   with an `embed_type` and a `shrapnel_type` leaves that item in the body the same way
//!   ([`try_embed_projectile`]), its chance changed by `embed_falloff_tile` per tile flown.
//! - Lodging in deals `throwforce + w_class × impact_pain_mult` to the bodypart (melee armor
//!   applies), `pain_stam_pct` of every embed damage is stamina instead of brute. A harmful
//!   embed throws the "embedded object" alert and moodlet and adds 0.25 to the bleed rate.
//! - Once a second ([`process`]): `fall_chance` to fall out (`w_class × remove_pain_mult`),
//!   else `pain_chance` to hurt (`w_class × pain_mult`); both a fifth while lying. Every step
//!   of the owner ([`jostle`]) rolls `jostle_chance` (half while walking or lying) for
//!   `w_class × jostle_pain_mult`.
//! - Out by hand ([`rip_out`]: the link of an examine or of checking oneself, `rip_time ×
//!   w_class`, a helper is a quarter faster and gentler), with a hemostat or wirecutters on the
//!   aimed bodypart ([`item_interaction`]), when the bodypart comes off, the owner is fully
//!   healed or is gone. `DROPDEL` shrapnel is deleted when it leaves the body.
// shortcut: the base datum only. The subtypes' own effects are not here: syringes and patches
// injecting while embedded, the tongue spike, healium rebar, the energy shuriken, the mending
// globule, `magic_pull` (the recall spell) and the chain detonation of an embedded grenade in
// the owner's `ex_act`. Add each with the mechanic that owns the item.
// shortcut: no surgery removes embeds (tg "remove embedded objects"); a health analyzer does
// not list them.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::contents::DmWeight;
use ss13_mapload::{DmEmbed, DmItem, DmTool, DmWounding, EmbedData};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::server::UiActed;
use ss13_world::Moved;

use crate::breath::{AlertCategory, Alerts};
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::examine::{ChatLinkClicked, ChatLinkSystems, chat_link};
use crate::health::{BodyPosition, COMBAT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::hud::AlertHudUi;
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::wounds::{Hit, Limbs};
use crate::mood::MoodEvent;
use crate::security::{name_of, show};
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, tell, tell_mob};
use crate::zone::BodyZone;

/// `EMBED_THROWSPEED_THRESHOLD`.
pub const EMBED_THROWSPEED_THRESHOLD: f32 = 4.0;
/// `EMBED_CHANCE_SPEED_BONUS`.
pub const EMBED_CHANCE_SPEED_BONUS: f32 = 10.0;
/// `RIPPING_OUT_HELP_TIME_MULTIPLIER`, `RIPPING_OUT_HELP_DAMAGE_MULTIPLIER`.
const HELP_MULTIPLIER: f32 = 0.75;
/// `CARBON_EXAMINE_EMBEDDING_MAX_DIST`.
const EXAMINE_MAX_DIST: u32 = 4;
/// What each harmful embed adds to its bodypart's bleed rate (`refresh_bleed_rate`).
const EMBED_BLEED_RATE: f32 = 0.25;
const CANT_WOUND: f32 = -100.0;
const SHARP_EDGED: u32 = 1;
/// `DROPDEL` (`item_flags`).
const DROPDEL: u32 = 1 << 5;
const EMBED_SOUND: &str = "sound/items/weapons/bladeslice.ogg";
const TOPIC_PREFIX: &str = "embed";
const MOOD: MoodEvent = MoodEvent {
    id: "embedded",
    description: "Вытащи это!",
    mood_change: -7.0,
    timeout: 0.0,
    hidden: false,
};

/// On an item lodged in a body (tg `embed_data.owner` and `owner_limb`): it is nowhere on the
/// map until it comes out.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Embedded {
    pub owner: Entity,
    pub zone: BodyZone,
    pub data: EmbedData,
}

/// On a carbon: what is lodged in it, oldest first (tg `embedded_objects` of its bodyparts).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct EmbeddedObjects(pub Vec<Entity>);

/// The timed removals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedJob {
    /// By hand.
    Rip(Entity),
    /// With the held hemostat or wirecutters.
    Pluck(Entity),
}

/// When the embeds are processed next (tg `SSprocessing`: once a second).
#[derive(Resource, Default)]
struct EmbedClock(f64);

pub struct EmbeddingPlugin;

impl Plugin for EmbeddingPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.init_resource::<EmbedClock>()
            .add_click_handler(ClickPhase::HeldOnTarget, 43, |w, c| {
                c.held
                    .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
            })
            .add_systems(
                Update,
                (
                    (jostle, process).run_if(any_with_component::<Embedded>),
                    link_clicks.after(ChatLinkSystems),
                    alert_clicks,
                )
                    .after(ss13_world::WorldSystems::Movement)
                    .before(crate::health::HealthSystems),
            )
            .add_observer(owner_gone);
    }
}

fn weight(world: &World, item: Entity) -> f32 {
    f32::from(
        world
            .get::<DmWeight>(item)
            .copied()
            .unwrap_or_default()
            .w_class,
    )
}

fn lying(world: &World, mob: Entity) -> bool {
    world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
}

/// What `item` embeds with: its own `embed_type`, none for the rest.
pub fn embed_of(world: &World, item: Entity) -> Option<EmbedData> {
    world
        .get::<DmEmbed>(item)
        .map(|e| e.data.clone())
        .filter(EmbedData::embeds)
}

/// tg `has_embedded_objects`.
pub fn has_embedded(world: &World, mob: Entity, include_harmless: bool) -> bool {
    world.get::<EmbeddedObjects>(mob).is_some_and(|list| {
        list.0.iter().any(|&item| {
            world
                .get::<Embedded>(item)
                .is_some_and(|e| include_harmless || !e.data.is_harmless(true))
        })
    })
}

/// What the embeds add to `mob`'s bleed rate.
pub fn bleed_rate(world: &World, mob: Entity) -> f32 {
    let Some(list) = world.get::<EmbeddedObjects>(mob) else {
        return 0.0;
    };
    let harmful = list
        .0
        .iter()
        .filter(|&&item| {
            world
                .get::<Embedded>(item)
                .is_some_and(|e| !e.data.is_harmless(false))
        })
        .count();
    let rate = harmful as f32 * EMBED_BLEED_RATE;
    if lying(world, mob) { rate * 0.75 } else { rate }
}

/// tg `can_embed`: a carbon without an immunity, hit fast enough.
fn can_embed(world: &World, data: &EmbedData, victim: Entity, flying_speed: Option<f32>) -> bool {
    if !crate::throw::is_carbon(world, victim) {
        return false;
    }
    if !data.ignores_pierce_immunity
        && crate::species::has_trait(world, victim, crate::species::Trait::PierceImmune)
    {
        return false;
    }
    flying_speed.is_none_or(|s| s >= EMBED_THROWSPEED_THRESHOLD || data.ignore_throwspeed_threshold)
}

/// tg `roll_embed_chance` with the rolled `chance` (speed bonus and falloff applied).
fn roll_embed_chance(
    world: &mut World,
    item: Entity,
    data: &EmbedData,
    victim: Entity,
    zone: BodyZone,
    chance: f32,
) -> bool {
    if data.is_harmless(false) {
        return world.resource_mut::<CombatRng>().chance(data.embed_chance);
    }
    let (penetration, weak) = world.get::<DmEmbed>(item).map_or((0.0, false), |e| {
        (e.armour_penetration, e.weak_against_armour)
    });
    // "We'll be nice and take the better of bullet and bomb armor, halved".
    let check = |flag: &str| {
        let worn = crate::armor::getarmor(world, victim, zone, flag);
        crate::armor::run_armor_check(worn, penetration, false, None, None).0
    };
    let mut armor = check("bullet").max(check("bomb")) * 0.5;
    if armor == 0.0 {
        return world.resource_mut::<CombatRng>().chance(chance);
    }
    if weak {
        armor *= crate::armor::ARMOR_WEAKENED_MULTIPLIER;
    }
    let chance = chance - armor;
    if chance < 0.0 {
        let (thing, name) = (name_of(world, item), name_of(world, victim));
        if let Some(&at) = world.get::<TilePos>(victim) {
            show(
                world,
                at,
                COMBAT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!(
                        "{} отскакивает от брони {name}, не сумев застрять!",
                        ss13_chat::capitalize(&thing)
                    ),
                ),
                &[(
                    victim,
                    Span::Notice,
                    format!(
                        "{} отскакивает от вашей брони, не сумев застрять!",
                        ss13_chat::capitalize(&thing)
                    ),
                )],
            );
        }
        return false;
    }
    world.resource_mut::<CombatRng>().chance(chance)
}

/// tg `failed_embed`: a `DROPDEL` item is gone.
fn failed_embed(world: &mut World, item: Entity) {
    if world
        .get::<DmItem>(item)
        .is_some_and(|i| i.item_flags & DROPDEL != 0)
    {
        world.entity_mut(item).despawn();
    }
}

/// tg `try_embed` on `COMSIG_MOVABLE_IMPACT_ZONE`: `item`, thrown at `speed`, hit `victim`'s
/// `zone` and was not blocked. True when it lodged in (the hit does nothing else).
pub fn try_embed_thrown(
    world: &mut World,
    item: Entity,
    victim: Entity,
    zone: BodyZone,
    speed: f32,
) -> bool {
    let Some(data) = embed_of(world, item) else {
        return false;
    };
    let own_speed = world.get::<DmItem>(item).map_or(0.0, |i| i.throw_speed);
    let flying = if speed > 0.0 { speed } else { own_speed };
    if !can_embed(world, &data, victim, Some(flying)) {
        failed_embed(world, item);
        return false;
    }
    let mut chance = data.embed_chance;
    if speed > own_speed {
        chance += (speed - own_speed) * EMBED_CHANCE_SPEED_BONUS;
    }
    if !roll_embed_chance(world, item, &data, victim, zone, chance) {
        failed_embed(world, item);
        return false;
    }
    embed_into(world, item, victim, zone, data);
    true
}

/// tg `try_embed_projectile`: a projectile that hit `victim`'s `zone` after `tiles` tiles,
/// `armor` of it blocked, leaves its `shrapnel_type` in the body.
pub fn try_embed_projectile(
    world: &mut World,
    projectile: &ss13_mapload::guns::ProjectileData,
    name: &str,
    victim: Entity,
    zone: BodyZone,
    armor: f32,
    tiles: f32,
) {
    let data = &projectile.embed;
    if !data.embeds() || projectile.shrapnel_type.is_empty() {
        return;
    }
    if armor >= 100.0 || !can_embed(world, data, victim, None) {
        return;
    }
    let Some(&at) = world.get::<TilePos>(victim) else {
        return;
    };
    // `setup_shrapnel`: the payload takes the projectile's embedding over.
    let Some(payload) = crate::construction::spawn_obj(world, &projectile.shrapnel_type, at) else {
        return;
    };
    let (penetration, weak) = world.get::<DmEmbed>(payload).map_or((0.0, false), |e| {
        (e.armour_penetration, e.weak_against_armour)
    });
    world.entity_mut(payload).insert(DmEmbed {
        data: data.clone(),
        armour_penetration: penetration,
        weak_against_armour: weak,
    });
    if ss13_mapload::fidelity::is_under(&projectile.shrapnel_type, "/obj/item/shrapnel/bullet") {
        world.entity_mut(payload).insert(Name::new(name.to_owned()));
    }
    let chance = data.embed_chance + projectile.embed_falloff_tile * tiles;
    if !roll_embed_chance(world, payload, data, victim, zone, chance) {
        failed_embed(world, payload);
        return;
    }
    embed_into(world, payload, victim, zone, data.clone());
}

/// The zone's name for "в ...": the accusative.
fn zone_into(zone: BodyZone) -> &'static str {
    zone.ru().1
}

/// The genitive ("из ...").
fn zone_of(zone: BodyZone) -> &'static str {
    match zone {
        BodyZone::Head => "головы",
        BodyZone::Chest => "груди",
        BodyZone::LArm => "левой руки",
        BodyZone::RArm => "правой руки",
        BodyZone::LLeg => "левой ноги",
        BodyZone::RLeg => "правой ноги",
    }
}

/// The prepositional and the dative, which these names share ("в ...", "к ...").
fn zone_in(zone: BodyZone) -> &'static str {
    match zone {
        BodyZone::Head => "голове",
        BodyZone::Chest => "груди",
        BodyZone::LArm => "левой руке",
        BodyZone::RArm => "правой руке",
        BodyZone::LLeg => "левой ноге",
        BodyZone::RLeg => "правой ноге",
    }
}

/// tg `apply_damage` of an embed: `(1 - pain_stam_pct)` of `damage` as brute to the bodypart
/// (`armor` of it blocked), the rest as stamina.
#[allow(clippy::too_many_arguments)]
fn hurt(
    world: &mut World,
    item: Entity,
    owner: Entity,
    zone: BodyZone,
    data: &EmbedData,
    damage: f32,
    armor: f32,
    wound_bonus: Option<f32>,
    always_sharp: bool,
) {
    let wounding = world.get::<DmWounding>(item).cloned().unwrap_or_default();
    let brute = crate::armor::blocked_damage((1.0 - data.pain_stam_pct) * damage, armor)
        * world
            .get::<Limbs>(owner)
            .map_or(1.0, |l| l.limb(zone).damage_multiplier());
    if brute > 0.0 {
        let mut before = 0.0;
        if let Ok(mut entity) = world.get_entity_mut(owner)
            && let Ok((mut health, mut stat)) =
                entity.get_components_mut::<(&mut Health, &mut Stat)>()
        {
            before = health.part(zone).damage();
            let mut new = *stat;
            crate::health::apply_damage_zone(&mut health, &mut new, zone, DamageType::Brute, brute);
            stat.set_if_neq(new);
        }
        let sharpness = if wounding.sharpness == 0 && always_sharp {
            SHARP_EDGED
        } else {
            wounding.sharpness
        };
        crate::medical::wounds::after_hit(
            world,
            owner,
            zone,
            Hit {
                kind: DamageType::Brute,
                damage: brute,
                sharpness,
                wound_bonus: wound_bonus.unwrap_or(CANT_WOUND),
                exposed_wound_bonus: if wound_bonus.is_some() {
                    wounding.exposed_wound_bonus
                } else {
                    0.0
                },
                part_damage_before: before,
            },
        );
    }
    let stamina = data.pain_stam_pct * damage;
    if stamina > 0.0 {
        crate::status_effects::adjust_stamina(world, owner, stamina);
    }
}

/// tg `embed_into`: `item` lodges in `victim`'s `zone` (the chest when that bodypart is gone).
pub fn embed_into(
    world: &mut World,
    item: Entity,
    victim: Entity,
    zone: BodyZone,
    data: EmbedData,
) {
    let zone = if world
        .get::<Limbs>(victim)
        .is_some_and(|l| l.limb(zone).missing)
    {
        BodyZone::Chest
    } else {
        zone
    };
    let Some(&at) = world.get::<TilePos>(victim) else {
        return;
    };
    if let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        for hand in &mut inventory.hands {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
    world
        .entity_mut(item)
        .remove::<(TilePos, CarriedBy, crate::throw::Thrown)>()
        .insert(Embedded {
            owner: victim,
            zone,
            data: data.clone(),
        });
    if world.get::<EmbeddedObjects>(victim).is_none() {
        world.entity_mut(victim).insert(EmbeddedObjects::default());
    }
    if let Some(mut list) = world.get_mut::<EmbeddedObjects>(victim) {
        list.0.push(item);
    }
    let (thing, name) = (
        ss13_chat::capitalize(&name_of(world, item)),
        name_of(world, victim),
    );
    let (verb, place, own) = if data.is_harmless(false) {
        ("прилипает к", zone_in(zone), "вашей")
    } else {
        ("впивается в", zone_into(zone), "вашу")
    };
    show(
        world,
        at,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{thing} {verb} {place} {name}!")),
        &[(
            victim,
            Span::BoldWarning,
            format!("{thing} {verb} {own} {place}!"),
        )],
    );
    let mut damage = world.get::<DmItem>(item).map_or(0.0, |i| i.throwforce);
    if !data.is_harmless(true) {
        if !data.stealthy_embed {
            if let Some(mut alerts) = world.get_mut::<Alerts>(victim) {
                alerts.throw(AlertCategory::EmbeddedObject, "embeddedobject", 0);
            }
            crate::mood::add_event(world, victim, MOOD.id, &MOOD);
        }
        if !data.is_harmless(false) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(at, EMBED_SOUND, 40.0, false),
            );
        }
        damage += weight(world, item) * data.impact_pain_mult;
    }
    if damage <= 0.0 {
        return;
    }
    let (penetration, weak) = world.get::<DmEmbed>(item).map_or((0.0, false), |e| {
        (e.armour_penetration, e.weak_against_armour)
    });
    let worn = crate::armor::getarmor(world, victim, zone, "melee");
    let part = zone.ru().1;
    let (armor, line) = crate::armor::run_armor_check(
        worn,
        penetration,
        weak,
        Some(format!("Ваша броня защитила вашу {part}.")),
        Some(format!("Ваша броня смягчила удар в вашу {part}.")),
    );
    if let Some((span, text)) = line {
        tell_mob(world, victim, span, text);
    }
    let bonus = world.get::<DmWounding>(item).map_or(0.0, |w| w.wound_bonus);
    hurt(
        world,
        item,
        victim,
        zone,
        &data,
        damage,
        armor,
        Some(bonus),
        false,
    );
}

/// tg `stop_embedding`: the item is no one's embed any more. True when it was `DROPDEL` and is
/// deleted.
fn stop_embedding(world: &mut World, item: Entity) -> bool {
    let Some(embedded) = world.entity_mut(item).take::<Embedded>() else {
        return false;
    };
    let owner = embedded.owner;
    if let Some(mut list) = world.get_mut::<EmbeddedObjects>(owner) {
        list.0.retain(|&e| e != item);
    }
    if world.get_entity(owner).is_ok() && !has_embedded(world, owner, false) {
        if let Some(mut alerts) = world.get_mut::<Alerts>(owner)
            && alerts.get(AlertCategory::EmbeddedObject).is_some()
        {
            alerts.clear(AlertCategory::EmbeddedObject);
        }
        crate::mood::clear_event(world, owner, MOOD.id);
    }
    if world
        .get::<DmItem>(item)
        .is_some_and(|i| i.item_flags & DROPDEL != 0)
    {
        world.entity_mut(item).despawn();
        return true;
    }
    false
}

/// tg `remove_embedding`: out onto the owner's tile, then into `to_hands`' free hand.
pub fn remove_embedding(world: &mut World, item: Entity, to_hands: Option<Entity>) {
    let Some(owner) = world.get::<Embedded>(item).map(|e| e.owner) else {
        return;
    };
    let pos = crate::explosions::turf_of(world, owner);
    if stop_embedding(world, item) {
        return;
    }
    let Some(pos) = pos else {
        world.entity_mut(item).despawn();
        return;
    };
    crate::inventory::put_down(world, item, pos);
    if let Some(mob) = to_hands
        && let Some(inventory) = world.get::<Inventory>(mob).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, mob, item, hand);
    }
}

/// tg `remove_all_embedded_objects`.
pub fn remove_all(world: &mut World, mob: Entity) {
    let items = world
        .get::<EmbeddedObjects>(mob)
        .map(|l| l.0.clone())
        .unwrap_or_default();
    for item in items {
        remove_embedding(world, item, None);
    }
}

/// tg `on_removed`: the bodypart of `zone` left `mob`, what was lodged in it falls out.
pub fn limb_removed(world: &mut World, mob: Entity, zone: BodyZone) {
    let items = world
        .get::<EmbeddedObjects>(mob)
        .map(|l| l.0.clone())
        .unwrap_or_default();
    for item in items {
        if world.get::<Embedded>(item).is_some_and(|e| e.zone == zone) {
            remove_embedding(world, item, None);
        }
    }
}

/// The owner is deleted or stops being a carbon with embeds: they fall where it was.
fn owner_gone(
    remove: On<Remove<EmbeddedObjects>>,
    owners: Query<(&EmbeddedObjects, Option<&TilePos>)>,
    mut commands: Commands,
) {
    let Ok((list, pos)) = owners.get(remove.entity) else {
        return;
    };
    let (items, pos) = (list.0.clone(), pos.copied());
    commands.queue(move |world: &mut World| {
        for item in items {
            if world.get_entity(item).is_err() {
                continue;
            }
            let dropdel = world
                .get::<DmItem>(item)
                .is_some_and(|i| i.item_flags & DROPDEL != 0);
            match pos {
                Some(pos) if !dropdel => {
                    world.entity_mut(item).remove::<Embedded>();
                    crate::inventory::put_down(world, item, pos);
                }
                _ => world.entity_mut(item).despawn(),
            }
        }
    });
}

/// tg `fall_out`.
fn fall_out(world: &mut World, item: Entity, embedded: &Embedded) {
    let owner = embedded.owner;
    let Some(&at) = world.get::<TilePos>(owner) else {
        remove_embedding(world, item, None);
        return;
    };
    let (thing, name, part) = (
        ss13_chat::capitalize(&name_of(world, item)),
        name_of(world, owner),
        zone_of(embedded.zone),
    );
    let harmless = embedded.data.is_harmless(false);
    if !harmless {
        let damage = weight(world, item) * embedded.data.remove_pain_mult;
        hurt(
            world,
            item,
            owner,
            embedded.zone,
            &embedded.data,
            damage,
            0.0,
            None,
            false,
        );
    }
    let (span, own_span, verb) = if harmless {
        (Span::Warning, Span::Warning, "отваливается от")
    } else {
        (Span::Danger, Span::BoldWarning, "выпадает из")
    };
    show(
        world,
        at,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (span, format!("{thing} {verb} {part} {name}!")),
        &[(owner, own_span, format!("{thing} {verb} вашей {part}!"))],
    );
    remove_embedding(world, item, None);
}

/// tg `/datum/embedding/process`, once a second for every embed.
fn process(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    {
        let mut clock = world.resource_mut::<EmbedClock>();
        if now < clock.0 {
            return;
        }
        clock.0 = now + 1.0;
    }
    let mut query = world.query::<(Entity, &Embedded)>();
    let embeds: Vec<(Entity, Embedded)> = query.iter(world).map(|(e, x)| (e, x.clone())).collect();
    for (item, embedded) in embeds {
        let owner = embedded.owner;
        if world.get::<Embedded>(item).is_none() || world.get_entity(owner).is_err() {
            continue;
        }
        if world.get::<Stat>(owner) == Some(&Stat::Dead) {
            continue;
        }
        let data = &embedded.data;
        let down = lying(world, owner);
        let fall = if down {
            data.fall_chance * 0.2
        } else {
            data.fall_chance
        };
        if world.resource_mut::<CombatRng>().chance(fall) {
            fall_out(world, item, &embedded);
            continue;
        }
        let mut damage = weight(world, item) * data.pain_mult;
        let mut pain = data.pain_chance;
        let stamcrit = world
            .get::<crate::status_effects::StatusEffects>(owner)
            .is_some_and(|e| e.stamcrit);
        // "if it's a less-lethal embed, give them a break if they're already stamcritted".
        if data.pain_stam_pct > 0.0 && stamcrit {
            pain *= 0.2;
            damage *= 0.5;
        } else if down {
            pain *= 0.2;
        }
        if data.is_harmless(true) || !world.resource_mut::<CombatRng>().chance(pain) {
            continue;
        }
        hurt(
            world,
            item,
            owner,
            embedded.zone,
            data,
            damage,
            0.0,
            None,
            false,
        );
        let part = zone_in(embedded.zone);
        let feels = if data.pain_stam_pct < 1.0 {
            "болит!"
        } else {
            "тянет вас вниз."
        };
        let text = if data.stealthy_embed {
            format!("Что-то в вашей {part} {feels}")
        } else {
            format!(
                "{} в вашей {part} {feels}",
                ss13_chat::capitalize(&name_of(world, item))
            )
        };
        tell_mob(world, owner, Span::BoldWarning, text);
    }
}

/// tg `owner_moved`: each step of the owner may jostle its embeds.
fn jostle(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let mut movers: Vec<Entity> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| m.entity)
        .filter(|&e| world.get::<EmbeddedObjects>(e).is_some())
        .collect();
    // A diagonal move is two steps here and one `Moved` in tg.
    movers.sort_unstable();
    movers.dedup();
    for owner in movers {
        let careful = lying(world, owner)
            || world
                .get::<crate::janitor::Walking>(owner)
                .is_some_and(|w| w.0);
        let items = world
            .get::<EmbeddedObjects>(owner)
            .map(|l| l.0.clone())
            .unwrap_or_default();
        for item in items {
            let Some(embedded) = world.get::<Embedded>(item).cloned() else {
                continue;
            };
            let data = &embedded.data;
            let chance = if careful {
                data.jostle_chance * 0.5
            } else {
                data.jostle_chance
            };
            if data.is_harmless(true) || !world.resource_mut::<CombatRng>().chance(chance) {
                continue;
            }
            let damage = weight(world, item) * data.jostle_pain_mult;
            hurt(
                world,
                item,
                owner,
                embedded.zone,
                data,
                damage,
                0.0,
                None,
                false,
            );
            let part = zone_in(embedded.zone);
            let text = if data.stealthy_embed {
                format!("Что-то в вашей {part} шевелится и жалит!")
            } else {
                format!(
                    "{} в вашей {part} шевелится и жалит!",
                    ss13_chat::capitalize(&name_of(world, item))
                )
            };
            tell_mob(world, owner, Span::BoldWarning, text);
        }
    }
}

/// `embed:<item bits>`.
fn topic(item: Entity) -> String {
    format!("{TOPIC_PREFIX}:{}", item.to_bits())
}

fn parse_topic(topic: &str) -> Option<Entity> {
    let bits = topic.strip_prefix(TOPIC_PREFIX)?.strip_prefix(':')?;
    Entity::try_from_bits(bits.parse().ok()?)
}

/// The embeds an examine shows, oldest first: not the stealthy ones.
fn shown(world: &World, mob: Entity) -> Vec<(Entity, Embedded)> {
    world
        .get::<EmbeddedObjects>(mob)
        .map(|l| l.0.clone())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| Some((item, world.get::<Embedded>(item)?.clone())))
        .collect()
}

/// tg carbon `examine`: one line per visible embed of `target`, a link to pull it out when the
/// living examiner is within 4 tiles.
pub fn examine_messages(
    world: &mut World,
    client: Entity,
    examiner: Entity,
    target: Entity,
) -> Vec<ChatMessage> {
    let near = world.get::<Health>(examiner).is_some()
        && matches!(
            (world.get::<TilePos>(examiner), world.get::<TilePos>(target)),
            (Some(a), Some(b)) if a.chebyshev(*b).is_some_and(|d| d <= EXAMINE_MAX_DIST)
        );
    let mut out = Vec::new();
    for (item, embedded) in shown(world, target) {
        if embedded.data.stealthy_embed {
            continue;
        }
        let harmless = embedded.data.is_harmless(false);
        let name = ss13_chat::capitalize(&name_of(world, item));
        let rest_span = if harmless {
            Span::Notice
        } else {
            Span::BoldWarning
        };
        let name_span = if near {
            Span::Link(chat_link(world, client, topic(item)))
        } else {
            rest_span
        };
        let verb = if harmless {
            "застревает"
        } else {
            "впивается"
        };
        out.push(ChatMessage {
            channel: ChatChannel::Info,
            segments: vec![
                ChatSegment::new(name_span, name),
                ChatSegment::new(
                    rest_span,
                    format!(" {verb} в {}!", zone_into(embedded.zone)),
                ),
            ],
        });
    }
    out
}

/// tg carbon `check_self_for_injuries`: the own embeds with their links. False without any
/// (the click does what it did).
pub fn check_self(world: &mut World, client: Entity, mob: Entity) -> bool {
    if world
        .get::<Stat>(mob)
        .is_some_and(|&s| s != Stat::Conscious)
    {
        return false;
    }
    let embeds = shown(world, mob);
    if embeds.is_empty() {
        return false;
    }
    let name = name_of(world, mob);
    crate::chem::visible(
        world,
        mob,
        "Вы проверяете себя на наличие осколков.",
        &format!("{} осматривает себя.", ss13_chat::capitalize(&name)),
        Span::Notice,
    );
    for (item, embedded) in embeds {
        let harmless = embedded.data.is_harmless(false);
        let verb = if harmless {
            "застревает у вас на"
        } else {
            "впивается у вас в"
        };
        let link = chat_link(world, client, topic(item));
        let text = format!(
            "{} {verb} {}!",
            ss13_chat::capitalize(&name_of(world, item)),
            zone_in(embedded.zone)
        );
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::Info,
                segments: vec![ChatSegment::new(Span::Link(link), text)],
            },
        });
    }
    true
}

/// `/atom/movable/screen/alert/embeddedobject/Click`: check oneself.
fn alert_clicks(world: &mut World, mut acts: Local<MessageCursor<UiActed<AlertHudUi>>>) {
    let acts: Vec<(Entity, Entity, crate::breath::AlertClick)> = acts
        .read(world.resource::<Messages<UiActed<AlertHudUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, click) in acts {
        if actor != source
            || crate::breath::clicked_alert(world, actor, &click)
                != Some(AlertCategory::EmbeddedObject)
        {
            continue;
        }
        if let Some(client) = world.get::<PlayerMob>(actor).map(|p| p.client) {
            check_self(world, client, actor);
        }
    }
}

/// tg carbon `Topic(embedded_object)`.
fn link_clicks(world: &mut World, mut cursor: Local<MessageCursor<ChatLinkClicked>>) {
    let Some(messages) = world.get_resource::<Messages<ChatLinkClicked>>() else {
        return;
    };
    let clicks: Vec<ChatLinkClicked> = cursor.read(messages).cloned().collect();
    for click in clicks {
        let Some(item) = parse_topic(&click.topic) else {
            continue;
        };
        let Some(mob) = world.resource::<crate::server::Players>().mob(click.client) else {
            continue;
        };
        rip_out(world, click.client, mob, item);
    }
}

/// Whether `mob` can work on `owner`'s embed now: itself or adjacent, conscious, hands free to
/// act.
fn can_reach(world: &World, mob: Entity, owner: Entity) -> bool {
    let near = mob == owner
        || matches!(
            (world.get::<TilePos>(mob), world.get::<TilePos>(owner)),
            (Some(a), Some(b)) if a.chebyshev(*b).is_some_and(|d| d <= 1)
        );
    near && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && world.get::<Inventory>(mob).is_some()
        && !world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|e| e.hands_blocked())
}

/// tg `rip_out` up to its `do_after`.
pub fn rip_out(world: &mut World, client: Entity, ripper: Entity, item: Entity) {
    let Some(embedded) = world.get::<Embedded>(item).cloned() else {
        return;
    };
    let owner = embedded.owner;
    if embedded.data.stealthy_embed || !can_reach(world, ripper, owner) {
        return;
    }
    let mut time = embedded.data.rip_time * weight(world, item);
    let (thing, name, who, part) = (
        name_of(world, item),
        name_of(world, owner),
        name_of(world, ripper),
        zone_of(embedded.zone),
    );
    if ripper == owner {
        crate::chem::visible(
            world,
            owner,
            &format!("Вы пытаетесь вытащить {thing} из своей {part}..."),
            &format!("{name} пытается вытащить {thing} из своей {part}."),
            Span::Warning,
        );
    } else {
        time *= HELP_MULTIPLIER;
        if let Some(&at) = world.get::<TilePos>(owner) {
            show(
                world,
                at,
                crate::health::DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Warning,
                    format!("{who} пытается вытащить {thing} из {part} {name}!"),
                ),
                &[
                    (
                        owner,
                        Span::BoldWarning,
                        format!("{who} пытается вытащить {thing} из вашей {part}!"),
                    ),
                    (
                        ripper,
                        Span::Notice,
                        format!("Вы пытаетесь вытащить {thing} из {part} {name}..."),
                    ),
                ],
            );
        }
    }
    let actor = Actor {
        client,
        mob: ripper,
        item: world
            .get::<Inventory>(ripper)
            .and_then(Inventory::active_item),
    };
    do_after(
        world,
        actor,
        owner,
        time,
        0,
        Job::Embed(EmbedJob::Rip(item)),
    );
}

/// tg `on_item_interaction`: a hemostat or wirecutters on the owner, aimed at the bodypart
/// whose first embed this is. True when the click is taken.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    user: Entity,
    tool: Entity,
    target: Entity,
) -> bool {
    let Some(list) = world.get::<EmbeddedObjects>(target) else {
        return false;
    };
    let behaviour = world.get::<DmTool>(tool).map(|t| t.behaviour.clone());
    if !matches!(behaviour.as_deref(), Some("hemostat" | "cutters")) {
        return false;
    }
    let aimed = world
        .get::<crate::health::ZoneSelected>(user)
        .map_or(BodyZone::Chest, |z| z.0.check_zone());
    // "Don't pluck everything at the same time".
    let Some((item, embedded)) = list
        .0
        .iter()
        .filter_map(|&item| Some((item, world.get::<Embedded>(item)?.clone())))
        .find(|(_, e)| e.zone == aimed)
    else {
        return false;
    };
    if world.get::<DoAfter>(user).is_some() {
        return true;
    }
    let hemostat = behaviour.as_deref() == Some("hemostat");
    let safe = !hemostat || embedded.data.is_harmless(false);
    let speed = world.get::<DmTool>(tool).map_or(1.0, |t| t.speed);
    let mut time = embedded.data.rip_time * (weight(world, item) * 0.3) * speed;
    if user == target {
        time *= 1.5;
    }
    // "Wirecutters are harder to use for this".
    if safe {
        time *= 1.5;
    }
    let (thing, name, who, with, part) = (
        name_of(world, item),
        name_of(world, target),
        name_of(world, user),
        name_of(world, tool),
        zone_of(embedded.zone),
    );
    if user == target {
        crate::chem::visible(
            world,
            user,
            &format!("Вы начинаете вытаскивать {thing} из своей {part} с помощью {with}..."),
            &format!("{who} начинает вытаскивать {thing} из своей {part} с помощью {with}..."),
            Span::Danger,
        );
    } else if let Some(&at) = world.get::<TilePos>(target) {
        show(
            world,
            at,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{who} начинает вытаскивать {thing} из {part} {name} с помощью {with}..."),
            ),
            &[
                (
                    target,
                    Span::BoldWarning,
                    format!(
                        "{who} начинает вытаскивать {thing} из вашей {part} с помощью \
                         {with}..."
                    ),
                ),
                (
                    user,
                    Span::Notice,
                    format!(
                        "Вы начинаете вытаскивать {thing} из {part} {name} с помощью \
                         {with}..."
                    ),
                ),
            ],
        );
    }
    let actor = Actor {
        client,
        mob: user,
        item: Some(tool),
    };
    do_after(
        world,
        actor,
        target,
        time,
        0,
        Job::Embed(EmbedJob::Pluck(item)),
    );
    true
}

/// tg `damaging_removal_effect`.
fn damaging_removal(world: &mut World, item: Entity, embedded: &Embedded, multiplier: f32) {
    let damage = weight(world, item) * embedded.data.remove_pain_mult * multiplier;
    // "It hurts to rip it out, get surgery you dingus": this one can wound.
    let bonus = world
        .get::<DmWounding>(item)
        .map_or(0.0, |w| w.wound_bonus.max(0.0));
    hurt(
        world,
        item,
        embedded.owner,
        embedded.zone,
        &embedded.data,
        damage,
        0.0,
        Some(bonus),
        true,
    );
    crate::medical::diseases::emote(world, embedded.owner, "scream");
}

/// The end of a removal's `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: EmbedJob) {
    let (EmbedJob::Rip(item) | EmbedJob::Pluck(item)) = job;
    // `still_in`.
    let Some(embedded) = world.get::<Embedded>(item).cloned() else {
        return;
    };
    let owner = embedded.owner;
    if owner != action.target || !can_reach(world, mob, owner) {
        return;
    }
    let (thing, name, who, part) = (
        name_of(world, item),
        name_of(world, owner),
        name_of(world, mob),
        zone_of(embedded.zone),
    );
    let harmless = embedded.data.is_harmless(false);
    match job {
        EmbedJob::Rip(_) => {
            if mob == owner {
                crate::chem::visible(
                    world,
                    owner,
                    &format!("Вы вытаскиваете {thing} из своей {part}."),
                    &format!("{name} вытаскивает {thing} из своей {part}!"),
                    Span::Notice,
                );
            } else if let Some(&at) = world.get::<TilePos>(owner) {
                show(
                    world,
                    at,
                    crate::health::DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Combat,
                    (
                        Span::Notice,
                        format!("{who} вытаскивает {thing} из {part} {name}!"),
                    ),
                    &[
                        (
                            owner,
                            Span::BoldWarning,
                            format!("{who} вытаскивает {thing} из вашей {part}!"),
                        ),
                        (
                            mob,
                            Span::Notice,
                            format!("Вы вытаскиваете {thing} из {part} {name}."),
                        ),
                    ],
                );
            }
            if !harmless {
                let multiplier = if mob == owner { 1.0 } else { HELP_MULTIPLIER };
                damaging_removal(world, item, &embedded, multiplier);
            }
        }
        EmbedJob::Pluck(_) => {
            let tool = action.item;
            let hemostat = tool
                .and_then(|t| world.get::<DmTool>(t))
                .is_some_and(|t| t.behaviour == "hemostat");
            let safe = !hemostat || harmless;
            if mob == owner {
                let hurts = if safe {
                    "."
                } else {
                    ", но это адски больно!"
                };
                tell(
                    world,
                    action.client,
                    Span::Notice,
                    format!("Вы вытаскиваете {thing} из своей {part}{hurts}"),
                );
            }
            if !safe {
                let own = if mob == owner { 1.0 } else { HELP_MULTIPLIER };
                let tool_weight = tool.map_or(1.0, |t| weight(world, t));
                damaging_removal(world, item, &embedded, own.min(0.4 * tool_weight));
            }
        }
    }
    remove_embedding(world, item, Some(mob));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_round_trip() {
        let item = Entity::from_bits(0x0000_0001_0000_0007);
        assert_eq!(parse_topic(&topic(item)), Some(item));
        assert_eq!(parse_topic("embed"), None);
        assert_eq!(parse_topic("medhud:1"), None);
    }

    #[test]
    fn harmless_follows_tg() {
        let sticky = EmbedData {
            path: "/datum/embedding/sticky_tape".into(),
            pain_mult: 0.0,
            jostle_pain_mult: 0.0,
            ..Default::default()
        };
        assert!(sticky.is_harmless(false));
        // All of its pain is stamina: harmless only when stamina counts.
        let weighing = EmbedData {
            pain_stam_pct: 1.0,
            ..sticky.clone()
        };
        assert!(!weighing.is_harmless(false));
        assert!(weighing.is_harmless(true));
        let blade = EmbedData {
            pain_mult: 2.0,
            ..sticky
        };
        assert!(!blade.is_harmless(true));
    }
}
