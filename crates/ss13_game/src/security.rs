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
//! around in view (`AOE_flash`: confusion only).
//!
//! Batons in detail:
//! - the stun goes through the chest armor of the baton's `armour_type_against_stun` (less its
//!   `stun_armour_penetration`), plays its `on_stun_sound`; a Hulk's fingers are too big for it;
//! - a security baton's cell ([`Baton::charge`] and its type) comes out with a screwdriver and a
//!   cell clicked onto the baton goes in (if it holds a hit); turning on and off sparks; thrown
//!   while on it stuns at `throw_stun_chance` ([`thrown`]); a conversion kit turns it into a
//!   police baton;
//! - the stunprod sparks its igniter on each stun (once per 3 s) and takes a bluespace crystal
//!   or a telecrystal (without a cell) to become a teleprod (the victim blinks up to 15 tiles,
//!   50 for a clumsy user's own head) or a snatcherprod (the victim's held item jumps into the
//!   user's other hand); the boomerang flies back at its thrower; nunchaku change their look
//!   after each swing;
//! - the electrostaff (BandaStation) is two-handed: wielding turns it on, letting go turns it
//!   off; wielded without charge it still strikes (20 stamina, knockdown, a wooden thud); a
//!   powered right click adds 5 burn; stunswords and electrostaves reskin once by alt-click;
//! - the NT gun cane (BandaStation) takes a diamond and, with the safety off (`attack_self`),
//!   fires it at what it is clicked on.
//!
//! Flashes in detail: the flash sound and the `cooldown` between uses; a clumsy user flashes
//! itself half the time; the hypnotic flash never burns out and hypnotises (a trance for the
//! vulnerable, else confusion, dizziness, drowsiness and pacifism); the arm implant overheats
//! for 2 s instead of burning out; a cyborg's flash is restored in a recharging station.
// shortcut: no `TRAIT_BATON_RESISTANCE`, force_say, baton or flash light, nunchaku block in
// throw mode, the electrostaff's block charge cost and powerless halving, the stunprod and
// boomerang crafting recipes (crafting group), flashes as assembly parts (assemblies group);
// the cell taken out is a new cell of the same type with the charge (its other state is lost);
// the cane's safety is toggled by using it in hand (tg: an action button); no walking-aid
// slowdown relief of the cane. No shoves into lockers or disposal bins and no directional
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
/// `try_use_flash`'s sound.
const FLASH_SOUND: &str = "sound/items/weapons/flash.ogg";
/// `clown_check`: a clumsy user's own `flash_mob(confusion_duration = 15 SECONDS)`.
const CLUMSY_FLASH_CONFUSION: f32 = 15.0;
/// `/obj/item/assembly/flash/hypnotic`, `/armimplant` (`flashcd` 2 s, twice that after a
/// burnout).
const HYPNOTIC_FLASH: &str = "/obj/item/assembly/flash/hypnotic";
const ARM_FLASH: &str = "/obj/item/assembly/flash/armimplant";
const ARM_FLASH_CD: f64 = 2.0;
/// The hypnotic flash's `cooldown = 20` (deciseconds).
const HYPNOTIC_COOLDOWN: f64 = 2.0;

/// Baton types with their own behaviour.
pub const CLASSIC_BATON: &str = "/obj/item/melee/baton";
const CONTRACTOR_BATON: &str = "/obj/item/melee/baton/telescopic/contractor_baton";
const STUNSWORD: &str = "/obj/item/melee/baton/security/stunsword";
const CATTLEPROD: &str = "/obj/item/melee/baton/security/cattleprod";
const TELEPROD: &str = "/obj/item/melee/baton/security/cattleprod/teleprod";
const SNATCHERPROD: &str = "/obj/item/melee/baton/security/cattleprod/telecrystalprod";
const BOOMERANG: &str = "/obj/item/melee/baton/security/boomerang";
const NUNCHAKU: &str = "/obj/item/melee/baton/nunchaku";
/// BandaStation `modular_bandastation/weapon/code/melee/electrostaff.dm`, `baton.dm`.
pub const ELECTROSTAFF: &str = "/obj/item/melee/baton/security/electrostaff";
const NT_CANE_GUN: &str = "/obj/item/melee/baton/nt_cane/gun";
const CONVERSION_KIT: &str = "/obj/item/conversion_kit";
const BLUESPACE_CRYSTAL: &str = "/obj/item/stack/ore/bluespace_crystal";
const TELECRYSTAL: &str = "/obj/item/stack/telecrystal";
const DIAMOND: &str = "/obj/item/stack/sheet/mineral/diamond";
const POWER_CELL: &str = "/obj/item/stock_parts/power_store/cell";
/// `SFX_SPARKS` of `turn_on`/`turn_off`, at 75.
const SPARKS_SOUND: &str = "sparks";
/// The cattleprod's igniter: `/obj/item/assembly/activate`'s `next_activate` 3 s.
const SPARKLER_COOLDOWN: f64 = 3.0;
/// `do_teleport` precision of the teleprod (`clumsy ? 50 : 15`).
const TELEPROD_PRECISION: i32 = 15;
const TELEPROD_CLUMSY_PRECISION: i32 = 50;
/// `/datum/component/boomerang`: `throw_range + 2`, and `BOOMERANG_REBOUND_INTERVAL`.
const BOOMERANG_RANGE: u32 = 7;
/// The electrostaff: `block_chance_two_handed`, `burn_damage`, `depleted_stamina_damage`,
/// `depleted_hit_sound`, `sound_on`.
pub const ELECTROSTAFF_BLOCK: f32 = 50.0;
const ELECTROSTAFF_BURN: f32 = 5.0;
const ELECTROSTAFF_DEPLETED_STAMINA: f32 = 20.0;
const WOOD_HIT_SOUND: &str = "sound/effects/woodhit.ogg";
const ELECTROSTAFF_SOUND_ON: &str = "modular_bandastation/weapon/sound/melee/electrostaff_on.ogg";
/// The gun cane: `fire_cooldown` 1.5 s, `safety_toggle_cooldown` 3 s, its sounds.
const CANE_FIRE_COOLDOWN: f64 = 1.5;
const CANE_SAFETY_COOLDOWN: f64 = 3.0;
const CANE_LOAD_SOUND: &str = "sound/effects/clock_tick.ogg";
const CANE_SHOT_SOUND: &str = "sound/items/weapons/effects/ric1.ogg";
const CANE_SAFETY_SOUND: &str = "sound/items/modsuit/atrocinator_step.ogg";

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
    /// The cell's `maxcharge` and `chargerate` (watts) and its type, while it has one.
    pub cell_max: f64,
    pub cell_rate: f64,
    pub cell_path: String,
    /// `Time::elapsed_secs_f64` before which it cannot stun again.
    pub ready_at: f64,
    base_inhand: String,
    base_force: f32,
    /// The stunprod's igniter `next_activate`.
    sparkler_ready_at: f64,
    /// The boomerang's `last_boomerang_throw` cooldown end.
    rebound_ready_at: f64,
}

