use super::*;

/// `/datum/quirk/alcohol_tolerance` (×0.7) and `light_drinker` (×2): the booze power of
/// `/datum/reagent/consumable/ethanol/on_mob_life`.
pub fn booze_multiplier(world: &World, mob: Entity) -> f32 {
    let Some(quirks) = world.get::<Quirks>(mob) else {
        return 1.0;
    };
    let mut multiplier = 1.0;
    if quirks.has(Quirk::AlcoholTolerance) {
        multiplier *= 0.7;
    }
    if quirks.has(Quirk::LightDrinker) {
        multiplier *= 2.0;
    }
    multiplier
}

/// tg `LOCATION_*` of `give_item_to_holder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Location {
    Eyes,
    LeftPocket,
    RightPocket,
    Backpack,
    Hands,
}

/// tg `give_item_to_holder`: a new `path` into the first location that takes it, else at the
/// mob's feet. Where it went (`None`: no template for it).
pub(super) fn give_item(
    world: &mut World,
    mob: Entity,
    path: &str,
    locations: &[Location],
) -> Option<&'static str> {
    let item = spawn_item(world, path)?;
    Some(place_item(world, mob, item, locations))
}

/// A new quirk item of `path` from its outfit template, nowhere yet.
pub(super) fn spawn_item(world: &mut World, path: &str) -> Option<Entity> {
    if !world.contains_resource::<OutfitTemplates>() {
        return None;
    }
    let item = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
        Some(templates.0.get(path)?.spawn(world, None))
    });
    let Some(item) = item else {
        warn!("quirk item {path} has no prototype");
        return None;
    };
    world.entity_mut(item).insert(Networked);
    crate::storage::finish_item(world, item, 0);
    Some(item)
}

/// The rest of `give_item_to_holder`: `item` into the first location that takes it.
pub(super) fn place_item(
    world: &mut World,
    mob: Entity,
    item: Entity,
    locations: &[Location],
) -> &'static str {
    for &location in locations {
        let Some(mut inventory) = world.get::<Inventory>(mob).cloned() else {
            break;
        };
        let (slot, where_) = match location {
            Location::Eyes => (InventorySlot::Eyes, "на глазах"),
            Location::LeftPocket => (InventorySlot::LeftPocket, "в левом кармане"),
            Location::RightPocket => (InventorySlot::RightPocket, "в правом кармане"),
            Location::Hands => {
                let Some(hand) = inventory.hands.iter().position(Option::is_none) else {
                    continue;
                };
                inventory.hands[hand] = Some(item);
                world.entity_mut(item).insert(CarriedBy(mob));
                world.entity_mut(mob).insert(inventory);
                return "в руках";
            }
            Location::Backpack => {
                let Some(pack) = inventory.back else {
                    continue;
                };
                if crate::storage::can_insert(world, pack, item).is_err() {
                    continue;
                }
                crate::storage::put_inside(world, item, pack);
                return "в рюкзаке";
            }
        };
        // Pockets need a jumpsuit (tg `mob_can_equip`).
        if matches!(slot, InventorySlot::LeftPocket | InventorySlot::RightPocket)
            && inventory.uniform.is_none()
        {
            continue;
        }
        if !crate::inventory::can_equip(world, mob, &inventory, item, slot) {
            continue;
        }
        match slot {
            InventorySlot::Eyes => inventory.eyes = Some(item),
            InventorySlot::LeftPocket => inventory.pockets[0] = Some(item),
            _ => inventory.pockets[1] = Some(item),
        }
        world.entity_mut(item).insert(CarriedBy(mob));
        world.entity_mut(mob).insert(inventory);
        return where_;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::inventory::put_down(world, item, pos);
    }
    "у ваших ног"
}

/// Tagger (`TRAIT_TAGGER`): paint costs and the syndicate spraycan's drawing time are halved.
pub fn tagger_multiplier(world: &World, mob: Entity) -> f32 {
    if has(world, mob, Quirk::Tagger) {
        0.5
    } else {
        1.0
    }
}

/// Pyromania's `MODIFY_PHYSIOLOGY(PHYS_COEFF_HEAT, 0.67)` (`crate::breath` body heat damage).
pub fn heat_multiplier(world: &World, mob: Entity) -> f64 {
    if has(world, mob, Quirk::Pyromania) {
        PYROMANIA_HEAT_MOD
    } else {
        1.0
    }
}

