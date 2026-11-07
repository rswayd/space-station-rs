//! Syndicate gear with its own behaviour (server), after tg:
//!
//! - the energy sword (`/obj/item/melee/energy/sword`, `code/game/objects/items/weaponry/melee/
//!   energy.dm`) and the energy dagger (`/obj/item/pen/edagger`, `code/modules/paperwork/pen.dm`):
//!   `/datum/component/transforming` toggled in hand. Active, the sword hits for 30 (edged,
//!   `blade1.ogg`, `WEIGHT_CLASS_HUGE`, `e_sword_on_<colour>`); the dagger for 18 (edged,
//!   `WEIGHT_CLASS_NORMAL`) and shows itself as an energy dagger. Inactive, the item's own vars
//!   come back. A saber's colour is its subtype's or a random one (`possible_sword_colors`);
//! - the sleepy pen (`/obj/item/pen/sleepy`): a pen stab (`try_inject`) injects its whole
//!   45 u holder (20 chloral hydrate, 15 mute toxin, 10 tirizene) into the victim's blood;
//! - the agent card (`/obj/item/card/id/advanced/chameleon`): `attack_self` shows it or forges
//!   it (a name and an assignment asked, or the forgery reset); used on another ID card it
//!   copies the accesses its wildcard slots can take.
// shortcut: no `block_chance`, embedding, jousting, alternative (pointy) sharpness, light or
// heat of the blades, no saber multitool rainbow; the sleepy pen does not fit a dart; the agent
// card takes every access that fits instead of the `ChameleonCard` window's choice, has no
// age, wallet spoofing, account change or chameleon trim action, does not copy from a worn ID
// with a right click, and a blank forged name keeps the default instead of a random one.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmName, DmWeight, DmWounding};
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use crate::chem::{MobReagents, Reagents};
use crate::combat::CombatRng;
use crate::health::Stat;
use crate::id_access::{AddMode, CardAccess, add_access};
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::outfit::{IdRegistration, id_label};
use crate::tools::{Actor, tell};

pub const ESWORD_ROOT: &str = "/obj/item/melee/energy/sword";
const SABER_ROOT: &str = "/obj/item/melee/energy/sword/saber";
pub const EDAGGER: &str = "/obj/item/pen/edagger";
pub const SLEEPY_PEN: &str = "/obj/item/pen/sleepy";
pub const AGENT_CARD: &str = "/obj/item/card/id/advanced/chameleon";
/// `possible_sword_colors`.
const SABER_COLORS: [&str; 4] = ["red", "blue", "green", "purple"];
/// `SHARP_EDGED`.
const SHARP_EDGED: u32 = 1;
/// `WEIGHT_CLASS_NORMAL`, `WEIGHT_CLASS_HUGE`.
const WEIGHT_CLASS_NORMAL: u8 = 3;
const WEIGHT_CLASS_HUGE: u8 = 5;
/// `active_hitsound` (and `hitsound_on`'s default), `saberon`/`saberoff`.
pub const BLADE_HIT: &str = "sound/items/weapons/blade1.ogg";
pub const SABER_ON: &str = "sound/items/weapons/saberon.ogg";
pub const SABER_OFF: &str = "sound/items/weapons/saberoff.ogg";
/// The sleepy pen's `create_reagents(45, OPENCONTAINER)` and its reagents.
const SLEEPY_VOLUME: f32 = 45.0;
const SLEEPY_REAGENTS: [(&str, f32); 3] = [
    ("/datum/reagent/toxin/chloralhydrate", 20.0),
    ("/datum/reagent/toxin/mutetoxin", 15.0),
    ("/datum/reagent/toxin/staminatoxin", 10.0),
];
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;
/// The agent card menu (`tgui_input_list`).
const SHOW: &str = "Показать";
const FORGE: &str = "Подделать/Сброс";

/// A blade's vars while inactive (the item's own), and its state.
#[derive(Component, Clone, Debug)]
pub struct EnergyBlade {
    pub active: bool,
    force: f32,
    throwforce: f32,
    hitsound: String,
    inhand: String,
    lefthand: String,
    righthand: String,
    sharpness: u32,
    w_class: u8,
    icon_state: String,
    name: Option<DmName>,
    /// `sword_color_icon`; empty for the dagger.
    color: String,
}

