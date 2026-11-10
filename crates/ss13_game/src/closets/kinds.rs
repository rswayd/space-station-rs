//! Closet types with their own behaviour (server):
//!
//! - Large crates (`crates/large.dm`): a hand or any item only tells that a crowbar is needed
//!   (out of combat mode); a crowbar pries it open (`crowbar_act`): it falls apart into
//!   `material_drop_amount` (4) wooden planks with its contents on the tile.
//! - Abandoned crates (`crates/abandoned_crates/abandoned_crates.dm`): a [`CodeLock`] of four
//!   unique digits. A hand asks for the code (`tgui_input_text`); the right code spawns the loot
//!   once (`pick_weight_recursive(possible_loot)`) and unlocks it, a well-formed wrong code
//!   costs one of 10 attempts and records its bulls and cows, the last one blows the crate up
//!   (`boom`: contents out, `explosion(heavy 1, light 5, flash 5)`). An ID does not unlock it
//!   while the anti-tamper is armed (`tamperproof`); locking it again rearms it and resets the
//!   attempts. A multitool shows the attempts (right click: asks for the code); an emag on the
//!   locked crate blows it up.
//! - Cardboard boxes (`closets/cardboardbox.dm`): a step of the mob inside moves the closed box
//!   (`relaymove`) once per `walk_delay` (0.4 s, ×2 for the metal box), not while incapacitated
//!   or without gravity; opening a box with a mob inside makes the mobs around look ("!",
//!   `do_alert_animation`, chime), at most once a minute.
//! - Body bags (`closets/bodybag.dm`, `items/tools/medical/bodybag.dm`): the folded bag in hand
//!   unfolds onto the user's tile, open (`deploy_bodybag`); the closed bag is not dense, holds
//!   two mobs (bluespace 15, environmental ones 1) and folds back into the hands with a right
//!   click when nothing is inside (`attempt_fold`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmCloset, DmObj, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Dir, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::Mapped;

use super::{Closet, InCloset};
use crate::click::ClickResolved;
use crate::click_handlers::ClickCtx;
use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::storage::Stored;
use crate::tools::{ToolKind, tell, tool_of};

pub const LARGE_CRATE: &str = "/obj/structure/closet/crate/large";
pub const LOOT_CRATE: &str = "/obj/structure/closet/crate/secure/loot";
pub const CARDBOARD: &str = "/obj/structure/closet/cardboard";
pub const METAL_BOX: &str = "/obj/structure/closet/cardboard/metal";
pub const BODY_BAG: &str = "/obj/structure/closet/body_bag";
const BLUESPACE_BAG: &str = "/obj/structure/closet/body_bag/bluespace";
const ENVIRONMENTAL_BAG: &str = "/obj/structure/closet/body_bag/environmental";
/// Large crates' `material_drop` and `material_drop_amount`.
pub const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
const WOOD_AMOUNT: u32 = 4;
const PRY_SOUND: &str = "sound/items/weapons/slashmiss.ogg";
const CHIME: &str = "sound/machines/chime.ogg";
const ZIP: &str = "sound/items/zip/zip.ogg";
/// `/obj/item/bodybag` types and their `unfoldedbag_path`; the bag folds back into the first
/// item whose bag it is (`foldedbag_path`).
pub const BAGS: [(&str, &str); 2] = [
    ("/obj/item/bodybag/bluespace", BLUESPACE_BAG),
    ("/obj/item/bodybag", BODY_BAG),
];

/// The behaviour a closet's type adds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClosetKind {
    #[default]
    Plain,
    Large,
    Loot,
    Cardboard,
    BodyBag,
}

impl ClosetKind {
    pub fn of(path: &str) -> Self {
        if is_under(path, LARGE_CRATE) {
            Self::Large
        } else if is_under(path, LOOT_CRATE) {
            Self::Loot
        } else if is_under(path, CARDBOARD) {
            Self::Cardboard
        } else if is_under(path, BODY_BAG) {
            Self::BodyBag
        } else {
            Self::Plain
        }
    }
}

/// `mob_storage_capacity` of the types that change it.
pub fn mob_capacity(path: &str) -> usize {
    if is_under(path, METAL_BOX) {
        5
    } else if is_under(path, CARDBOARD) || is_under(path, ENVIRONMENTAL_BAG) {
        1
    } else if is_under(path, BLUESPACE_BAG) {
        15
    } else if is_under(path, BODY_BAG) {
        2
    } else {
        super::MOB_STORAGE_CAPACITY
    }
}

