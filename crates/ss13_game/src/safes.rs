//! Mechanical safes (server), after tg `code/game/objects/structures/safe.dm`, with tgui's
//! `Safe` window (`crate::safe_ui`).
//!
//! A safe ([`Safe`]) has two random tumblers (0..=99) and starts closed and locked (the `/open`
//! subtypes open), taking in the items on its tile up to 24 w_class. An empty hand (or a
//! stethoscope) opens the window: the dial turns left or right by 1, 10 or 50 (anything else is
//! refused); the first tumbler is hit turning right, the second turning left, a wrong turn or
//! one past the last resets them; past the last it unlocks ("Пружина!"), any other turn locks it
//! again. A stethoscope in hand hears each tick (always on a single tick, 10% otherwise). An
//! unlocked (or broken) safe opens and closes; open, it takes items while they fit and hands
//! them back from the window. Three devastating explosions (or heavy ones aimed at it) break the
//! lock for good. The vault safe starts with the vault piggy bank.
// A wrench on an open safe asks a number per tumbler, then resets the lock in 10 s.
// shortcut: no climbing or elevation, the floor safe is not hidden under tiles; a heavy
// explosion aimed at the safe does not count (`target == src` is unknown to `ex_act_movable`). Add each with its mechanic.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmObj, DmWeight};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::medical::{name_of, visible};
use crate::obj_damage::{is_under, set_icon_state};
use crate::safe_ui::*;
use crate::storage::Stored;
use crate::tools::tell;

pub const SAFE: &str = "/obj/structure/safe";
const STETHOSCOPE: &str = "/obj/item/clothing/neck/stethoscope";
/// `/obj/item/piggy_bank/vault` of the vault safe.
const VAULT_PIGGY: &str = "/obj/item/piggy_bank/vault";
/// `maxspace`.
const MAX_SPACE: u32 = 24;
/// `BROKEN_THRESHOLD`.
const BROKEN_THRESHOLD: u8 = 3;
/// `SOUND_CHANCE`.
const SOUND_CHANCE: f32 = 10.0;

/// `/obj/structure/safe`: `dial`, `tumblers`, `current_tumbler_index` (0-based here), `open`,
/// `locked`, `explosion_count`, and the window's contents list.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Safe {
    pub dial: u8,
    pub tumblers: Vec<u8>,
    pub index: usize,
    pub open: bool,
    pub locked: bool,
    pub explosions: u8,
    pub contents: Vec<SafeItem>,
}

/// What a single dial tick did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// A tumbler fell into place.
    Hit,
    Miss,
}

impl Safe {
    pub fn broken(&self) -> bool {
        self.explosions >= BROKEN_THRESHOLD
    }

    /// One tick of `turnright` (`left == false`) or `turnleft`: the dial moves, a wrong-way turn
    /// resets the tumblers, a hit advances them.
    pub fn tick(&mut self, left: bool) -> Tick {
        self.dial = if left {
            (self.dial + 1) % 100
        } else {
            (self.dial + 99) % 100
        };
        // 1-based `current_tumbler_index`: odd ones turn right, even ones left.
        let wrong_way = (self.index % 2 == 1) != left;
        let invalid = wrong_way || self.index >= self.tumblers.len();
        if invalid {
            self.index = 0;
        }
        if !invalid && self.tumblers.get(self.index) == Some(&self.dial) {
            self.index += 1;
            Tick::Hit
        } else {
            Tick::Miss
        }
    }

