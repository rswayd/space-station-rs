use super::*;

pub(super) fn tell_opt(
    world: &mut World,
    mob: Option<Entity>,
    span: Span,
    text: impl Into<String>,
) {
    if let Some(mob) = mob {
        tell_mob(world, mob, span, text);
    }
}

pub(crate) fn sound_at(world: &mut World, e: Entity, sound: &str, vol: f32, extrarange: f32) {
    if let Some(pos) = crate::chem::map_pos(world, e) {
        playsound(
            world,
            PlaySound::new(pos, sound, vol, true).extrarange(extrarange),
        );
    }
}

/// The scanner buzz of every refusal.
pub(crate) fn buzz(world: &mut World, control: Entity) {
    sound_at(world, control, SCANBUZZ, 25.0, SILENCED_SOUND_EXTRARANGE);
}

pub(super) fn refuse(world: &mut World, control: Entity, user: Option<Entity>, text: &str) {
    tell_opt(world, user, Span::Warning, text);
    buzz(world, control);
}

pub(crate) fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

/// tg `allowed(user)`: the suit's `req_access` against the access of the ID tg's
/// `get_idcard(hand_first = TRUE)` finds.
pub fn allowed(world: &World, control: Entity, user: Entity) -> bool {
    let Some(suit) = world.get::<ModSuit>(control) else {
        return false;
    };
    crate::inventory::id_allowed(world, user, &suit.req_access, &[])
}

/// The core's cell, if still inside it.
pub(super) fn cell_of(world: &World, control: Entity) -> Option<Entity> {
    let core = world.get::<ModSuit>(control)?.core?;
    let cell = world.get::<ModCore>(core)?.cell?;
    world
        .get::<StoredIn>(cell)
        .is_some_and(|s| s.0 == core)
        .then_some(cell)
}

pub(super) fn core_state(world: &World, control: Entity) -> Option<ModCore> {
    let core = world.get::<ModSuit>(control)?.core?;
    world.get::<ModCore>(core).copied()
}

/// tg `get_charge()`.
pub fn charge(world: &World, control: Entity) -> f64 {
    match core_state(world, control) {
        Some(c) if c.infinite => f64::INFINITY,
        Some(ModCore {
            plasma: Some((charge, _)),
            ..
        }) => charge,
        Some(_) => cell_of(world, control)
            .and_then(|c| world.get::<ItemCell>(c))
            .map_or(0.0, |c| c.0.charge),
        None => 0.0,
    }
}

/// tg `get_max_charge()` (1 to avoid dividing by zero).
pub fn max_charge(world: &World, control: Entity) -> f64 {
    match core_state(world, control) {
        Some(c) if c.infinite => f64::INFINITY,
        Some(ModCore {
            plasma: Some((_, max)),
            ..
        }) => max.max(1.0),
        Some(_) => cell_of(world, control)
            .and_then(|c| world.get::<ItemCell>(c))
            .map_or(1.0, |c| c.0.maxcharge),
        None => 1.0,
    }
}

/// tg `check_charge(amount)`.
pub fn check_charge(world: &World, control: Entity, amount: f64) -> bool {
    charge(world, control) >= amount
}

/// tg `subtract_charge(amount)`: the cell's `use(amount, force = TRUE)`.
pub fn subtract_charge(world: &mut World, control: Entity, amount: f64) -> bool {
    match core_state(world, control) {
        Some(c) if c.infinite => true,
        Some(ModCore {
            plasma: Some((charge, _)),
            ..
        }) => {
            // `amount = min(amount, charge); charge -= amount`.
            let taken = amount.max(0.0).min(charge);
            if let Some(core) = world.get::<ModSuit>(control).and_then(|s| s.core)
                && let Some(mut c) = world.get_mut::<ModCore>(core)
                && let Some((now, _)) = c.plasma.as_mut()
            {
                *now -= taken;
            }
            taken > 0.0
        }
        Some(_) => {
            let Some(cell) = cell_of(world, control) else {
                return false;
            };
            world
                .get_mut::<ItemCell>(cell)
                .is_some_and(|mut c| c.0.take(amount, true) > 0.0)
        }
        None => false,
    }
}