/// The NT gun cane (`/obj/item/melee/baton/nt_cane/gun`): `safety_on`, `diamond_loaded` and
/// its cooldowns.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CaneGun {
    pub safety_on: bool,
    pub loaded: bool,
    fire_ready_at: f64,
    safety_ready_at: f64,
}

/// `/datum/component/reskinable_item` of stunswords and electrostaves, until used.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatonSkins;

/// On a radial prompt: the baton whose skin is asked for.
#[derive(Component, Clone, Copy, Debug)]
struct BatonSkinAsk(Entity);

/// A flash's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Flash {
    pub times_used: u32,
    pub burnt_out: bool,
    /// `last_used`, `Time::elapsed_secs_f64`.
    pub last_used: f64,
    /// `last_trigger` of the `cooldown`.
    pub(crate) last_trigger: f64,
    /// The arm implant's `overheat`, until.
    pub(crate) overheat_until: f64,
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
        // A baton toggles (an electrostaff wields, a gun cane's safety), a flash flashes
        // around.
        app.add_click_handler(ClickPhase::AttackSelf, 900, |w, c| {
            crate::security::attack_self(w, c.client, c.mob, c.target)
        });
        // Tools, cells, kits, crystals and diamonds onto a baton in the other hand or on the
        // floor.
        app.add_click_handler(ClickPhase::OtherHand, 1477, item_interaction);
        app.add_click_handler(ClickPhase::HeldOnTarget, 4287, item_interaction);
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
                    cane_ranged_clicks.after(ResolveClicks),
                    skin_answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
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
        cell_max: if charge.is_some() {
            dm.cell_maxcharge
        } else {
            0.0
        },
        cell_rate: if charge.is_some() {
            dm.cell_chargerate
        } else {
            0.0
        },
        cell_path: if charge.is_some() {
            dm.cell_path.clone()
        } else {
            String::new()
        },
        ready_at: 0.0,
        base_inhand: item.inhand_icon_state.clone(),
        base_force: item.force,
        sparkler_ready_at: 0.0,
        rebound_ready_at: 0.0,
    });
    if is_under(&dm.path, NT_CANE_GUN) {
        commands.entity(entity).insert_if_new(CaneGun {
            safety_on: true,
            loaded: false,
            fire_ready_at: 0.0,
            safety_ready_at: 0.0,
        });
    }
    if is_under(&dm.path, STUNSWORD) || is_under(&dm.path, ELECTROSTAFF) {
        commands.entity(entity).insert_if_new(BatonSkins);
    }
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
                let handled = baton_attack(world, actor, item, target, click.modifiers.right, now);
                // The gun cane's `afterattack` with the safety off.
                if world.get::<CaneGun>(item).is_some() {
                    try_fire_cane(world, actor, item, target);
                }
                handled
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
            // `/datum/element/cuffsnapping`: a right click cuts the target's cuffs.
            Some(item)
                if click.modifiers.right
                    && crate::restraints::cuffsnap(world, actor, item, target) =>
            {
                true
            }
            // The power-fist's and the supermatter sword's own hits (`crate::weapons::special`).
            Some(item)
                if crate::weapons::special::attack(world, click.client, mob, item, target) =>
            {
                true
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
                    // BandaStation's strongarms shove harder (`crate::medical::strong_limbs`).
                    if !crate::medical::strong_limbs::strong_shove(world, mob, target) {
                        shove(world, mob, target);
                    }
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

/// The electrostaff (`/obj/item/melee/baton/security/electrostaff`).
fn is_staff(dm: &DmBaton) -> bool {
    is_under(&dm.path, ELECTROSTAFF)
}

/// The electrostaff's `has_power`: a cell holding a hit.
fn has_power(state: &Baton, dm: &DmBaton) -> bool {
    state.charge.is_some_and(|c| c >= dm.cell_hit_cost)
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
    let state = world.get::<Baton>(baton).cloned().expect("checked");
    // The electrostaff's `pre_attack`: a powered right click burns too (its `afterattack`).
    let staff_burn = harm && is_staff(&dm) && has_power(&state, &dm);
    // `nunchaku/afterattack`: `randomize_state`.
    if is_under(&dm.path, NUNCHAKU) {
        randomize_nunchaku(world, baton);
    }
    let handled = stun_attack(world, actor, baton, &dm, &state, target, harm, now);
    if staff_burn && !handled {
        damage(
            world,
            target,
            None,
            crate::health::DamageType::Burn,
            ELECTROSTAFF_BURN,
        );
    }
    handled
}

/// `pre_attack` (`try_stun`, the clumsy redirect) and `afterattack` (the stun and its lines).
#[allow(clippy::too_many_arguments)]
fn stun_attack(
    world: &mut World,
    actor: Actor,
    baton: Entity,
    dm: &DmBaton,
    state: &Baton,
    target: Entity,
    harm: bool,
    now: f64,
) -> bool {
    if harm && !dm.stun_on_harmbaton {
        return false;
    }
    let combat = world.get::<CombatMode>(actor.mob).is_some_and(|c| c.0);
    // `electrostaff/try_stun`: wielded without power it strikes as if on.
    let active = state.active
        || (is_staff(dm)
            && !has_power(state, dm)
            && crate::weapons::two_handed::is_wielded(world, baton));
    let (user, victim, item) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, baton),
    );
    // `try_stun`.
    let can_stun = if dm.uses_cell && !active && !harm && !combat {
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
    } else if !active {
        false
    } else if crate::genetics::effects::is_hulk(world, actor.mob) {
        // `check_chunky_fingers` (no baton is `chunky_finger_usable`).
        if !harm {
            tell(
                world,
                actor.client,
                Span::Warning,
                "пальцы слишком большие!",
            );
        }
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
    } else if is_under(&dm.path, CATTLEPROD) {
        // `cattleprod/try_stun`: `sparkler.activate()`.
        spark_igniter(world, baton, now)
    } else {
        true
    };
    if !can_stun {
        // Harmbatons always hit; an active baton that cannot stun, or a poke, does not.
        return !harm && (active || !combat);
    }
    let attack_speed = world.get::<DmItem>(baton).map_or(8.0, |i| i.attack_speed);
    // Clumsy people redirect the attack: the stun and twice the force on their own head.
    if crate::genetics::is_clumsy(world, actor.mob)
        && world.resource_mut::<CombatRng>().chance(50.0)
    {
        clumsy_baton(world, actor, baton, dm, now, attack_speed, &user, &item);
        return true;
    }
    if !harm {
        // The stun replaces the hit: force 0, no hit message, the attack cooldown.
        world
            .entity_mut(actor.mob)
            .insert(NextMove(now + f64::from(attack_speed) / 10.0));
    }
    finalize_baton_attack(world, baton, dm, Some(actor.mob), target, now, false);
    if let Some(&at) = world.get::<TilePos>(target) {
        // `afterattack`: a cyborg gets `get_cyborg_stun_description` (and the bonk, unless the
        // baton stuns cyborgs).
        let cyborg = world
            .get::<crate::silicon::cyborg::Cyborg>(target)
            .is_some();
        let (visible, local, local_span) = if cyborg && stuns_cyborgs(dm) {
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

/// The stunprod's igniter (`/obj/item/assembly/igniter/activate`): once per 3 s, with sparks.
fn spark_igniter(world: &mut World, baton: Entity, now: f64) -> bool {
    let Some(mut state) = world.get_mut::<Baton>(baton) else {
        return false;
    };
    if now < state.sparkler_ready_at {
        return false;
    }
    state.sparkler_ready_at = now + SPARKLER_COOLDOWN;
    if let Some(pos) = crate::explosions::turf_of(world, baton) {
        crate::effects::particles::do_sparks(world, 2, false, pos);
    }
    true
}

/// `nunchaku/randomize_state`.
fn randomize_nunchaku(world: &mut World, baton: Entity) {
    const STATES: [&str; 3] = ["nunchaku", "nunchaku_x", "nunchaku_y"];
    let i = world.resource_mut::<CombatRng>().range(0, 2) as usize;
    crate::obj_damage::set_icon_state(world, baton, STATES[i]);
}

/// `amount` of `kind` to a living mob, on `zone` or over the body.
pub(crate) fn damage(
    world: &mut World,
    mob: Entity,
    zone: Option<crate::zone::BodyZone>,
    kind: crate::health::DamageType,
    amount: f32,
) {
    if let Ok(mut entity) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        let mut new = *stat;
        match zone {
            Some(zone) => {
                crate::health::apply_damage_zone(&mut health, &mut new, zone, kind, amount);
            }
            None => {
                crate::health::apply_damage(&mut health, &mut new, kind, amount);
            }
        }
        stat.set_if_neq(new);
    }
}

/// `playsound(src, sound, vol, TRUE)` where the item is.
fn play_at(world: &mut World, item: Entity, sound: &str, vol: f32) {
    if sound.is_empty() {
        return;
    }
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
    }
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
    finalize_baton_attack(world, baton, dm, Some(mob), mob, now, true);
    let force = world.get::<DmItem>(baton).map_or(0.0, |i| i.force);
    damage(
        world,
        mob,
        Some(crate::zone::BodyZone::Head),
        crate::health::DamageType::Brute,
        2.0 * force,
    );
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
    dm.path.starts_with(CONTRACTOR_BATON)
}

