//! Security gear and shoves (server): stun batons (`code/game/objects/items/weaponry/melee/
//! baton.dm`), flashes (`code/modules/assembly/flash.dm`, `carbon_defense.dm` `flash_act`),
//! the shove (`living_defense.dm` `disarm`, `human_defense.dm` `disarm_collision`) and the clicks
//! of handcuffs (`crate::restraints`).
//!
//! Clicks on a living mob are taken here before `crate::combat` ([`MeleeHandled`] tells melee
//! which clicks are already handled):
//! - a baton (tg `pre_attack`/`afterattack`): an active baton stuns on a left click in any
//!   mode (no damage, `attack_speed` cooldown); a right click (harmbaton) hits normally and also
//!   stuns if the baton allows it; an inactive one pokes out of combat mode and hits in it. A
//!   stun is refused during the baton's `cooldown` and on a target this user batoned within it
//!   (`TRAIT_IWASBATONED`). The stun (`baton_effect`): stamina damage, then the classic baton's
//!   knockdown at once; a stun baton uses `cell_hit_cost` of its cell (and turns off below it),
//!   adds confusion and knocks down `knockdown_time` 2 s later (`apply_stun_effect_end`);
//!   a telescopic baton dazes;
//! - a flash: tg `flash_mob` (targeted): unless eye protection (the sum of worn clothing's
//!   `flash_protect`; infinite without eyes or when blind) reaches 1 or the victim faces away,
//!   `flash_act` (`crate::vision`: the flash overlay, eye damage), confusion, 80-120 stamina
//!   damage and a 2.5-5 s knockdown, halved from the side; each use may burn it out;
//! - handcuffs: `crate::restraints::attempt_to_cuff` (taking them off is the strip menu's,
//!   `crate::strip`);
//! - an empty hand's right click (any mode): the shove. The target is pushed one tile away; if
//!   it cannot move it is knocked down 2 s (dazed 3 s), into another human both fall; a target
//!   knocked down and dazed is kicked to the side instead (paralyzed 3 s); a lying target drops
//!   its active item; otherwise it is staggered.
//!
//! Using a baton or a flash in hand (`attack_self`) toggles the baton or flashes everyone
//! around (`AOE_flash`: confusion only).
// shortcut: no armor (the stun's `run_armor_check`), no clumsiness, no silicon blindness (a
// flashed cyborg is only remembered as blinded, see `flash_silicon`), no
// sounds (but the bonk on a cyborg), sparks or baton light, no `TRAIT_BATON_RESISTANCE`, no cell removal or replacement (the cell
// is the baton's charge), no shoves into tables, lockers or disposal bins and no directional
// (border) shove checks.

use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmBaton, DmClothing, DmHandcuffs, DmItem, DmName, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, Dir, Facing, Overlay, TilePos};
use ss13_world::SpatialIndex;

use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::{CLICK_CD_MELEE, CombatMode, CombatRng, NextMove};
use crate::health::{BodyPosition, COMBAT_MESSAGE_RANGE, DEFAULT_MESSAGE_RANGE, Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::pull::{GrabState, dir_between, move_to_adjacent};
use crate::server::PlayerMob;
use crate::status_effects::{
    DAZED_SECS, Incap, NO_SIDE_KICK_SECS, StatusSystems, adjust_stamina, effects_mut, knockdown,
};
use crate::tools::{Actor, tell};

/// `/obj/item/assembly/flash` and subtypes (the handheld one officers carry).
pub const FLASH_ROOT: &str = "/obj/item/assembly/flash";
/// `GLOB.shove_disarming_types`.
const SHOVE_DISARMING: &str = "/obj/item/gun";
/// `SHOVE_KNOCKDOWN_SOLID`, `SHOVE_KNOCKDOWN_HUMAN`, `SHOVE_KNOCKDOWN_COLLATERAL`,
/// `SHOVE_CHAIN_PARALYZE`, `STAGGERED_SLOWDOWN_LENGTH`, seconds.
pub const SHOVE_KNOCKDOWN_SOLID: f32 = 2.0;
pub const SHOVE_KNOCKDOWN_HUMAN: f32 = 2.0;
pub const SHOVE_KNOCKDOWN_COLLATERAL: f32 = 0.1;
pub const SHOVE_CHAIN_PARALYZE: f32 = 3.0;
pub const STAGGERED_SLOWDOWN_LENGTH: f32 = 3.0;
/// Security baton: `apply_stun_effect_end` 2 s after the hit, `set_confusion_if_lower(10 SECONDS)`.
pub const BATON_STUN_DELAY: f64 = 2.0;
pub const BATON_CONFUSION: f32 = 10.0;
/// `stun_time_cyborg`: the contractor baton's paralysis of a cyborg, seconds.
const STUN_TIME_CYBORG: f32 = 5.0;
/// `playsound` of `afterattack` for a baton that does nothing to a cyborg.
const BONK_SOUND: &str = "sound/effects/bang.ogg";
/// `flash/attack`: `confusion_duration = 5 SECONDS`; `CONFUSION_STACK_MAX_MULTIPLIER`; AOE
/// range.
pub const FLASH_CONFUSION: f32 = 5.0;
const CONFUSION_STACK_MAX_MULTIPLIER: f32 = 2.0;
const FLASH_AOE_RANGE: u32 = 3;
/// `FLASH_PROTECTION_SENSITIVE`.
const FLASH_PROTECTION_SENSITIVE: i32 = -1;

/// Attackers whose click on a mob this tick was handled here; `crate::combat` skips them.
#[derive(Resource, Default, Debug)]
pub struct MeleeHandled(pub EntityHashSet);

/// A baton's live state (tg `active`, `cell.charge`, `cooldown_check`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Baton {
    pub active: bool,
    /// Joules left in the cell; `None`: no cell (a classic or telescopic baton, or an empty
    /// stun baton).
    pub charge: Option<f64>,
    /// `Time::elapsed_secs_f64` before which it cannot stun again.
    pub ready_at: f64,
    base_inhand: String,
    base_force: f32,
}