/// The fan pin moodlet while the pin of `quirk` is on the worn jumpsuit.
// shortcut: the clown's and mime's own job traits give no pin moodlet; no fan HUD.
pub(super) fn fan_pin(world: &mut World, mob: Entity, quirk: Quirk) {
    let (pin, category, event) = if quirk == Quirk::ClownEnjoyer {
        (CLOWN_PIN, "clown_enjoyer_pin", &CLOWN_PIN_EVENT)
    } else {
        (MIME_PIN, "mime_fan_pin", &MIME_PIN_EVENT)
    };
    let worn = world
        .get::<Inventory>(mob)
        .and_then(|i| i.uniform)
        .is_some_and(|uniform| {
            crate::clothing::accessories::attached(world, uniform)
                .into_iter()
                .any(|a| {
                    world
                        .get::<DmItem>(a)
                        .is_some_and(|dm| ss13_mapload::fidelity::is_under(&dm.path, pin))
                })
        });
    let has_event = world
        .get::<Mood>(mob)
        .is_some_and(|m| m.get(category).is_some());
    if worn && !has_event {
        mood::add_event(world, mob, category, event);
    } else if !worn && has_event {
        mood::clear_event(world, mob, category);
    }
}

/// `count_nearby_fire_sources(5)` (unweighted): burning mobs, bonfires, lit fireplaces,
/// hotspots and burning objects in 5 tiles.
// shortcut: no line of sight; lit lighters, matches and flares are not ported as flames.
// Cost: 1 Hz per pyromaniac, one 5-tile spatial lookup.
pub fn count_nearby_fire_sources(world: &World, mob: Entity) -> i32 {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return 0;
    };
    let Some(index) = world.get_resource::<ss13_world::SpatialIndex>() else {
        return 0;
    };
    index
        .in_radius(pos, PYROMANIA_RADIUS)
        .filter(|&e| {
            world
                .get::<crate::fire::FireStacks>(e)
                .is_some_and(|f| f.on_fire)
                || world
                    .get::<crate::fireplace::Bonfire>(e)
                    .is_some_and(|b| b.burning)
                || world
                    .get::<crate::fireplace::Fireplace>(e)
                    .is_some_and(|f| f.lit)
                || world.get::<ss13_atmos::HotspotEffect>(e).is_some()
                || world.get::<crate::fire::Burning>(e).is_some()
        })
        .count() as i32
}

/// Pyromania's `process`: the fire moodlet follows the fire count (its change the count, kept
/// at the highest while fires stay, `be_replaced`).
// shortcut: no impulsive lighter flicking (lighters are not lit-able); the damage pause goes
// with it.
pub(super) fn pyromania(world: &mut World, mob: Entity) {
    let fire_size = count_nearby_fire_sources(world, mob);
    let last = world.get::<LastFireSize>(mob).map_or(-1, |l| l.0);
    if fire_size != last {
        if fire_size >= 1 {
            let now = world.resource::<Time>().elapsed_secs_f64();
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                let current = mood.get("pyromania_fire").map_or(0.0, |m| m.mood_change);
                let mut event = ActiveMood::new("pyromania_fire", &PYROMANIA_EVENT);
                event.mood_change = current.max(fire_size as f32);
                mood.add_instance(now, event);
            }
        } else {
            mood::clear_event(world, mob, "pyromania_fire");
        }
    }
    world.entity_mut(mob).insert(LastFireSize(fire_size));
}

/// Social Anxiety's `calculate_mood_mod`: the mood factor (`1 + 0.02 * (50 - max(50,
/// mood_level * (7 - sanity_level)))`: a good mood calms, the nutrition stands in without a
/// mood) times 12.5 per human with a player within 3 tiles.
// shortcut: no line of sight (`oview`), a player body counts (no minds without clients).
// Cost: 1 Hz per anxious mob, one 3-tile spatial lookup.
pub fn anxiety_mood_mod(world: &World, mob: Entity) -> f32 {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return 0.0;
    };
    let nearby = world
        .get_resource::<ss13_world::SpatialIndex>()
        .map_or(0, |index| {
            index
                .in_radius(pos, ANXIETY_RANGE)
                .filter(|&e| e != mob)
                .filter(|&e| world.get::<PlayerMob>(e).is_some())
                .filter(|&e| world.get::<crate::human::HumanLook>(e).is_some())
                .count()
        });
    anxiety_mod(world.get::<Mood>(mob), nutrition(world, mob), nearby)
}

