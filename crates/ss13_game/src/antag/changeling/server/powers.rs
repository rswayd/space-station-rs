//! The bought and other powers, and the emporium.

use super::*;

/// `regenerate/sting_action`.
pub(super) fn regenerate(world: &mut World, mob: Entity) -> bool {
    chat_to(
        world,
        mob,
        Span::Notice,
        "Вы чувствуете зуд, как внутри, так и снаружи, когда ваши ткани плоти вяжутся и \
         перевязываются.",
    );
    heal_body(world, mob);
    true
}

/// `adrenaline/sting_action`: up at once, stamina reset, four units of changeling adrenaline.
pub(super) fn adrenaline(world: &mut World, mob: Entity) -> bool {
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            effects.set(kind, 0.0);
        }
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    if let (Some(chem), Some(mut reagents)) = (
        world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone()),
        world.get::<crate::chem::MobReagents>(mob).cloned(),
    ) && let Some(id) = chem.id(ADRENALINE)
    {
        reagents
            .blood
            .add_reagent(&chem, id, 4.0, 310.15, None, None, false, false);
        world.entity_mut(mob).insert(reagents);
    }
    chat_to(
        world,
        mob,
        Span::Changeling,
        "The staggering rush of a stimulant honed precisely to our biology is INVIGORATING. We \
         will not be subdued.",
    );
    true
}

/// `fleshmend/sting_action`: 10 s of healing (`/datum/status_effect/fleshmend`).
pub(super) fn fleshmend(world: &mut World, client: Entity, mob: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world
        .get::<Changeling>(mob)
        .is_some_and(|c| c.fleshmend_until > now)
    {
        tell(world, client, Span::Warning, "уже лечимся!");
        return false;
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.fleshmend_until = now + FLESHMEND_SECS;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Мы начинаем быстро выздоравливать.",
    );
    true
}

/// Spawns `path` into the mob's possession, stuck to it.
pub(super) fn grow(world: &mut World, mob: Entity, path: &str) -> Option<Entity> {
    let pos = *world.get::<TilePos>(mob)?;
    let item = crate::construction::spawn_obj(world, path, pos)?;
    world
        .entity_mut(item)
        .remove::<TilePos>()
        .insert((CarriedBy(mob), NoDrop));
    Some(item)
}

pub(super) fn held_of(world: &World, mob: Entity, path: &str) -> Vec<Entity> {
    world
        .get::<Inventory>(mob)
        .map(|i| {
            i.hands
                .iter()
                .flatten()
                .copied()
                .filter(|&e| crate::construction::path_of(world, e).as_deref() == Some(path))
                .collect()
        })
        .unwrap_or_default()
}

/// `weapon/unequip_held`: the blade goes back into the arm. True when one was held.
pub(super) fn retract_blade(world: &mut World, mob: Entity) -> bool {
    let blades = held_of(world, mob, ARM_BLADE);
    if blades.is_empty() {
        return false;
    }
    for blade in blades {
        world.despawn(blade);
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Мы ассимилируем blade обратно в наше тело.",
        &format!("С отвратительным хрустом, {user} превращает blade в руку!"),
        Span::Warning,
    );
    true
}

/// `weapon/sting_action`: the active hand drops what it holds and becomes the blade.
pub(super) fn form_blade(world: &mut World, client: Entity, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    if let Some(held) = inventory.active_item() {
        if no_drop(world, held) {
            tell(world, client, Span::Warning, "рука занята!");
            return false;
        }
        world.entity_mut(held).remove::<CarriedBy>().insert(pos);
    }
    let Some(blade) = grow(world, mob, ARM_BLADE) else {
        return false;
    };
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        let hand = inventory.active;
        inventory.hands[hand] = Some(blade);
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Наша рука скручивается и мутирует, превращаясь в смертоносный клинок.",
        &format!("Страшный клинок формируется вокруг руки {user}!"),
        Span::Warning,
    );
    true
}

/// `suit/check_suit`: the chitin is cast off. True when it was worn.
pub(super) fn cast_off_armor(world: &mut World, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let is = |slot: Option<Entity>, path: &str| {
        slot.filter(|&e| crate::construction::path_of(world, e).as_deref() == Some(path))
    };
    let (suit, helmet) = (
        is(inventory.suit, LING_ARMOR),
        is(inventory.head, LING_HELMET),
    );
    if suit.is_none() && helmet.is_none() {
        return false;
    }
    for item in [suit, helmet].into_iter().flatten() {
        world.despawn(item);
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.chem_recharge_slowdown -= ARMOR_SLOWDOWN;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Мы сбрасываем нашу armor.",
        &format!("{user} сбрасывает свой armor!"),
        Span::Warning,
    );
    true
}