/// The type's behaviour onto a new [`Closet`]: its kind and capacities, a code lock, a box's
/// move delay, a sealed closet's air. Idempotent.
pub fn attach(world: &mut World, closet: Entity) {
    let Some(path) = world.get::<DmObj>(closet).map(|o| o.path.clone()) else {
        return;
    };
    let kind = ClosetKind::of(&path);
    {
        let Some(mut state) = world.get_mut::<Closet>(closet) else {
            return;
        };
        state.kind = kind;
        state.mob_capacity = mob_capacity(&path);
        // `body_bag/after_close`: `set_density(FALSE)`.
        state.dense_when_closed = kind != ClosetKind::BodyBag;
    }
    let entity = world.entity(closet);
    let (has_lock, has_box, has_air) = (
        entity.contains::<CodeLock>(),
        entity.contains::<CardboardBox>(),
        entity.contains::<super::air::ClosetAir>(),
    );
    let mapped = entity.contains::<Mapped>();
    let pos = entity.get::<TilePos>().copied();
    if kind == ClosetKind::Loot && !has_lock {
        let lock = CodeLock::new(&mut world.get_resource_or_init::<crate::combat::CombatRng>());
        world.entity_mut(closet).insert(lock);
    }
    if kind == ClosetKind::Cardboard && !has_box {
        let speed = if is_under(&path, METAL_BOX) {
            2.0
        } else if is_under(&path, crate::implants::stealth::AGENT_BOX) {
            0.5
        } else {
            1.0
        };
        world.entity_mut(closet).insert(CardboardBox {
            move_speed_multiplier: speed,
            next_move: 0.0,
            next_alert: 0.0,
        });
    }
    if super::air::sealed(&path) && !has_air {
        let air = super::air::initial_air(world, pos, mapped);
        world
            .entity_mut(closet)
            .insert(super::air::ClosetAir { air, freezer: true });
    }
}

/// Closets made after map load (cargo crates, unfolded bags) get their type's behaviour too.
pub(super) fn attach_new(world: &mut World) {
    let new: Vec<Entity> = world
        .query_filtered::<Entity, Added<Closet>>()
        .iter(world)
        .collect();
    for closet in new {
        attach(world, closet);
    }
}

fn kind(world: &World, closet: Entity) -> ClosetKind {
    world.get::<Closet>(closet).map_or_else(default, |c| c.kind)
}

fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

fn visible(world: &mut World, pos: TilePos, span: Span, text: String) {
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Local,
        (span, text),
        &[],
    );
}

/// The empty-hand clicks these types answer themselves (from `closet_clicks`); true when the
/// click was theirs.
pub(super) fn intercept_click(
    world: &mut World,
    click: &ClickResolved,
    state: &Closet,
    held: Option<Entity>,
) -> bool {
    if held.is_some() {
        return false;
    }
    match state.kind {
        // `large/attack_hand`.
        ClosetKind::Large => {
            if !click.modifiers.right {
                tell(
                    world,
                    click.client,
                    Span::Warning,
                    "Нужен лом, чтобы вскрыть это!",
                );
            }
            true
        }
        // `loot/attack_hand` of the locked crate.
        ClosetKind::Loot if state.locked && !click.modifiers.right => {
            ask_code(world, click.client, click.mob, click.target);
            true
        }
        // `body_bag/attack_hand_secondary`.
        ClosetKind::BodyBag if click.modifiers.right => {
            fold(world, click.client, click.mob, click.target);
            true
        }
        _ => false,
    }
}

/// `HeldOnTarget`: a held item on a large crate (`item_interaction`, `crowbar_act`) or a
/// multitool on a locked abandoned crate (`multitool_act`, `multitool_act_secondary`).
pub fn held_on_target(world: &mut World, c: &ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    let tool = tool_of(world, held).map(|(t, _)| t);
    match kind(world, c.target) {
        ClosetKind::Large => {
            if crate::medical::surgery::combat_mode(world, c.mob) {
                return false;
            }
            if tool == Some(ToolKind::Crowbar) {
                pry(world, c.mob, c.target);
            } else {
                tell(
                    world,
                    c.client,
                    Span::Warning,
                    "Нужен лом, чтобы вскрыть это!",
                );
            }
            true
        }
        ClosetKind::Loot if tool == Some(ToolKind::Multitool) => {
            if !world.get::<Closet>(c.target).is_some_and(|s| s.locked) {
                return false;
            }
            if c.modifiers.right {
                ask_code(world, c.client, c.mob, c.target);
            } else {
                show_attempts(world, c.client, c.target);
            }
            true
        }
        _ => false,
    }
}