/// tg `finalize_baton_attack`: the cooldown, `on_stun_sound`, `baton_effect` and
/// `set_batoned`. `user` is `None` for a baton thrown by nobody.
fn finalize_baton_attack(
    world: &mut World,
    baton: Entity,
    dm: &DmBaton,
    user: Option<Entity>,
    target: Entity,
    now: f64,
    clumsy: bool,
) {
    let cooldown = f64::from(dm.cooldown) / 10.0;
    if let Some(mut state) = world.get_mut::<Baton>(baton) {
        state.ready_at = now + cooldown;
    }
    play_at(world, baton, &dm.on_stun_sound, dm.on_stun_volume);
    if baton_effect(world, baton, dm, user, target, now, clumsy)
        && let Some(user) = user
    {
        set_batoned(world, user, target, cooldown, now);
    }
}

/// tg `baton_effect` (+ the security baton's charge and `additional_effects_non_cyborg`, the
/// electrostaff's powerless strike, the prods' tricks). True when it took.
fn baton_effect(
    world: &mut World,
    baton: Entity,
    dm: &DmBaton,
    user: Option<Entity>,
    target: Entity,
    now: f64,
    clumsy: bool,
) -> bool {
    if dm.uses_cell {
        let state = world.get::<Baton>(baton).cloned().expect("checked");
        if is_staff(dm) && !has_power(&state, dm) {
            // `electrostaff/baton_effect` without power.
            adjust_stamina(world, target, ELECTROSTAFF_DEPLETED_STAMINA);
            knockdown(world, target, dm.knockdown_time / 10.0, 0.0);
            play_at(world, baton, WOOD_HIT_SOUND, 75.0);
            return true;
        }
        let holder = world.get::<CarriedBy>(baton).map(|c| c.0);
        if let Some(mut cyborg) =
            holder.and_then(|h| world.get_mut::<crate::silicon::cyborg::Cyborg>(h))
        {
            // `security/baton_effect`, `iscyborg(loc)`: the cyborg's own cell pays.
            if cyborg.charge < dm.cell_hit_cost {
                return false;
            }
            cyborg.charge -= dm.cell_hit_cost;
        } else {
            // `deductcharge`: `cell.use` refuses when short; below a hit the baton turns off.
            let Some(charge) = state.charge.filter(|&c| c >= dm.cell_hit_cost) else {
                return false;
            };
            let left = charge - dm.cell_hit_cost;
            if let Some(mut b) = world.get_mut::<Baton>(baton) {
                b.charge = Some(left);
            }
            if state.active && left < dm.cell_hit_cost {
                set_active(world, baton, false);
            }
        }
    }
    if world
        .get::<crate::silicon::cyborg::Cyborg>(target)
        .is_some()
    {
        // `baton_effect`: `if(!affect_cyborg) return FALSE`; the rest of it is the paralysis.
        if !stuns_cyborgs(dm) {
            return false;
        }
        crate::status_effects::apply(
            world,
            target,
            crate::status_effects::Incap::Paralyze,
            STUN_TIME_CYBORG,
        );
        return true;
    }
    // `run_armor_check(BODY_ZONE_CHEST, armour_type_against_stun, armour_penetration =
    // stun_armour_penetration)`, then `apply_damage(stamina_damage, STAMINA, blocked)`.
    let armor = crate::armor::getarmor(
        world,
        target,
        crate::zone::BodyZone::Chest,
        &dm.armour_type_against_stun,
    );
    let (armor, line) =
        crate::armor::run_armor_check(armor, dm.stun_armour_penetration, false, None, None);
    if let Some((span, text)) = line {
        tell_mob(world, target, span, text);
    }
    adjust_stamina(
        world,
        target,
        crate::armor::blocked_damage(dm.stamina_damage, armor),
    );
    // `additional_effects_non_cyborg`: the security baton jitters (40 s), confuses and stutters
    // (16 s); the contractor baton jitters and stutters 40 s.
    let contractor = dm.path.starts_with(CONTRACTOR_BATON);
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
        // `stun_override = 0`: the knockdown comes 2 s later (`apply_stun_effect_end`).
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
    if is_under(&dm.path, TELEPROD) {
        teleprod_effect(world, target, clumsy);
    } else if is_under(&dm.path, SNATCHERPROD) {
        snatcherprod_effect(world, baton, user, target, clumsy);
    }
    true
}

/// `teleprod/baton_effect`: the victim blinks away (`move_resist` of mobs is below
/// `MOVE_FORCE_OVERPOWERING`).
fn teleprod_effect(world: &mut World, target: Entity, clumsy: bool) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let precision = if clumsy {
        TELEPROD_CLUMSY_PRECISION
    } else {
        TELEPROD_PRECISION
    };
    crate::teleport::do_teleport(
        world,
        target,
        pos,
        crate::teleport::Tele {
            precision,
            channel: crate::teleport::Channel::Bluespace,
            ..default()
        },
    );
}