pub(super) fn nutrition(world: &World, mob: Entity) -> f32 {
    world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(0.0, |h| h.nutrition)
}

/// The formula of `calculate_mood_mod`.
pub fn anxiety_mod(mood: Option<&Mood>, nutrition: f32, nearby: usize) -> f32 {
    let factor = match mood {
        Some(m) => {
            let level = f32::from(m.mood_level)
                * (f32::from(crate::mood::SANITY_LEVEL_INSANE) + 1.0 - f32::from(m.sanity_level));
            1.0 + 0.02 * (50.0 - level.max(50.0))
        }
        None => 1.0 + 0.02 * (50.0 - (0.1 * nutrition).max(50.0)),
    };
    factor * nearby as f32 * 12.5
}

/// After a normal examine (`crate::examine`): `recent_examines`, then `handle_eye_contact`
/// with Social Anxiety's `eye_contact` and `looks_at_floor`.
// shortcut: the plain eye contact lines come at once (tg: 0.3 s later); no face covering
// (`is_eyes_visible`), no Shifty Eyes imagined contact.
pub fn examined(world: &mut World, examiner: Entity, target: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<PlayerMob>(examiner).is_none() {
        return;
    }
    // `COMSIG_MOB_EXAMINATE`: `looks_at_floor`, 15% at anything without a mind.
    if has(world, examiner, Quirk::SocialAnxiety)
        && world.get::<PlayerMob>(target).is_none()
        && world.get_resource_or_init::<CombatRng>().chance(15.0)
    {
        let name = crate::examine::name_of(world, target);
        tell_mob(
            world,
            examiner,
            (Span::Notice, &format!("Вы смотрите в глаза {name}.")),
        );
    }
    {
        let mut recent = world
            .get::<RecentExamines>(examiner)
            .cloned()
            .unwrap_or_default();
        recent
            .0
            .retain(|(e, at)| *e != target && now - *at < RECENT_EXAMINE_SECS);
        recent.0.push((target, now));
        world.entity_mut(examiner).insert(recent);
    }
    eye_contact(world, examiner, target, now);
}

/// tg `/mob/living/handle_eye_contact`: a living, awake target that looked at the examiner
/// in the last 2 s, within 5 tiles.
pub(super) fn eye_contact(world: &mut World, examiner: Entity, target: Entity, now: f64) {
    if examiner == target
        || world
            .get::<Stat>(target)
            .is_none_or(|s| *s >= Stat::HardCrit)
        || crate::vision::is_blind(world, examiner)
    {
        return;
    }
    let looked_back = world.get::<RecentExamines>(target).is_some_and(|r| {
        r.0.iter()
            .any(|(e, at)| *e == examiner && now - *at < RECENT_EXAMINE_SECS)
    });
    if !looked_back {
        return;
    }
    let (Some(&a), Some(&b)) = (world.get::<TilePos>(examiner), world.get::<TilePos>(target))
    else {
        return;
    };
    if a.chebyshev(b).is_none_or(|d| d > EYE_CONTACT_RANGE) {
        return;
    }
    if !anxious_eye_contact(world, examiner, target, true) {
        let name = crate::examine::name_of(world, target);
        tell_mob(
            world,
            examiner,
            (Span::Notice, &format!("Вы смотрите в глаза {name}.")),
        );
    }
    if !crate::vision::is_blind(world, target)
        && !anxious_eye_contact(world, target, examiner, false)
    {
        let name = crate::examine::name_of(world, examiner);
        tell_mob(
            world,
            target,
            (Span::Notice, &format!("{name} смотрит вам в глаза.")),
        );
    }
}

/// Social Anxiety's `eye_contact` (`COMSIG_MOB_EYECONTACT` on `holder`): 25% to jitter 20 s,
/// stutter 6 s or freeze 2 s, with the moodlet. True if it blocked the plain line.
pub(super) fn anxious_eye_contact(
    world: &mut World,
    holder: Entity,
    other: Entity,
    examiner: bool,
) -> bool {
    if !has(world, holder, Quirk::SocialAnxiety) {
        return false;
    }
    let pick = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        if rng.chance(75.0) {
            return false;
        }
        rng.range(1, 3)
    };
    let name = crate::examine::name_of(world, other);
    let mut text = if examiner {
        format!("Вы смотрите в глаза {name}, ")
    } else {
        format!("{name} смотрит вам в глаза, ")
    };
    use crate::status_effects::{Incap, Timed, set_timed};
    match pick {
        1 => {
            set_timed(world, holder, Timed::Jitter, 20.0, true);
            text.push_str("что заставляет вас нервничать!");
        }
        2 => {
            set_timed(world, holder, Timed::Stutter, 6.0, true);
            text.push_str("что заставляет вас заикаться!");
        }
        _ => {
            crate::status_effects::apply(world, holder, Incap::Stun, 2.0);
            text.push_str("что заставляет вас застыть на месте!");
        }
    }
    mood::add_event(
        world,
        holder,
        "anxiety_eyecontact",
        &ANXIETY_EYECONTACT_EVENT,
    );
    tell_mob(world, holder, (Span::BoldWarning, &text));
    true
}

