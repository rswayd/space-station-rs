//! What rituals make (`code/modules/antagonists/heretic/items/`, `structures/`): the foci and
//! amulets (`heretic_necks.dm`), the Abyssal Mask (`madness_mask.dm`), the flask of eldritch
//! essence and its reagent (`eldritch_flask.dm`, `/datum/reagent/eldritch`), the phylactery and
//! the ether, the mawed crucible and its brews (`mawed_crucible.dm`).
//!
//! - Amber focus on the neck of a heretic: every 3 minutes its spells get back their
//!   `focus_recharge_amount` of charges; the crimson medallion does it every minute, halves the
//!   heretic's spell cooldowns and can be squeezed (1.25 s) for 50 brute/burn healing.
//! - Eldritch amulets give heretics and their summons thermal (warm) or x-ray (piercing)
//!   vision, the moonlight amulet thermal vision; a non-heretic wearing the moonlight amulet loses
//!   sanity.
//! - The Abyssal Mask drains the sanity of the crew its wearer sees and sticks to a
//!   non-heretic's face.
//! - Eldritch essence heals heretics and their summons and poisons everyone else.
//! - The mawed crucible eats organs and bodyparts (a non-heretic's hand), refills a mass every
//!   30 s and, full, brews a potion or refills an eldritch flask; the codex or the grasp
//!   anchors it.
// shortcut: no crimson medallion aura healing or cult buffs; the moonlight amulet does not
// convert (no moon_converted / moon_slept) and does not change blades; the mask ignores
// blindness and magic resistance; eldritch essence does not purge other reagents, warm or add
// blood; the crucible spills no glow when broken; the brew of the crucible soul (phasing) and of
// the wounded soldier (wound healing) are drunk without effect; the ether puts to sleep without
// stasis or implant removal; the phylactery draws only in reach (tg: 30 tiles). Add with
// incorporeal movement, wound healing and ranged item clicks.

use super::*;
use crate::clothing::traits::{THERMAL_VISION, XRAY_VISION};
use crate::sound::PlaySound;
use crate::status_effects::Timed;

pub const PAINTING: &str = "/datum/heretic_knowledge/painting";
const FOCUS: &str = "/obj/item/clothing/neck/heretic_focus";
const CRIMSON_MEDALLION: &str = "/obj/item/clothing/neck/heretic_focus/crimson_medallion";
const ELDRITCH_AMULET: &str = "/obj/item/clothing/neck/eldritch_amulet";
const PIERCING_AMULET: &str = "/obj/item/clothing/neck/eldritch_amulet/piercing";
const MOON_AMULET: &str = "/obj/item/clothing/neck/moon_amulet";
const MADNESS_MASK: &str = "/obj/item/clothing/mask/madness_mask";
pub const CRUCIBLE: &str = "/obj/structure/destructible/eldritch_crucible";
const FLASK: &str = "/obj/item/reagent_containers/cup/beaker/eldritch";
const PHYLACTERY: &str = "/obj/item/reagent_containers/cup/phylactery";
const ETHER: &str = "/obj/item/ether";
const POTION: &str = "/obj/item/eldritch_potion";
const CRUCIBLE_SOUL: &str = "/obj/item/eldritch_potion/crucible_soul";
const DUSK_AND_DAWN: &str = "/obj/item/eldritch_potion/duskndawn";
const WOUNDED: &str = "/obj/item/eldritch_potion/wounded";
const ELDRITCH_REAGENT: &str = "/datum/reagent/eldritch";
const UNHOLY_WATER: &str = "/datum/reagent/fuel/unholywater";
const GIBS: &str = "/obj/effect/gibspawner/generic";
const ELDRITCH_OBJ_DMI: &str = "icons/obj/antags/eldritch.dmi";

/// `heretic_focus` `cooldown_period`, the crimson medallion's.
const FOCUS_PERIOD: f64 = 180.0;
const CRIMSON_PERIOD: f64 = 60.0;
/// The crimson medallion's squeeze (`do_after(user, 1.25 SECONDS)`), deciseconds.
const SQUEEZE_DS: f32 = 12.5;
/// The crucible's `max_mass` and refill cooldown.
pub const CRUCIBLE_MAX_MASS: u32 = 3;
const CRUCIBLE_REFILL_SECS: f64 = 30.0;
/// `/datum/status_effect/duskndawn` duration.
const DUSK_SECS: f64 = 90.0;
/// The phylactery's `drain_cooldown`.
const PHYLACTERY_CD: f64 = 5.0;

/// The potions in the crucible's radial: (type, name, `crucible_tip`).
pub const POTIONS: [(&str, &str, &str); 3] = [
    (
        CRUCIBLE_SOUL,
        "отвар из души тигеля",
        "Позволяет проходить сквозь стены. После истечения срока действия вы телепортируетесь в \
         исходное место. Действует 40 секунд.",
    ),
    (
        DUSK_AND_DAWN,
        "отвар заката и рассвета",
        "Позволяет видеть сквозь стены и предметы. Действует 90 секунд.",
    ),
    (
        WOUNDED,
        "отвар раненого солдата",
        "Приводит к тому, что все ранения начинают лечить вас. Переломы, растяжения, порезы и \
         проколы исцеляют ушибы а повреждения плоти исцеляют ожоги. Чем серьезнее раны, тем \
         сильнее исцеление. Кроме того, предотвращает замедление от повреждений. Действует 60 \
         секунд. ",
    ),
];