/// `telecrystalprod/baton_effect`: the victim's held item jumps into the user's other hand (or
/// at its feet); a clumsy user loses the prod to a blink.
fn snatcherprod_effect(
    world: &mut World,
    baton: Entity,
    user: Option<Entity>,
    target: Entity,
    clumsy: bool,
) {
    let Some(user) = user else {
        return;
    };
    let Some(stuff) = world
        .get::<Inventory>(target)
        .and_then(Inventory::active_item)
    else {
        return;
    };
    if crate::inventory::no_drop(world, stuff) {
        return;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(target) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(stuff) {
                *hand = None;
            }
        }
    }
    world.entity_mut(stuff).remove::<CarriedBy>();
    let name = ss13_shared::capitalize(&name_of(world, stuff));
    let inactive = world.get::<Inventory>(user).and_then(|i| {
        let other = 1 - i.active;
        (i.hands[other].is_none() && crate::medical::bodyparts::has_hand(world, user, other))
            .then_some(other)
    });
    let Some(&user_pos) = world.get::<TilePos>(user) else {
        return;
    };
    let text = if let Some(hand) = inactive {
        world
            .entity_mut(stuff)
            .remove::<TilePos>()
            .insert(CarriedBy(user));
        if let Some(mut inventory) = world.get_mut::<Inventory>(user) {
            inventory.hands[hand] = Some(stuff);
        }
        format!(
            "{name} внезапно появляется в руке {}!",
            name_of(world, user)
        )
    } else {
        crate::inventory::put_down(world, stuff, user_pos);
        format!("{name} внезапно появляется!")
    };
    show(
        world,
        user_pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Warning, text),
        &[],
    );
    if clumsy && world.get::<CarriedBy>(baton).is_some_and(|c| c.0 == user) {
        crate::inventory::put_down(world, baton, user_pos);
        crate::teleport::do_teleport(
            world,
            baton,
            user_pos,
            crate::teleport::Tele {
                precision: TELEPROD_CLUMSY_PRECISION,
                channel: crate::teleport::Channel::Bluespace,
                ..default()
            },
        );
    }
}

/// `turn_on`/`turn_off` of a security baton: the state, the sparks sound (and sparks on), the
/// look.
fn set_active(world: &mut World, baton: Entity, on: bool) {
    if let Some(mut b) = world.get_mut::<Baton>(baton) {
        b.active = on;
    }
    if let Some(pos) = crate::explosions::turf_of(world, baton) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPARKS_SOUND, 75.0, true),
        );
        if on {
            crate::effects::particles::do_sparks(world, 1, true, pos);
        }
    }
    refresh_baton(world, baton);
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
    if world.get::<CaneGun>(item).is_some() {
        toggle_cane_safety(world, client, mob, item);
        return true;
    }
    let (Some(dm), Some(mut state)) = (
        world.get::<DmBaton>(item).cloned(),
        world.get::<Baton>(item).cloned(),
    ) else {
        return false;
    };
    if is_staff(&dm) {
        // `electrostaff/attack_self`: wield or let go; the wield turns it on ([`on_wield`]).
        return crate::weapons::two_handed::attack_self(world, client, mob, item);
    }
    let text = if dm.uses_cell {
        // `/obj/item/melee/baton/security/attack_self`.
        let charge = state.charge.unwrap_or(0.0);
        let on = charge >= dm.cell_hit_cost && !state.active;
        set_active(world, item, on);
        if on {
            "включение"
        } else if state.charge.is_none() {
            "нет источника питания!"
        } else if charge < dm.cell_hit_cost {
            "не хватает заряда!"
        } else {
            "выключение"
        }
    } else if dm.telescopic {
        // `/datum/component/transforming` -> `on_transform`.
        state.active = !state.active;
        let active = state.active;
        world.entity_mut(item).insert(state);
        play_at(world, item, &dm.on_sound, 50.0);
        refresh_baton(world, item);
        if active {
            "вытянуто"
        } else {
            "втянуто"
        }
    } else {
        return false;
    };
    tell(world, client, Span::Notice, text);
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    true
}

/// The electrostaff's `on_wield`/`on_unwield` (`crate::weapons::two_handed`).
pub fn on_wield(world: &mut World, client: Option<Entity>, item: Entity, wielded: bool) {
    let (Some(dm), Some(state)) = (
        world.get::<DmBaton>(item).cloned(),
        world.get::<Baton>(item).cloned(),
    ) else {
        return;
    };
    if !is_staff(&dm) {
        return;
    }
    let powered = has_power(&state, &dm);
    let text = if wielded {
        if powered {
            play_at(world, item, ELECTROSTAFF_SOUND_ON, 75.0);
            set_active(world, item, true);
            Some("включение")
        } else if state.charge.is_none() {
            Some("нет источника питания!")
        } else {
            Some("не хватает заряда!")
        }
    } else if state.active {
        if powered {
            play_at(world, item, ELECTROSTAFF_SOUND_ON, 75.0);
        }
        set_active(world, item, false);
        powered.then_some("выключение")
    } else {
        None
    };
    refresh_baton(world, item);
    if let (Some(client), Some(text)) = (client, text) {
        tell(world, client, Span::Notice, text);
    }
}

/// A baton's look and live vars: `update_icon_state` of a stun baton (`_active`, `_nocell`;
/// the electrostaff's `_wield`, `_nocell_wield`), a telescopic baton's `_on` state, force and
/// in-hand sprite.
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
    // `active_changes_inhand = FALSE` of the stunprod and the boomerang.
    let inhand_follows = !is_under(&dm.path, CATTLEPROD) && !is_under(&dm.path, BOOMERANG);
    let (icon_state, inhand, force) = if dm.telescopic {
        if state.active {
            let inhand = dm.on_inhand_icon_state.clone();
            (format!("{base}_on"), inhand, Some(dm.active_force))
        } else {
            (base.clone(), String::new(), Some(state.base_force))
        }
    } else if is_staff(&dm) {
        // `electrostaff/update_icon_state`; its force is the two-handed component's.
        let suffix = if crate::weapons::two_handed::is_wielded(world, item) {
            if state.active && has_power(&state, &dm) {
                "_active"
            } else if state.charge.is_some() {
                "_wield"
            } else {
                "_nocell_wield"
            }
        } else if state.charge.is_none() {
            "_nocell"
        } else {
            ""
        };
        let look = format!("{base}{suffix}");
        (look.clone(), look, None)
    } else if dm.uses_cell && state.active {
        let inhand = if inhand_follows {
            format!("{}_active", state.base_inhand)
        } else {
            state.base_inhand.clone()
        };
        (format!("{base}_active"), inhand, None)
    } else if dm.uses_cell && state.charge.is_none() {
        (format!("{base}_nocell"), state.base_inhand.clone(), None)
    } else {
        (base.clone(), state.base_inhand.clone(), None)
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|icons| icons.get(&dmi, &icon_state));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(item))
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
    if let Some(mut item) = world.get_mut::<DmItem>(item) {
        if item.inhand_icon_state != inhand {
            item.inhand_icon_state = inhand;
        }
        if let Some(force) = force
            && item.force != force
        {
            item.force = force;
        }
    }
    if let Some(carrier) = world.get::<CarriedBy>(item).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(carrier)
    {
        inventory.set_changed();
    }
}