/// A flash's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Flash {
    pub times_used: u32,
    pub burnt_out: bool,
    /// `last_used`, `Time::elapsed_secs_f64`.
    pub last_used: f64,
}

/// `TRAIT_IWASBATONED`: who batoned this mob, until when.
#[derive(Component, Clone, Debug, Default)]
struct Batoned(Vec<(Entity, f64)>);

/// Until when a flashed silicon counts as blinded (tg `set_temp_blindness_if_lower`; a silicon
/// has no blindness here, only this memory: the next flash in the window paralyzes it).
#[derive(Component, Clone, Copy, Debug)]
struct SiliconFlashBlinded(f64);

/// Timed follow-ups of a hit (tg `addtimer`): at the time, knock down for the seconds (the
/// stun baton's delayed knockdown), or end the knockdown (`SetKnockdown(0)` after a side kick).
#[derive(Component, Clone, Debug, Default)]
struct Pending(Vec<(f64, PendingEffect)>);

#[derive(Clone, Copy, Debug, PartialEq)]
enum PendingEffect {
    BatonKnockdown(f32),
    EndKnockdown,
}

pub struct SecurityServerPlugin;

impl Plugin for SecurityServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A baton toggles, a flash flashes around.
        app.add_click_handler(ClickPhase::AttackSelf, 900, |w, c| {
            crate::security::attack_self(w, c.client, c.mob, c.target)
        });
        app.init_resource::<MeleeHandled>()
            .add_systems(
                Update,
                (
                    security_clicks
                        .after(ResolveClicks)
                        .before(crate::combat::MeleeSystems)
                        .before(StatusSystems),
                    run_pending
                        .run_if(any_with_component::<Pending>)
                        .before(StatusSystems),
                ),
            )
            .add_observer(add_baton_state)
            .add_observer(add_flash_state);
    }
}

fn add_baton_state(
    add: On<Add<DmBaton>>,
    batons: Query<(&DmBaton, &DmItem)>,
    mut commands: Commands,
) {
    let Ok((dm, item)) = batons.get(add.entity) else {
        return;
    };
    let charge = (dm.uses_cell && dm.cell_maxcharge > 0.0).then_some(dm.cell_maxcharge);
    let entity = add.entity;
    commands.entity(entity).insert_if_new(Baton {
        active: dm.active && !dm.uses_cell,
        charge,
        ready_at: 0.0,
        base_inhand: item.inhand_icon_state.clone(),
        base_force: item.force,
    });
    commands.queue(move |world: &mut World| refresh_baton(world, entity));
}

fn add_flash_state(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, FLASH_ROOT))
    {
        commands.entity(add.entity).insert_if_new(Flash::default());
    }
}

/// A mob's name, or an item's.
pub(crate) fn name_of(world: &World, entity: Entity) -> String {
    let Ok(e) = world.get_entity(entity) else {
        return "Кто-то".to_owned();
    };
    if let Some(name) = e.get::<Name>() {
        return name.as_str().to_owned();
    }
    e.get::<DmName>()
        .map_or_else(|| "что-то".to_owned(), |n| n.name.clone())
}

/// tg `visible_message` within `range` of `at`: `others` to everyone else, each of `personal`
/// its own line.
// shortcut: range only, no line of sight, as `crate::health::clients_in_view`.
pub(crate) fn show(
    world: &mut World,
    at: TilePos,
    range: u32,
    channel: ChatChannel,
    others: (Span, String),
    personal: &[(Entity, Span, String)],
) {
    let client = |world: &World, e: Entity| world.get::<PlayerMob>(e).map(|p| p.client);
    let message = |span, text: &str| ChatMessage {
        channel,
        segments: vec![ChatSegment::new(span, text)],
    };
    let mut out = Vec::new();
    let exclude: Vec<Entity> = personal.iter().map(|(p, ..)| *p).collect();
    let clients = crate::obj_damage::clients_in_range(world, at, range, &exclude);
    out.push(SendChat {
        clients,
        message: message(others.0, &others.1),
    });
    for (mob, span, text) in personal {
        if let Some(c) = client(world, *mob) {
            out.push(SendChat {
                clients: vec![c],
                message: message(*span, text),
            });
        }
    }
    for chat in out {
        world.write_message(chat);
    }
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: String) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// Clicks of conscious mobs on living mobs that the held item (or an empty hand) acts on
/// before melee.
pub(crate) fn security_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    world.resource_mut::<MeleeHandled>().0.clear();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world.get::<Health>(target).is_none() {
            continue;
        }
        if world.get::<NextMove>(mob).is_some_and(|n| now < n.0) {
            continue;
        }
        let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
        let actor = Actor {
            client: click.client,
            mob,
            item: held,
        };
        let handled = match held {
            // The advanced baton's modes in an abductor's hand (`crate::antag::abductor`).
            Some(item)
                if world
                    .get::<crate::antag::abductor::server::AbductorBaton>(item)
                    .is_some()
                    && crate::antag::abductor::server::baton_attack(world, actor, item, target) =>
            {
                true
            }
            Some(item) if world.get::<Baton>(item).is_some() => {
                baton_attack(world, actor, item, target, click.modifiers.right, now)
            }
            Some(item) if world.get::<Flash>(item).is_some() => {
                flash_attack(world, mob, item, target, now);
                true
            }
            Some(item) if world.get::<DmHandcuffs>(item).is_some() => {
                crate::restraints::attempt_to_cuff(world, actor, item, target)
            }
            // Point blank (`crate::guns`).
            Some(item) if world.get::<crate::guns::Gun>(item).is_some() => {
                crate::guns::interact_with_mob(world, &click, item)
            }
            // tg `check_block` of the defender against a melee hit (`crate::martial`).
            Some(item) if target != mob && blocks_melee(world, mob, item, target) => {
                let speed = world.get::<DmItem>(item).map_or(8.0, |i| i.attack_speed);
                world
                    .entity_mut(mob)
                    .insert(NextMove(now + f64::from(speed) / 10.0));
                true
            }
            Some(_) => false,
            // The attacker's martial art takes the empty hand first (`crate::martial`).
            None if crate::martial::unarmed_attack(world, &click, now) => true,
            // A cow is tipped over instead of shoved (`crate::mobs::farm`).
            None if click.modifiers.right && crate::mobs::farm::tip_click(world, actor, target) => {
                true
            }
            // tg `check_block` of the defender against a punch or a shove.
            None if target != mob
                && (click.modifiers.right || world.get::<CombatMode>(mob).is_some_and(|c| c.0))
                && {
                    let user = name_of(world, mob);
                    let text = if click.modifiers.right {
                        format!("толчок {user}")
                    } else {
                        format!("удар {user}")
                    };
                    crate::martial::check_block(
                        world,
                        target,
                        Some(mob),
                        crate::martial::AttackKind::Unarmed,
                        0.0,
                        &text,
                    )
                } =>
            {
                world.entity_mut(mob).insert(NextMove(now + CLICK_CD_MELEE));
                true
            }
            None if click.modifiers.right => {
                if target != mob {
                    world.entity_mut(mob).insert(NextMove(now + CLICK_CD_MELEE));
                    shove(world, mob, target);
                }
                true
            }
            None => false,
        };
        if handled {
            world.resource_mut::<MeleeHandled>().0.insert(mob);
        }
    }
}