/// `/datum/mood_event/amulet_insanity`.
const AMULET_INSANITY: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "amulet_insanity",
    description: "Я вИжУ сВеТ, и ЕгО нУжНо ПоТуШиТь!",
    mood_change: -6.0,
    timeout: 300.0,
    hidden: false,
};

/// A heretic's focus on its neck (`spell_recharge_cd`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FocusWorn {
    pub item: Entity,
    pub next_at: f64,
}

/// A moonlight amulet a non-heretic put on (`channel_amulet` ran once for it).
#[derive(Component, Clone, Copy, Debug)]
pub struct MoonAmuletWorn(pub Entity);

/// The Abyssal Mask on someone's face (`local_user`).
#[derive(Component, Clone, Copy, Debug)]
pub struct MaskWearer(pub Entity);

/// `/obj/structure/destructible/eldritch_crucible`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Crucible {
    pub mass: u32,
    pub next_refill: f64,
    pub in_use: bool,
}

impl Default for Crucible {
    fn default() -> Self {
        Self {
            mass: CRUCIBLE_MAX_MASS,
            next_refill: 0.0,
            in_use: false,
        }
    }
}

/// `/datum/status_effect/duskndawn`: x-ray vision until then.
#[derive(Component, Clone, Copy, Debug)]
pub struct DuskAndDawn {
    pub until: f64,
}

/// The phylactery's `drain_cooldown`.
#[derive(Component, Clone, Copy, Debug)]
pub struct PhylacteryCooldown(pub f64);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The crimson medallion squeezed, a brew or the ether drunk.
    app.add_click_handler(ClickPhase::AttackSelf, 601, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    // The mawed crucible: a brew, or a non-heretic's hand.
    app.add_click_handler(ClickPhase::EmptyHand, 401, |w, c| {
        crucible_hand(w, c.client, c.mob, c.target)
    });
    // Before the grasp's and the codex's own clicks (1400): the crucible takes organs, fills
    // flasks and is anchored; the phylactery steals blood (right click); wirecutters take an
    // eldritch painting down.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1391, |w, c| {
        c.held
            .is_some_and(|h| held_on_target(w, c.client, c.mob, h, c.target, c.modifiers.right))
    });
    app.add_systems(
        Update,
        (tag_crucibles, items_tick)
            .chain()
            .before(crate::health::HealthSystems),
    )
    .add_systems(
        Update,
        eldritch_essence.before(crate::health::HealthSystems),
    );
}

fn is_under(world: &World, e: Entity, root: &str) -> bool {
    path_of(world, e).is_some_and(|p| knowledge::is_type(&p, root))
}

/// The neck item's type.
fn neck_path(world: &World, mob: Entity) -> Option<(Entity, String)> {
    let item = world.get::<Inventory>(mob)?.neck?;
    Some((item, path_of(world, item)?))
}

/// What the heretic amulets and the brew of dusk and dawn add to [`crate::vision::worn_vision`]:
/// (thermal, x-ray).
pub fn heretic_vision(world: &World, mob: Entity) -> (bool, bool) {
    let mut thermal = false;
    let mut xray = world.get::<DuskAndDawn>(mob).is_some();
    if let Some((_, path)) = neck_path(world, mob)
        && world.get::<HumanLook>(mob).is_some()
    {
        if knowledge::is_type(&path, ELDRITCH_AMULET) && is_heretic_or_monster(world, mob) {
            if knowledge::is_type(&path, PIERCING_AMULET) {
                xray = true;
            } else {
                thermal = true;
            }
        }
        if knowledge::is_type(&path, MOON_AMULET) && is_heretic(world, mob) {
            thermal = true;
        }
    }
    (thermal, xray)
}

/// [`heretic_vision`] as clothing trait names.
pub fn has_heretic_vision(world: &World, mob: Entity, name: &str) -> bool {
    let (thermal, xray) = heretic_vision(world, mob);
    (thermal && name == THERMAL_VISION) || (xray && name == XRAY_VISION)
}

/// The crimson medallion's `cooldown_time *= 0.5` on a heretic or summon wearing it.
pub fn hastened(world: &World, mob: Entity) -> bool {
    is_heretic_or_monster(world, mob)
        && neck_path(world, mob).is_some_and(|(_, p)| knowledge::is_type(&p, CRIMSON_MEDALLION))
}

