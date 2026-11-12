//! World side: treating wounds with tools and wounding after a hit.

use super::*;

// World side: treating wounds with tools (tg `try_treating` -> `treat`).

/// What a wound treatment `do_after` does at its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Treat {
    /// `tool_cauterize` of a cut or puncture.
    Cauterize,
    /// A bonesetter on a dislocation.
    SetBone,
    /// Bone gel on a fracture.
    Gel,
    /// Surgical tape on a gelled fracture.
    Tape,
}

pub(super) fn treatment_for(
    wound: &Wound,
    tool: &super::super::surgery::ToolInfo,
    self_treat: bool,
    prepped: bool,
) -> Option<Treat> {
    let path = tool.path.as_deref().unwrap_or_default();
    match wound.def().series {
        Series::Slash | Series::Pierce => (tool.behaviour == "cautery"
            || (self_treat
                && tool.temperature >= super::super::surgery::FIRE_MINIMUM_TEMPERATURE_TO_EXIST))
            .then_some(Treat::Cauterize),
        Series::Bone if wound.kind == WoundKind::Dislocation => {
            (tool.behaviour == "bonesetter").then_some(Treat::SetBone)
        }
        // `item_can_treat` leaves a limb ready to operate on to real surgery.
        Series::Bone if !prepped => {
            if crate::obj_damage::is_under(path, "/obj/item/stack/medical/bone_gel") {
                Some(Treat::Gel)
            } else if crate::obj_damage::is_under(
                path,
                "/obj/item/stack/medical/wrap/sticky_tape/surgical",
            ) {
                Some(Treat::Tape)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// tg `/datum/wound/try_treating` over the wounds of the user's selected zone. True when a
/// wound took the click (a treatment started or was refused).
pub fn try_treating(world: &mut World, actor: crate::tools::Actor, patient: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let zone = world
        .get::<crate::health::ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let Some(limb) = world.get::<Limbs>(patient).map(|l| l.limb(zone).clone()) else {
        return false;
    };
    let tool = super::super::surgery::ToolInfo::read(world, Some(item));
    if super::super::surgery::combat_mode(world, actor.mob) && tool.force > 0.0 {
        return false;
    }
    let self_treat = patient == actor.mob;
    let Some((treat, kind)) = limb
        .wounds
        .iter()
        .find_map(|w| treatment_for(w, &tool, self_treat, limb.prepped).map(|t| (t, w.kind)))
    else {
        return false;
    };
    if world.get::<crate::tools::DoAfter>(actor.mob).is_some() {
        let name = super::super::name_of(world, patient);
        crate::tools::tell(
            world,
            actor.client,
            Span::Warning,
            format!("Вы уже взаимодействуете с {name}!"),
        );
        return true;
    }
    if !crate::chem::try_inject(world, actor, patient) {
        return true;
    }
    start_treatment(world, actor, patient, zone, treat, kind, &tool);
    true
}

pub(super) fn start_treatment(
    world: &mut World,
    actor: crate::tools::Actor,
    patient: Entity,
    zone: BodyZone,
    treat: Treat,
    kind: WoundKind,
    tool: &super::super::surgery::ToolInfo,
) {
    let self_mult = if patient == actor.mob { 1.5 } else { 1.0 };
    let (user, target) = (
        super::super::name_of(world, actor.mob),
        super::super::name_of(world, patient),
    );
    let item = actor
        .item
        .map(|i| super::super::name_of(world, i))
        .unwrap_or_default();
    let part = zone.ru().1;
    let (delay, own, others) = match treat {
        Treat::Cauterize => {
            let improv = if tool.behaviour == "cautery" {
                1.0
            } else {
                1.25
            };
            (
                30.0 * self_mult * improv,
                format!("Вы начинаете прижигать {part} у {target} с помощью {item}..."),
                format!("{user} начинает прижигать {part} у {target} с помощью {item}..."),
            )
        }
        Treat::SetBone => (
            50.0 * self_mult,
            format!("Вы начинаете вправлять {part} у {target} с помощью {item}..."),
            format!("{user} начинает вправлять {part} у {target} с помощью {item}..."),
        ),
        Treat::Gel => {
            let gelled = world
                .get::<Limbs>(patient)
                .and_then(|l| l.limb(zone).wound(Series::Bone).map(|w| w.gelled))
                .unwrap_or(true);
            if gelled {
                return;
            }
            (
                50.0 * 1.5 * self_mult,
                format!("Вы начинаете наносить {item} на {part} у {target}..."),
                format!("{user} начинает наносить {item} на {part} у {target}..."),
            )
        }
        Treat::Tape => {
            let ready = world
                .get::<Limbs>(patient)
                .and_then(|l| {
                    l.limb(zone)
                        .wound(Series::Bone)
                        .map(|w| w.gelled && !w.taped)
                })
                .unwrap_or(false);
            if !ready {
                return;
            }
            (
                50.0 * self_mult,
                format!("Вы начинаете обматывать {part} у {target} с помощью {item}..."),
                format!("{user} начинает обматывать {part} у {target} с помощью {item}..."),
            )
        }
    };
    super::super::visible(world, actor.mob, &own, &others, Span::Warning);
    crate::tools::do_after(
        world,
        actor,
        patient,
        delay,
        0,
        crate::construction::Job::Medical(super::super::MedJob::Treat { zone, treat, kind }),
    );
}

/// The end of a wound treatment.
pub fn finish_treatment(
    world: &mut World,
    actor: crate::tools::Actor,
    patient: Entity,
    zone: BodyZone,
    treat: Treat,
    kind: WoundKind,
) {
    let Some(mut limbs) = world.get::<Limbs>(patient).cloned() else {
        return;
    };
    let series = kind.def().series;
    // `still_exists`: that very wound is still there; one that worsened meanwhile is a new
    // wound (`replace_wound`), so a dislocation turned compound fracture is not set.
    let Some(index) = limbs.limb(zone).wounds.iter().position(|w| w.kind == kind) else {
        return;
    };
    let self_treat = patient == actor.mob;
    let self_mult = if self_treat { 1.5 } else { 1.0 };
    let mut rng = super::super::fork_rng(world);
    let (user, target) = (
        super::super::name_of(world, actor.mob),
        super::super::name_of(world, patient),
    );
    match treat {
        Treat::Cauterize => {
            let tool = super::super::surgery::ToolInfo::read(world, actor.item);
            let improv = if tool.behaviour == "cautery" {
                1.0
            } else {
                1.25
            };
            let severity = f32::from(limbs.limb(zone).wounds[index].def().severity);
            let limb = limbs.limb_mut(zone);
            let by = -0.6 / (self_mult * improv);
            let left = match series {
                Series::Slash => {
                    let mut effects = Vec::new();
                    adjust_slash(limb, index, by, zone, &mut effects);
                    limb.wounds
                        .iter()
                        .find(|w| w.def().series == Series::Slash)
                        .is_some_and(|w| w.blood_flow > w.def().minimum_flow)
                }
                _ => adjust_pierce(limb, index, by),
            };
            world.entity_mut(patient).insert(limbs);
            super::super::hurt_limb(world, patient, zone, DamageType::Burn, 2.0 + severity);
            super::super::visible(
                world,
                actor.mob,
                &format!("Вы прижигаете часть кровотечений у {target}."),
                &format!("{user} прижигает часть кровотечений у {target}."),
                Span::Notice,
            );
            if left {
                try_treating(world, actor, patient);
            }
        }
        Treat::SetBone => {
            heal_wound(limbs.limb_mut(zone), index);
            world.entity_mut(patient).insert(limbs);
            super::super::hurt_limb(
                world,
                patient,
                zone,
                DamageType::Brute,
                if self_treat { 15.0 } else { 10.0 },
            );
            super::super::visible(
                world,
                actor.mob,
                &format!("Вы вправляете {} у {target} на место!", zone.ru().1),
                &format!("{user} вправляет {} у {target} на место!", zone.ru().1),
                Span::Notice,
            );
        }
        Treat::Gel => {
            let Some(item) = actor.item else {
                return;
            };
            if limbs.limb(zone).wounds[index].gelled || !crate::tools::use_stack(world, item, 1) {
                return;
            }
            let severity = f32::from(kind.def().severity);
            if self_treat && rng.chance(25.0 + 20.0 * (severity - 2.0)) {
                super::super::visible(
                    world,
                    actor.mob,
                    "Вы теряете сознание от боли, нанося гель на кость!",
                    &format!("{user} теряет сознание от боли!"),
                    Span::Danger,
                );
                crate::status_effects::apply(
                    world,
                    patient,
                    crate::status_effects::Incap::Paralyze,
                    5.0,
                );
                return;
            }
            limbs.limb_mut(zone).wounds[index].gelled = true;
            world.entity_mut(patient).insert(limbs);
            super::super::hurt_limb(world, patient, zone, DamageType::Brute, 25.0);
            crate::status_effects::adjust_stamina(world, patient, 100.0);
            super::super::visible(
                world,
                actor.mob,
                &format!(
                    "Вы наносите гель на {} у {target}, кость начинает срастаться!",
                    zone.ru().1
                ),
                &format!("{user} наносит гель на {} у {target}!", zone.ru().1),
                Span::Notice,
            );
        }
        Treat::Tape => {
            let Some(item) = actor.item else {
                return;
            };
            let wound = &limbs.limb(zone).wounds[index];
            if !wound.gelled || wound.taped || !crate::tools::use_stack(world, item, 1) {
                return;
            }
            let wound = &mut limbs.limb_mut(zone).wounds[index];
            if self_treat {
                wound.regen_ticks_needed = (wound.regen_ticks_needed as f32 * 1.5) as u32;
            }
            wound.taped = true;
            world.entity_mut(patient).insert(limbs);
            super::super::visible(
                world,
                actor.mob,
                &format!(
                    "Вы закрепляете {} у {target} хирургической лентой.",
                    zone.ru().1
                ),
                &format!(
                    "{user} закрепляет {} у {target} хирургической лентой.",
                    zone.ru().1
                ),
                Span::Notice,
            );
        }
    }
}

/// `/datum/quirk/glass_jaw/punch_out`: 5 or more brute to the head knocks its owner out for
/// 3 s with a chance of the damage in percent (a sharp hit 65% of it).
fn glass_jaw(world: &mut World, victim: Entity, zone: BodyZone, hit: &Hit) {
    if zone != BodyZone::Head
        || hit.kind != crate::health::DamageType::Brute
        || hit.damage < 5.0
        || !crate::quirks::has(world, victim, crate::quirks::Quirk::GlassJaw)
    {
        return;
    }
    let chance = (hit.damage * if hit.sharpness != 0 { 0.65 } else { 1.0 }).ceil();
    if !world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(chance)
    {
        return;
    }
    if world.get::<Stat>(victim) == Some(&Stat::Conscious) {
        let name = crate::security::name_of(world, victim);
        crate::chem::visible(
            world,
            victim,
            "Вы отправлены в нокаут!",
            &format!("{name} отправляется в нокаут!"),
            ss13_shared::chat::Span::Warning,
        );
    }
    crate::status_effects::apply(
        world,
        victim,
        crate::status_effects::Incap::Unconscious,
        3.0,
    );
}

/// tg `receive_damage`'s wound handling after a hit landed on `victim`'s `zone` (melee, thrown
/// items): the roll, the existing wounds' reactions, the messages, blood lost at once and a
/// dropped item.
pub fn after_hit(world: &mut World, victim: Entity, zone: BodyZone, hit: Hit) {
    glass_jaw(world, victim, zone, &hit);
    let Some(mut limbs) = world.get::<Limbs>(victim).cloned() else {
        return;
    };
    // The hit went to the chest (`crate::health::Health::damage_zone`).
    let zone = if limbs.limb(zone).missing {
        BodyZone::Chest
    } else {
        zone
    };
    // `check_woundings_mods` with `wound_clothing`: the clothing there takes zone damage.
    crate::clothing::damage::after_hit(world, victim, zone, hit);
    let dead = world.get::<Stat>(victim) == Some(&Stat::Dead);
    let has_blood = world
        .get::<super::super::blood::Blood>(victim)
        .is_some_and(|b| b.volume > 0.0);
    let can_dismember = super::super::bodyparts::can_dismember(world, victim, zone);
    let traits = WoundTraits {
        worn_wound_armor: crate::armor::worn_wound_armor(world, victim, zone),
        ..WoundTraits::of(world, victim)
    };
    let mut rng = super::super::fork_rng(world);
    let out = receive_damage_with(
        &mut rng,
        limbs.limb_mut(zone),
        zone,
        hit,
        dead,
        has_blood,
        can_dismember,
        traits,
    );
    world.entity_mut(victim).insert(limbs);
    if let Some((wounding, outright)) = out.dismember {
        super::super::bodyparts::apply_dismember(world, victim, zone, wounding, outright);
        return;
    }
    if let Some(kind) = out.gained {
        let def = kind.def();
        let name = super::super::name_of(world, victim);
        let part = capitalize(zone.ru().0);
        let others = format!("{part} {name} {}!", def.occur_text);
        let own = format!("Ваша {} {}!", zone.ru().0, def.occur_text);
        let range = if def.severity > 2 {
            crate::health::DEFAULT_MESSAGE_RANGE
        } else {
            crate::health::COMBAT_MESSAGE_RANGE
        };
        super::super::visible_range(world, victim, &own, &others, Span::Danger, range);
    }
    for (amount, text) in out.bled {
        super::super::blood::bleed(world, victim, amount);
        if !text.is_empty()
            && let Some(client) = world
                .get::<crate::server::PlayerMob>(victim)
                .map(|p| p.client)
        {
            crate::tools::tell(world, client, Span::Danger, text);
        }
    }
    if let Some(hand) = out.drop_hand
        && let Some(item) = world
            .get::<crate::inventory::Inventory>(victim)
            .and_then(|inv| inv.hands[hand])
        && let Some(&pos) = world.get::<ss13_shared::TilePos>(victim)
    {
        world
            .entity_mut(item)
            .remove::<crate::inventory::CarriedBy>()
            .insert(pos);
    }
}