/// Whether a hit of `item` (one `crate::combat` would swing) is blocked by the defender's
/// martial art.
fn blocks_melee(world: &mut World, user: Entity, item: Entity, target: Entity) -> bool {
    let Some(dm) = world.get::<DmItem>(item) else {
        return false;
    };
    let combat = world.get::<CombatMode>(user).is_some_and(|c| c.0);
    if dm.force <= 0.0
        || crate::medical::interacts_with_mobs(&dm.path)
        || crate::chem::interacts_with_mobs(&dm.path)
        || crate::antag::abductor::server::interacts_with_mobs(&dm.path)
        || (crate::food::is_food(&dm.path) && !combat)
    {
        return false;
    }
    let force = dm.force;
    let text = format!("{} {}", name_of(world, item), name_of(world, user));
    crate::martial::check_block(
        world,
        target,
        Some(user),
        crate::martial::AttackKind::Melee,
        force,
        &text,
    )
}

/// tg `/obj/item/melee/baton/pre_attack` + `afterattack`. True when melee must not hit.
fn baton_attack(
    world: &mut World,
    actor: Actor,
    baton: Entity,
    target: Entity,
    harm: bool,
    now: f64,
) -> bool {
    let Some(dm) = world.get::<DmBaton>(baton).cloned() else {
        return false;
    };
    if harm && !dm.stun_on_harmbaton {
        return false;
    }
    let state = world.get::<Baton>(baton).cloned().expect("checked");
    let combat = world.get::<CombatMode>(actor.mob).is_some_and(|c| c.0);
    let (user, victim, item) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, baton),
    );
    // `try_stun`.
    let can_stun = if dm.uses_cell && !state.active && !harm && !combat {
        if let Some(&at) = world.get::<TilePos>(target) {
            show(
                world,
                at,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Warning,
                    format!(
                        "{user} тыкает {victim} с помощью {item}. К счастью, оно было выключено."
                    ),
                ),
                &[(
                    target,
                    Span::Warning,
                    format!("{user} тыкает вас с помощью {item}. К счастью, оно было выключено."),
                )],
            );
        }
        false
    } else if !state.active {
        false
    } else if now < state.ready_at {
        if dm.uses_cell && !harm {
            tell(world, actor.client, Span::Warning, "ещё перезаряжается!");
        }
        false
    } else if world.get::<Batoned>(target).is_some_and(|b| {
        b.0.iter()
            .any(|&(by, until)| by == actor.mob && now < until)
    }) {
        if !harm {
            tell(world, actor.client, Span::Warning, "нельзя пока оглушить!");
        }
        false
    } else {
        true
    };
    if !can_stun {
        // Harmbatons always hit; an active baton that cannot stun, or a poke, does not.
        return !harm && (state.active || !combat);
    }
    let attack_speed = world.get::<DmItem>(baton).map_or(8.0, |i| i.attack_speed);
    // Clumsy people redirect the attack: the stun and twice the force on their own head.
    if crate::genetics::is_clumsy(world, actor.mob)
        && world.resource_mut::<CombatRng>().chance(50.0)
    {
        clumsy_baton(world, actor, baton, &dm, now, attack_speed, &user, &item);
        return true;
    }
    if !harm {
        // The stun replaces the hit: force 0, no hit message, the attack cooldown.
        world
            .entity_mut(actor.mob)
            .insert(NextMove(now + f64::from(attack_speed) / 10.0));
    }
    finalize_baton_attack(world, baton, &dm, actor.mob, target, now);
    if let Some(&at) = world.get::<TilePos>(target) {
        // `afterattack`: a cyborg gets `get_cyborg_stun_description` (and the bonk, unless the
        // baton stuns cyborgs).
        let cyborg = world
            .get::<crate::silicon::cyborg::Cyborg>(target)
            .is_some();
        let (visible, local, local_span) = if cyborg && stuns_cyborgs(&dm) {
            (
                format!("{user} пульсирует сенсоры {victim} с помощью {item}!"),
                format!("Вы пульсируете сенсоры {victim} с помощью {item}!"),
                Span::Danger,
            )
        } else if cyborg {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(at, BONK_SOUND, 10.0, true),
            );
            let (verb, short) = if dm.uses_cell {
                ("оглушить", "оглушить")
            } else {
                ("сбить с ног", "сбить")
            };
            (
                format!(
                    "{user} пытается {verb} {victim} с помощью {item}, и предсказуемо терпит неудачу!"
                ),
                format!("{user} пытается... {short} вас с помощью {item}?"),
                Span::BoldWarning,
            )
        } else {
            let verb = if dm.uses_cell {
                "оглушает"
            } else {
                "сбивает с ног"
            };
            (
                format!("{user} {verb} {victim} с помощью {item}!"),
                format!("{user} {verb} вас с помощью {item}!"),
                Span::BoldWarning,
            )
        };
        show(
            world,
            at,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, visible.clone()),
            &[
                (target, local_span, local),
                (actor.mob, Span::Danger, visible),
            ],
        );
    }
    !harm
}