/// `focus_recharge_amount` of the spells (the share of `max_charges` a focus gives back).
pub fn focus_recharge_amount(knowledge: &str) -> f32 {
    match knowledge.rsplit('/').next().unwrap_or_default() {
        "rust_construction" => 0.5,
        "burglar_finesse" | "area_conversion" | "mind_gate" | "realignment" | "furious_steel"
        | "fire_blast" | "flame_birth" => 0.33,
        "entropic_plume" | "basic" | "moon_parade" | "moon_ringleader" | "star_blast"
        | "star_touch" | "void_phase" | "void_pull" => 0.25,
        "ash_passage" => 0.15,
        _ => 0.0,
    }
}

/// `add_charges(ceil(max_charges * focus_recharge_amount))`, capped.
pub fn focus_charges(charges: u32, max: u32, amount: f32) -> u32 {
    let add = (max as f32 * amount).ceil() as u32;
    charges.saturating_add(add).min(max)
}

/// `eldritch_crucible/process` at `t`: true when a mass came back.
pub fn crucible_refill(c: &mut Crucible, t: f64) -> bool {
    if c.mass >= CRUCIBLE_MAX_MASS || t < c.next_refill {
        return false;
    }
    c.mass += 1;
    c.next_refill = t + CRUCIBLE_REFILL_SECS;
    true
}

/// `heretic_focus/process`: the spells get charges back.
fn recharge_spells(world: &mut World, mob: Entity) {
    let mut changed = false;
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        for r in &mut h.researched {
            let def = r.def();
            if def.kind != Kind::Spell || def.max_charges == u32::MAX {
                continue;
            }
            let new = focus_charges(r.charges, def.max_charges, focus_recharge_amount(def.path));
            changed |= new != r.charges;
            r.charges = new;
        }
    }
    if changed {
        refresh(world, mob);
    }
}

/// Once a second: foci, the moonlight amulet, the Abyssal Mask; every tick: crucible refills
/// and the brew of dusk and dawn running out.
fn items_tick(world: &mut World, mut next_second: Local<f64>) {
    let t = now(world);
    tick_crucibles(world, t);
    let mut q = world.query::<(Entity, &DuskAndDawn)>();
    let over: Vec<Entity> = q
        .iter(world)
        .filter(|(_, d)| d.until <= t)
        .map(|(e, _)| e)
        .collect();
    for e in over {
        world.entity_mut(e).remove::<DuskAndDawn>();
    }
    if t < *next_second {
        return;
    }
    *next_second = t + 1.0;
    tick_foci(world, t);
    tick_moon_amulets(world);
    tick_masks(world, 1.0);
}

fn tick_foci(world: &mut World, t: f64) {
    let mut q = world.query_filtered::<Entity, With<Heretic>>();
    let heretics: Vec<Entity> = q.iter(world).collect();
    for mob in heretics {
        let focus = neck_path(world, mob).filter(|(_, p)| knowledge::is_type(p, FOCUS));
        let worn = world.get::<FocusWorn>(mob).copied();
        match (focus, worn) {
            (None, Some(_)) => {
                world.entity_mut(mob).remove::<FocusWorn>();
            }
            (None, None) => {}
            (Some((item, path)), worn) => {
                let period = if knowledge::is_type(&path, CRIMSON_MEDALLION) {
                    CRIMSON_PERIOD
                } else {
                    FOCUS_PERIOD
                };
                match worn {
                    // `equipped`: the cooldown starts.
                    Some(w) if w.item == item => {
                        if t >= w.next_at {
                            recharge_spells(world, mob);
                            world.entity_mut(mob).insert(FocusWorn {
                                item,
                                next_at: t + period,
                            });
                        }
                    }
                    _ => {
                        world.entity_mut(mob).insert(FocusWorn {
                            item,
                            next_at: t + period,
                        });
                    }
                }
            }
        }
    }
}

/// `moon_amulet/equipped` by a non-heretic: a fat mood debuff, once per wearing.
fn tick_moon_amulets(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &Inventory), With<HumanLook>>();
    let wearers: Vec<(Entity, Option<Entity>)> = q.iter(world).map(|(e, i)| (e, i.neck)).collect();
    for (mob, neck) in wearers {
        let amulet = neck.filter(|&n| is_under(world, n, MOON_AMULET));
        let seen = world.get::<MoonAmuletWorn>(mob).map(|m| m.0);
        if amulet == seen {
            continue;
        }
        match amulet {
            None => {
                world.entity_mut(mob).remove::<MoonAmuletWorn>();
            }
            Some(item) => {
                world.entity_mut(mob).insert(MoonAmuletWorn(item));
                if !is_heretic_or_monster(world, mob) {
                    tell_balloon(
                        world,
                        mob,
                        "вы чувствуете чьё-то присутствие, наблюдающее за вами",
                    );
                    crate::mood::add_event(world, mob, "Moon Amulet Insanity", &AMULET_INSANITY);
                    super::super::paths::adjust_sanity(world, mob, -50.0);
                }
            }
        }
    }
}

fn tell_balloon(world: &mut World, mob: Entity, text: &str) {
    chat_to(world, mob, Span::Warning, text);
}

