//! Statue materials, busts and sculpting (server), after tg `code/modules/art/statues.dm`,
//! `code/game/objects/items/busts_and_figurines.dm`,
//! `datums/components/combustible_flooder.dm` (fastening and slicing statues: `crate::decor`;
//! uranium statues' radiation: `crate::radiation::Radioactive`).
//!
//! - Plasma statues (`combustible_flooder`): a lit welder or a fire hotter than
//!   `FIRE_MINIMUM_TEMPERATURE_TO_EXIST` floods their tile with plasma at that temperature and
//!   destroys them (sheets two fewer).
//! - The Hippocrates bust in hand swears its oath (4 + 2 + 3 + 3 s of lines, then
//!   `TRAIT_PACIFISM`) or rescinds a sworn one (5 s); a 5 minute cooldown, an interrupted oath
//!   costs 2 brain damage and resets it. The blood-caked "Solemn Vow" (1%) refuses.
//! - A chisel on a carving block prepares it, on any atom on a turf picks that as the model, on
//!   the block again carves (30 s, an interrupted carve keeps its progress); the result is a
//!   custom statue with the model's look tinted in the block's colour.
// shortcut: no art/beauty elements, rotation or marionettes; plasma statues ignore lighters,
// matches, sparks and burn projectiles (no item temperature but the welder's); the Solemn Vow
// gives no medical HUD and keeps the normal attack speed; sculpting has no preset statue menu,
// no ranged targeting, no greyscale filter or line-by-line reveal, and the block keeps no
// materials (the custom statue slices into nothing); add each with its mechanic.

use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use super::server::set_name;
use crate::chem::name_of;
use crate::construction::Job;
use crate::obj_damage::is_under;
use crate::tools::{Actor, DoAfter, do_after, tell};

const PLASMA_STATUE: &str = "/obj/structure/statue/plasma";
const CUSTOM_STATUE: &str = "/obj/structure/statue/custom";
const HIPPOCRATIC: &str = "/obj/item/statuebust/hippocratic";
const HIPPOCRATIC_EVIL: &str = "/obj/item/statuebust/hippocratic/evil";
const CHISEL: &str = "/obj/item/chisel";
const CARVING_BLOCK: &str = "/obj/structure/carving_block";
/// `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`, kelvin.
const FIRE_MIN_TEMP: f64 = ss13_atmos::reactions::FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
/// `mat_amount * 0.05 * multiplier` of `SHEET_MATERIAL_AMOUNT * 5` plasma (one material: the
/// multiplier is 1), moles.
const PLASMA_STATUE_MOLES: f64 = 500.0 * 0.05;
/// `COOLDOWN_START(src, oath_cd, 5 MINUTES)`.
const OATH_COOLDOWN: f64 = 300.0;
/// The oath's `do_after`s (deciseconds) and lines.
const OATH: [(f32, &str); 4] = [
    (
        40.0,
        "Я клянусь выполнять, наилучшим образом, в соответствии с моими способностями и суждением, этот завет:",
    ),
    (
        20.0,
        "Я буду применять все необходимые меры на благо больных, избегая двух крайностей: чрезмерного ухода и терапевтического нигилизма.",
    ),
    (
        30.0,
        "Я буду помнить, что остаюсь членом общества, у которого есть особые обязательства по отношению ко всем моим собратьям - как к здоровым душой и телом, так и к немощным.",
    ),
    (
        30.0,
        "Если я не нарушу эту клятву, пусть я буду наслаждаться жизнью и искусством, меня будут уважать, пока я жив, и вспоминать с любовью после этого. Пусть мои действия всегда будут направлены на сохранение лучших традиций моего призвания, и пусть я долго буду испытывать радость от исцеления тех, кто обращается ко мне за помощью.",
    ),
];
const RESCIND_DS: f32 = 50.0;
const RESCIND_LINE: &str = "Да уж, с этим 'Гиппопотамом' ничего не вышло. Я ухожу!";
/// `sculpting_time`, deciseconds.
const SCULPT_DS: f32 = 300.0;