/// An agent card's forgery (`forged`) and the name it had before.
#[derive(Component, Clone, Debug)]
pub struct AgentCard {
    pub forged: bool,
    original: Option<DmName>,
}

pub struct SyndieItemsPlugin;

impl Plugin for SyndieItemsPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An energy blade toggles, an agent card is shown or forged.
        app.add_click_handler(ClickPhase::AttackSelf, 1600, |w, c| {
            crate::antag::syndie_items::attack_self(w, c.client, c.mob, c.target)
        });
        // The agent card on the ID card in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 1100, |w, c| {
            c.held.is_some_and(|h| {
                crate::antag::syndie_items::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // The sleepy pen on a mob, the agent card on an ID card.
        app.add_click_handler(ClickPhase::HeldOnTarget, 200, |w, c| {
            c.held.is_some_and(|h| {
                crate::antag::syndie_items::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.add_systems(
            Update,
            agent_card_answers
                .in_set(UiSystems::Handle)
                .after(PromptSystems::Answer),
        );
    }
}

fn path_of(world: &World, item: Entity) -> Option<String> {
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

/// Whether `path` is a transforming energy blade ported here.
pub fn is_blade(path: &str) -> bool {
    // `/obj/item/melee/energy/blade/make_transformable` returns FALSE: always out.
    (is_under(path, ss13_mapload::melee::ENERGY_ROOT) && !is_under(path, ENERGY_BLADE))
        || is_under(path, EDAGGER)
}

/// `/obj/item/melee/energy/blade`, the non-transforming energy blades.
const ENERGY_BLADE: &str = "/obj/item/melee/energy/blade";

/// The sleepy pen's clicks on mobs are its own: `crate::combat` leaves them.
pub fn interacts_with_mobs(path: &str) -> bool {
    is_under(path, SLEEPY_PEN)
}

/// tg `attack_self` of the blades and the agent card. False for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(path) = path_of(world, item) else {
        return false;
    };
    if is_blade(&path) {
        toggle_blade(world, client, mob, item, &path);
        return true;
    }
    if is_under(&path, AGENT_CARD) {
        agent_card_menu(world, client, mob, item);
        return true;
    }
    false
}

/// `/datum/component/transforming/do_transform` -> `set_active`/`set_inactive`, then the
/// item's `on_transform` (sound, the dagger's disguise).
pub fn toggle_blade(world: &mut World, client: Entity, mob: Entity, item: Entity, path: &str) {
    let dagger = is_under(path, EDAGGER);
    if world.get::<EnergyBlade>(item).is_none() {
        let color = if dagger {
            String::new()
        } else {
            SABER_COLORS
                .iter()
                .find(|c| path.ends_with(&format!("/{c}")))
                .map(|c| (*c).to_owned())
                .or_else(|| {
                    is_under(path, SABER_ROOT).then(|| {
                        let i = world.resource_mut::<CombatRng>().range(0, 3) as usize;
                        SABER_COLORS[i].to_owned()
                    })
                })
                .unwrap_or_default()
        };
        let dm = world.get::<DmItem>(item).cloned().unwrap_or_default();
        let blade = EnergyBlade {
            active: false,
            force: dm.force,
            throwforce: dm.throwforce,
            hitsound: dm.hitsound.clone(),
            inhand: dm.inhand_icon_state.clone(),
            lefthand: dm.lefthand_file.clone(),
            righthand: dm.righthand_file.clone(),
            sharpness: world.get::<DmWounding>(item).map_or(0, |w| w.sharpness),
            w_class: world
                .get::<DmWeight>(item)
                .copied()
                .unwrap_or_default()
                .w_class,
            icon_state: world
                .get::<DmAtom>(item)
                .map(|a| a.icon_state.clone())
                .unwrap_or_default(),
            name: world.get::<DmName>(item).cloned(),
            color,
        };
        world.entity_mut(item).insert(blade);
    }
    let mut blade = world.get::<EnergyBlade>(item).cloned().expect("inserted");
    blade.active = !blade.active;
    apply_blade(world, item, &blade, dagger);
    world.entity_mut(item).insert(blade.clone());
    // `balloon_alert(user, "[name] enabled/disabled")`, the dagger's "active/concealed".
    let name = world
        .get::<DmName>(item)
        .map_or_else(|| "energy sword".to_owned(), |n| n.name.clone());
    let text = match (dagger, blade.active) {
        (true, true) => "energy dagger active".to_owned(),
        (true, false) => "energy dagger concealed".to_owned(),
        (false, true) => format!("{name} enabled"),
        (false, false) => format!("{name} disabled"),
    };
    tell(world, client, Span::Notice, text);
    if let Some(pos) = turf_of(world, mob) {
        let (file, volume) = match (blade.active, dagger) {
            (true, true) => (SABER_ON, 5.0),
            (false, true) => (SABER_OFF, 5.0),
            (true, false) => (SABER_ON, 35.0),
            (false, false) => (SABER_OFF, 35.0),
        };
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, volume, true));
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// The blade's live vars for its state.
fn apply_blade(world: &mut World, item: Entity, blade: &EnergyBlade, dagger: bool) {
    let on = blade.active;
    // `/obj/item/melee/energy`'s `active_*` vars (the axe's 150), 30/20/huge without them.
    let active = world
        .get::<ss13_mapload::DmMelee>(item)
        .filter(|m| m.active_force > 0.0)
        .map(|m| {
            (
                m.active_force,
                m.active_throwforce,
                m.active_w_class,
                m.active_hitsound.clone(),
            )
        });
    let (force, throwforce, w_class) = match (on, dagger, &active) {
        (true, true, _) => (18.0, 35.0, WEIGHT_CLASS_NORMAL),
        (true, false, Some((f, t, w, _))) => (*f, *t, *w),
        (true, false, None) => (30.0, 20.0, WEIGHT_CLASS_HUGE),
        (false, _, _) => (blade.force, blade.throwforce, blade.w_class),
    };
    let active_hit = active
        .map(|a| a.3)
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| BLADE_HIT.to_owned());
    let sharpness = if on { SHARP_EDGED } else { blade.sharpness };
    let (icon_state, inhand, lefthand, righthand) = match (on, dagger) {
        // `hidden_icon` and the swords' in-hand files.
        (true, true) => (
            "edagger".to_owned(),
            "edagger".to_owned(),
            "icons/mob/inhands/weapons/swords_lefthand.dmi".to_owned(),
            "icons/mob/inhands/weapons/swords_righthand.dmi".to_owned(),
        ),
        // `update_icon_state`: `[base_icon_state]_on_[sword_color_icon]`, in hand too.
        (true, false) if !blade.color.is_empty() => {
            let state = format!("{}_on_{}", blade.icon_state, blade.color);
            (
                state.clone(),
                state,
                blade.lefthand.clone(),
                blade.righthand.clone(),
            )
        }
        // `set_active` without a colour: `[icon_state]_on`.
        (true, false) => (
            format!("{}_on", blade.icon_state),
            if blade.inhand.is_empty() {
                String::new()
            } else {
                format!("{}_on", blade.inhand)
            },
            blade.lefthand.clone(),
            blade.righthand.clone(),
        ),
        (false, _) => (
            blade.icon_state.clone(),
            blade.inhand.clone(),
            blade.lefthand.clone(),
            blade.righthand.clone(),
        ),
    };
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.force = force;
        dm.throwforce = throwforce;
        dm.hitsound = if on {
            active_hit
        } else {
            blade.hitsound.clone()
        };
        dm.inhand_icon_state = inhand;
        dm.lefthand_file = lefthand;
        dm.righthand_file = righthand;
    }
    match world.get_mut::<DmWounding>(item) {
        Some(mut w) => w.sharpness = sharpness,
        None => {
            world.entity_mut(item).insert(DmWounding {
                sharpness,
                ..default()
            });
        }
    }
    world.entity_mut(item).insert(DmWeight { w_class });
    crate::obj_damage::set_icon_state(world, item, &icon_state);
    // The dagger's disguise: `hidden_name`, `hidden_desc` while out.
    if dagger && let Some(mut name) = world.get_mut::<DmName>(item) {
        match (on, &blade.name) {
            (true, _) => {
                name.name = "energy dagger".to_owned();
                name.desc = "It's a normal black ink pe- Wait. That's a thing used to stab people!"
                    .to_owned();
            }
            (false, Some(original)) => *name = original.clone(),
            (false, None) => {}
        }
    }
}

/// Where `entity` is: its tile, or its carrier's.
fn turf_of(world: &World, entity: Entity) -> Option<TilePos> {
    world.get::<TilePos>(entity).copied().or_else(|| {
        world
            .get::<CarriedBy>(entity)
            .and_then(|c| world.get::<TilePos>(c.0))
            .copied()
    })
}

/// tg `item_interaction` of the sleepy pen on a mob and of the agent card on an ID card.
/// False for anything else.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(path) = path_of(world, held) else {
        return false;
    };
    if is_under(&path, SLEEPY_PEN) && world.get::<MobReagents>(target).is_some() {
        sleepy_pen_attack(world, client, mob, held, target);
        return true;
    }
    if is_under(&path, AGENT_CARD) && held != target && world.get::<IdCard>(target).is_some() {
        copy_access(world, client, held, target);
        return true;
    }
    false
}