/// tg `add_charge(amount)`: the cell's `give`, the plasma core's own charge. What went in.
pub fn add_charge(world: &mut World, control: Entity, amount: f64) -> f64 {
    match core_state(world, control) {
        Some(c) if c.infinite => 0.0,
        Some(ModCore {
            plasma: Some((charge, max)),
            ..
        }) => {
            let given = amount.min(max - charge).max(0.0);
            if let Some(core) = world.get::<ModSuit>(control).and_then(|s| s.core)
                && let Some(mut c) = world.get_mut::<ModCore>(core)
                && let Some((now, _)) = c.plasma.as_mut()
            {
                *now += given;
            }
            given
        }
        Some(_) => {
            let Some(cell) = cell_of(world, control) else {
                return 0.0;
            };
            world
                .get_mut::<ItemCell>(cell)
                .map_or(0.0, |mut c| c.0.give(amount))
        }
        None => 0.0,
    }
}

pub(super) fn chargebar(world: &World, control: Entity) -> (String, String) {
    let Some(core) = core_state(world, control) else {
        return ("transparent".into(), "No Core Detected".into());
    };
    if core.infinite {
        return ("teal".into(), "Infinite".into());
    }
    if core.ethereal {
        return ("transparent".into(), "Biological Battery Missing".into());
    }
    if core.plasma.is_none() && (!core.standard || cell_of(world, control).is_none()) {
        return ("transparent".into(), "Power Cell Missing".into());
    }
    let (now, max) = (charge(world, control), max_charge(world, control));
    let ratio = ((now / max) * 100.0).round() / 100.0;
    let color = if core.plasma.is_some() {
        if ratio < 0.33 { "bad" } else { "purple" }
    } else if ratio < 0.33 {
        "bad"
    } else if ratio < 0.66 {
        "average"
    } else {
        "good"
    };
    let text = format!(
        "{} of {} ({}%)",
        display_energy(now),
        display_energy(max),
        (100.0 * now / max).round()
    );
    (color.into(), text)
}

/// Every control unit `mob` carries (hands and slots).
pub(super) fn carried_suits(world: &World, mob: Entity) -> Vec<Entity> {
    // The AI assistant's eye: the unit it is in (tg grants it the `ai_action`s).
    if let Some(control) = crate::modsuit::ai::suit_of_eye(world, mob) {
        return vec![control];
    }
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return Vec::new();
    };
    ALL_SLOTS
        .iter()
        .filter_map(|&s| inventory.slot(s))
        .filter(|&e| world.get::<ModSuit>(e).is_some())
        .collect()
}

/// `mob` carries the unit, or is its AI assistant's eye (tg `ai_action`s, `contained_state`).
pub(crate) fn controls(world: &World, mob: Entity, control: Entity) -> bool {
    world.get::<CarriedBy>(control).is_some_and(|c| c.0 == mob)
        || crate::modsuit::ai::suit_of_eye(world, mob) == Some(control)
}

/// Whose stat counts for `actor`: an AI eye's AI, else the actor itself.
pub(crate) fn body_of(world: &World, actor: Entity) -> Entity {
    world
        .get::<crate::silicon::ai::AiEye>(actor)
        .map_or(actor, |e| e.ai)
}

/// The unit `mob` wears.
pub fn worn_suit(world: &World, mob: Entity) -> Option<Entity> {
    carried_suits(world, mob).into_iter().find(|&e| {
        world
            .get::<ModSuit>(e)
            .is_some_and(|s| s.wearer == Some(mob))
    })
}