/// Items used on a baton (in the other hand or on the floor): a screwdriver takes the cell out
/// (`screwdriver_act`), a cell goes in, a conversion kit (`convert`), a crystal onto a stunprod,
/// a diamond into the gun cane.
fn item_interaction(world: &mut World, c: &crate::click_handlers::ClickCtx) -> bool {
    let (Some(held), target) = (c.held, c.target) else {
        return false;
    };
    let Some(dm) = world.get::<DmBaton>(target).cloned() else {
        return false;
    };
    if world.get::<Baton>(target).is_none() {
        return false;
    }
    let held_path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if world.get::<CaneGun>(target).is_some() && is_under(&held_path, DIAMOND) {
        load_cane(world, c.client, held, target);
        return true;
    }
    if !dm.uses_cell {
        return false;
    }
    if is_under(&held_path, CONVERSION_KIT) {
        if !dm.convertible {
            return false;
        }
        convert(world, held, target);
        return true;
    }
    if crate::tools::tool_of(world, held).map(|(t, _)| t)
        == Some(crate::tools::ToolKind::Screwdriver)
    {
        if remove_cell(world, c.client, target) {
            play_at(world, target, "sound/items/tools/screwdriver.ogg", 50.0);
        }
        return true;
    }
    if is_under(&held_path, POWER_CELL) {
        insert_cell(world, c.client, c.mob, held, target);
        return true;
    }
    let upgradable = is_under(&dm.path, CATTLEPROD)
        && !is_under(&dm.path, TELEPROD)
        && !is_under(&dm.path, SNATCHERPROD);
    if world.get::<ss13_mapload::DmStack>(held).is_some() && is_under(&dm.path, CATTLEPROD) {
        upgrade_prod(world, c.client, c.mob, held, target, &held_path, upgradable);
        return true;
    }
    false
}

/// `tryremovecell`: the cell drops where the baton is (`drop_location`).
fn remove_cell(world: &mut World, client: Entity, baton: Entity) -> bool {
    let (Some(dm), Some(state)) = (
        world.get::<DmBaton>(baton).cloned(),
        world.get::<Baton>(baton).cloned(),
    ) else {
        return false;
    };
    let Some(charge) = state.charge else {
        return false;
    };
    if dm.cell_fixed {
        return false;
    }
    let Some(pos) = crate::explosions::turf_of(world, baton) else {
        return false;
    };
    // An unconverted baton does not know its cell type: a standard cell.
    let path = if state.cell_path.is_empty() {
        POWER_CELL
    } else {
        state.cell_path.as_str()
    };
    if let Some(cell) = crate::construction::spawn_obj(world, path, pos) {
        // The charge after the prototype's own (`mecha::core::cell_charge`).
        world.flush();
        world
            .entity_mut(cell)
            .insert(crate::mecha::core::ItemCell(crate::power::Cell {
                charge,
                maxcharge: state.cell_max,
                chargerate: state.cell_rate,
            }));
    }
    if let Some(mut b) = world.get_mut::<Baton>(baton) {
        b.charge = None;
        b.cell_max = 0.0;
        b.cell_rate = 0.0;
        b.cell_path.clear();
    }
    // `Exited`: no cell, the baton turns off.
    if state.active {
        set_active(world, baton, false);
    }
    refresh_baton(world, baton);
    let name = name_of(world, baton);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы вынимаете батарею из {name}."),
    );
    true
}

/// `security/item_interaction` with a cell.
fn insert_cell(world: &mut World, client: Entity, mob: Entity, cell: Entity, baton: Entity) {
    let (Some(dm), Some(state)) = (
        world.get::<DmBaton>(baton).cloned(),
        world.get::<Baton>(baton).cloned(),
    ) else {
        return;
    };
    let Some(data) = world.get::<crate::mecha::core::ItemCell>(cell).map(|c| c.0) else {
        return;
    };
    let name = ss13_shared::capitalize(&name_of(world, baton));
    if state.charge.is_some() {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} уже имеет батарею!"),
        );
        return;
    }
    if data.maxcharge < dm.cell_hit_cost {
        tell(
            world,
            client,
            Span::Notice,
            format!("{name} нуждается в более емкой батарее."),
        );
        return;
    }
    // `transferItemToLoc`: only from the user's hands.
    if world.get::<CarriedBy>(cell).is_none_or(|c| c.0 != mob)
        || crate::inventory::no_drop(world, cell)
    {
        return;
    }
    let path = world
        .get::<DmItem>(cell)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    world.despawn(cell);
    if let Some(mut b) = world.get_mut::<Baton>(baton) {
        b.charge = Some(data.charge);
        b.cell_max = data.maxcharge;
        b.cell_rate = data.chargerate;
        b.cell_path = path;
    }
    refresh_baton(world, baton);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы вставляете батарею в {name}."),
    );
}

/// `security/convert`: a police baton where the stun baton was, the kit and the baton gone.
fn convert(world: &mut World, kit: Entity, baton: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, baton) else {
        return;
    };
    crate::construction::spawn_obj(world, CLASSIC_BATON, pos);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/items/tools/drill_use.ogg", 80.0, true),
    );
    world.despawn(kit);
    world.despawn(baton);
}

/// `cattleprod/item_interaction` with a stack: a bluespace crystal makes a teleprod, a
/// telecrystal a snatcherprod (the new prod into the user's hands).
fn upgrade_prod(
    world: &mut World,
    client: Entity,
    mob: Entity,
    stack: Entity,
    prod: Entity,
    stack_path: &str,
    upgradable: bool,
) {
    let name = name_of(world, prod);
    if !upgradable {
        tell(
            world,
            client,
            Span::Warning,
            format!("{} уже улучшен!", ss13_shared::capitalize(&name)),
        );
        return;
    }
    if world.get::<Baton>(prod).is_some_and(|b| b.charge.is_some()) {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы не можете вставить кристалл в {name} пока установлен источник питания!"),
        );
        return;
    }
    let new = if is_under(stack_path, BLUESPACE_CRYSTAL) {
        TELEPROD
    } else if is_under(stack_path, TELECRYSTAL) {
        SNATCHERPROD
    } else {
        let what = name_of(world, stack);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы не думаете, что {what} способен хоть как-то улучшить {name}."),
        );
        return;
    };
    let what = name_of(world, stack);
    if !crate::tools::use_stack(world, stack, 1) {
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы крепко вставляете {what} в воспламенитель."),
    );
    let Some(pos) = world.get::<TilePos>(mob).copied() else {
        return;
    };
    world.despawn(prod);
    if let Some(new) = crate::construction::spawn_obj(world, new, pos)
        && let Some(inventory) = world.get::<Inventory>(mob).cloned()
    {
        crate::storage::put_in_hands(world, mob, &inventory, new);
    }
}