/// `/datum/component/combustible_flooder`: plasma moles released when ignited.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CombustibleFlooder {
    pub moles: f64,
}

/// `/obj/item/statuebust/hippocratic`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct HippocraticBust {
    /// The blood-caked "Solemn Vow".
    pub reference: bool,
    /// `oath_cd`: the next oath from this time.
    pub ready_at: f64,
}

/// `TRAIT_PACIFISM` from the Hippocrates bust's oath, on the mob.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct HippocraticOath;

/// `/obj/item/chisel`: the prepared block and where its user stood (`tracked_user` moving
/// cancels it).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Chisel {
    pub block: Option<(Entity, TilePos)>,
}

/// `/obj/structure/carving_block`: the model (its name and look) and the carving progress.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CarvingBlock {
    pub target: Option<(String, Appearance)>,
    pub completion: f32,
}

/// A statue or bust `do_after` ([`Job::Art`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtJob {
    /// Line `step` of the oath said by the bust's holder.
    Oath { bust: Entity, step: u8 },
    /// Rescinding a sworn oath.
    Rescind { bust: Entity },
    /// Carving the block (the target) with a chisel.
    Sculpt,
    /// A hand labeler refill roll into a ticket machine (the target).
    TicketRefill,
    /// The held ID sticker onto `card`.
    ApplySticker { card: Entity },
    /// A sticker peeled off `card`.
    PeelSticker { card: Entity },
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, PLASMA_STATUE) {
        world.entity_mut(e).insert(CombustibleFlooder {
            moles: PLASMA_STATUE_MOLES,
        });
    } else if is_under(path, CARVING_BLOCK) && world.get::<CarvingBlock>(e).is_none() {
        world.entity_mut(e).insert(CarvingBlock::default());
    }
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, HIPPOCRATIC) && world.get::<HippocraticBust>(e).is_none() {
        // `prob(reference_chance)`: 1%, always for the evil one.
        let reference = is_under(path, HIPPOCRATIC_EVIL)
            || world
                .get_resource_mut::<crate::combat::CombatRng>()
                .is_some_and(|mut rng| rng.chance(1.0));
        world.entity_mut(e).insert(HippocraticBust {
            reference,
            ready_at: 0.0,
        });
        if reference {
            set_name(world, e, "Solemn Vow");
        }
    } else if is_under(path, CHISEL) && world.get::<Chisel>(e).is_none() {
        world.entity_mut(e).insert(Chisel::default());
    }
}

/// `combustible_flooder/flood`: the gas on the tile at `temperature`, the object destroyed.
fn flood(world: &mut World, target: Entity, temperature: f64) {
    let Some(flooder) = world.get::<CombustibleFlooder>(target).copied() else {
        return;
    };
    world.entity_mut(target).remove::<CombustibleFlooder>();
    if let Some(&pos) = world.get::<TilePos>(target)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        let gas = format!("plasma={};TEMP={temperature}", flooder.moles);
        if atmos.spawn_air(pos, &gas).is_err() {
            warn!("combustible flooder gas {gas:?} does not parse");
        }
    }
    crate::obj_damage::deconstruct(world, target, false);
}

/// `combustible_flooder/flame_react` (`fire_act`). True when `thing` flooded.
pub fn fire_act(world: &mut World, thing: Entity, temperature: f64) -> bool {
    if temperature <= FIRE_MIN_TEMP || world.get::<CombustibleFlooder>(thing).is_none() {
        return false;
    }
    flood(world, thing, temperature);
    true
}

/// `get_temperature` of a held item: a lit welder's, lighter's, match's or cigarette's heat.
fn item_temperature(world: &World, item: Entity) -> f64 {
    f64::from(crate::medical::surgery::ToolInfo::read(world, Some(item)).temperature)
}