/// `large/crowbar_act`: opens, falls apart into planks, contents on the tile.
pub fn pry(world: &mut World, user: Entity, crate_: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(crate_) else {
        return false;
    };
    if !super::open_if_allowed(world, crate_, Some(user)) {
        return false;
    }
    let (who, what) = (name(world, user), name(world, crate_));
    visible(world, pos, Span::Notice, format!("{who} вскрывает {what}."));
    playsound(world, PlaySound::new(pos, PRY_SOUND, 75.0, true));
    crate::construction::spawn_stack(world, WOOD, pos, WOOD_AMOUNT);
    world.despawn(crate_);
    true
}

/// One guess of an abandoned crate: `attempt`, its bulls and cows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub guess: String,
    pub bulls: u8,
    pub cows: u8,
}

/// `/obj/structure/closet/crate/secure/loot` state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CodeLock {
    pub code: String,
    pub attempts: u32,
    pub previous: Vec<Attempt>,
    /// Percent; nonzero: no unlocking with an ID.
    pub tamperproof: f32,
    pub spawned_loot: bool,
}

/// `attempts`, `code_length`, `tamperproof`.
pub const ATTEMPTS: u32 = 10;
pub const CODE_LENGTH: usize = 4;
const TAMPERPROOF: f32 = 90.0;

impl CodeLock {
    /// `generate_code(code_length)`: unique digits.
    pub fn new(rng: &mut crate::combat::CombatRng) -> Self {
        let mut digits: Vec<char> = "1234567890".chars().collect();
        let mut code = String::new();
        for _ in 0..CODE_LENGTH {
            let i = rng.range(0, digits.len() as u32 - 1) as usize;
            code.push(digits.remove(i));
        }
        Self::with_code(code)
    }

    pub fn with_code(code: String) -> Self {
        Self {
            code,
            attempts: ATTEMPTS,
            previous: Vec::new(),
            tamperproof: TAMPERPROOF,
            spawned_loot: false,
        }
    }
}

/// `validate_input`: `code_length` digits, none repeated.
pub fn valid_guess(input: &str) -> bool {
    let chars: Vec<char> = input.chars().collect();
    chars.len() == CODE_LENGTH
        && chars.iter().all(char::is_ascii_digit)
        && chars
            .iter()
            .enumerate()
            .all(|(i, c)| !chars[..i].contains(c))
}

/// `bulls_and_cows(guess)`: right digits in the right place, right digits elsewhere.
pub fn bulls_and_cows(code: &str, guess: &str) -> (u8, u8) {
    let code: Vec<char> = code.chars().collect();
    let (mut bulls, mut cows) = (0, 0);
    for (i, g) in guess.chars().enumerate() {
        if code.get(i) == Some(&g) {
            bulls += 1;
        } else if code.contains(&g) {
            cows += 1;
        }
    }
    (bulls, cows)
}

/// On the prompt entity of a code question.
#[derive(Component, Clone, Copy, Debug)]
struct CodePrompt {
    crate_: Entity,
}

fn ask_code(world: &mut World, client: Entity, mob: Entity, crate_: Entity) {
    let ask = Ask::new(
        client,
        mob,
        "Deca-code lock",
        format!("Введите {CODE_LENGTH} цифры. Все цифры должны быть разными."),
        PromptKind::Text {
            default: String::new(),
            max_length: CODE_LENGTH as u32,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(CodePrompt { crate_ });
}

/// The answers to code questions, revalidated: the body is the client's, conscious and next to
/// the crate, which is still locked.
pub(super) fn code_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(CodePrompt { crate_ }) = world.get::<CodePrompt>(prompt).copied() else {
            continue;
        };
        let (Some(&at), Some(&pos)) = (world.get::<TilePos>(actor), world.get::<TilePos>(crate_))
        else {
            continue;
        };
        let near = at.z == pos.z && (at.x - pos.x).abs() <= 1 && (at.y - pos.y).abs() <= 1;
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
            || !near
        {
            continue;
        }
        let input = match answer {
            Some(PromptAnswer::Text(text)) => Some(text),
            _ => None,
        };
        enter_code(world, Some(actor), crate_, input.as_deref());
    }
}

/// What a code entry did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeResult {
    /// Not a locked abandoned crate (any more).
    Nothing,
    Opened,
    LeftAlone,
    Wrong,
    Boom,
}