/// `/obj/item/melee/baton/security/throw_impact`: an active security baton that was not
/// caught stuns what it hit at `throw_stun_chance`; a boomerang flies back at its thrower.
pub fn thrown(world: &mut World, item: Entity, hit: Option<Entity>, thrower: Option<Entity>) {
    let (Some(dm), Some(state)) = (
        world.get::<DmBaton>(item).cloned(),
        world.get::<Baton>(item).cloned(),
    ) else {
        return;
    };
    if is_under(&dm.path, NUNCHAKU) {
        // `nunchaku/after_throw`.
        randomize_nunchaku(world, item);
    }
    if world.get::<CarriedBy>(item).is_some() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(hit) = hit
        && dm.uses_cell
        && state.active
        && world.get::<Health>(hit).is_some()
        && world
            .resource_mut::<CombatRng>()
            .chance(dm.throw_stun_chance)
    {
        finalize_baton_attack(world, item, &dm, thrower, hit, now, false);
    }
    // `/datum/component/boomerang`: `return_hit_throw`, `return_missed_throw`.
    if is_under(&dm.path, BOOMERANG)
        && now >= state.rebound_ready_at
        && let Some(thrower) = thrower
        && let Some(&to) = world.get::<TilePos>(thrower)
    {
        if let Some(mut b) = world.get_mut::<Baton>(item) {
            b.rebound_ready_at = now + 1.0;
        }
        let name = ss13_shared::capitalize(&name_of(world, item));
        if let Some(&at) = world.get::<TilePos>(item) {
            show(
                world,
                at,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!("{name} летит обратно к {}!", name_of(world, thrower)),
                ),
                &[(
                    thrower,
                    Span::Danger,
                    format!("{name} летит обратно к вам!"),
                )],
            );
        }
        let speed = world
            .get::<DmItem>(item)
            .map_or(1.0, |i| i.throw_speed.max(1.0));
        if crate::throw::throw_at(world, item, to, BOOMERANG_RANGE, speed)
            && let Some(mut flight) = world.get_mut::<crate::throw::Thrown>(item)
        {
            flight.thrower = Some(thrower);
            flight.target = Some(thrower);
        }
    }
}

/// `try_load_diamond`.
fn load_cane(world: &mut World, client: Entity, diamond: Entity, cane: Entity) {
    if world.get::<CaneGun>(cane).is_some_and(|g| g.loaded) {
        tell(world, client, Span::Warning, "уже заряжена!");
        return;
    }
    if crate::tools::use_stack(world, diamond, 1)
        && let Some(mut gun) = world.get_mut::<CaneGun>(cane)
    {
        gun.loaded = true;
    }
    play_at(world, cane, CANE_LOAD_SOUND, 50.0);
    tell(world, client, Span::Notice, "заряжено");
}

/// `toggle_safety` (tg's action button; here the cane used in the active hand).
fn toggle_cane_safety(world: &mut World, client: Entity, mob: Entity, cane: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<Inventory>(mob).and_then(Inventory::active_item) != Some(cane) {
        tell(world, client, Span::Warning, "возьмите в активную руку!");
        return;
    }
    let Some(mut gun) = world.get_mut::<CaneGun>(cane) else {
        return;
    };
    if now < gun.safety_ready_at {
        tell(world, client, Span::Warning, "перезарядка!");
        return;
    }
    gun.safety_ready_at = now + CANE_SAFETY_COOLDOWN;
    gun.safety_on = !gun.safety_on;
    let on = gun.safety_on;
    let text = if on {
        "предохранитель: включён."
    } else {
        "предохранитель: выключен."
    };
    tell(world, client, Span::Notice, text);
    // `play_safety_sound`.
    // shortcut: the three clicks play at once (tg spaces them 0.5 s apart).
    play_at(world, cane, CANE_SAFETY_SOUND, 50.0);
}

/// `try_fire_diamond`: true when the click was the cane's shot (fired or refused).
fn try_fire_cane(world: &mut World, actor: Actor, cane: Entity, target: Entity) -> bool {
    let Some(gun) = world.get::<CaneGun>(cane).copied() else {
        return false;
    };
    if gun.safety_on || !gun.loaded {
        return false;
    }
    if target == actor.mob
        || world
            .get::<CarriedBy>(target)
            .is_some_and(|c| c.0 == actor.mob)
    {
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < gun.fire_ready_at {
        tell(world, actor.client, Span::Warning, "перезарядка!");
        return true;
    }
    let (Some(&from), Some(to)) = (
        world.get::<TilePos>(actor.mob),
        crate::explosions::turf_of(world, target),
    ) else {
        return true;
    };
    let Some(data) = world.get::<DmBaton>(cane).map(|d| d.projectile.clone()) else {
        return true;
    };
    if let Some(mut g) = world.get_mut::<CaneGun>(cane) {
        g.fire_ready_at = now + CANE_FIRE_COOLDOWN;
        g.loaded = false;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(from, CANE_SHOT_SOUND, 50.0, true),
    );
    let zone = world
        .get::<crate::health::ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    crate::projectile::fire(
        world,
        crate::projectile::Shot {
            data: &data,
            firer: actor.mob,
            from,
            target: Some(target),
            target_pos: to,
            zone,
            hit_prone: world.get::<CombatMode>(actor.mob).is_some_and(|c| c.0),
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: Some(cane),
        },
    );
    true
}

/// `ranged_interact_with_atom` of the gun cane, and its `afterattack` refusal far away.
fn cane_ranged_clicks(
    world: &mut World,
    mut cursor: Local<MessageCursor<crate::click::RangedClick>>,
) {
    let clicks: Vec<crate::click::RangedClick> = cursor
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(cane) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&h| world.get::<CaneGun>(h).is_some())
        else {
            continue;
        };
        let actor = Actor {
            client: click.client,
            mob: click.mob,
            item: Some(cane),
        };
        let safety = world.get::<CaneGun>(cane).is_some_and(|g| g.safety_on);
        if !try_fire_cane(world, actor, cane, click.target) && !safety {
            tell(world, click.client, Span::Warning, "нет заряда!");
        }
    }
}

/// `/datum/atom_skin/stunsword/*` and `/datum/atom_skin/electrostaff/*`: `preview_name` and
/// the new icon state (sorted by name, as the radial lists them).
const STUNSWORD_SKINS: [(&str, &str); 2] =
    [("Default", "stunsword"), ("Energy", "stunsword_energy")];
const ELECTROSTAFF_SKINS: [(&str, &str); 4] = [
    ("Blue", "electrostaff_blue"),
    ("Orange", "electrostaff_orange"),
    ("Purple", "electrostaff_purple"),
    ("Red", "electrostaff_red"),
];

fn skins_of(world: &World, baton: Entity) -> &'static [(&'static str, &'static str)] {
    match world.get::<DmBaton>(baton) {
        Some(dm) if is_under(&dm.path, STUNSWORD) => &STUNSWORD_SKINS,
        Some(dm) if is_under(&dm.path, ELECTROSTAFF) => &ELECTROSTAFF_SKINS,
        _ => &[],
    }
}