/// `/obj/item/melee/baton/pre_attack` of a clumsy user (`TRAIT_CLUMSY`, `prob(50)`).
#[allow(clippy::too_many_arguments)]
fn clumsy_baton(
    world: &mut World,
    actor: Actor,
    baton: Entity,
    dm: &DmBaton,
    now: f64,
    attack_speed: f32,
    user: &str,
    item: &str,
) {
    let mob = actor.mob;
    crate::chem::visible(
        world,
        mob,
        &format!("Вы случайно бьёте себя по голове {item}!"),
        &format!("{user} случайно бьёт себя по голове {item}! Что за придурок!"),
        Span::Danger,
    );
    finalize_baton_attack(world, baton, dm, mob, mob, now);
    let force = world.get::<DmItem>(baton).map_or(0.0, |i| i.force);
    if let Ok(mut entity) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        let mut new = *stat;
        crate::health::apply_damage_zone(
            &mut health,
            &mut new,
            crate::zone::BodyZone::Head,
            crate::health::DamageType::Brute,
            2.0 * force,
        );
        stat.set_if_neq(new);
    }
    crate::animation::do_attack_animation_with(
        world,
        mob,
        mob,
        crate::animation::AttackVisual::Item(baton),
    );
    world
        .entity_mut(mob)
        .insert(NextMove(now + f64::from(attack_speed) / 10.0));
}

/// tg `affect_cyborg`: only the contractor baton has it.
fn stuns_cyborgs(dm: &DmBaton) -> bool {
    dm.path
        .starts_with("/obj/item/melee/baton/telescopic/contractor_baton")
}

/// tg `finalize_baton_attack` -> `baton_effect` (+ the security baton's charge and
/// `additional_effects_non_cyborg`), `set_batoned`.
fn finalize_baton_attack(
    world: &mut World,
    baton: Entity,
    dm: &DmBaton,
    user: Entity,
    target: Entity,
    now: f64,
) {
    let cooldown = f64::from(dm.cooldown) / 10.0;
    let mut state = world.get_mut::<Baton>(baton).expect("checked");
    state.ready_at = now + cooldown;
    if dm.uses_cell {
        if let Some(mut cyborg) = world.get_mut::<crate::silicon::cyborg::Cyborg>(user) {
            // `security/baton_effect`, `iscyborg(loc)`: the cyborg's own cell pays.
            if cyborg.charge < dm.cell_hit_cost {
                return;
            }
            cyborg.charge -= dm.cell_hit_cost;
        } else {
            let mut state = world.get_mut::<Baton>(baton).expect("checked");
            // `deductcharge`: `cell.use` refuses when short; below a hit the baton turns off.
            let Some(charge) = state.charge.filter(|&c| c >= dm.cell_hit_cost) else {
                return;
            };
            let left = charge - dm.cell_hit_cost;
            state.charge = Some(left);
            if state.active && left < dm.cell_hit_cost {
                state.active = false;
                refresh_baton(world, baton);
            }
        }
    }
    if world
        .get::<crate::silicon::cyborg::Cyborg>(target)
        .is_some()
    {
        // `baton_effect`: `if(!affect_cyborg) return FALSE`; the rest of it is the paralysis.
        if stuns_cyborgs(dm) {
            crate::status_effects::apply(
                world,
                target,
                crate::status_effects::Incap::Paralyze,
                STUN_TIME_CYBORG,
            );
            set_batoned(world, user, target, cooldown, now);
        }
        return;
    }
    adjust_stamina(world, target, dm.stamina_damage);
    // `additional_effects_non_cyborg`: the security baton jitters (40 s), confuses and stutters
    // (16 s); the contractor baton jitters and stutters 40 s.
    let contractor = dm
        .path
        .starts_with("/obj/item/melee/baton/telescopic/contractor_baton");
    if dm.uses_cell || contractor {
        use crate::status_effects::{Timed, set_timed};
        set_timed(world, target, Timed::Jitter, 40.0, true);
        set_timed(
            world,
            target,
            Timed::Stutter,
            if contractor { 40.0 } else { 16.0 },
            true,
        );
    }
    if dm.uses_cell {
        if let Some(mut e) = effects_mut(world, target) {
            e.set_confusion_if_lower(BATON_CONFUSION);
        }
        let at = now + BATON_STUN_DELAY;
        let effect = PendingEffect::BatonKnockdown(dm.knockdown_time / 10.0);
        schedule(world, target, at, effect);
    } else {
        knockdown(world, target, dm.knockdown_time / 10.0, 0.0);
        if dm.telescopic {
            let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
            if let Some(mut e) = effects_mut(world, target)
                && !dead
                && e.no_side_kick <= 0.0
                && !e.immobilized()
                && e.dazed <= 0.0
            {
                e.dazed = DAZED_SECS;
            }
        }
    }
    set_batoned(world, user, target, cooldown, now);
}