/// The pen's holder, made on first use (tg `Initialize`).
fn sleepy_holder(world: &mut World, pen: Entity, path: &str) {
    if world.get::<Reagents>(pen).is_some() {
        return;
    }
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let mut holder = Holder::new(
        SLEEPY_VOLUME,
        flags::OPENCONTAINER,
        Owner {
            path: path.to_owned(),
            living: false,
        },
    );
    let list: Vec<_> = SLEEPY_REAGENTS
        .iter()
        .filter_map(|(p, amount)| chem.id(p).map(|id| (id, *amount)))
        .collect();
    holder.add_reagent_list(&chem, &list, None, true);
    world.entity_mut(pen).insert(Reagents(holder));
}

/// `/obj/item/pen/attack` (forceless: `try_inject`, the stab lines) and
/// `/obj/item/pen/sleepy/attack`: everything it holds into the victim's blood.
fn sleepy_pen_attack(world: &mut World, client: Entity, mob: Entity, pen: Entity, target: Entity) {
    if world
        .get::<Stat>(mob)
        .is_some_and(|s| *s != Stat::Conscious)
        || !crate::chem::melee_cooldown(world, mob)
    {
        return;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(pen),
    };
    if !crate::chem::try_inject(world, actor, target) {
        return;
    }
    let victim = crate::chem::name_of(world, target);
    tell(
        world,
        client,
        Span::Warning,
        format!("Вы колете {victim} ручкой."),
    );
    crate::chem::tell_mob(world, target, Span::Danger, "Вы чувствуете лёгкий укол!");
    let path = path_of(world, pen).unwrap_or_default();
    sleepy_holder(world, pen, &path);
    let total = world.get::<Reagents>(pen).map_or(0.0, |r| r.0.total_volume);
    if total > 0.0 {
        crate::chem::transfer_to_mob(world, pen, target, total, false);
    }
    info!("{mob} stabbed {target} with a sleepy pen ({total} u)");
}