/// `reskinable_item/on_click_alt_reskin` of a stunsword or electrostaff: the skin radial.
/// True when it handled the click.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, baton: Entity) -> bool {
    if world.get::<BatonSkins>(baton).is_none() {
        return false;
    }
    let skins = skins_of(world, baton);
    let Some(dmi) = world.get::<DmAtom>(baton).map(|a| a.icon.clone()) else {
        return false;
    };
    let choices = skins
        .iter()
        .map(|(name, state)| ss13_tgui::prompt::RadialChoice {
            name: (*name).to_owned(),
            icon: crate::radial::icon_of(world, &dmi, state),
        })
        .collect();
    let title = name_of(world, baton);
    if let Some(prompt) = crate::radial::show(
        world,
        crate::radial::RadialMenu {
            client,
            actor: mob,
            anchor: baton,
            title,
            choices,
            require_near: true,
        },
    ) {
        world.entity_mut(prompt).insert(BatonSkinAsk(baton));
    }
    true
}

fn skin_answers(
    world: &mut World,
    mut cursor: Local<MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    let answers: Vec<(
        Entity,
        Entity,
        Entity,
        Option<ss13_tgui::prompt::PromptAnswer>,
    )> = cursor
        .read(world.resource::<Messages<ss13_tgui::prompt::PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(BatonSkinAsk(baton)) = world.get::<BatonSkinAsk>(prompt).copied() else {
            continue;
        };
        let Some(ss13_tgui::prompt::PromptAnswer::Item(i)) = answer else {
            continue;
        };
        // Again at the moment of effect: still reskinnable and in reach.
        let skins = skins_of(world, baton);
        if i >= skins.len()
            || world.get::<BatonSkins>(baton).is_none()
            || !crate::storage::in_reach(world, mob, baton)
        {
            continue;
        }
        let (name, state) = skins[i];
        world.entity_mut(baton).remove::<BatonSkins>();
        // `change_base_icon_state`, `change_inhand_icon_state` (and the electrostaff's
        // `change_worn_icon_state`).
        if let Some(mut atom) = world.get_mut::<DmAtom>(baton) {
            atom.icon_state = state.to_owned();
        }
        if let Some(mut b) = world.get_mut::<Baton>(baton) {
            b.base_inhand = state.to_owned();
        }
        let staff = world.get::<DmBaton>(baton).is_some_and(is_staff);
        if staff && let Some(mut item) = world.get_mut::<DmItem>(baton) {
            item.worn_icon_state = state.to_owned();
        }
        refresh_baton(world, baton);
        let text = format!("{} is now skinned as '{name}.'", name_of(world, baton));
        tell(world, client, Span::Info, text);
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

fn flash_path(world: &World, flash: Entity) -> String {
    world
        .get::<DmItem>(flash)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// tg `try_use_flash(user)` + `flash_recharge` (`interval` 10 s, burnout chance
/// `times_used * 3`) + `clown_check`; the arm implant's and the hypnotic flash's own rules.
fn try_use_flash(world: &mut World, flash: Entity, user: Option<Entity>, now: f64) -> bool {
    let Some(mut state) = world.get::<Flash>(flash).cloned() else {
        return false;
    };
    let path = flash_path(world, flash);
    if is_under(&path, ARM_FLASH) {
        // `armimplant/try_use_flash`: an overheat of `flashcd` instead of the burnout.
        if now < state.overheat_until {
            if let Some(owner) = arm_flash_owner(world, flash) {
                tell_mob(
                    world,
                    owner,
                    Span::Warning,
                    "Ваш фотонный проектор слишком перегрет, чтобы снова использовать его так скоро!"
                        .to_owned(),
                );
            }
            return false;
        }
        state.overheat_until = now + ARM_FLASH_CD;
        world.entity_mut(flash).insert(state);
        play_at(world, flash, FLASH_SOUND, 100.0);
        return true;
    }
    let cooldown = if is_under(&path, HYPNOTIC_FLASH) {
        HYPNOTIC_COOLDOWN
    } else {
        0.0
    };
    if state.burnt_out || now < state.last_trigger + cooldown {
        return false;
    }
    state.last_trigger = now;
    play_at(world, flash, FLASH_SOUND, 100.0);
    state.times_used += 1;
    let recharged = ((now - state.last_used) / 10.0).floor().max(0.0) as u32;
    state.times_used = state.times_used.saturating_sub(recharged);
    state.last_used = now;
    let burn = world
        .resource_mut::<CombatRng>()
        .chance(state.times_used as f32 * 3.0);
    world.entity_mut(flash).insert(state);
    if burn {
        burn_out(world, flash);
        if world.get::<Flash>(flash).is_some_and(|f| f.burnt_out) {
            return false;
        }
    }
    // `clown_check`: a clumsy user flashes itself instead.
    if let Some(user) = user
        && crate::genetics::is_clumsy(world, user)
        && world.resource_mut::<CombatRng>().chance(50.0)
    {
        flash_mob_with(world, flash, user, user, CLUMSY_FLASH_CONFUSION, false);
        return false;
    }
    true
}

/// The mob whose arm implant holds this flash.
fn arm_flash_owner(world: &World, flash: Entity) -> Option<Entity> {
    world.get::<CarriedBy>(flash).map(|c| c.0)
}

/// tg `burn_out`: "burnt-out flash", the `flashburnt` overlay, the burnt in-hand sprite. The
/// hypnotic flash never burns out; the arm implant overheats 4 s and retracts.
pub(crate) fn burn_out(world: &mut World, flash: Entity) {
    let path = flash_path(world, flash);
    if is_under(&path, HYPNOTIC_FLASH) {
        return;
    }
    if is_under(&path, ARM_FLASH) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        if let Some(mut state) = world.get_mut::<Flash>(flash) {
            state.overheat_until = now + 2.0 * ARM_FLASH_CD;
        }
        let implant = world
            .query::<(Entity, &crate::medical::cyberimps::Implant)>()
            .iter(world)
            .find(|(_, i)| i.extended == Some(flash))
            .map(|(e, _)| e);
        if let Some(implant) = implant {
            if let Some(owner) = arm_flash_owner(world, flash) {
                tell_mob(
                    world,
                    owner,
                    Span::Warning,
                    "Ваш имплант фотонного проектора перегревается и отключается!".to_owned(),
                );
            }
            crate::medical::cyberimps::retract(world, implant);
        }
        return;
    }
    if world.get::<Flash>(flash).is_some_and(|f| f.burnt_out) {
        return;
    }
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
    if !try_use_flash(world, flash, Some(user), now) {
        return;
    }
    let hypnotic = is_under(&flash_path(world, flash), HYPNOTIC_FLASH);
    if flash_mob_with(world, flash, user, target, FLASH_CONFUSION, true) && !hypnotic {
        // `COMSIG_MOB_SUCCESSFUL_FLASHED_MOB`.
        crate::antag::brother::on_successful_flash(world, user, target, flash);
        if crate::antag::revolution::server::on_successful_flash(world, user, target)
            && let Some(mut state) = world.get_mut::<Flash>(flash)
        {
            state.times_used = state.times_used.saturating_sub(1);
        }
    }
}

/// tg `AOE_flash` (range 3, every mob that sees the flash but the user: `viewers`).
fn aoe_flash(world: &mut World, user: Entity, flash: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if !try_use_flash(world, flash, Some(user), now) {
        return;
    }
    let Some(&at) = world.get::<TilePos>(user) else {
        return;
    };
    let name = ss13_shared::capitalize(&name_of(world, flash));
    tell_mob(
        world,
        user,
        Span::Danger,
        format!("{name} испускает ослепляющий свет!"),
    );
    let hypnotic = is_under(&flash_path(world, flash), HYPNOTIC_FLASH);
    let targets: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(at, FLASH_AOE_RANGE)
        .filter(|&e| e != user)
        .collect();
    for target in targets {
        let seen = world.get::<TilePos>(target).is_some_and(|&pos| {
            crate::mobs::ai::behaviors::can_see(world, at, pos, FLASH_AOE_RANGE)
        });
        if world.get::<Health>(target).is_none() || !seen {
            continue;
        }
        // `generic_message`.
        let (span, text) = if hypnotic {
            (
                Span::Notice,
                format!("{name} испускает успокаивающий свет..."),
            )
        } else {
            (Span::Danger, format!("{name} испускает ослепляющий свет!"))
        };
        tell_mob(world, target, span, text);
        flash_mob_with(world, flash, user, target, FLASH_CONFUSION, false);
    }
}

/// The flash's own `flash_mob`: the hypnotic one hypnotises.
fn flash_mob_with(
    world: &mut World,
    flash: Entity,
    user: Entity,
    victim: Entity,
    confusion: f32,
    targeted: bool,
) -> bool {
    if is_under(&flash_path(world, flash), HYPNOTIC_FLASH) {
        return hypno_flash_mob(world, user, victim, confusion, targeted);
    }
    flash_mob(world, user, victim, confusion, targeted)
}

/// tg `hypnosis_vulnerable`.
fn hypnosis_vulnerable(world: &World, mob: Entity) -> bool {
    if crate::antag::revolution::server::mindshielded(world, mob) {
        return false;
    }
    let effects = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .cloned()
        .unwrap_or_default();
    world
        .get::<crate::hallucination::Hallucinating>(mob)
        .is_some()
        || effects.drugginess > 0.0
        || effects.unconscious > 0.0
        || effects.sleeping > 0.0
        || matches!(world.get::<Stat>(mob), Some(Stat::HardCrit))
        || crate::mood::sanity(world, mob) < crate::mood::SANITY_UNSTABLE
}

/// `/obj/item/assembly/flash/hypnotic/flash_mob`: no deviation; a trance for the vulnerable,
/// else confusion, dizziness, drowsiness and pacifism (less of them from the AOE).
fn hypno_flash_mob(
    world: &mut World,
    user: Entity,
    victim: Entity,
    confusion: f32,
    targeted: bool,
) -> bool {
    use crate::status_effects::{Timed, adjust_timed};
    let Some(&victim_at) = world.get::<TilePos>(victim) else {
        return false;
    };
    let (name_user, name_victim) = (name_of(world, user), name_of(world, victim));
    if !crate::vision::flash_act(
        world,
        victim,
        1,
        targeted,
        false,
        crate::vision::FLASH_LENGTH,
    ) {
        if targeted {
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
                    user,
                    Span::Warning,
                    format!("Вам не удаётся гипно-ослепить {name_victim}!"),
                )],
            );
        }
        return false;
    }
    if !targeted {
        tell_mob(
            world,
            victim,
            Span::Notice,
            "Какой красивый свет...".to_owned(),
        );
        if let Some(mut e) = effects_mut(world, victim) {
            e.adjust_confusion_up_to(confusion, confusion * 2.0 * CONFUSION_STACK_MAX_MULTIPLIER);
        }
        adjust_timed(world, victim, Timed::Dizziness, 8.0, Some(40.0));
        adjust_timed(world, victim, Timed::Drowsiness, 8.0, Some(40.0));
        adjust_timed(world, victim, Timed::Pacify, 4.0, None);
        return true;
    }
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
            user,
            Span::Danger,
            format!("Вы гипно-ослепляете {name_victim}!"),
        )],
    );
    if hypnosis_vulnerable(world, victim) {
        // `apply_status_effect(/datum/status_effect/trance, 200, TRUE)`.
        crate::curses::apply_trance(world, victim, 20.0, true);
    } else {
        tell_mob(
            world,
            victim,
            Span::Hypnophrase,
            "Свет заставляет вас чувствовать себя странно расслабленно...".to_owned(),
        );
        if let Some(mut e) = effects_mut(world, victim) {
            e.adjust_confusion_up_to(
                confusion * 2.0,
                confusion * 2.0 * CONFUSION_STACK_MAX_MULTIPLIER,
            );
        }
        adjust_timed(world, victim, Timed::Dizziness, 20.0, Some(40.0));
        adjust_timed(world, victim, Timed::Drowsiness, 20.0, Some(40.0));
        adjust_timed(world, victim, Timed::Pacify, 10.0, None);
    }
    true
}