/// `welder_react`/`attackby_react` of flooders, the chisel. True when the click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<CombustibleFlooder>(target).is_some() {
        let temperature = item_temperature(world, held);
        if temperature >= FIRE_MIN_TEMP {
            let name = name_of(world, target);
            tell(world, client, Span::Danger, format!("{name} вспыхивает!"));
            flood(world, target, temperature);
            return true;
        }
    }
    if world.get::<Chisel>(held).is_some() {
        return chisel_on(world, client, mob, held, target);
    }
    false
}

/// `hippocratic/attack_self`. True when `item` was a Hippocrates bust.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(bust) = world.get::<HippocraticBust>(item).copied() else {
        return false;
    };
    // `iscarbon`.
    if world.get::<crate::human::HumanLook>(mob).is_none() {
        tell(
            world,
            client,
            Span::Warning,
            "You remember how the Hippocratic Oath specifies 'my fellow human beings' and realize that it's completely meaningless to you.",
        );
        return true;
    }
    if bust.reference {
        tell(
            world,
            client,
            Span::Warning,
            "As you prepare yourself to swear the Oath, you realize that doing so on a blood-caked bust is probably not a good idea.",
        );
        return true;
    }
    let now = super::server::now(world);
    if now < bust.ready_at {
        tell(
            world,
            client,
            Span::Warning,
            "You've sworn or forsworn an oath too recently to undo your decisions. The bust looks at you with disgust.",
        );
        return true;
    }
    if let Some(mut b) = world.get_mut::<HippocraticBust>(item) {
        b.ready_at = now + OATH_COOLDOWN;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    if world.get::<HippocraticOath>(mob).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "You've already sworn a vow. You start preparing to rescind it...",
        );
        // tg goes on to swear the oath again after rescinding it (no `return`): not ported.
        do_after(
            world,
            actor,
            mob,
            RESCIND_DS,
            0,
            Job::Art(ArtJob::Rescind { bust: item }),
        );
        return true;
    }
    if world
        .get::<crate::medical::traumas::Pacifist>(mob)
        .is_some()
    {
        tell(
            world,
            client,
            Span::Warning,
            "You already don't want to harm people, this isn't going to do anything!",
        );
    }
    tell(
        world,
        client,
        Span::Notice,
        "You remind yourself of the Hippocratic Oath's contents and prepare to swear yourself to it...",
    );
    do_after(
        world,
        actor,
        mob,
        OATH[0].0,
        0,
        Job::Art(ArtJob::Oath {
            bust: item,
            step: 0,
        }),
    );
    true
}

/// The statue and bust jobs' effects.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ArtJob) {
    let client = action.client;
    match job {
        ArtJob::Oath { bust, step } => {
            let Some(&(_, line)) = OATH.get(usize::from(step)) else {
                return;
            };
            super::religion::say(world, mob, line);
            let next = step + 1;
            if let Some(&(delay, _)) = OATH.get(usize::from(next)) {
                let actor = Actor {
                    client,
                    mob,
                    item: Some(bust),
                };
                do_after(
                    world,
                    actor,
                    mob,
                    delay,
                    0,
                    Job::Art(ArtJob::Oath { bust, step: next }),
                );
                return;
            }
            tell(
                world,
                client,
                Span::Notice,
                "Contentment, understanding, and purpose washes over you as you finish the oath. You consider for a second the concept of harm and shudder.",
            );
            world.entity_mut(mob).insert(HippocraticOath);
            crate::medical::traumas::refresh(world, mob);
        }
        ArtJob::Rescind { .. } => {
            super::religion::say(world, mob, RESCIND_LINE);
            world.entity_mut(mob).remove::<HippocraticOath>();
            crate::medical::traumas::refresh(world, mob);
        }
        ArtJob::Sculpt => {
            let block = action.target;
            if let Some(mut state) = world.get_mut::<CarvingBlock>(block) {
                state.completion = 1.0;
            }
            create_statue(world, block);
            if let Some(chisel) = action.item
                && let Some(mut c) = world.get_mut::<Chisel>(chisel)
            {
                c.block = None;
            }
            tell(world, client, Span::Notice, "Статуя готова.");
        }
        ArtJob::TicketRefill => {
            super::tickets::refilled(world, client, action.target, action.item);
        }
        ArtJob::ApplySticker { card } => {
            super::id_stickers::apply(world, client, mob, card, action.item);
        }
        ArtJob::PeelSticker { card } => super::id_stickers::peeled(world, client, mob, card),
    }
}