/// `loot/attack_hand` after the input: `input` against the code.
pub fn enter_code(
    world: &mut World,
    user: Option<Entity>,
    crate_: Entity,
    input: Option<&str>,
) -> CodeResult {
    let locked = world.get::<Closet>(crate_).is_some_and(|c| c.locked);
    let Some(lock) = world.get::<CodeLock>(crate_).cloned().filter(|_| locked) else {
        return CodeResult::Nothing;
    };
    let say = |world: &mut World, span: Span, text: &str| {
        super::tell_mob(world, user, span, text.to_owned());
    };
    if input == Some(lock.code.as_str()) {
        if !lock.spawned_loot {
            spawn_loot(world, crate_);
        }
        if let Some(mut lock) = world.get_mut::<CodeLock>(crate_) {
            lock.spawned_loot = true;
            // No accidental boom from a multitool afterwards.
            lock.tamperproof = 0.0;
        }
        super::toggle_lock(world, user, crate_, None);
        return CodeResult::Opened;
    }
    let Some(guess) = input.filter(|i| valid_guess(i)) else {
        say(world, Span::Notice, "Вы оставляете ящик в покое.");
        return CodeResult::LeftAlone;
    };
    say(world, Span::Warning, "Мигает красный огонёк.");
    let (bulls, cows) = bulls_and_cows(&lock.code, guess);
    let left = {
        let mut lock = world.get_mut::<CodeLock>(crate_).expect("cloned above");
        lock.previous.push(Attempt {
            guess: guess.to_owned(),
            bulls,
            cows,
        });
        lock.attempts = lock.attempts.saturating_sub(1);
        lock.attempts
    };
    if left == 0 {
        boom(world, user, crate_);
        return CodeResult::Boom;
    }
    CodeResult::Wrong
}

/// The `AbandonedCrate` window's data (`previous_attempts`, `attempts_left`) in chat.
// shortcut: chat lines instead of the `AbandonedCrate` tgui window; port the window with the
// tgui interfaces of the crates.
fn show_attempts(world: &mut World, client: Entity, crate_: Entity) {
    let Some(lock) = world.get::<CodeLock>(crate_).cloned() else {
        return;
    };
    let mut text = format!("Осталось попыток: {}.", lock.attempts);
    for a in &lock.previous {
        text.push_str(&format!(
            "\n{}: быков {}, коров {}",
            a.guess, a.bulls, a.cows
        ));
    }
    tell(world, client, Span::Notice, text);
}

/// `loot/togglelock`: the armed anti-tamper keeps an ID from unlocking the crate.
pub fn lock_blocked(world: &World, closet: Entity) -> bool {
    world.get::<Closet>(closet).is_some_and(|c| c.locked)
        && world
            .get::<CodeLock>(closet)
            .is_some_and(|l| l.tamperproof > 0.0)
}

/// `loot/togglelock` after locking: the anti-tamper, attempts and their record reset.
pub fn relocked(world: &mut World, closet: Entity) {
    if let Some(mut lock) = world.get_mut::<CodeLock>(closet) {
        lock.tamperproof = TAMPERPROOF;
        lock.attempts = ATTEMPTS;
        lock.previous.clear();
    }
}

/// `loot/emag_act` on the locked crate: it blows up.
// Deviation: tg runs the parent `emag_act` first, which unlocks the crate, so its `if(locked)`
// never fires and an emag opens it for free; the intended anti-tamper explosion is ported.
pub fn emag(world: &mut World, user: Entity, closet: Entity) -> bool {
    let locked = world.get::<Closet>(closet).is_some_and(|c| c.locked);
    if !locked || !world.entity(closet).contains::<CodeLock>() {
        return false;
    }
    boom(world, Some(user), closet);
    true
}

/// `crate/secure/boom`: contents out, an explosion, the crate gone.
pub fn boom(world: &mut World, user: Option<Entity>, crate_: Entity) {
    let Some(&pos) = world.get::<TilePos>(crate_) else {
        return;
    };
    super::tell_mob(
        world,
        user,
        Span::Danger,
        "Срабатывает система защиты ящика от взлома!",
    );
    if let Some(user) = user {
        let who = name(world, user);
        info!("bomb: {who} detonated an abandoned crate at {pos:?}");
    }
    super::open(world, crate_);
    let mut ex = crate::explosions::Explosion::new(0, 1, 5);
    ex.flash = Some(5.0);
    ex.cause = "abandoned crate".into();
    crate::explosions::explosion(world, pos, ex);
    if world.get_entity(crate_).is_ok() {
        world.despawn(crate_);
    }
}

type Spawns = &'static [(&'static str, u32)];