/// The agent card's `ChameleonCard` theft, all at once: every access of `target` the card's
/// wildcard slots can take (`TRY_ADD_ALL`).
fn copy_access(world: &mut World, client: Entity, card: Entity, target: Entity) {
    let Some(theirs) = world.get::<IdCard>(target).map(|c| c.access.clone()) else {
        return;
    };
    let path = path_of(world, card).unwrap_or_default();
    let mut ours = world
        .get::<IdCard>(card)
        .cloned()
        .unwrap_or(IdCard { access: Vec::new() });
    let mut data = world
        .get::<CardAccess>(card)
        .cloned()
        .unwrap_or_else(|| CardAccess::of_type(&path));
    let before = ours.access.len();
    let new: Vec<String> = theirs
        .into_iter()
        .filter(|a| !ours.access.contains(a))
        .collect();
    add_access(&mut ours, &mut data, &new, None, AddMode::TryAddAll);
    let added = ours.access.len() - before;
    world.entity_mut(card).insert((ours, data));
    let text = if added > 0 {
        format!("ID синхронизированы: скопировано доступов — {added}.")
    } else {
        "Ошибка ID: ID-карта отклонила вашу попытку изменить доступ.".to_owned()
    };
    tell(world, client, Span::Notice, text);
    info!("agent card {card} copied {added} accesses from {target}");
}

/// What an agent card prompt was for.
#[derive(Component, Clone, Debug)]
enum AgentCardPrompt {
    Menu { card: Entity },
    Name { card: Entity },
    Assignment { card: Entity, name: String },
}