/// tg `set_batoned`: `target` remembers who batoned it.
fn set_batoned(world: &mut World, user: Entity, target: Entity, cooldown: f64, now: f64) {
    if cooldown > 0.0 {
        let mut entity = world.entity_mut(target);
        let mut batoned = entity.entry::<Batoned>().or_default();
        let batoned = &mut batoned.get_mut().0;
        batoned.retain(|&(by, until)| by != user && until > now);
        batoned.push((user, now + cooldown));
    }
}

fn schedule(world: &mut World, target: Entity, at: f64, effect: PendingEffect) {
    if let Ok(mut entity) = world.get_entity_mut(target) {
        entity
            .entry::<Pending>()
            .or_default()
            .get_mut()
            .0
            .push((at, effect));
    }
}

/// The follow-ups that are due. Cost: per tick, mobs hit in the last few seconds only.
fn run_pending(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &mut Pending)>();
    let mut due = Vec::new();
    let mut done = Vec::new();
    for (mob, mut pending) in query.iter_mut(world) {
        pending.0.retain(|&(at, effect)| {
            let ready = now >= at;
            if ready {
                due.push((mob, effect));
            }
            !ready
        });
        if pending.0.is_empty() {
            done.push(mob);
        }
    }
    for mob in done {
        world.entity_mut(mob).remove::<Pending>();
    }
    for (mob, effect) in due {
        match effect {
            PendingEffect::BatonKnockdown(secs) => {
                // `apply_stun_effect_end`.
                let down = world
                    .get::<crate::status_effects::StatusEffects>(mob)
                    .is_some_and(|e| e.knockdown > 0.0);
                if !down {
                    tell_mob(
                        world,
                        mob,
                        Span::Warning,
                        "Ваши мышцы сводит судорогой, заставляя вас упасть!".to_owned(),
                    );
                }
                knockdown(world, mob, secs, 0.0);
            }
            PendingEffect::EndKnockdown => {
                if let Some(mut e) = effects_mut(world, mob) {
                    e.set(Incap::Knockdown, 0.0);
                }
            }
        }
    }
}

/// tg `attack_self` of batons and flashes. False for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Flash>(item).is_some() {
        aoe_flash(world, mob, item);
        return true;
    }
    let (Some(dm), Some(mut state)) = (
        world.get::<DmBaton>(item).cloned(),
        world.get::<Baton>(item).cloned(),
    ) else {
        return false;
    };
    let text = if dm.uses_cell {
        // `/obj/item/melee/baton/security/attack_self`.
        let charge = state.charge.unwrap_or(0.0);
        if charge >= dm.cell_hit_cost && !state.active {
            state.active = true;
            "включение"
        } else {
            state.active = false;
            if state.charge.is_none() {
                "нет источника питания!"
            } else if charge < dm.cell_hit_cost {
                "не хватает заряда!"
            } else {
                "выключение"
            }
        }
    } else if dm.telescopic {
        // `/datum/component/transforming` -> `on_transform`.
        state.active = !state.active;
        if state.active {
            "вытянуто"
        } else {
            "втянуто"
        }
    } else {
        return false;
    };
    world.entity_mut(item).insert(state);
    tell(world, client, Span::Notice, text);
    refresh_baton(world, item);
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    true
}

/// A baton's look and live vars: `update_icon_state` of a stun baton (`_active`, `_nocell`),
/// a telescopic baton's `_on` state, force and in-hand sprite.
pub fn refresh_baton(world: &mut World, item: Entity) {
    let (Some(dm), Some(state)) = (
        world.get::<DmBaton>(item).cloned(),
        world.get::<Baton>(item).cloned(),
    ) else {
        return;
    };
    let Some((dmi, base)) = world
        .get::<DmAtom>(item)
        .map(|a| (a.icon.clone(), a.icon_state.clone()))
    else {
        return;
    };
    let (icon_state, inhand, force) = if dm.telescopic {
        if state.active {
            let inhand = dm.on_inhand_icon_state.clone();
            (format!("{base}_on"), inhand, dm.active_force)
        } else {
            (base.clone(), String::new(), state.base_force)
        }
    } else if dm.uses_cell && state.active {
        let inhand = format!("{}_active", state.base_inhand);
        (format!("{base}_active"), inhand, state.base_force)
    } else if dm.uses_cell && state.charge.is_none() {
        (
            format!("{base}_nocell"),
            state.base_inhand.clone(),
            state.base_force,
        )
    } else {
        (base.clone(), state.base_inhand.clone(), state.base_force)
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|icons| icons.get(&dmi, &icon_state));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(item))
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
    if let Some(mut item) = world.get_mut::<DmItem>(item)
        && (item.inhand_icon_state != inhand || item.force != force)
    {
        item.inhand_icon_state = inhand;
        item.force = force;
    }
    if let Some(carrier) = world.get::<CarriedBy>(item).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(carrier)
    {
        inventory.set_changed();
    }
}