/// `madness_mask/equipped`, `dropped` and `process`.
fn tick_masks(world: &mut World, secs: f32) {
    // Who wears a mask now.
    let mut q = world.query_filtered::<(Entity, &Inventory), With<HumanLook>>();
    let worn: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter_map(|(e, i)| i.mask.map(|m| (e, m)))
        .collect();
    let worn: Vec<(Entity, Entity)> = worn
        .into_iter()
        .filter(|&(_, m)| is_under(world, m, MADNESS_MASK))
        .collect();
    // `dropped`: masks no longer worn lose their grip.
    let mut q = world.query::<(Entity, &MaskWearer)>();
    let tracked: Vec<(Entity, Entity)> = q.iter(world).map(|(m, w)| (m, w.0)).collect();
    for (mask, wearer) in tracked {
        if !worn.contains(&(wearer, mask)) && world.get_entity(mask).is_ok() {
            world.entity_mut(mask).remove::<(MaskWearer, NoDrop)>();
        }
    }
    for (wearer, mask) in worn {
        let heretic = is_heretic_or_monster(world, wearer);
        if world.get::<MaskWearer>(mask).map(|w| w.0) != Some(wearer) {
            world.entity_mut(mask).insert(MaskWearer(wearer));
            if !heretic {
                world.entity_mut(mask).insert(NoDrop);
                let name = capitalize(&name_of(world, mask));
                chat_to(
                    world,
                    wearer,
                    Span::Danger,
                    format!(
                        "{name} плотно прижимается к вашему лицу, и вы начинаете чувствовать, как \
                         из вас вытекает душа!"
                    ),
                );
            }
        }
        if heretic && world.get::<NoDrop>(mask).is_some() {
            world.entity_mut(mask).remove::<NoDrop>();
        }
        madness_pulse(world, wearer, secs);
    }
}

/// `madness_mask/process` on the crew the wearer sees.
fn madness_pulse(world: &mut World, wearer: Entity, secs: f32) {
    let Some(&pos) = world.get::<TilePos>(wearer) else {
        return;
    };
    let seen: Vec<Entity> = carbons_in_range(world, pos, 7)
        .into_iter()
        .filter(|&e| {
            world
                .get::<TilePos>(e)
                .is_some_and(|&p| crate::mobs::ai::behaviors::can_see(world, pos, p, 7))
        })
        .collect();
    for victim in seen {
        if is_heretic_or_monster(world, victim) || unconscious(world, victim) {
            continue;
        }
        let drain = 2.0 + rand(world, 19) as f32;
        super::super::paths::adjust_sanity(world, victim, -drain * secs);
        let (halluc, jitter, laugh, dizzy) = {
            let mut rng = world.resource_mut::<CombatRng>();
            (
                rng.chance(60.0 * secs),
                rng.chance(40.0 * secs),
                rng.chance(30.0 * secs),
                rng.chance(25.0 * secs),
            )
        };
        if halluc {
            crate::hallucination::adjust_hallucinations_up_to(world, victim, 10.0, 120.0);
        }
        if jitter {
            crate::status_effects::set_timed(world, victim, Timed::Jitter, 10.0, true);
        }
        let stamina = world
            .get::<crate::status_effects::StatusEffects>(victim)
            .map_or(0.0, |s| s.stamina);
        if stamina <= 85.0 && laugh {
            let text = if rand(world, 2) == 0 {
                "хихикает."
            } else {
                "смеётся."
            };
            crate::mobs::defense::emote(world, victim, text);
            crate::status_effects::adjust_stamina(world, victim, 10.0);
        }
        if dizzy {
            crate::status_effects::set_timed(world, victim, Timed::Dizziness, 10.0, true);
        }
    }
}

/// `IS_UNCONSCIOUS`.
pub(super) fn unconscious(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob).copied().unwrap_or_default() >= Stat::HardCrit
        || crate::status_effects::incapacitation(world, mob).knocked_out
}

/// `/datum/reagent/eldritch/on_mob_life` every `Life()`.
fn eldritch_essence(world: &mut World, mut last_fired: Local<Option<u64>>) {
    let Some(clock) = world.get_resource::<crate::breath::MobsClock>() else {
        return;
    };
    let fired = clock.times_fired;
    let fires = fired.saturating_sub(last_fired.unwrap_or(fired));
    *last_fired = Some(fired);
    if fires == 0 {
        return;
    }
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(ELDRITCH_REAGENT) else {
        return;
    };
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32 * fires as f32;
    let mut q = world.query::<(Entity, &crate::chem::MobReagents)>();
    let mut drinkers: Vec<Entity> = q
        .iter(world)
        .filter(|(_, r)| r.blood.has(id).is_some())
        .map(|(e, _)| e)
        .collect();
    drinkers.sort();
    for mob in drinkers {
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            continue;
        }
        let common = 0.4 * secs;
        if is_heretic_or_monster(world, mob) {
            // tg's drowsiness and immobility amounts are deciseconds.
            if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
                effects.adjust_timed(Timed::Drowsiness, -0.2 * secs, None);
                for kind in [
                    Incap::Stun,
                    Incap::Knockdown,
                    Incap::Paralyze,
                    Incap::Immobilize,
                ] {
                    effects.adjust(kind, -0.8 * secs);
                }
            }
            heal(world, mob, common);
            crate::status_effects::adjust_stamina(world, mob, -(2.0 * secs - common));
        } else {
            crate::medical::organs::adjust_organ_loss_up_to(
                world,
                mob,
                crate::medical::organs::Slot::Brain,
                0.6 * secs,
                150.0,
            );
            for kind in [
                DamageType::Tox,
                DamageType::Burn,
                DamageType::Oxy,
                DamageType::Brute,
            ] {
                hurt(world, mob, kind, common);
            }
        }
    }
}