/// `suit/sting_action`: whatever is worn there falls, the chitin grows in its place.
pub(super) fn form_armor(world: &mut World, client: Entity, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    if inventory.suit.is_some_and(|e| no_drop(world, e)) {
        tell(world, client, Span::Warning, "слот тела занят!");
        return false;
    }
    if inventory.head.is_some_and(|e| no_drop(world, e)) {
        tell(world, client, Span::Warning, "слот головы занят!");
        return false;
    }
    for (slot, worn) in [
        (InventorySlot::Suit, inventory.suit),
        (InventorySlot::Head, inventory.head),
    ] {
        if let Some(worn) = worn {
            unequip(world, mob, pos, slot);
            world.entity_mut(worn).remove::<CarriedBy>().insert(pos);
        }
    }
    let (Some(suit), Some(helmet)) = (grow(world, mob, LING_ARMOR), grow(world, mob, LING_HELMET))
    else {
        return false;
    };
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.suit = Some(suit);
        inventory.head = Some(helmet);
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.chem_recharge_slowdown += ARMOR_SLOWDOWN;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Мы закаляем свою плоть, создавая броню!",
        &format!("Плоть {user} чернеет, быстро превращаясь в твердую, хитиновую массу!"),
        Span::Warning,
    );
    true
}

/// The bought powers' `Remove()`: the blade and the chitin go.
pub(super) fn remove_powers(world: &mut World, mob: Entity) {
    retract_blade(world, mob);
    cast_off_armor(world, mob);
}

// ---------------------------------------------------------------------------------------------
// The emporium

impl ServerUi for CellularEmporiumUi {
    type Source = (&'static Changeling, &'static EmporiumCatalog);
    type UserParam = ();
    type Act = EmporiumAct;

    fn static_data((_, catalog): (&Changeling, &EmporiumCatalog)) -> EmporiumStatic {
        EmporiumStatic {
            abilities: catalog.0.clone(),
        }
    }

    fn data((ling, _): (&Changeling, &EmporiumCatalog), _: Entity, _: &()) -> EmporiumData {
        EmporiumData {
            can_readapt: ling.can_respec,
            genetic_points_count: ling.genetic_points,
            owned_abilities: ling
                .purchased
                .iter()
                .map(|p| p.def().path.to_owned())
                .collect(),
            absorb_count: ling.true_absorbs,
            dna_count: ling.absorbed_count,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<EmporiumAct, UiReject> {
        match action {
            READAPT => Ok(EmporiumAct::Readapt),
            EVOLVE => params
                .get("path")
                .and_then(Power::from_path)
                .map(EmporiumAct::Evolve)
                .ok_or_else(|| UiReject::BadParams("path: not a changeling power".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmporiumAct {
    Evolve(Power),
    Readapt,
}

/// `ui_act` of the emporium: `purchase_power` and `readapt`, checked at the moment of effect.
pub(super) fn emporium_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<CellularEmporiumUi>>>,
) {
    let pending: Vec<(Entity, Entity, EmporiumAct)> = acts
        .read(world.resource::<Messages<UiActed<CellularEmporiumUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, source, act) in pending {
        if actor != source || world.get::<Changeling>(source).is_none_or(|c| c.fallen) {
            continue;
        }
        match act {
            EmporiumAct::Evolve(power) => purchase(world, source, power),
            EmporiumAct::Readapt => readapt(world, source),
        }
    }
}

/// `purchase_power` + `give_power`.
pub fn purchase(world: &mut World, mob: Entity, power: Power) {
    let Some(result) = world
        .get::<Changeling>(mob)
        .map(|c| c.check_purchase(power))
    else {
        return;
    };
    match result {
        Ok(cost) => {
            if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
                ling.genetic_points -= cost;
                ling.purchased.push(power);
            }
            info!("{mob} adapted {power:?} (changeling)");
            refresh(world, mob);
        }
        Err(text) => chat_to(world, mob, Span::Warning, text),
    }
}

/// `readapt()`: every bought power goes, the points come back, one respec spent.
pub fn readapt(world: &mut World, mob: Entity) {
    let Some(ling) = world.get::<Changeling>(mob).cloned() else {
        return;
    };
    if world.get::<HumanLook>(mob).is_none() {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Мы не можем избавиться от нашего развития в такой форме!",
        );
        return;
    }
    if ling.in_stasis() {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Мы слишком заняты реформированием себя, чтобы переадаптироваться прямо сейчас!",
        );
        return;
    }
    if ling.can_respec == 0 {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Вам не хватает силы, чтобы переадаптировать ваше развитие!",
        );
        return;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Мы избавились от нашего развития в этой форме и теперь готовы к повторной адаптации.",
    );
    remove_powers(world, mob);
    // `void_adaption/Remove` while protecting.
    if ling.void_active {
        chat_to(
            world,
            mob,
            Span::Changeling,
            "Наши клетки расслабляются, несмотря на опасность!",
        );
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.remove_purchased();
        ling.can_respec -= 1;
    }
    world.entity_mut(mob).remove::<crate::click::StingReady>();
    info!("{mob} readapted (changeling)");
    refresh(world, mob);
}