/// tg `get_eye_protection` of a human: the summed `flash_protect` of what it wears (hands and
/// pockets excluded); infinite without eyes or when blind (`crate::vision::eye_protection`).
pub fn eye_protection(world: &World, mob: Entity) -> i32 {
    let worn = world.get::<Inventory>(mob).map_or(0, |inventory| {
        crate::vision::worn(inventory)
            .filter_map(|item| world.get::<DmClothing>(item))
            .map(|c| c.flash_protect)
            .sum()
    });
    crate::vision::eye_protection(world, mob, worn)
}

/// tg `calculate_deviation`: 0 facing the flash, 1 sideways (or on the same tile), 2 facing
/// away.
pub fn deviation(protection: i32, victim: TilePos, facing: Dir, attacker: TilePos) -> u8 {
    if protection < FLASH_PROTECTION_SENSITIVE {
        return 0;
    }
    let Some(to_attacker) = dir_between(victim, attacker) else {
        return 1;
    };
    let (facing, to, away) = (facing as u8, to_attacker as u8, to_attacker.reverse() as u8);
    if facing & to != 0 {
        0
    } else if facing & away != 0 {
        2
    } else {
        1
    }
}

/// tg `try_use_flash` + `flash_recharge` (`interval` 10 s, burnout chance `times_used * 3`).
fn try_use_flash(world: &mut World, flash: Entity, now: f64) -> bool {
    let Some(mut state) = world.get::<Flash>(flash).cloned() else {
        return false;
    };
    if state.burnt_out {
        return false;
    }
    state.times_used += 1;
    let recharged = ((now - state.last_used) / 10.0).floor().max(0.0) as u32;
    state.times_used = state.times_used.saturating_sub(recharged);
    state.last_used = now;
    let burn = world
        .resource_mut::<CombatRng>()
        .chance(state.times_used as f32 * 3.0);
    if burn {
        state.burnt_out = true;
    }
    world.entity_mut(flash).insert(state);
    if burn {
        burn_out(world, flash);
        return false;
    }
    true
}

/// tg `burn_out`: "burnt-out flash", the `flashburnt` overlay, the burnt in-hand sprite.
pub(crate) fn burn_out(world: &mut World, flash: Entity) {
    if let Some(mut state) = world.get_mut::<Flash>(flash) {
        state.burnt_out = true;
    }
    let name = name_of(world, flash);
    let holder = world.get::<CarriedBy>(flash).map(|c| c.0);
    let at = holder
        .and_then(|h| world.get::<TilePos>(h))
        .or_else(|| world.get::<TilePos>(flash))
        .copied();
    if let Some(mut dm_name) = world.get_mut::<DmName>(flash) {
        dm_name.name = format!("burnt-out {}", dm_name.name);
    }
    if let Some(mut item) = world.get_mut::<DmItem>(flash) {
        item.inhand_icon_state = "flashtool_burnt".to_owned();
    }
    let overlay = world
        .get::<DmAtom>(flash)
        .map(|a| a.icon.clone())
        .and_then(|dmi| {
            world
                .get_resource::<IconIndex>()
                .and_then(|icons| icons.get(&dmi, "flashburnt"))
        });
    if let (Some(id), Some(mut appearance)) = (overlay, world.get_mut::<Appearance>(flash)) {
        appearance.overlays.push(Overlay::new(id));
    }
    if let Some(holder) = holder
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
    if let Some(at) = at {
        let personal: Vec<(Entity, Span, String)> = holder
            .map(|h| (h, Span::BoldWarning, format!("{name} перегорает!")))
            .into_iter()
            .collect();
        show(
            world,
            at,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, format!("{name} перегорает!")),
            &personal,
        );
    }
}

/// A silicon's end of `flash_mob`: blinded for 5-15 s, and flashed again while blinded it is
/// overloaded and paralyzed for 8-12 s.
fn flash_silicon(world: &mut World, source: &FlashSource, victim: Entity, at: TilePos) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let blinded = world
        .get::<SiliconFlashBlinded>(victim)
        .is_some_and(|b| now < b.0);
    let (name_user, name_victim) = (source.name.clone(), name_of(world, victim));
    let mut rng = world.resource_mut::<CombatRng>();
    let (paralysis, blindness) = (rng.range(8, 12) as f32, rng.range(5, 15) as f64);
    let (verb, secs) = if blinded {
        crate::status_effects::apply(
            world,
            victim,
            crate::status_effects::Incap::Paralyze,
            paralysis,
        );
        ("перегружает сенсоры и вычисления", f64::from(paralysis))
    } else {
        ("ослепляет", blindness)
    };
    world
        .entity_mut(victim)
        .insert(SiliconFlashBlinded(now + secs));
    if source.user.is_some() {
        show(
            world,
            at,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("{name_user} {verb} {name_victim} вспышкой!"),
            ),
            &[],
        );
    } else {
        tell_mob(
            world,
            victim,
            Span::Danger,
            format!("{name_user} {verb} вас!"),
        );
    }
}

/// tg `/obj/item/assembly/flash/attack`.
fn flash_attack(world: &mut World, user: Entity, flash: Entity, target: Entity, now: f64) {
    if try_use_flash(world, flash, now) && flash_mob(world, user, target, FLASH_CONFUSION, true) {
        // `COMSIG_MOB_SUCCESSFUL_FLASHED_MOB`.
        crate::antag::brother::on_successful_flash(world, user, target, flash);
        if crate::antag::revolution::server::on_successful_flash(world, user, target)
            && let Some(mut state) = world.get_mut::<Flash>(flash)
        {
            state.times_used = state.times_used.saturating_sub(1);
        }
    }
}