/// Adds `amount` of `path` to a mob's blood (`reagents.add_reagent`).
pub(super) fn add_to_blood(world: &mut World, mob: Entity, path: &str, amount: f32) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(path) else {
        return;
    };
    if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
        reagents
            .blood
            .add_reagent(&chem, id, amount, 310.15, None, None, false, false);
    }
}

// ---------------------------------------------------------------------------------------------
// Use in hand

/// The crimson medallion, the brews and the ether used in hand.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(path) = path_of(world, item) else {
        return false;
    };
    if knowledge::is_type(&path, CRIMSON_MEDALLION) {
        let name = name_of(world, item);
        tell(
            world,
            client,
            Span::Danger,
            format!("Вы начинаете крепко сжимать {name}..."),
        );
        let actor = Actor {
            client,
            mob,
            item: Some(item),
        };
        do_after(
            world,
            actor,
            mob,
            SQUEEZE_DS,
            0,
            Job::Heretic(HereticJob::Squeeze),
        );
        return true;
    }
    if knowledge::is_type(&path, POTION) {
        drink_potion(world, client, mob, item, &path);
        return true;
    }
    if path == ETHER {
        // `user.revive(HEAL_ALL)`, then a minute of sleep.
        heal(world, mob, 1000.0);
        crate::status_effects::apply(world, mob, Incap::Sleeping, 60.0);
        despawn_held(world, mob, item);
        return true;
    }
    false
}

/// The squeeze's end: gibs, 50 healing, unholy water and eldritch essence; the medallion goes.
pub(super) fn squeeze_medallion(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some(item) = action.item else {
        return;
    };
    if !is_under(world, item, CRIMSON_MEDALLION) {
        return;
    }
    let name = capitalize(&name_of(world, item));
    tell(
        world,
        action.client,
        Span::Danger,
        format!(
            "{name} взрывается, заливая вашу руку кровью. Вы чувствуете, как кровь впитывается в \
             вашу кожу. Вы сразу же начинаете чувствовать себя лучше, но вскоре ваши вены \
             начинают зудеть и это ощущение сменяется ощущением пустоты."
        ),
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::construction::spawn_obj(world, GIBS, pos);
    }
    // `adjust_brute_loss(-50)`, the rest from burn.
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        let brute = health.brute.min(50.0);
        health.adjust(DamageType::Brute, -brute);
        health.adjust(DamageType::Burn, -(50.0 - brute));
        world.entity_mut(mob).insert(health);
    }
    let (a, b) = (6 + rand(world, 5), 6 + rand(world, 5));
    add_to_blood(world, mob, UNHOLY_WATER, a as f32);
    add_to_blood(world, mob, ELDRITCH_REAGENT, b as f32);
    despawn_held(world, mob, item);
}

/// `eldritch_potion/attack_self`.
fn drink_potion(world: &mut World, client: Entity, mob: Entity, item: Entity, path: &str) {
    if world.get::<HumanLook>(mob).is_none() {
        return;
    }
    let name = name_of(world, item);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            PlaySound::new(pos, "sound/effects/bubbles/bubbles.ogg", 50.0, true),
        );
    }
    if !is_heretic_or_monster(world, mob) {
        tell(
            world,
            client,
            Span::Danger,
            format!(
                "Вы выпиваете немного жидкости из {name}. Вкус вызывает у вас отвращение, и стакан \
                 исчезает."
            ),
        );
        add_to_blood(world, mob, ELDRITCH_REAGENT, 10.0);
        despawn_held(world, mob, item);
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы выпиваете вязкую жидкость из {name}, стакан дематериализовался."),
    );
    if path == DUSK_AND_DAWN {
        let until = now(world) + DUSK_SECS;
        world.entity_mut(mob).insert(DuskAndDawn { until });
    }
    despawn_held(world, mob, item);
}

// ---------------------------------------------------------------------------------------------
// The mawed crucible

/// New crucibles get their state.
fn tag_crucibles(
    mut commands: Commands,
    q: Query<(Entity, &ss13_mapload::DmObj), Added<ss13_mapload::DmObj>>,
) {
    for (e, obj) in &q {
        if knowledge::is_type(&obj.path, CRUCIBLE) {
            commands.entity(e).insert(Crucible::default());
        }
    }
}