    /// `check_unlocked`: true when it just unlocked ("Spring!").
    pub fn check_unlocked(&mut self) -> bool {
        if self.broken() {
            return false;
        }
        let unlocked = self.index >= self.tumblers.len();
        let sprang = unlocked && self.locked;
        self.locked = !unlocked;
        sprang
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafeAct {
    Open,
    Turn {
        left: bool,
        ticks: u32,
    },
    /// 1-based, as tgui sends it.
    Retrieve(usize),
}

impl ServerUi for SafeUi {
    type Source = &'static Safe;
    type UserParam = ();
    type Act = SafeAct;

    fn static_data(_: &Safe) {}

    fn data(s: &Safe, _: Entity, _: &()) -> SafeData {
        SafeData {
            dial: s.dial,
            open: s.open,
            locked: s.locked,
            broken: s.broken(),
            contents: if s.open {
                s.contents.clone()
            } else {
                Vec::new()
            },
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SafeAct, UiReject> {
        match action {
            OPEN => Ok(SafeAct::Open),
            TURN_LEFT | TURN_RIGHT => {
                let ticks = params.number("num")?;
                // The `IS_FINITE` and 1/10/50 check (tg logs anything else as an href attack).
                let ticks = TICKS
                    .into_iter()
                    .find(|&t| f64::from(t) == ticks)
                    .ok_or_else(|| UiReject::BadParams("num".to_owned()))?;
                Ok(SafeAct::Turn {
                    left: action == TURN_LEFT,
                    ticks,
                })
            }
            RETRIEVE => {
                let index = params.number("index")?;
                if index < 1.0 || index.fract() != 0.0 {
                    return Err(UiReject::BadParams("index".to_owned()));
                }
                Ok(SafeAct::Retrieve(index as usize))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub struct SafesPlugin;

impl Plugin for SafesPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<SafeUi>()
            .add_observer(add_safe)
            .add_systems(ss13_world::MapInit, fill_mapped_safes)
            .add_systems(Update, safe_acts.in_set(UiSystems::Handle))
            .add_systems(Update, reset_answers.after(crate::click::ResolveClicks));
        app.add_click_handler(ClickPhase::EmptyHand, 3169, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3429, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
    }
}

fn add_safe(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&dm.path, SAFE) {
        return;
    }
    let open = dm.path.ends_with("/open");
    let entity = add.entity;
    commands.queue(move |world: &mut World| {
        let tumblers = {
            let mut rng = world.resource_mut::<CombatRng>();
            vec![rng.range(0, 99) as u8, rng.range(0, 99) as u8]
        };
        world.entity_mut(entity).insert(Safe {
            tumblers,
            open,
            locked: !open,
            ..Safe::default()
        });
        refresh(world, entity);
    });
}

/// `safe/Initialize(mapload)`: the items on its tile go in while they fit; the vault safe's
/// piggy bank.
fn fill_mapped_safes(world: &mut World) {
    let safes: Vec<(Entity, String, TilePos)> = world
        .query_filtered::<(Entity, &DmObj, &TilePos), (With<Safe>, With<ss13_world::Mapped>)>()
        .iter(world)
        .map(|(e, dm, &pos)| (e, dm.path.clone(), pos))
        .collect();
    for (safe, path, pos) in safes {
        if world.get::<Safe>(safe).is_some_and(|s| s.open) {
            continue;
        }
        if is_under(&path, "/obj/structure/safe/vault")
            && let Some(piggy) = crate::construction::spawn_detached(world, VAULT_PIGGY)
        {
            crate::storage::put_inside(world, piggy, safe);
        }
        let items: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<DmItem>(e).is_some())
            .collect();
        for item in items {
            if space(world, safe) + w_class(world, item) <= MAX_SPACE {
                crate::storage::put_inside(world, item, safe);
            }
        }
        refresh(world, safe);
    }
}

fn w_class(world: &World, item: Entity) -> u32 {
    u32::from(
        world
            .get::<DmWeight>(item)
            .copied()
            .unwrap_or_default()
            .w_class,
    )
}

fn items(world: &World, safe: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(safe)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// `space`: the w_class of everything inside.
fn space(world: &World, safe: Entity) -> u32 {
    items(world, safe)
        .into_iter()
        .map(|i| w_class(world, i))
        .sum()
}

/// `update_icon_state` (`safe` / `safe_locked`, `floorsafe`...) and the window's list.
fn refresh(world: &mut World, safe: Entity) {
    let contents: Vec<SafeItem> = items(world, safe)
        .into_iter()
        .map(|i| SafeItem {
            name: name_of(world, i),
            sprite: world
                .get::<DmAtom>(i)
                .map(|a| a.icon_state.clone())
                .unwrap_or_default(),
        })
        .collect();
    let floor = world
        .get::<DmObj>(safe)
        .is_some_and(|o| is_under(&o.path, "/obj/structure/safe/floor"));
    let Some(mut s) = world.get_mut::<Safe>(safe) else {
        return;
    };
    if s.contents != contents {
        s.contents = contents;
    }
    let base = if floor { "floorsafe" } else { "safe" };
    let state = if s.open {
        base.to_owned()
    } else {
        format!("{base}_locked")
    };
    set_icon_state(world, safe, &state);
}

fn holds_stethoscope(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::inventory::Inventory>(mob)
        .is_some_and(|inv| {
            inv.hands.iter().flatten().any(|&h| {
                world
                    .get::<DmItem>(h)
                    .is_some_and(|i| is_under(&i.path, STETHOSCOPE))
            })
        })
}

fn open_window(world: &mut World, client: Entity, mob: Entity, safe: Entity) {
    let title = name_of(world, safe);
    world.write_message(OpenUi::new::<SafeUi>(client, mob, safe, title));
}

fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Safe>(target).is_none() {
        return false;
    }
    open_window(world, client, mob, target);
    true
}

/// `safe/item_interaction`: a stethoscope on a closed one opens the window, an open one takes
/// what fits.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(open) = world.get::<Safe>(target).map(|s| s.open) else {
        return false;
    };
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    // `safe/wrench_act`: a new combination, one number per tumbler, then 10 s.
    if matches!(
        crate::tools::tool_of(world, held),
        Some((crate::tools::ToolKind::Wrench, _))
    ) {
        if !open {
            tell(world, client, Span::Warning, "сначала откройте!");
        } else {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете сбрасывать замок. Нужно задать 2 числа.",
            );
            ask_tumbler(world, client, mob, target, held, Vec::new());
        }
        return true;
    }
    let name = name_of(world, held);
    if !open {
        let stethoscope = world
            .get::<DmItem>(held)
            .is_some_and(|i| is_under(&i.path, STETHOSCOPE));
        if stethoscope {
            open_window(world, client, mob, target);
        } else {
            tell(
                world,
                client,
                Span::Warning,
                format!("Нельзя положить {name} в закрытый сейф!"),
            );
        }
        return true;
    }
    if space(world, target) + w_class(world, held) > MAX_SPACE {
        let safe = name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} не помещается в {safe}."),
        );
        return true;
    }
    crate::storage::put_inside(world, held, target);
    let safe = name_of(world, target);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы кладёте {name} в {safe}."),
    );
    refresh(world, target);
    true
}