/// `possible_loot` with the `/obj/effect/spawner/abandoned_crate` spawners expanded into what
/// they spawn: groups by weight, then items by weight.
// shortcut: the random toy mech is the Ripley, the weed spawner always gives 3 seeds and 3
// buds of the base strain, the gibtonite is not live, the bwoink mines and the "crate with a
// crate" are left out, and mobs spawn only where a mob prototype can be built (the mimic does
// not delete its crate); add with random subtypes and live gibtonite.
pub const POSSIBLE_LOOT: &[(u32, &[(u32, Spawns)])] = &[
    (
        34,
        &[
            (5, &[("/obj/item/melee/skateboard/pro", 1)]),
            (5, &[("/obj/item/toy/snappop/phoenix", 5)]),
            (5, &[("/obj/item/poster/random_contraband", 5)]),
            (2, &[("/obj/item/toy/balloon/corgi", 1)]),
            (2, &[("/obj/item/toy/mecha/ripley", 1)]),
            (2, &[("/obj/item/toy/balloon/syndicate", 1)]),
            (2, &[("/obj/item/toy/plush/snakeplushie", 1)]),
            (2, &[("/obj/item/bikehorn/airhorn", 1)]),
            (2, &[("/obj/item/toy/plush/beeplushie", 1)]),
            (2, &[("/obj/item/toy/plush/lizard_plushie", 1)]),
            (1, &[("/obj/item/toy/plush/nukeplushie", 1)]),
            (1, &[("/obj/item/banhammer", 1)]),
            (
                1,
                &[
                    ("/obj/item/clothing/mask/balaclava", 4),
                    ("/obj/item/gun/ballistic/shotgun/toy", 1),
                    ("/obj/item/gun/ballistic/automatic/pistol/toy", 1),
                    ("/obj/item/gun/ballistic/automatic/toy", 1),
                    (
                        "/obj/item/gun/ballistic/automatic/l6_saw/toy/unrestricted",
                        1,
                    ),
                    ("/obj/item/ammo_box/foambox", 1),
                ],
            ),
            (1, &[("/obj/item/melee/skateboard/hoverboard", 1)]),
            (1, &[("/obj/item/implanter/sad_trombone", 1)]),
        ],
    ),
    (
        5,
        &[
            (2, &[("/obj/item/melee/baton", 1)]),
            (1, &[("/obj/item/clothing/gloves/boxing/evil", 1)]),
            (1, &[("/obj/item/melee/energy/sword/bananium", 1)]),
            (1, &[("/obj/item/storage/belt/sheath/katana", 1)]),
        ],
    ),
    (
        13,
        &[
            (
                5,
                &[("/obj/item/storage/box/syndie_kit/chameleon/broken", 1)],
            ),
            (2, &[("/obj/item/clothing/shoes/kindle_kicks", 1)]),
            (
                2,
                &[
                    ("/obj/item/clothing/head/costume/kitty", 5),
                    ("/obj/item/clothing/neck/petcollar", 5),
                ],
            ),
            (
                2,
                &[
                    ("/obj/item/borg/upgrade/modkit/cooldown/aoe/mobs", 1),
                    ("/obj/item/clothing/suit/space", 1),
                    ("/obj/item/clothing/head/helmet/space", 1),
                ],
            ),
            (
                2,
                &[
                    (
                        "/obj/item/clothing/suit/costume/wellworn_shirt/graphic/ian",
                        1,
                    ),
                    ("/obj/item/clothing/suit/hooded/ian_costume", 1),
                ],
            ),
        ],
    ),
    (
        7,
        &[
            (5, &[("/obj/item/modular_computer/pda/clear", 1)]),
            (2, &[("/obj/item/defibrillator/compact", 1)]),
        ],
    ),
    (
        9,
        &[
            (5, &[("/obj/item/stack/ore/diamond", 10)]),
            (2, &[("/obj/item/stack/ore/bluespace_crystal", 5)]),
            (2, &[("/obj/item/stack/sheet/mineral/bananium", 10)]),
        ],
    ),
    (
        6,
        &[
            (5, &[("/obj/item/seeds/firelemon", 1)]),
            (
                1,
                &[
                    ("/obj/item/seeds/cannabis", 3),
                    ("/obj/item/food/grown/cannabis", 3),
                ],
            ),
        ],
    ),
    (
        11,
        &[
            (
                5,
                &[
                    ("/obj/item/reagent_containers/cup/glass/bottle/rum", 1),
                    ("/obj/item/reagent_containers/cup/glass/bottle/whiskey", 2),
                    ("/obj/item/lighter", 1),
                    (
                        "/obj/item/reagent_containers/cup/glass/bottle/absinthe/premium",
                        1,
                    ),
                    ("/obj/item/cigarette/rollie", 3),
                ],
            ),
            (
                5,
                &[
                    ("/obj/item/vending_refill/sovietsoda", 1),
                    ("/obj/item/circuitboard/machine/vendor", 1),
                ],
            ),
            (
                1,
                &[(
                    "/obj/item/reagent_containers/cup/glass/bottle/lizardwine",
                    1,
                )],
            ),
        ],
    ),
    (
        8,
        &[
            (5, &[("/mob/living/basic/bot/secbot/honkbot", 1)]),
            (2, &[("/mob/living/basic/pet/gondola", 1)]),
            (1, &[("/mob/living/basic/cockroach/bloodroach", 30)]),
        ],
    ),
    (
        2,
        &[
            (1, &[("/obj/item/dnainjector/xraymut", 1)]),
            (1, &[("/obj/item/dnainjector/wackymut", 1)]),
        ],
    ),
    (
        4,
        &[
            (2, &[("/obj/item/gibtonite", 1)]),
            (1, &[("/mob/living/basic/mimic/crate", 1)]),
            (1, &[("/mob/living/basic/bee/toxin", 3)]),
        ],
    ),
];

