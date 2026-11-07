//! Transforming and regenerative stasis.

use super::*;

/// `transform/sting_action` -> `select_dna`: the profiles to pick from.
pub(super) fn open_transform(world: &mut World, client: Entity, mob: Entity) -> bool {
    world.write_message(OpenUi::new::<TransformListWindow>(
        client,
        mob,
        mob,
        "Трансформация",
    ));
    false
}

impl ServerUi for TransformListWindow {
    type Source = &'static Changeling;
    type UserParam = ();
    type Act = Option<String>;

    fn static_data(_: &Changeling) {}

    fn data(ling: &Changeling, _: Entity, _: &()) -> ListInputData {
        let items: Vec<String> = ling.profiles.iter().map(|p| p.name.clone()).collect();
        ListInputData {
            title: "Трансформация".into(),
            message: "Выберите облик.".into(),
            init_value: items.first().cloned().unwrap_or_default(),
            items,
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| Some(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected a profile".into())),
            "cancel" => Ok(None),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The pick: checked again at the moment of effect, then `transform(user, profile)`.
pub(super) fn transform_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<TransformListWindow>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, Entity, Option<String>)> = acts
        .read(world.resource::<Messages<UiActed<TransformListWindow>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (window, client, actor, source, pick) in pending {
        if let Some(mut window) = world.get_mut::<ss13_tgui::server::UiWindow>(window) {
            window.cap_status(ss13_shared::ui::UiStatus::Close);
        }
        let Some(pick) = pick else {
            continue;
        };
        if actor != source || !checked(world, client, actor, Power::Transform) {
            continue;
        }
        let Some(profile) = world
            .get::<Changeling>(actor)
            .and_then(|c| c.profiles.iter().find(|p| p.name == pick).cloned())
        else {
            continue;
        };
        transform(world, actor, &profile);
        spend(world, actor, Power::Transform);
    }
}

/// `/datum/antagonist/changeling/transform`: the name and the body of `profile`; worn items and
/// the ID stay.
pub fn transform(world: &mut World, mob: Entity, profile: &Profile) {
    world
        .entity_mut(mob)
        .insert(Name::new(profile.name.clone()));
    if let (Some(look), Some(mut body)) = (&profile.look, world.get_mut::<HumanLook>(mob)) {
        *body = HumanLook {
            uniform: body.uniform.clone(),
            shoes: body.shoes.clone(),
            husk: false,
            missing: body.missing,
            robotic: body.robotic,
            eyeless: body.eyeless,
            ..look.clone()
        };
    }
    // `transform` copies the profile's species too (`hardset_dna(..., mrace)`).
    if let Some(look) = &profile.look
        && look.species != crate::species::species_of(world, mob)
    {
        crate::species::set_species(world, mob, look.species);
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    info!("{mob} transformed into {:?} (changeling)", profile.name);
}

// ---------------------------------------------------------------------------------------------
// Stasis

/// `fakedeath/sting_action`: a ready revive revives; otherwise fake death starts.
pub(super) fn fakedeath(world: &mut World, mob: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let stasis = world.get::<Changeling>(mob).map(|c| c.stasis);
    if let Some(Stasis::Ready { .. }) = stasis {
        revive(world, mob);
        // The revive costs nothing (`chemical_cost = 0` while ready).
        refresh(world, mob);
        return false;
    }
    let died = world.get::<Stat>(mob) == Some(&Stat::Dead);
    // `fakedeath(CHANGELING_TRAIT)`: the deathgasp comes with the stat (`crate::health`).
    world.entity_mut(mob).insert((Stat::Dead, DeathComa));
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.stasis = Stasis::Waiting {
            ready_at: now + FAKEDEATH_SECS,
            died,
        };
    }
    chat_to(
        world,
        mob,
        Span::Changeling,
        "Мы начинаем стазис, подготавливая энергию, чтобы восстать вновь.",
    );
    true
}

/// `fakedeath/revive`: `revive(HEAL_DAMAGE | HEAL_BODY | HEAL_STATUS | HEAL_CC_STATUS)`.
pub(super) fn revive(world: &mut World, mob: Entity) {
    if world.get::<DeathComa>(mob).is_none() {
        return;
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.stasis = Stasis::None;
    }
    world.entity_mut(mob).remove::<DeathComa>();
    heal_body(world, mob);
    if let Some(health) = world.get::<Health>(mob).cloned() {
        world.entity_mut(mob).insert(health.healed());
    }
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
    world.entity_mut(mob).insert(Stat::Conscious);
    if let Some(mut alerts) = world.get_mut::<crate::breath::Alerts>(mob) {
        alerts.0.clear();
    }
    chat_to(world, mob, Span::Changeling, "Мы возродили себя.");
    info!("{mob} revived from stasis (changeling)");
}

/// `fully_heal(HEAL_BODY)`: organs, limbs, the blood and the wounds.
pub(super) fn heal_body(world: &mut World, mob: Entity) {
    crate::medical::organs::regenerate(world, mob);
    crate::medical::bodyparts::regenerate_limbs(world, mob);
    if let Some(mut blood) = world.get_mut::<Blood>(mob)
        && blood.volume < BLOOD_VOLUME_NORMAL
    {
        blood.volume = BLOOD_VOLUME_NORMAL;
    }
    if let Some(mut limbs) = world.get_mut::<crate::medical::wounds::Limbs>(mob)
        && limbs.parts.iter().any(|l| !l.wounds.is_empty())
    {
        for limb in &mut limbs.parts {
            limb.wounds.clear();
        }
    }
}