/// A pending tumbler question of a wrench reset.
#[derive(Component, Clone, Debug)]
struct ResetAsk {
    safe: Entity,
    tool: Entity,
    got: Vec<u8>,
}

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafeJob {
    /// The wrench reset's `do_after(10 SECONDS)` with the new combination.
    Reset { tumblers: [u8; 2] },
}

fn ask_tumbler(
    world: &mut World,
    client: Entity,
    mob: Entity,
    safe: Entity,
    tool: Entity,
    got: Vec<u8>,
) {
    let ask = Ask::new(
        client,
        mob,
        "Set Lock",
        format!("Set tumbler #{} (0-99):", got.len() + 1),
        PromptKind::Number {
            default: 0.0,
            min: 0.0,
            max: 99.0,
            round: true,
        },
    );
    let prompt = ask_world(world, ask);
    world
        .entity_mut(prompt)
        .insert(ResetAsk { safe, tool, got });
}

/// The answers of [`ResetAsk`]: the next tumbler, or the 10 s reset; rechecked each time
/// (`can_perform_action`).
fn reset_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in pending {
        let Some(ask) = world.get::<ResetAsk>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Number(value)) = answer else {
            tell(world, client, Span::Warning, "сброс отменён!");
            continue;
        };
        let carried = world
            .get::<crate::inventory::CarriedBy>(ask.tool)
            .is_some_and(|c| c.0 == mob);
        let near = world
            .get::<TilePos>(mob)
            .zip(world.get::<TilePos>(ask.safe))
            .and_then(|(a, b)| a.chebyshev(*b))
            .is_some_and(|d| d <= 1);
        let open = world.get::<Safe>(ask.safe).is_some_and(|s| s.open);
        if !carried || !near || !open {
            tell(world, client, Span::Warning, "сброс прерван!");
            continue;
        }
        let mut got = ask.got;
        got.push(value.clamp(0.0, 99.0) as u8);
        if got.len() < 2 {
            ask_tumbler(world, client, mob, ask.safe, ask.tool, got);
            continue;
        }
        let actor = crate::tools::Actor {
            client,
            mob,
            item: Some(ask.tool),
        };
        crate::tools::do_after(
            world,
            actor,
            ask.safe,
            100.0,
            0,
            crate::construction::Job::Safe(SafeJob::Reset {
                tumblers: [got[0], got[1]],
            }),
        );
    }
}