/// `robot_model/respawn_consumable` of a cyborg in a recharging station: its flashes are
/// whole again (`times_used = 0`, not burnt out).
pub fn restock_flashes(world: &mut World, borg: Entity) {
    let model = world
        .get::<crate::silicon::cyborg::Cyborg>(borg)
        .and_then(|c| c.model_item);
    let mut query =
        world.query_filtered::<(Entity, &Flash), With<crate::silicon::cyborg::RobotModule>>();
    let worn: Vec<Entity> = query
        .iter(world)
        .filter(|(_, f)| f.burnt_out || f.times_used > 0)
        .map(|(e, _)| e)
        .collect();
    for flash in worn {
        let owned = world.get::<CarriedBy>(flash).is_some_and(|c| c.0 == borg)
            || model.is_some_and(|m| {
                world
                    .get::<crate::storage::StoredIn>(flash)
                    .is_some_and(|s| s.0 == m)
            });
        if !owned {
            continue;
        }
        let was_burnt = world.get::<Flash>(flash).is_some_and(|f| f.burnt_out);
        if let Some(mut state) = world.get_mut::<Flash>(flash) {
            state.times_used = 0;
            state.burnt_out = false;
        }
        if was_burnt {
            unburn(world, flash);
        }
    }
}

/// `update_appearance` of a flash that is no longer burnt out: its name, in-hand sprite and
/// overlay back.
fn unburn(world: &mut World, flash: Entity) {
    if let Some(mut dm_name) = world.get_mut::<DmName>(flash)
        && let Some(name) = dm_name.name.strip_prefix("burnt-out ")
    {
        dm_name.name = name.to_owned();
    }
    if let Some(mut item) = world.get_mut::<DmItem>(flash) {
        item.inhand_icon_state = "flashtool".to_owned();
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
        appearance.overlays.retain(|o| o.icon_state != id);
    }
    if let Some(holder) = world.get::<CarriedBy>(flash).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
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