/// `eldritch_crucible/process`: a mass every 30 s.
fn tick_crucibles(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &Crucible)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| {
            let mut c = **c;
            crucible_refill(&mut c, t)
        })
        .map(|(e, _)| e)
        .collect();
    for e in due {
        if let Some(mut c) = world.get_mut::<Crucible>(e) {
            crucible_refill(&mut c, t);
        }
        eat_sound(world, e);
        update_crucible_icon(world, e);
    }
}

fn eat_sound(world: &mut World, e: Entity) {
    if let Some(&pos) = world.get::<TilePos>(e) {
        crate::sound::playsound(
            world,
            PlaySound::new(pos, "sound/items/eatfood.ogg", 100.0, true),
        );
    }
}

/// `update_icon_state`: `crucible` full, `crucible_empty` otherwise.
fn update_crucible_icon(world: &mut World, e: Entity) {
    let Some(c) = world.get::<Crucible>(e).copied() else {
        return;
    };
    let state = if c.mass == CRUCIBLE_MAX_MASS {
        "crucible"
    } else {
        "crucible_empty"
    };
    if let Some(id) = icon(world, ELDRITCH_OBJ_DMI, state)
        && let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(e)
    {
        appearance.icon_state = id;
    }
}

/// The crucible's examine lines for heretics, their summons and ghosts.
pub fn examine_lines(
    world: &World,
    examiner: Entity,
    target: Entity,
    lines: &mut Vec<crate::examine::Line>,
) {
    let Some(c) = world.get::<Crucible>(target) else {
        return;
    };
    if !is_heretic_or_monster(world, examiner) && world.get::<Ghost>(examiner).is_none() {
        return;
    }
    let name = name_of(world, target);
    if c.mass > 0 {
        lines.push((
            Span::Notice,
            "Вы можете пополнять потустороннюю флягу из него.".into(),
        ));
    }
    if c.mass < CRUCIBLE_MAX_MASS {
        lines.push((
            Span::Notice,
            format!(
                "Для {name} необходимо больше органов или частей тела: {}.",
                CRUCIBLE_MAX_MASS - c.mass
            ),
        ));
    } else {
        lines.push((
            Span::BoldNotice,
            format!(
                "{} пузырится вязкой жидкостью до краев и готов к использованию.",
                capitalize(&name)
            ),
        ));
    }
    let anchored = crate::obj_damage::anchored(world, target);
    lines.push((
        Span::Notice,
        format!(
            "Вы можете {} {name} с помощью Codex Cicatrix или хваткой Мансуса.",
            if anchored {
                "открепить и переместить"
            } else {
                "закрепить на месте"
            }
        ),
    ));
    lines.push((Span::Info, "Можно сварить следующие зелья:".into()));
    for (_, potion, tip) in POTIONS {
        lines.push((Span::Info, format!("{potion} - {tip}")));
    }
}

/// `eldritch_crucible/attack_hand`.
pub fn crucible_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(c) = world.get::<Crucible>(target).copied() else {
        return false;
    };
    if !is_heretic_or_monster(world, mob) {
        if world.get::<HumanLook>(mob).is_some() {
            bite_the_hand(world, client, mob, target);
        }
        return true;
    }
    if c.in_use {
        tell(world, client, Span::Warning, "используется!");
        return true;
    }
    if c.mass < CRUCIBLE_MAX_MASS {
        tell(world, client, Span::Warning, "не достаточно полный!");
        return true;
    }
    if !is_heretic(world, mob) {
        return true;
    }
    if let Some(mut c) = world.get_mut::<Crucible>(target) {
        c.in_use = true;
    }
    open_prompt(
        world,
        client,
        mob,
        HereticPrompt {
            kind: PromptKind::Potion { crucible: target },
            title: capitalize(&name_of(world, target)),
            message: String::new(),
            items: POTIONS.iter().map(|p| p.1.to_owned()).collect(),
            paths: POTIONS.iter().map(|p| p.0).collect(),
            targets: Vec::new(),
        },
    );
    true
}

/// `create_potion` after the radial: still full, still in reach.
pub(super) fn brew_potion(
    world: &mut World,
    client: Entity,
    mob: Entity,
    crucible: Entity,
    potion: &str,
) {
    let full = world
        .get::<Crucible>(crucible)
        .is_some_and(|c| c.mass >= CRUCIBLE_MAX_MASS);
    let (Some(&pos), Some(&at)) = (world.get::<TilePos>(crucible), world.get::<TilePos>(mob))
    else {
        return;
    };
    let near = pos.z == at.z && (pos.x - at.x).abs() <= 1 && (pos.y - at.y).abs() <= 1;
    if !full || !near || !POTIONS.iter().any(|p| p.0 == potion) {
        return;
    }
    let Some(made) = crate::construction::spawn_obj(world, potion, pos) else {
        warn!("heretic: no prototype for {potion}");
        return;
    };
    crate::sound::playsound(
        world,
        PlaySound::new(
            pos,
            "sound/effects/desecration/desecration-02.ogg",
            75.0,
            true,
        ),
    );
    let (name, brew) = (name_of(world, crucible), name_of(world, made));
    crate::medical::visible(
        world,
        crucible,
        "",
        &format!("Сияющая жидкость из {name} стекает в колбу, создавая {brew}!"),
        Span::Notice,
    );
    tell(world, client, Span::Notice, "зелье готово");
    if let Some(mut c) = world.get_mut::<Crucible>(crucible) {
        c.mass = 0;
    }
    update_crucible_icon(world, crucible);
}