/// The chat's [`ss13_chat::server::AnxiousSpoke`]: the withdrawal's 6 s of silence and the
/// once-a-life spaghetti (`prob(1)`).
pub(super) fn anxious_speech(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ss13_chat::server::AnxiousSpoke>>,
) {
    let said: Vec<ss13_chat::server::AnxiousSpoke> = cursor
        .read(world.resource::<Messages<ss13_chat::server::AnxiousSpoke>>())
        .copied()
        .collect();
    for spoke in said {
        if world.get_entity(spoke.speaker).is_err() {
            continue;
        }
        if spoke.outcome == ss13_chat::speech::AnxiousSay::Withdrew {
            crate::status_effects::set_timed(
                world,
                spoke.speaker,
                crate::status_effects::Timed::Silence,
                6.0,
                true,
            );
        }
        if spoke.spaghetti
            && let Some(&pos) = world.get::<TilePos>(spoke.speaker)
        {
            crate::construction::spawn_obj(world, SPAGHETTI, pos);
        }
    }
}

/// The taste quirks on a tongue's `(liked, disliked, toxic)` foodtypes: tg
/// `get_liked_foodtypes`/`get_disliked_foodtypes` (`TRAIT_AGEUSIA`, `TRAIT_VEGETARIAN`) and the
/// tongue changes of Deviant Tastes and Ananas Affinity/Aversion. Toxic food stays toxic.
pub fn food_tastes(world: &World, mob: Entity, tastes: (u32, u32, u32)) -> (u32, u32, u32) {
    let Some(quirks) = world.get::<Quirks>(mob) else {
        return tastes;
    };
    let (mut liked, mut disliked, toxic) = tastes;
    if quirks.has(Quirk::DeviantTastes) {
        std::mem::swap(&mut liked, &mut disliked);
    }
    if quirks.has(Quirk::PineappleLiker) {
        liked |= PINEAPPLE;
    }
    if quirks.has(Quirk::PineappleHater) {
        disliked |= PINEAPPLE;
    }
    if quirks.has(Quirk::Ageusia) {
        return (0, 0, toxic);
    }
    if quirks.has(Quirk::Vegetarian) {
        liked &= !MEAT;
        disliked |= MEAT;
    }
    (liked, disliked, toxic)
}

/// `/datum/quirk/csl/add` and `remove`: Galactic Common only partly understood (90%, the
/// default strength), Galactic Uncommon for a mob left with no language at all.
pub(super) fn csl(world: &mut World, mob: Entity, add: bool) {
    use ss13_chat::language::{ALL, LanguageId, LanguageSource, Preset, UNDERSTOOD};
    let species_speaks_common = Preset::of_path(crate::species::def_of(world, mob).languages)
        .is_some_and(|p| p.spoken.contains(&LanguageId::Common));
    ss13_chat::language::edit_holder(world, mob, |h| {
        if add {
            h.remove(
                LanguageId::Common,
                UNDERSTOOD,
                Some(LanguageSource::Species),
            );
            h.remove(LanguageId::Common, UNDERSTOOD, Some(LanguageSource::Atom));
            if h.understood_languages().is_empty() {
                h.grant(LanguageId::Uncommon, ALL, LanguageSource::Quirk);
            }
            h.grant_partial(LanguageId::Common, 90, LanguageSource::Quirk);
        } else {
            h.remove_partial(LanguageId::Common, LanguageSource::Quirk);
            h.remove(LanguageId::Uncommon, ALL, Some(LanguageSource::Quirk));
            // Only a species that speaks Common understands it again.
            if species_speaks_common {
                h.grant(LanguageId::Common, UNDERSTOOD, LanguageSource::Species);
            }
        }
    });
}