fn agent_card_menu(world: &mut World, client: Entity, mob: Entity, card: Entity) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "ID агента",
            "Выберите действие",
            PromptKind::List {
                items: vec![SHOW.to_owned(), FORGE.to_owned()],
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(AgentCardPrompt::Menu { card });
}

/// `after_input_check`: the user is conscious and holds the card.
fn after_input_check(world: &World, mob: Entity, card: Entity) -> bool {
    world
        .get::<Stat>(mob)
        .is_some_and(|s| *s == Stat::Conscious)
        && world.get::<Inventory>(mob).is_some_and(|i| i.holds(card))
}

fn agent_card_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(asked) = world.get::<AgentCardPrompt>(prompt).cloned() else {
            continue;
        };
        let card = match &asked {
            AgentCardPrompt::Menu { card }
            | AgentCardPrompt::Name { card }
            | AgentCardPrompt::Assignment { card, .. } => *card,
        };
        if !after_input_check(world, mob, card) {
            continue;
        }
        match (asked, answer) {
            (AgentCardPrompt::Menu { .. }, Some(PromptAnswer::Item(0))) => {
                show_card(world, mob, card)
            }
            (AgentCardPrompt::Menu { .. }, Some(PromptAnswer::Item(1))) => {
                forge_or_reset(world, client, mob, card)
            }
            (AgentCardPrompt::Name { .. }, Some(PromptAnswer::Text(name))) => {
                let name = match name.trim() {
                    "" => world
                        .get::<Name>(mob)
                        .map(|n| n.to_string())
                        .unwrap_or_default(),
                    name => name.to_owned(),
                };
                let assignment = world
                    .get::<IdRegistration>(card)
                    .map(|r| r.assignment.clone())
                    .filter(|a| !a.is_empty())
                    .unwrap_or_else(|| "Ассистент".to_owned());
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        mob,
                        "Должность агентской карты",
                        "Какую должность вы хотите указать на этой карте?\nПримечание: Это не \
                         предоставит никаких уровней доступа.",
                        PromptKind::Text {
                            default: assignment,
                            max_length: MAX_NAME_LEN,
                            multiline: false,
                        },
                    ),
                );
                world
                    .entity_mut(prompt)
                    .insert(AgentCardPrompt::Assignment { card, name });
            }
            (AgentCardPrompt::Assignment { name, .. }, Some(PromptAnswer::Text(assignment))) => {
                forge(world, client, card, &name, assignment.trim());
            }
            _ => {}
        }
    }
}

/// `/obj/item/card/id/attack_self`: "[user] показывает вам: [name]."
fn show_card(world: &mut World, mob: Entity, card: Entity) {
    let Some(pos) = turf_of(world, mob) else {
        return;
    };
    let user = crate::chem::name_of(world, mob);
    let name = crate::chem::name_of(world, card);
    let messages = crate::pull::visible_message(
        world,
        pos,
        ChatChannel::System,
        (Span::Notice, format!("{user} показывает вам: {name}.")),
        &[(mob, Span::Notice, format!("Вы показываете {name}."))],
    );
    for m in messages {
        world.write_message(m);
    }
}