/// `bite_the_hand`: the active hand's arm is torn off and eaten.
fn bite_the_hand(world: &mut World, client: Entity, mob: Entity, crucible: Entity) {
    let zone = if world.get::<Inventory>(mob).map_or(0, |i| i.active) == 0 {
        crate::zone::BodyZone::LArm
    } else {
        crate::zone::BodyZone::RArm
    };
    let has_arm = world
        .get::<crate::medical::wounds::Limbs>(mob)
        .is_some_and(|l| !l.limb(zone).missing);
    if !has_arm {
        return;
    }
    let name = capitalize(&name_of(world, crucible));
    tell(
        world,
        client,
        Span::Danger,
        format!("{name} хватает вашу {}!", zone.ru().1),
    );
    if let Some(Some(arm)) = crate::medical::bodyparts::dismember_limb(world, mob, zone, false) {
        consume_fuel(world, None, crucible, arm);
    }
}

/// `consume_fuel`: one mass for the organ or bodypart, which is gone.
fn consume_fuel(world: &mut World, feeder: Option<Entity>, crucible: Entity, consumed: Entity) {
    let Some(c) = world.get::<Crucible>(crucible).copied() else {
        return;
    };
    if c.mass >= CRUCIBLE_MAX_MASS {
        if let Some(client) = feeder {
            tell(world, client, Span::Warning, "уже полон!");
        }
        return;
    }
    if let Some(mut c) = world.get_mut::<Crucible>(crucible) {
        c.mass += 1;
    }
    eat_sound(world, crucible);
    let (name, food) = (
        capitalize(&name_of(world, crucible)),
        name_of(world, consumed),
    );
    crate::medical::visible(
        world,
        crucible,
        "",
        &format!("{name} пожирает {food} и наполняет себя жидкостью!"),
        Span::Notice,
    );
    if let Some(client) = feeder {
        tell(
            world,
            client,
            Span::Notice,
            format!("накормлен ({} / {CRUCIBLE_MAX_MASS})", c.mass + 1),
        );
    }
    match world.get::<CarriedBy>(consumed).map(|c| c.0) {
        Some(holder) => despawn_held(world, holder, consumed),
        None => despawn(world, consumed),
    }
    update_crucible_icon(world, crucible);
}

/// `IS_ORGANIC_LIMB` / `IS_ORGANIC_ORGAN` by type.
fn organic(path: &str) -> bool {
    !["cyber", "robot", "synth", "surplus"]
        .iter()
        .any(|k| path.contains(k))
}

/// The crucible's `item_interaction`, the phylactery's right click, wirecutters on a painting.
pub fn held_on_target(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    let Some(path) = path_of(world, held) else {
        return false;
    };
    if right && path == PHYLACTERY && is_living(world, target) {
        steal_blood(world, client, mob, held, target);
        return true;
    }
    if super::paintings::cut(world, client, mob, held, target) {
        return true;
    }
    if world.get::<Crucible>(target).is_none() {
        return false;
    }
    if knowledge::is_type(&path, CODEX) || world.get::<MansusFist>(held).is_some() {
        if let Some(&pos) = world.get::<TilePos>(target) {
            crate::sound::playsound(
                world,
                PlaySound::new(pos, "sound/items/deconstruct.ogg", 30.0, true).ignore_walls(false),
            );
        }
        let anchored = !crate::obj_damage::anchored(world, target);
        crate::obj_damage::set_anchored(world, target, anchored);
        tell(
            world,
            client,
            Span::Notice,
            if anchored {
                "закреплён"
            } else {
                "откреплён"
            },
        );
        return true;
    }
    let c = world.get::<Crucible>(target).copied().unwrap_or_default();
    if path == FLASK {
        if c.mass < CRUCIBLE_MAX_MASS {
            tell(world, client, Span::Warning, "не полная!");
            return true;
        }
        let full = world
            .get::<crate::chem::Reagents>(held)
            .is_some_and(|r| r.0.holder_full());
        if full {
            tell(world, client, Span::Warning, "фляга заполнена!");
            return true;
        }
        let Some(chem) = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone())
        else {
            return true;
        };
        if let (Some(id), Some(mut r)) = (
            chem.id(ELDRITCH_REAGENT),
            world.get_mut::<crate::chem::Reagents>(held),
        ) {
            r.0.add_reagent(&chem, id, 50.0, 300.0, None, None, false, false);
        }
        if let Some(mut c) = world.get_mut::<Crucible>(target) {
            c.mass -= 1;
        }
        update_crucible_icon(world, target);
        tell(world, client, Span::Notice, "пополнение фляги");
        return true;
    }
    let bodypart = knowledge::is_type(&path, "/obj/item/bodypart");
    let organ = knowledge::is_type(&path, "/obj/item/organ");
    if !bodypart && !organ {
        return false;
    }
    if !organic(&path) {
        tell(world, client, Span::Warning, "не органика!");
        return true;
    }
    if organ && knowledge::is_type(&path, "/obj/item/organ/brain") {
        tell(world, client, Span::Warning, "неподходящий орган!");
        return true;
    }
    if !is_heretic_or_monster(world, mob) {
        if crate::medical::surgery::combat_mode(world, mob) {
            return false;
        }
        bite_the_hand(world, client, mob, target);
        return true;
    }
    consume_fuel(world, Some(client), target, held);
    true
}