/// tg `install(new_module, user)`: incompatibility, complexity, required parts. True when it
/// went in.
pub fn install(world: &mut World, control: Entity, module: Entity, user: Option<Entity>) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(new) = modules::def_of(world, module) else {
        return false;
    };
    let theme = theme_of(world, &suit);
    for &old in &suit.modules {
        let Some(old_def) = modules::def_of(world, old) else {
            continue;
        };
        let clash = new.incompatible.iter().any(|t| is_under(&old_def.path, t))
            || old_def.incompatible.iter().any(|t| is_under(&new.path, t));
        if clash {
            if user.is_some() {
                let text = format!("несовместим с {}!", name(world, old));
                refuse(world, control, user, &text);
            }
            return false;
        }
    }
    if suit.complexity + new.complexity > theme.complexity_max {
        if user.is_some() {
            refuse(world, control, user, "превышена максимальная сложность!");
        }
        return false;
    }
    if !modules::has_required_parts(&new, suit.part_slots(&theme, false)) {
        if user.is_some() {
            refuse(world, control, user, "нет нужных частей!");
        }
        return false;
    }
    // `finish_install`.
    if let Some(u) = user {
        let inventory = world.get::<Inventory>(u).cloned();
        if let Some(slot) = inventory.and_then(|i| i.slot_of(module)) {
            *world
                .get_mut::<Inventory>(u)
                .expect("checked above")
                .slot_mut(slot) = None;
        }
    }
    world
        .entity_mut(module)
        .remove::<(CarriedBy, TilePos, StoredIn)>()
        .insert(InSuit(control));
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.modules.push(module);
        s.complexity += new.complexity;
    }
    modules::on_install(world, control, module);
    if suit.active && modules::has_required_parts(&new, suit.part_slots(&theme, true)) {
        modules::on_part_activation(world, control, module);
        if let Some(mut m) = world.get_mut::<ModModule>(module) {
            m.part_activated = true;
        }
    }
    if let Some(u) = user {
        let text = format!("{} добавлен", name(world, module));
        tell_mob(world, u, Span::Notice, text);
        sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
        refresh_buttons(world, suit.holder);
    }
    true
}

/// tg `uninstall(old_module)`: the module is out of the suit, nowhere (the caller places it).
pub fn uninstall(world: &mut World, control: Entity, module: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let complexity = modules::def_of(world, module).map_or(0, |d| d.complexity);
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.modules.retain(|&m| m != module);
        s.complexity -= complexity;
        s.pins.retain(|&(_, m)| m != module);
        if s.selected == Some(module) {
            s.selected = None;
        }
    }
    if suit.active {
        let (part_activated, active) = world
            .get::<ModModule>(module)
            .map_or((false, false), |m| (m.part_activated, m.active));
        if part_activated {
            modules::on_part_deactivation(world, control, module);
        }
        if active {
            modules::deactivate(world, control, module, true);
        }
    }
    if let Some(mut m) = world.get_mut::<ModModule>(module) {
        m.part_activated = false;
        m.active = false;
    }
    modules::on_uninstall(world, control, module);
    world.entity_mut(module).remove::<InSuit>();
    refresh_buttons(world, suit.holder);
}

/// Where things the suit lets go of land (tg `drop_location()`).
pub(super) fn drop_pos(world: &World, control: Entity) -> Option<TilePos> {
    crate::chem::map_pos(world, control)
}

/// A detached item back into the world: on `pos`.
pub(super) fn place_on(world: &mut World, item: Entity, pos: TilePos) {
    world.entity_mut(item).remove::<InSuit>();
    crate::storage::take_out(world, item, pos);
}

/// tg `set_wearer`: modules see it; `pre_equipped` pins its default modules once per wearer.
pub(super) fn set_wearer(world: &mut World, control: Entity, mob: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let defaults = world.resource::<ModDb>().data.suits[suit.suit]
        .default_pins
        .clone();
    let mut pins = Vec::new();
    for &module in &suit.modules {
        let Some(def) = modules::def_of(world, module) else {
            continue;
        };
        if defaults.contains(&def.path) && !suit.pinned_once.contains(&(module, mob)) {
            pins.push(module);
        }
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.wearer = Some(mob);
        for &m in &pins {
            s.pinned_once.push((m, mob));
            if !s.pins.contains(&(mob, m)) {
                s.pins.push((mob, m));
            }
        }
    }
}

/// tg `clean_up` + `unset_wearer`: active modules off, parts unsealed and retracted at once,
/// the unit off.
pub(super) fn clean_up(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if suit.active || suit.activating {
        for &m in &suit.modules {
            if world.get::<ModModule>(m).is_some_and(|m| m.active) {
                modules::deactivate(world, control, m, false);
            }
        }
        for i in 0..suit.parts.len() {
            seal_part(world, control, i, false);
        }
    }
    for i in 0..suit.parts.len() {
        do_retract(world, control, i, None);
    }
    if suit.active {
        control_activation(world, control, false);
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.steps.clear();
        s.activating = false;
        s.wearer = None;
    }
    if let Some(old) = suit.wearer
        && world.get_entity(old).is_ok()
    {
        world
            .entity_mut(old)
            .remove::<(SuitSlowdown, ModuleSelected, modules::NoSlip)>()
            .remove::<(modules::Jetpack, modules::VisorHud, modules::FastCuffing)>();
    }
}