/// An interrupted statue or bust job: `fuck_it_up`, or the carving's progress so far.
pub fn interrupted(world: &mut World, mob: Entity, action: &DoAfter, job: ArtJob) {
    let client = action.client;
    match job {
        ArtJob::Oath { bust, .. } => {
            tell(
                world,
                client,
                Span::Warning,
                "You forget what comes next like a dumbass. The Hippocrates bust looks down on you, disappointed.",
            );
            crate::medical::organs::adjust_organ_loss(
                world,
                mob,
                crate::medical::organs::Slot::Brain,
                2.0,
            );
            if let Some(mut b) = world.get_mut::<HippocraticBust>(bust) {
                b.ready_at = 0.0;
            }
        }
        ArtJob::Rescind { .. }
        | ArtJob::TicketRefill
        | ArtJob::ApplySticker { .. }
        | ArtJob::PeelSticker { .. } => {}
        ArtJob::Sculpt => {
            let now = super::server::now(world);
            let left = ((action.ends_at - now).max(0.0) * 10.0) as f32;
            if let Some(mut state) = world.get_mut::<CarvingBlock>(action.target) {
                state.completion = (1.0 - left / SCULPT_DS).clamp(state.completion, 1.0);
            }
            if let Some(chisel) = action.item
                && let Some(mut c) = world.get_mut::<Chisel>(chisel)
            {
                c.block = None;
            }
            tell(world, client, Span::Warning, "Резьба прервана!");
        }
    }
}