/// `phylactery/interact_with_atom_secondary`: five units of the target's blood.
fn steal_blood(world: &mut World, client: Entity, mob: Entity, phylactery: Entity, target: Entity) {
    let t = now(world);
    if world
        .get::<PhylacteryCooldown>(phylactery)
        .is_some_and(|c| t < c.0)
    {
        tell(world, client, Span::Warning, "нельзя похитить так быстро!");
        return;
    }
    if target == mob {
        return;
    }
    let Some((total, max)) = world
        .get::<crate::chem::Reagents>(phylactery)
        .map(|r| (r.0.total_volume, r.0.maximum_volume))
    else {
        return;
    };
    let name = capitalize(&name_of(world, phylactery));
    let victim = name_of(world, target);
    if total >= max {
        tell(world, client, Span::Notice, format!("{name} полон."));
        return;
    }
    let drawn = crate::medical::blood::transfer_blood_to(
        world,
        target,
        phylactery,
        (max - total).min(5.0),
        false,
    );
    if drawn > 0.0 {
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы забираете немного крови у {victim}."),
        );
        chat_to(
            world,
            target,
            Span::Warning,
            "Вы чувствуете крошечный укол!",
        );
        world
            .entity_mut(phylactery)
            .insert(PhylacteryCooldown(t + PHYLACTERY_CD));
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::sound::playsound(
                world,
                PlaySound::new(pos, "sound/effects/chemistry/catalyst.ogg", 20.0, true)
                    .falloff_exponent(10.0),
            );
        }
    } else {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы не можете похитить кровь у {victim}!"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_recharge_amounts_follow_the_spells() {
        assert_eq!(
            focus_recharge_amount("/datum/heretic_knowledge/spell/rust_construction"),
            0.5
        );
        assert_eq!(
            focus_recharge_amount("/datum/heretic_knowledge/spell/void_pull"),
            0.25
        );
        assert_eq!(
            focus_recharge_amount("/datum/heretic_knowledge/spell/ash_passage"),
            0.15
        );
        assert_eq!(
            focus_recharge_amount("/datum/heretic_knowledge/spell/cosmic_runes"),
            0.0
        );
    }

    #[test]
    fn a_focus_gives_back_a_share_of_the_charges() {
        // Void Pull: 4 charges, a quarter back.
        assert_eq!(focus_charges(0, 4, 0.25), 1);
        // Fire Blast: 3 charges, ceil(0.99) = 1.
        assert_eq!(focus_charges(1, 3, 0.33), 2);
        assert_eq!(focus_charges(3, 3, 0.33), 3, "capped at the maximum");
        assert_eq!(focus_charges(2, 5, 0.0), 2);
    }

    #[test]
    fn the_crucible_refills_a_mass_every_30_seconds() {
        let mut c = Crucible {
            mass: 0,
            ..default()
        };
        assert!(crucible_refill(&mut c, 0.0), "the first mass comes at once");
        assert_eq!((c.mass, c.next_refill), (1, 30.0));
        assert!(!crucible_refill(&mut c, 29.0));
        assert!(crucible_refill(&mut c, 30.0));
        assert!(crucible_refill(&mut c, 60.0));
        assert_eq!(c.mass, CRUCIBLE_MAX_MASS);
        assert!(!crucible_refill(&mut c, 1000.0), "full");
    }

    #[test]
    fn summons_map_to_their_mobs() {
        assert_eq!(
            summon_of("/datum/heretic_knowledge/summon/rusty"),
            Some((
                "/mob/living/basic/heretic_summon/rust_walker",
                "rust_spirit"
            ))
        );
        assert_eq!(summon_of("/datum/heretic_knowledge/summon/nope"), None);
    }

    #[test]
    fn only_organic_parts_feed_the_crucible() {
        assert!(organic("/obj/item/organ/liver"));
        assert!(!organic("/obj/item/organ/liver/cybernetic"));
        assert!(!organic("/obj/item/bodypart/arm/left/robot"));
    }
}