/// [`SafeJob`] at the end of its `do_after`.
pub fn complete(world: &mut World, action: &crate::tools::DoAfter, job: SafeJob) {
    let SafeJob::Reset { tumblers } = job;
    let Some(mut s) = world.get_mut::<Safe>(action.target) else {
        return;
    };
    if !s.open {
        return;
    }
    s.tumblers = tumblers.to_vec();
    s.index = 0;
    s.dial = 0;
    tell(
        world,
        action.client,
        Span::Notice,
        format!(
            "Вы сбрасываете замок. Новая комбинация: {}-{}.",
            tumblers[0], tumblers[1]
        ),
    );
}

/// `notify_user`'s sounds of a right turn (hit, miss) and a left one.
fn sounds(left: bool, tick: Tick) -> [&'static str; 3] {
    match (left, tick) {
        (false, Tick::Hit) => ["тинк", "кринк", "плинк"],
        (false, Tick::Miss) => ["клак", "скрип", "кланк"],
        (true, Tick::Hit) => ["тонк", "крунк", "плунк"],
        (true, Tick::Miss) => ["щёлк", "чинк", "клинк"],
    }
}

/// `ui_act`, rechecked at the moment of effect.
fn safe_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SafeUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, SafeAct)> = acts
        .read(world.resource::<Messages<UiActed<SafeUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, user, safe, act) in pending {
        // `ishuman(usr)`.
        if world.get::<crate::inventory::Inventory>(user).is_none() {
            continue;
        }
        let Some(mut s) = world.get::<Safe>(safe).cloned() else {
            continue;
        };
        let name = name_of(world, safe);
        match act {
            SafeAct::Open => {
                if s.locked && !s.open && !s.broken() {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        format!("{name} не открыть — замок заперт!"),
                    );
                    continue;
                }
                s.open = !s.open;
                let verb = if s.open {
                    "открываете"
                } else {
                    "закрываете"
                };
                tell(world, client, Span::Notice, format!("Вы {verb} {name}."));
            }
            SafeAct::Turn { left, ticks } => {
                if s.open {
                    continue;
                }
                if s.broken() {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "Диск не поворачивается: механизм разрушен!",
                    );
                    continue;
                }
                let hear = holds_stethoscope(world, user);
                for n in 1..=ticks {
                    let tick = s.tick(left);
                    if !hear {
                        continue;
                    }
                    if n == 2 {
                        tell(
                            world,
                            client,
                            Span::Notice,
                            format!("Звуки из {name} слишком быстрые и сливаются."),
                        );
                    }
                    let heard =
                        ticks == 1 || world.resource_mut::<CombatRng>().chance(SOUND_CHANCE);
                    if heard {
                        let pick = world.resource_mut::<CombatRng>().range(0, 2) as usize;
                        tell(world, client, Span::Notice, sounds(left, tick)[pick]);
                    }
                }
                if s.check_unlocked() {
                    let pick = world.resource_mut::<CombatRng>().range(0, 4) as usize;
                    let text = ["Пружина!", "Спранг!", "Спроинг!", "Кланк!", "Крунк!"][pick];
                    visible(world, safe, "", text, Span::Notice);
                }
            }
            SafeAct::Retrieve(index) => {
                if !s.open {
                    continue;
                }
                let Some(&item) = items(world, safe).get(index - 1) else {
                    continue;
                };
                let near = world
                    .get::<TilePos>(user)
                    .zip(world.get::<TilePos>(safe))
                    .and_then(|(a, b)| a.chebyshev(*b))
                    .is_some_and(|d| d <= 1);
                if !near {
                    continue;
                }
                crate::cooking::give_to(world, user, safe, item);
            }
        }
        // Keep the contents list `refresh` wrote, take the rest of the state.
        if let Some(mut live) = world.get_mut::<Safe>(safe) {
            let contents = std::mem::take(&mut live.contents);
            *live = Safe { contents, ..s };
        }
        refresh(world, safe);
    }
}