/// tg `AOE_flash` (range 3, every mob that sees the flash but the user).
fn aoe_flash(world: &mut World, user: Entity, flash: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if !try_use_flash(world, flash, now) {
        return;
    }
    let Some(&at) = world.get::<TilePos>(user) else {
        return;
    };
    let name = name_of(world, flash);
    tell_mob(
        world,
        user,
        Span::Danger,
        format!("{name} испускает ослепляющий свет!"),
    );
    let targets: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(at, FLASH_AOE_RANGE)
        .filter(|&e| e != user)
        .collect();
    for target in targets {
        if world.get::<Health>(target).is_some() {
            tell_mob(
                world,
                target,
                Span::Danger,
                format!("{name} испускает ослепляющий свет!"),
            );
            flash_mob(world, user, target, FLASH_CONFUSION, false);
        }
    }
}

/// tg `flash_mob` with a user (`targeted`: a click on the victim, else the AOE flash). True when
/// it reached the end (a targeted flash sends `COMSIG_MOB_SUCCESSFUL_FLASHED_MOB` then).
fn flash_mob(
    world: &mut World,
    user: Entity,
    victim: Entity,
    confusion: f32,
    targeted: bool,
) -> bool {
    let Some(&user_at) = world.get::<TilePos>(user) else {
        return false;
    };
    let source = FlashSource {
        user: Some(user),
        at: user_at,
        name: name_of(world, user),
    };
    flash_mob_from(world, &source, victim, confusion, targeted)
}

/// Who or what flashes: tg `flash_mob(flashed, user)` with `user`, or `user = null` (a mounted
/// flasher's bulb: the deviation is from the bulb, the victim's own lines name it).
pub(crate) struct FlashSource {
    pub user: Option<Entity>,
    pub at: TilePos,
    pub name: String,
}

/// tg `/obj/item/assembly/flash/proc/flash_mob`.
pub(crate) fn flash_mob_from(
    world: &mut World,
    source: &FlashSource,
    victim: Entity,
    confusion: f32,
    targeted: bool,
) -> bool {
    let Some(&victim_at) = world.get::<TilePos>(victim) else {
        return false;
    };
    let protection = eye_protection(world, victim);
    let facing = world.get::<Facing>(victim).map_or(Dir::South, |f| f.0);
    // `calculate_deviation`: "deep-rooted weakness to flashes" of silicons.
    let silicon = world.get::<crate::silicon::Silicon>(victim).is_some();
    let mut deviation = if silicon {
        0
    } else {
        deviation(protection, victim_at, facing, source.at)
    };
    // `COMSIG_MOB_PRE_FLASHED_MOB`: `can_flash_from_behind` makes a full deviation partial.
    if let Some(user) = source.user
        && deviation == 2
        && (crate::antag::brother::can_flash_from_behind(world, user)
            || crate::antag::revolution::server::can_flash_from_behind(world, user))
    {
        deviation = 1;
    }
    if deviation == 2 {
        return false;
    }
    let (name_user, name_victim) = (source.name.clone(), name_of(world, victim));
    // `flash_act(1, override_blindness_check = targeted)`: eye protection at or above the
    // intensity blocks it; the eyes burn by how much it gets through.
    if !crate::vision::flash_act(
        world,
        victim,
        1,
        targeted,
        false,
        crate::vision::FLASH_LENGTH,
    ) {
        if targeted {
            if source.user.is_some() {
                show(
                    world,
                    victim_at,
                    DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Combat,
                    (
                        Span::Warning,
                        format!("{name_user} не удаётся ослепить {name_victim} вспышкой!"),
                    ),
                    &[(
                        victim,
                        Span::Danger,
                        format!("{name_user} не удаётся ослепить вас вспышкой!"),
                    )],
                );
            } else {
                tell_mob(
                    world,
                    victim,
                    Span::Danger,
                    format!("{name_user} не удаётся ослепить вас!"),
                );
            }
        }
        return false;
    }
    if let Some(mut e) = effects_mut(world, victim) {
        e.adjust_confusion_up_to(confusion, confusion * CONFUSION_STACK_MAX_MULTIPLIER);
    }
    if !targeted {
        return true;
    }
    if silicon {
        flash_silicon(world, source, victim, victim_at);
        return true;
    }
    let scale = 1.0 - f32::from(deviation) * 0.5;
    let (stamina, down) = {
        let mut rng = world.resource_mut::<CombatRng>();
        (rng.range(80, 120) as f32, rng.range(25, 50) as f32)
    };
    adjust_stamina(world, victim, stamina * scale);
    knockdown(world, victim, down * scale / 10.0, 0.0);
    if source.user.is_some() {
        show(
            world,
            victim_at,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{name_user} ослепляет {name_victim} вспышкой!"),
            ),
            &[(
                victim,
                Span::BoldWarning,
                format!("{name_user} ослепляет вас вспышкой!"),
            )],
        );
    } else {
        tell_mob(
            world,
            victim,
            Span::Danger,
            format!("Вас ослепляет {name_user}!"),
        );
    }
    true
}