/// CSL's `reremove_common` on `COMSIG_SPECIES_GAIN` (`crate::species::set_species`).
pub fn reremove_common(world: &mut World, mob: Entity) {
    use ss13_chat::language::{LanguageId, LanguageSource, UNDERSTOOD};
    if !has(world, mob, Quirk::Csl) {
        return;
    }
    ss13_chat::language::edit_holder(world, mob, |h| {
        h.remove(
            LanguageId::Common,
            UNDERSTOOD,
            Some(LanguageSource::Species),
        );
    });
}

/// `/datum/quirk/foreigner/add` and `remove`: Galactic Common blocked; a plain human learns
/// Galactic Uncommon instead.
// The removal takes back only the quirk's Uncommon (tg removes it from every source).
pub(super) fn foreigner(world: &mut World, mob: Entity, add: bool) {
    use ss13_chat::language::{ALL, LanguageId, LanguageSource};
    let human = crate::species::species_of(world, mob) == crate::species::SpeciesId::Human;
    ss13_chat::language::edit_holder(world, mob, |h| {
        if add {
            h.block(LanguageId::Common, ALL, LanguageSource::Quirk);
            if human {
                h.grant(LanguageId::Uncommon, ALL, LanguageSource::Quirk);
            }
        } else {
            h.unblock(LanguageId::Common, ALL, Some(LanguageSource::Quirk));
            if human {
                h.remove(LanguageId::Uncommon, ALL, Some(LanguageSource::Quirk));
            }
        }
    });
}

/// `/datum/quirk/tin_man/add_unique`: the heart, lungs, liver and stomach the species has
/// become surplus prosthetics (the old organs are deleted), then `post_add`'s warning.
pub(super) fn tin_man(world: &mut World, mob: Entity) {
    for (slot, path, skip) in TIN_MAN_ORGANS {
        if crate::species::has_trait(world, mob, skip) {
            continue;
        }
        let Some(new) = organs::spawn_organ(world, path) else {
            continue;
        };
        // `DELETE_IF_REPLACED`.
        if let Some(old) = organs::organ(world, mob, slot) {
            organs::remove(world, old, true);
            world.despawn(old);
        }
        if !organs::insert(world, new, mob) {
            world.despawn(new);
        }
    }
    tell_mob(
        world,
        mob,
        (
            Span::BoldWarning,
            "Большинство ваших внутренних органов заменены на дешевые протезы. Они хрупкие и \
             легко разлетаются на части под давлением. Кроме того, любое ЭМИ воздействие \
             заставит их полностью прекратить работу.",
        ),
    );
}

/// Body Purist's `check_cybernetics`/`update_mood`: −10 mood per robotic limb or organ.
// Cost: 1 Hz per Body Purist, a pass over its organs (about ten).
pub(super) fn update_cybernetics(world: &mut World, mob: Entity) {
    let limbs = world
        .get::<crate::medical::wounds::Limbs>(mob)
        .map_or(0, |l| {
            l.parts.iter().filter(|p| p.robotic && !p.missing).count()
        });
    let organ_count = world.get::<organs::Organs>(mob).map_or(0, |owned| {
        owned
            .iter()
            .filter_map(|o| world.get::<organs::Organ>(o))
            .filter(|o| o.robotic)
            .count()
    });
    let level = (limbs + organ_count) as u32;
    if world.get::<Cybernetics>(mob) == Some(&Cybernetics(level)) {
        return;
    }
    world.entity_mut(mob).insert(Cybernetics(level));
    mood::clear_event(world, mob, "body_purist");
    if level == 0 {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut mood) = world.get_mut::<Mood>(mob) {
        let mut event = ActiveMood::new("body_purist", &BODY_PURIST_EVENT);
        event.mood_change = -(level as f32) * 10.0;
        mood.add_instance(now, event);
    }
}

pub(super) fn head_covered(world: &World, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.head.is_some())
}

/// Smooth-Headed's `equip_hat`/`unequip_hat`: taking the headwear off is a `bald` moodlet,
/// putting one on clears it.
// shortcut: no wigs (`confident_mane`).
pub(super) fn bald_head(world: &mut World, mob: Entity) {
    let covered = head_covered(world, mob);
    {
        let Some(mut bald) = world.get_mut::<BaldHead>(mob) else {
            return;
        };
        if bald.covered == covered {
            return;
        }
        bald.covered = covered;
    }
    if covered {
        mood::clear_event(world, mob, "bad_hair_day");
    } else {
        mood::add_event(world, mob, "bad_hair_day", &BALD_EVENT);
    }
}