/// Every constant path the loot table spawns (for the runtime prototype list).
pub fn loot_paths() -> impl Iterator<Item = &'static str> {
    POSSIBLE_LOOT
        .iter()
        .flat_map(|(_, group)| group.iter())
        .flat_map(|(_, spawns)| spawns.iter().map(|(p, _)| *p))
}

fn pick_weighted<T: Copy>(rng: &mut crate::combat::CombatRng, list: &[(u32, T)]) -> T {
    let total: u32 = list.iter().map(|(w, _)| w).sum();
    let mut roll = rng.range(1, total.max(1));
    for &(w, item) in list {
        if roll <= w {
            return item;
        }
        roll -= w;
    }
    list[list.len() - 1].1
}

/// `pick_weight_recursive(possible_loot)`: what to spawn.
pub fn roll_loot(rng: &mut crate::combat::CombatRng) -> Spawns {
    let group = pick_weighted(rng, POSSIBLE_LOOT);
    pick_weighted(rng, group)
}

/// `spawn_loot`: the rolled loot inside the crate.
fn spawn_loot(world: &mut World, crate_: Entity) {
    let spawns = roll_loot(&mut world.get_resource_or_init::<crate::combat::CombatRng>());
    for &(path, count) in spawns {
        if is_under(path, "/obj/item/stack") {
            if let Some(stack) = crate::construction::spawn_detached(world, path) {
                world
                    .entity_mut(stack)
                    .insert(crate::tools::Stack { amount: count });
                crate::storage::put_inside(world, stack, crate_);
            }
            continue;
        }
        for _ in 0..count {
            if let Some(item) = crate::construction::spawn_detached(world, path) {
                crate::storage::put_inside(world, item, crate_);
            }
        }
    }
}

/// `/obj/structure/closet/cardboard` move and alert timers.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CardboardBox {
    pub move_speed_multiplier: f32,
    /// `move_delay` until this game time.
    pub next_move: f64,
    /// `alert_cooldown`.
    pub next_alert: f64,
}

/// `walk_delay` (`config/game_options.txt` `WALK_DELAY 4`), seconds.
pub const WALK_DELAY_SECS: f64 = 0.4;
/// `time_between_alerts`.
const ALERT_COOLDOWN_SECS: f64 = 60.0;
/// `viewers(7, src)`.
const ALERT_RANGE: i32 = 7;

/// `cardboard/relaymove`: a step of `mob` inside moves the box. True when the closet is a box
/// (the step is its, moved or not).
pub(super) fn cardboard_step(world: &mut World, mob: Entity, closet: Entity, dir: Dir) -> bool {
    let Some(state) = world.get::<CardboardBox>(closet).copied() else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let opened = world.get::<Closet>(closet).is_none_or(|c| c.opened);
    let incapacitated = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|e| e.incapacitated());
    let Some(&from) = world.get::<TilePos>(closet) else {
        return true;
    };
    if opened
        || now < state.next_move
        || incapacitated
        || world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || !crate::multiz::has_gravity(world, from)
    {
        return true;
    }
    if let Err(blockers) = crate::pull::step_once(world, closet, dir, None) {
        // `agent/Bump`: a living mob in the way shows the agent box.
        crate::implants::stealth::bumped_into(world, closet, &blockers);
        return true;
    }
    let Some(&to) = world.get::<TilePos>(closet) else {
        return true;
    };
    // The mobs inside stay on the box's tile.
    let inside: Vec<Entity> = world
        .query::<(Entity, &InCloset)>()
        .iter(world)
        .filter(|(_, c)| c.closet == closet)
        .map(|(e, _)| e)
        .collect();
    for m in inside {
        world.entity_mut(m).insert(to);
    }
    if let Some(mut b) = world.get_mut::<CardboardBox>(closet) {
        b.next_move = now + WALK_DELAY_SECS * f64::from(state.move_speed_multiplier);
    }
    true
}