/// tg `/mob/living/proc/disarm` with an empty hand (`can_disarm`, carbon `get_shove_flags`).
pub(crate) fn shove(world: &mut World, user: Entity, target: Entity) {
    let lying = |world: &World, e: Entity| world.get::<BodyPosition>(e).is_some_and(|b| b.lying());
    let (Some(&from), Some(&target_at)) =
        (world.get::<TilePos>(user), world.get::<TilePos>(target))
    else {
        return;
    };
    // `can_disarm`.
    if lying(world, user) || from == target_at {
        return;
    }
    let Some(dir) = dir_between(from, target_at) else {
        return;
    };
    let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
    let effects = world
        .get::<crate::status_effects::StatusEffects>(target)
        .cloned()
        .unwrap_or_default();
    let kick_side = effects.knockdown > 0.0
        && effects.paralyze <= 0.0
        && effects.dazed > 0.0
        && effects.no_side_kick <= 0.0;
    let shove_to = target_at.step(dir);
    let blocked = !move_to_adjacent(world, target, shove_to);
    if !blocked && let Some(mut grab) = world.get_mut::<GrabState>(target) {
        // `setGrabState(GRAB_PASSIVE)`.
        grab.set_if_neq(GrabState::Passive);
    }
    let (name_user, name_target) = (name_of(world, user), name_of(world, target));
    let at = world.get::<TilePos>(target).copied().unwrap_or(target_at);
    if blocked {
        // `COMSIG_LIVING_DISARM_COLLIDE` of a table on the tile behind (`table_smash`).
        if crate::tables::shoved_into(world, user, target, shove_to) {
            return;
        }
        // `COMSIG_LIVING_DISARM_COLLIDE` of a human on the tile behind (`disarm_collision`).
        let other = world
            .resource::<SpatialIndex>()
            .on_tile(shove_to)
            .iter()
            .copied()
            .find(|&e| e != target && world.get::<Inventory>(e).is_some());
        if let Some(other) = other {
            knockdown(world, target, SHOVE_KNOCKDOWN_HUMAN, DAZED_SECS);
            knockdown(world, other, SHOVE_KNOCKDOWN_COLLATERAL, DAZED_SECS);
            let name_other = name_of(world, other);
            show(
                world,
                at,
                COMBAT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!("{name_user} толкает {name_target} в {name_other}!"),
                ),
                &[
                    (
                        target,
                        Span::BoldWarning,
                        format!("Вы врезаетесь в {name_other} из-за толчка {name_user}!"),
                    ),
                    (
                        user,
                        Span::Danger,
                        format!("Вы толкаете {name_target} в {name_other}!"),
                    ),
                ],
            );
            return;
        }
        if !kick_side {
            knockdown(world, target, SHOVE_KNOCKDOWN_SOLID, DAZED_SECS);
            show(
                world,
                at,
                COMBAT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!("{name_user} толкает {name_target}, сбивая его с ног!"),
                ),
                &[
                    (
                        target,
                        Span::BoldWarning,
                        format!("Вы сбиты с ног от толчка {name_user}!"),
                    ),
                    (
                        user,
                        Span::Danger,
                        format!("Вы толкаете {name_target}, сбивая его с ног!"),
                    ),
                ],
            );
            return;
        }
    }
    if kick_side {
        let paralyzed = effects_mut(world, target)
            .is_some_and(|mut e| e.apply(Incap::Paralyze, SHOVE_CHAIN_PARALYZE));
        if paralyzed {
            if let Some(mut e) = effects_mut(world, target) {
                e.no_side_kick = NO_SIDE_KICK_SECS;
            }
            let now = world.resource::<Time>().elapsed_secs_f64();
            schedule(
                world,
                target,
                now + f64::from(SHOVE_CHAIN_PARALYZE),
                PendingEffect::EndKnockdown,
            );
            show(
                world,
                at,
                COMBAT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!("{name_user} пинает {name_target} в его бок!"),
                ),
                &[
                    (
                        target,
                        Span::BoldWarning,
                        format!("{name_user} пинает вас в ваш бок!"),
                    ),
                    (
                        user,
                        Span::Danger,
                        format!("Вы пинаете {name_target} в его бок!"),
                    ),
                ],
            );
            return;
        }
    }
    // `get_shoving_message`.
    show(
        world,
        at,
        COMBAT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{name_user} толкает {name_target}!")),
        &[
            (
                target,
                Span::BoldWarning,
                format!("{name_user} толкает вас!"),
            ),
            (user, Span::Danger, format!("Вы толкаете {name_target}!")),
        ],
    );
    // Take their lunch money: a staggered target drops a gun, a lying one anything.
    let held = world
        .get::<Inventory>(target)
        .and_then(Inventory::active_item);
    if let Some(item) = held {
        let gun = world
            .get::<DmItem>(item)
            .is_some_and(|i| is_under(&i.path, SHOVE_DISARMING));
        if (effects.staggered > 0.0 && gun) || lying(world, target) {
            world.entity_mut(item).remove::<CarriedBy>().insert(at);
            let item_name = name_of(world, item);
            show(
                world,
                at,
                COMBAT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (Span::Danger, format!("{name_target} роняет {item_name}!")),
                &[(target, Span::Warning, format!("Вы роняете {item_name}!"))],
            );
        }
    }
    if let Some(mut e) = effects_mut(world, target) {
        e.adjust_staggered_up_to(STAGGERED_SLOWDOWN_LENGTH, 10.0, dead);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y, 1)
    }

    #[test]
    fn flash_deviation_follows_facing() {
        // The flash is north of the victim.
        let (victim, flash) = (at(5, 5), at(5, 6));
        assert_eq!(deviation(0, victim, Dir::North, flash), 0);
        assert_eq!(deviation(0, victim, Dir::South, flash), 2);
        assert_eq!(deviation(0, victim, Dir::East, flash), 1);
        // Diagonal: within 45 degrees of the facing counts as facing it.
        assert_eq!(deviation(0, victim, Dir::East, at(6, 6)), 0);
        assert_eq!(deviation(0, victim, Dir::West, at(6, 6)), 2);
        assert_eq!(deviation(0, victim, Dir::North, victim), 1, "same tile");
        assert_eq!(
            deviation(-2, victim, Dir::South, flash),
            0,
            "sensitive eyes"
        );
    }
}