/// `chisel/interact_with_atom`.
fn chisel_on(
    world: &mut World,
    client: Entity,
    mob: Entity,
    chisel: Entity,
    target: Entity,
) -> bool {
    let Some(&user_pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    // `on_moved` -> `stop_sculpting`: a block prepared from another tile is forgotten.
    let mut prepared = world.get::<Chisel>(chisel).and_then(|c| c.block);
    if prepared.is_some_and(|(block, at)| at != user_pos || world.get_entity(block).is_err()) {
        if let Some((block, _)) = prepared {
            reset_unstarted(world, block);
        }
        prepared = None;
    }
    let prepared = prepared.map(|(b, _)| b);
    let set_block = |world: &mut World, block: Option<Entity>| {
        if let Some(mut c) = world.get_mut::<Chisel>(chisel) {
            c.block = block.map(|b| (b, user_pos));
        }
    };
    if let Some(block) = world.get::<CarvingBlock>(target).cloned() {
        if block.completion > 0.0 || (prepared == Some(target) && block.target.is_some()) {
            set_block(world, Some(target));
            start_sculpting(world, client, mob, chisel, target, block.completion);
        } else if prepared.is_none() {
            set_block(world, Some(target));
            tell(world, client, Span::Notice, "Выберите, что вырезать.");
        } else if prepared == Some(target) {
            // shortcut: no preset statue radial (`show_generic_statues_prompt`).
            tell(
                world,
                client,
                Span::Warning,
                "Сначала выберите, что вырезать.",
            );
        }
        return true;
    }
    let Some(block) = prepared else {
        return false;
    };
    // `is_viable_target`: only things on turfs.
    let on_turf = world.get::<TilePos>(target).is_some()
        && world.get::<crate::inventory::CarriedBy>(target).is_none()
        && world.get::<crate::storage::StoredIn>(target).is_none();
    let Some(look) = world.get::<Appearance>(target).cloned().filter(|_| on_turf) else {
        tell(world, client, Span::Warning, "Это нельзя вырезать!");
        return true;
    };
    let name = name_of(world, target);
    if let Some(mut state) = world.get_mut::<CarvingBlock>(block) {
        state.target = Some((name.clone(), look));
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Модель для статуи: {name}."),
    );
    true
}

/// `stop_sculpting` on an unstarted block: `reset_target`.
fn reset_unstarted(world: &mut World, block: Entity) {
    if let Some(mut state) = world.get_mut::<CarvingBlock>(block)
        && state.completion == 0.0
    {
        state.target = None;
    }
}

/// `start_sculpting`: the rest of the 30 s in one `do_after`.
fn start_sculpting(
    world: &mut World,
    client: Entity,
    mob: Entity,
    chisel: Entity,
    block: Entity,
    completion: f32,
) {
    tell(
        world,
        client,
        Span::Notice,
        "Вы начинаете вырезать статую...",
    );
    super::server::sound_at(
        world,
        block,
        "sound/effects/pickaxe/picaxe1.ogg",
        75.0,
        true,
    );
    let left = SCULPT_DS * (1.0 - completion.clamp(0.0, 1.0));
    let actor = Actor {
        client,
        mob,
        item: Some(chisel),
    };
    do_after(world, actor, block, left, 0, Job::Art(ArtJob::Sculpt));
}

/// `carving_block/create_statue`: a custom statue in the model's look, tinted in the block's
/// colour, with its name.
fn create_statue(world: &mut World, block: Entity) {
    let Some(state) = world.get::<CarvingBlock>(block).cloned() else {
        return;
    };
    let Some((name, model)) = state.target else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(block) else {
        return;
    };
    let tint = world.get::<Appearance>(block).and_then(|a| a.color);
    let Some(statue) = crate::construction::spawn_obj(world, CUSTOM_STATUE, pos) else {
        return;
    };
    let overlays = statue_overlays(&model, tint);
    if let Some(mut look) = world.get_mut::<Appearance>(statue) {
        look.overlays.extend(overlays);
    }
    set_name(world, statue, &format!("statue of {name}"));
    if let Some(mut dm) = world.get_mut::<ss13_mapload::DmName>(statue) {
        dm.desc = format!("A carved statue depicting {name}.");
    }
    world.despawn(block);
    crate::construction::tile_changed(world, pos);
}

/// `set_visuals`: the model's sprite and overlays at no offset, in the material's colour.
fn statue_overlays(model: &Appearance, tint: Option<[u8; 4]>) -> Vec<Overlay> {
    std::iter::once(Overlay {
        color: tint,
        ..Overlay::new(model.icon_state)
    })
    .chain(model.overlays.iter().map(|o| Overlay {
        color: tint.or(o.color),
        ..o.clone()
    }))
    .collect()
}

/// `traumas::refresh` reads this: the oath's pacifism.
pub fn sworn(world: &World, mob: Entity) -> bool {
    world.get::<HippocraticOath>(mob).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::{IconStateId, Layer, MouseOpacity};

    #[test]
    fn statue_takes_the_model_look_in_the_block_colour() {
        let model = Appearance {
            icon_state: IconStateId(7),
            layer: Layer(4.0),
            plane: 0,
            color: None,
            pixel_offset: [5, 5],
            rotation: 0,
            overlays: vec![Overlay::new(IconStateId(9))],
            mouse_opacity: MouseOpacity::Icon,
            glide: 0,
        };
        let tint = Some([200, 200, 200, 255]);
        let out = statue_overlays(&model, tint);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].icon_state, IconStateId(7));
        assert_eq!(out[0].pixel_offset, [0, 0], "set_visuals zeroes the offset");
        assert!(out.iter().all(|o| o.color == tint));
    }

    #[test]
    fn oath_takes_twelve_seconds() {
        let total: f32 = OATH.iter().map(|(d, _)| d).sum();
        assert_eq!(total, 120.0);
    }
}