/// `cardboard/before_open` + `after_open`: a box that held a living mob makes the conscious
/// mobs within 7 tiles look ("!" over each), once per `time_between_alerts`.
// shortcut: no line of sight or blindness check, the watchers do not turn to the box, the "!"
// is a temporary visual on their tile rather than a flick overlay that follows them.
pub(super) fn cardboard_opened(world: &mut World, closet: Entity, had_mob: bool) {
    let Some(state) = world.get::<CardboardBox>(closet).copied() else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(&pos) = world.get::<TilePos>(closet) else {
        return;
    };
    if !had_mob || now < state.next_alert {
        return;
    }
    let watchers: Vec<TilePos> = world
        .query::<(&TilePos, &Stat, &crate::health::Health)>()
        .iter(world)
        .filter(|(p, s, _)| {
            p.z == pos.z
                && (p.x - pos.x).abs() <= ALERT_RANGE
                && (p.y - pos.y).abs() <= ALERT_RANGE
                && **s == Stat::Conscious
        })
        .map(|(p, ..)| *p)
        .collect();
    if watchers.is_empty() {
        return;
    }
    if let Some(mut b) = world.get_mut::<CardboardBox>(closet) {
        b.next_alert = now + ALERT_COOLDOWN_SECS;
    }
    for at in watchers {
        let mut v = crate::effects::temp_visual::Visual::new(
            "icons/obj/storage/closet.dmi",
            "cardboard_special",
            1.0,
        );
        v.plane = ss13_shared::appearance::ABOVE_LIGHTING_PLANE;
        v.offset = [0, 32];
        v.dir = Some(Dir::South);
        crate::effects::temp_visual::spawn(world, at, v);
    }
    playsound(
        world,
        PlaySound::new(pos, CHIME, 50.0, false).extrarange(-5.0),
    );
}

/// A closet entity from its prototype (a bag unfolded at run time).
pub fn make_closet(world: &mut World, entity: Entity) -> bool {
    let closet = {
        let icons = world.get_resource::<IconIndex>();
        let e = world.entity(entity);
        match (e.get::<DmCloset>(), e.get::<DmAtom>()) {
            (Some(dm), Some(atom)) => Some(Closet::from_dm(dm, atom, icons)),
            _ => None,
        }
    };
    let Some(mut closet) = closet else {
        return false;
    };
    closet.opened = false;
    world.entity_mut(entity).insert(closet);
    attach(world, entity);
    super::refresh_look(world, entity);
    true
}

/// `/obj/item/bodybag/attack_self` -> `deploy_bodybag` onto the user's tile, open.
pub fn bodybag_attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(item)
        .map(|i| i.path.clone())
    else {
        return false;
    };
    let Some(&(_, bag_path)) = BAGS.iter().find(|(p, _)| is_under(&path, p)) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return true;
    };
    let _ = deploy(world, mob, item, bag_path, pos);
    true
}

/// `deploy_bodybag`: the bag structure on `pos`, open; the folded item is used up.
// shortcut: the item is deleted and folding makes a new one (tg keeps `foldedbag_instance`),
// the same for every bag ported here.
pub fn deploy(
    world: &mut World,
    mob: Entity,
    item: Entity,
    bag_path: &str,
    pos: TilePos,
) -> Option<Entity> {
    let bag = crate::construction::spawn_obj(world, bag_path, pos)?;
    if !make_closet(world, bag) {
        world.despawn(bag);
        return None;
    }
    super::open(world, bag);
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.slot_of(item)
    {
        *inventory.slot_mut(slot) = None;
    }
    world.despawn(item);
    Some(bag)
}