/// `safe/ex_act`: a devastating explosion, or a heavy one aimed at it, bangs it up; the third
/// breaks the lock. True when it counted.
pub fn ex_act(world: &mut World, safe: Entity, devastating: bool, targeted_heavy: bool) -> bool {
    let Some(mut s) = world.get_mut::<Safe>(safe) else {
        return false;
    };
    if !(devastating || targeted_heavy) || s.explosions >= BROKEN_THRESHOLD {
        return false;
    }
    s.explosions += 1;
    true
}

/// `examine` and the banged-up `desc` lines.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(s) = world.get::<Safe>(target) else {
        return;
    };
    let wear = match s.explosions {
        0 => None,
        1 => Some("Он выглядит слегка помятым."),
        2 => Some("Он сильно повреждён."),
        _ => Some("Замок, похоже, сломан."),
    };
    if let Some(wear) = wear {
        lines.push((Span::Notice, wear.to_owned()));
    }
    lines.push((
        Span::Notice,
        "Шестерни запирающего механизма закреплены <b>болтами</b>.".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe(tumblers: [u8; 2]) -> Safe {
        Safe {
            tumblers: tumblers.to_vec(),
            locked: true,
            ..Safe::default()
        }
    }

    #[test]
    fn right_then_left_on_the_combination_unlocks() {
        let mut s = safe([97, 2]);
        // Right from 0: 99, 98, 97 hits the first tumbler.
        let ticks: Vec<Tick> = (0..3).map(|_| s.tick(false)).collect();
        assert_eq!(ticks.last(), Some(&Tick::Hit));
        assert!(!s.check_unlocked());
        // Left from 97: 98, 99, 0, 1, 2 hits the second.
        for _ in 0..4 {
            assert_eq!(s.tick(true), Tick::Miss);
        }
        assert_eq!(s.tick(true), Tick::Hit);
        assert!(s.check_unlocked());
        assert!(!s.locked);
        // One more turn goes past the last tumbler: reset and locked again.
        s.tick(true);
        assert!(!s.check_unlocked());
        assert!(s.locked);
    }

    #[test]
    fn a_wrong_way_turn_resets_the_tumblers() {
        let mut s = safe([99, 50]);
        assert_eq!(s.tick(false), Tick::Hit);
        assert_eq!(s.index, 1);
        // Right again where left is due.
        s.tick(false);
        assert_eq!(s.index, 0);
    }

    #[test]
    fn three_explosions_break_the_lock() {
        let mut s = safe([1, 2]);
        s.explosions = BROKEN_THRESHOLD;
        assert!(s.broken());
        assert!(!s.check_unlocked());
    }
}