/// "Forge/Reset": a forged card goes back to its default; else the name is asked.
fn forge_or_reset(world: &mut World, client: Entity, mob: Entity, card: Entity) {
    if let Some(agent) = world.get::<AgentCard>(card).cloned()
        && agent.forged
    {
        let mut entity = world.entity_mut(card);
        entity.remove::<IdRegistration>();
        if let Some(original) = agent.original.clone() {
            entity.insert(original);
        }
        entity.insert(AgentCard {
            forged: false,
            original: agent.original,
        });
        tell(
            world,
            client,
            Span::Notice,
            "Вы успешно сбросили настройки ID-карты.",
        );
        return;
    }
    let default = world
        .get::<IdRegistration>(card)
        .map(|r| r.registered_name.clone())
        .filter(|n| !n.is_empty())
        .or_else(|| world.get::<Name>(mob).map(|n| n.to_string()))
        .unwrap_or_default();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Имя агентской карты",
            "Какое имя вы хотите указать на этой карте? Оставьте пустым для случайного имени.",
            PromptKind::Text {
                default,
                max_length: MAX_NAME_LEN,
                multiline: false,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(AgentCardPrompt::Name { card });
}

/// The forgery: `registered_name`, `assignment` (kept when left blank), `update_label`.
pub fn forge(world: &mut World, client: Entity, card: Entity, name: &str, assignment: &str) {
    let original = match world.get::<AgentCard>(card) {
        Some(agent) => agent.original.clone(),
        None => world.get::<DmName>(card).cloned(),
    };
    let old = world.get::<IdRegistration>(card).cloned();
    let assignment = if assignment.is_empty() {
        old.as_ref()
            .map(|r| r.assignment.clone())
            .unwrap_or_default()
    } else {
        assignment.to_owned()
    };
    let job = old.map(|r| r.job).unwrap_or_default();
    let label = id_label(name, &assignment);
    let desc = world
        .get::<DmName>(card)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    world.entity_mut(card).insert((
        IdRegistration {
            registered_name: name.to_owned(),
            assignment,
            job,
        },
        DmName { name: label, desc },
        AgentCard {
            forged: true,
            original,
        },
    ));
    tell(
        world,
        client,
        Span::Notice,
        "Вы успешно подделали ID-карту.",
    );
    info!("agent card {card} forged as {name}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sword_toggles_its_vars_and_back() {
        let mut world = World::new();
        world.insert_resource(CombatRng(7));
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        let client = world.spawn_empty().id();
        let mob = world.spawn(TilePos::new(1, 1, 1)).id();
        let sword = world
            .spawn((
                DmItem {
                    path: "/obj/item/melee/energy/sword/saber/blue".into(),
                    force: 3.0,
                    throwforce: 5.0,
                    inhand_icon_state: "e_sword".into(),
                    ..default()
                },
                DmWeight { w_class: 2 },
            ))
            .id();
        toggle_blade(
            &mut world,
            client,
            mob,
            sword,
            "/obj/item/melee/energy/sword/saber/blue",
        );
        let dm = world.get::<DmItem>(sword).unwrap();
        assert_eq!((dm.force, dm.throwforce), (30.0, 20.0));
        assert_eq!(dm.hitsound, BLADE_HIT);
        assert_eq!(
            world.get::<DmWounding>(sword).unwrap().sharpness,
            SHARP_EDGED
        );
        assert_eq!(
            world.get::<DmWeight>(sword).unwrap().w_class,
            WEIGHT_CLASS_HUGE
        );
        assert_eq!(world.get::<EnergyBlade>(sword).unwrap().color, "blue");
        toggle_blade(
            &mut world,
            client,
            mob,
            sword,
            "/obj/item/melee/energy/sword/saber/blue",
        );
        let dm = world.get::<DmItem>(sword).unwrap();
        assert_eq!((dm.force, dm.throwforce), (3.0, 5.0));
        assert_eq!(dm.inhand_icon_state, "e_sword");
        assert_eq!(world.get::<DmWounding>(sword).unwrap().sharpness, 0);
        assert_eq!(world.get::<DmWeight>(sword).unwrap().w_class, 2);
        assert!(!world.get::<EnergyBlade>(sword).unwrap().active);
    }

    #[test]
    fn the_agent_card_copies_what_fits_and_forges() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let client = world.spawn_empty().id();
        let card = world
            .spawn((
                DmItem {
                    path: "/obj/item/card/id/advanced/chameleon/elite".into(),
                    ..default()
                },
                DmName {
                    name: "agent card".into(),
                    desc: String::new(),
                },
                IdCard {
                    access: vec!["maint_tunnels".into()],
                },
            ))
            .id();
        let theirs = world
            .spawn(IdCard {
                access: vec!["maint_tunnels".into(), "bar".into()],
            })
            .id();
        copy_access(&mut world, client, card, theirs);
        let access = &world.get::<IdCard>(card).unwrap().access;
        assert!(access.contains(&"bar".to_owned()), "{access:?}");
        forge(&mut world, client, card, "Джон Доу", "Капитан");
        assert_eq!(
            world.get::<DmName>(card).unwrap().name,
            id_label("Джон Доу", "Капитан")
        );
        assert!(world.get::<AgentCard>(card).unwrap().forged);
        assert_eq!(
            world.get::<IdRegistration>(card).unwrap().registered_name,
            "Джон Доу"
        );
    }
}