/// `attempt_fold` + `perform_fold`: a closed bag with nothing inside folds into the hands.
pub fn fold(world: &mut World, client: Entity, mob: Entity, bag: Entity) -> bool {
    let Some(state) = world.get::<Closet>(bag).cloned() else {
        return false;
    };
    if world.get::<Inventory>(mob).is_none() {
        return false;
    }
    let what = name(world, bag);
    if state.opened {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы возитесь с {what}, но расстёгнутый мешок не сворачивается."),
        );
        return false;
    }
    let mobs_inside = world
        .query::<&InCloset>()
        .iter(world)
        .any(|c| c.closet == bag);
    let items_inside = world
        .get::<Stored>(bag)
        .is_some_and(|s| !s.items().is_empty());
    if mobs_inside || items_inside {
        tell(
            world,
            client,
            Span::Warning,
            format!("В {what} слишком много всего, чтобы его свернуть!"),
        );
        return false;
    }
    let Some(path) = world.get::<DmObj>(bag).map(|o| o.path.clone()) else {
        return false;
    };
    let Some(&(item_path, _)) = BAGS.iter().find(|(_, b)| *b == path) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(bag) else {
        return false;
    };
    let Some(item) = crate::construction::spawn_detached(world, item_path) else {
        return false;
    };
    let who = name(world, mob);
    visible(
        world,
        pos,
        Span::Notice,
        format!("{who} сворачивает {what}."),
    );
    world.despawn(bag);
    world.entity_mut(item).insert(ss13_net::server::Networked);
    let inventory = world.get::<Inventory>(mob).cloned().expect("checked above");
    crate::storage::put_in_hands(world, mob, &inventory, item);
    playsound(world, PlaySound::new(pos, ZIP, 15.0, true));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_their_paths() {
        assert_eq!(
            ClosetKind::of("/obj/structure/closet/crate/large/hats"),
            ClosetKind::Large
        );
        assert_eq!(ClosetKind::of(LOOT_CRATE), ClosetKind::Loot);
        assert_eq!(ClosetKind::of(METAL_BOX), ClosetKind::Cardboard);
        assert_eq!(ClosetKind::of(BLUESPACE_BAG), ClosetKind::BodyBag);
        assert_eq!(
            ClosetKind::of("/obj/structure/closet/crate"),
            ClosetKind::Plain
        );
        assert_eq!(mob_capacity(CARDBOARD), 1);
        assert_eq!(mob_capacity(METAL_BOX), 5);
        assert_eq!(mob_capacity(BODY_BAG), 2);
        assert_eq!(mob_capacity(BLUESPACE_BAG), 15);
    }

    #[test]
    fn codes_are_unique_digits_and_scored_as_bulls_and_cows() {
        let mut rng = crate::combat::CombatRng::default();
        for _ in 0..20 {
            let lock = CodeLock::new(&mut rng);
            assert!(valid_guess(&lock.code), "{}", lock.code);
        }
        assert!(!valid_guess("1123") && !valid_guess("12a4") && !valid_guess("123"));
        assert_eq!(bulls_and_cows("1234", "1243"), (2, 2));
        assert_eq!(bulls_and_cows("1234", "5678"), (0, 0));
        assert_eq!(bulls_and_cows("1234", "1234"), (4, 0));
        // Every rolled loot spawns something.
        for _ in 0..50 {
            assert!(!roll_loot(&mut rng).is_empty());
        }
    }

    #[test]
    fn wrong_codes_cost_attempts_and_the_right_one_unlocks() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let dm = DmCloset {
            secure: true,
            locked: true,
            is_crate: true,
            ..default()
        };
        let c = world
            .spawn((
                Closet::from_dm(&dm, &DmAtom::default(), None),
                CodeLock::with_code("1234".into()),
            ))
            .id();
        assert_eq!(
            enter_code(&mut world, None, c, Some("1123")),
            CodeResult::LeftAlone
        );
        assert_eq!(enter_code(&mut world, None, c, None), CodeResult::LeftAlone);
        assert_eq!(
            enter_code(&mut world, None, c, Some("4321")),
            CodeResult::Wrong
        );
        let lock = world.get::<CodeLock>(c).unwrap();
        assert_eq!(lock.attempts, ATTEMPTS - 1);
        assert_eq!(
            lock.previous,
            vec![Attempt {
                guess: "4321".into(),
                bulls: 0,
                cows: 4
            }]
        );
        // An ID cannot unlock it while the anti-tamper is armed.
        assert!(lock_blocked(&world, c));
        assert_eq!(
            enter_code(&mut world, None, c, Some("1234")),
            CodeResult::Opened
        );
        assert!(!world.get::<Closet>(c).unwrap().locked);
        assert!(world.get::<CodeLock>(c).unwrap().spawned_loot);
        // Locked again: rearmed with all its attempts.
        world.get_mut::<Closet>(c).unwrap().locked = true;
        relocked(&mut world, c);
        let lock = world.get::<CodeLock>(c).unwrap();
        assert!(lock.previous.is_empty() && lock.attempts == ATTEMPTS && lock.tamperproof > 0.0);
    }
}
