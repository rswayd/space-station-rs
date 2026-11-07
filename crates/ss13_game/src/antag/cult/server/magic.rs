//! Blood magic: carving spells and casting their auras.

use super::*;

/// `blood_magic/Activate`: the spell list (or a spell to remove first at the limit).
pub(super) fn prepare_magic(world: &mut World, client: Entity, mob: Entity) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    let empowered = near_empower_rune(world, mob);
    let limit = spell_limit(empowered);
    if cultist.spells.len() >= limit {
        if empowered {
            chat_to(
                world,
                mob,
                Span::CultItalic,
                format!("You cannot store more than {limit} spells. Pick a spell to remove."),
            );
        } else {
            chat_to(
                world,
                mob,
                Span::CultItalic,
                format!(
                    "You cannot store more than {RUNELESS_MAX_BLOODCHARGE} spells without an \
                     empowering rune! Pick a spell to remove."
                ),
            );
        }
        open_remove_spell(world, client, mob);
        return;
    }
    let mut items: Vec<String> = Spell::ALL.iter().map(|s| s.def().name.to_owned()).collect();
    items.push("(REMOVE SPELL)".into());
    let values = vec![mob; items.len()];
    open_list(
        world,
        client,
        mob,
        CultPrompt {
            kind: PromptKind::PrepareSpell { empowered },
            title: "Spell Choices".into(),
            message: "Blood spell to prepare".into(),
            items,
            values,
            spell_ids: Vec::new(),
        },
    );
}

pub(super) fn open_remove_spell(world: &mut World, client: Entity, mob: Entity) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    if cultist.spells.is_empty() {
        return;
    }
    let items: Vec<String> = cultist
        .spells
        .iter()
        .map(|s| s.spell.def().name.to_owned())
        .collect();
    let values = vec![mob; items.len()];
    let spell_ids = cultist.spells.iter().map(|s| s.id).collect();
    open_list(
        world,
        client,
        mob,
        CultPrompt {
            kind: PromptKind::RemoveSpell,
            title: "Current Spells".into(),
            message: "Spell to remove".into(),
            items,
            values,
            spell_ids,
        },
    );
}

pub(super) fn near_empower_rune(world: &mut World, mob: Entity) -> bool {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let mut q = world.query::<(&Rune, &TilePos)>();
    q.iter(world).any(|(r, p)| {
        r.kind == RuneKind::Empower
            && p.z == at.z
            && (p.x - at.x).abs() <= 1
            && (p.y - at.y).abs() <= 1
    })
}

pub(super) fn start_carving(
    world: &mut World,
    client: Entity,
    mob: Entity,
    spell: Spell,
    empowered: bool,
) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    // Revalidated: the rune may be gone, the slots filled meanwhile.
    if (empowered && !near_empower_rune(world, mob))
        || cultist.spells.len() >= spell_limit(empowered)
    {
        return;
    }
    if cultist.carving.is_some() {
        chat_to(
            world,
            mob,
            Span::CultItalic,
            "You are already invoking blood magic!",
        );
        return;
    }
    chat_to(
        world,
        mob,
        Span::Warning,
        "You begin to carve unnatural symbols into your flesh!",
    );
    let (delay, _) = carving(empowered);
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        c.carving = Some((spell, empowered));
    }
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    if !do_after(
        world,
        Actor { client, mob, item },
        mob,
        delay,
        0,
        Job::Cult(CultJob::Carve),
    ) && let Some(mut c) = world.get_mut::<Cultist>(mob)
    {
        c.carving = None;
    }
    refresh(world, mob);
}

pub(super) fn finish_carving(world: &mut World, mob: Entity) {
    let Some((spell, empowered)) = world
        .get_mut::<Cultist>(mob)
        .and_then(|mut c| c.carving.take())
    else {
        return;
    };
    let full = world
        .get::<Cultist>(mob)
        .is_some_and(|c| c.spells.len() >= spell_limit(empowered));
    if full {
        refresh(world, mob);
        return;
    }
    bleed(world, mob, carving(empowered).1);
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        let id = c.next_spell;
        c.next_spell += 1;
        c.spells.push(PreparedSpell {
            id,
            spell,
            charges: spell.def().charges,
        });
    }
    chat_to(
        world,
        mob,
        Span::Warning,
        format!(
            "Your wounds glow with power, you have prepared a {} invocation!",
            spell.def().name
        ),
    );
    refresh(world, mob);
}

pub(super) fn remove_spell(world: &mut World, mob: Entity, id: u32) {
    let aura = world.get::<Inventory>(mob).and_then(|i| {
        i.hands
            .iter()
            .flatten()
            .copied()
            .find(|&h| world.get::<BloodAura>(h).is_some_and(|a| a.spell == id))
    });
    if let Some(aura) = aura {
        despawn_aura(world, mob, aura);
    }
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        c.spells.retain(|s| s.id != id);
    }
    refresh(world, mob);
}

/// `blood_spell/Activate`: the aura into an empty hand, or back; the dagger summoned at once.
pub(super) fn activate_spell(world: &mut World, client: Entity, mob: Entity, id: u32) {
    let Some(spell) = world.get::<Cultist>(mob).and_then(|c| c.spell(id).copied()) else {
        return;
    };
    if spell.charges == 0 {
        return;
    }
    let def = spell.spell.def();
    if spell.spell == Spell::Emp {
        // `emp/Activate`: an EMP from the cultist (2 heavy, 5 light).
        whisper(world, mob, def.invocation);
        let Some(&pos) = world.get::<TilePos>(mob) else {
            return;
        };
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            "You speak the cursed words, emitting an EMP blast from your hand.",
            &format!("{name}'s hand flashes a bright blue!"),
            Span::Warning,
        );
        world.write_message(crate::emp::Empulse {
            epicenter: pos,
            heavy: 2,
            light: 5,
        });
        use_charge(world, mob, id);
        return;
    }
    let Some(aura_path) = def.aura else {
        // `dagger/Activate`.
        whisper(world, mob, def.invocation);
        let Some(&pos) = world.get::<TilePos>(mob) else {
            return;
        };
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            "Your plea for aid is answered, and light begins to shimmer and take form within your \
             hand!",
            &format!("{name}'s hand glows red for a moment."),
            Span::Warning,
        );
        if let Some(dagger) = crate::construction::spawn_obj(world, DAGGER, pos) {
            put_in_hands(world, mob, dagger);
        }
        use_charge(world, mob, id);
        return;
    };
    let held = world.get::<Inventory>(mob).and_then(|i| {
        i.hands
            .iter()
            .flatten()
            .copied()
            .find(|&h| world.get::<BloodAura>(h).is_some_and(|a| a.spell == id))
    });
    if let Some(aura) = held {
        despawn_aura(world, mob, aura);
        tell(
            world,
            client,
            Span::Warning,
            "You snuff out the spell, saving it for later.",
        );
        refresh(world, mob);
        return;
    }
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    let hand = if inventory.hands[inventory.active].is_none() {
        Some(inventory.active)
    } else {
        inventory.hands.iter().position(Option::is_none)
    };
    let Some(hand) = hand else {
        tell(
            world,
            client,
            Span::Warning,
            "You have no empty hand for invoking blood magic!",
        );
        return;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(aura) = crate::construction::spawn_obj(world, aura_path, pos) else {
        return;
    };
    world.entity_mut(aura).remove::<TilePos>().insert((
        CarriedBy(mob),
        NoDrop,
        BloodAura {
            owner: mob,
            spell: id,
        },
    ));
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.hands[hand] = Some(aura);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Your wounds glow as you invoke the {}.", def.name),
    );
    refresh(world, mob);
}

pub(super) fn put_in_hands(world: &mut World, mob: Entity, item: Entity) {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    if let Some(hand) = inventory.hands.iter().position(Option::is_none) {
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
            inv.hands[hand] = Some(item);
        }
    }
}

pub(super) fn despawn_aura(world: &mut World, mob: Entity, aura: Entity) {
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        for h in &mut inv.hands {
            if *h == Some(aura) {
                *h = None;
            }
        }
    }
    if world.get_entity(aura).is_ok() {
        world.despawn(aura);
    }
}

/// Every aura in the cultist's hands goes away (deconversion, holy water).
pub(super) fn drop_auras(world: &mut World, mob: Entity) {
    let auras: Vec<Entity> = world
        .get::<Inventory>(mob)
        .map(|i| {
            i.hands
                .iter()
                .flatten()
                .copied()
                .filter(|&h| world.get::<BloodAura>(h).is_some())
                .collect()
        })
        .unwrap_or_default();
    for aura in auras {
        despawn_aura(world, mob, aura);
    }
}

pub(super) fn held_aura(world: &World, mob: Entity, spell: Spell) -> Option<Entity> {
    let cultist = world.get::<Cultist>(mob)?;
    world
        .get::<Inventory>(mob)?
        .hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| {
            world
                .get::<BloodAura>(h)
                .and_then(|a| cultist.spell(a.spell))
                .is_some_and(|s| s.spell == spell)
        })
}

/// One charge used; the spell goes when it has none left.
pub(super) fn use_charge(world: &mut World, mob: Entity, id: u32) {
    let left = world.get_mut::<Cultist>(mob).and_then(|mut c| {
        let s = c.spells.iter_mut().find(|s| s.id == id)?;
        s.charges = s.charges.saturating_sub(1);
        Some(s.charges)
    });
    if left == Some(0)
        && let Some(mut c) = world.get_mut::<Cultist>(mob)
    {
        c.spells.retain(|s| s.id != id);
    }
    refresh(world, mob);
}

/// `cast_spell` after the effect: the health cost to the hand's arm, a charge, the aura gone
/// when empty.
pub(super) fn spend_aura(world: &mut World, mob: Entity, aura: Entity) {
    let Some(a) = world.get::<BloodAura>(aura).copied() else {
        return;
    };
    let Some(spell) = world
        .get::<Cultist>(mob)
        .and_then(|c| c.spell(a.spell).copied())
    else {
        return;
    };
    let def = spell.spell.def();
    if def.health_cost > 0.0 {
        let zone = active_arm(world, mob);
        hurt(world, mob, Some(zone), def.health_cost);
    }
    if spell.charges <= 1 {
        despawn_aura(world, mob, aura);
    }
    use_charge(world, mob, a.spell);
}

/// An aura used on `target` (`cast_spell`).
pub(super) fn cast_aura(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    aura: BloodAura,
    target: Entity,
) {
    if aura.owner != mob || world.get::<Cultist>(mob).is_none() {
        despawn_aura(world, mob, item);
        return;
    }
    let Some(spell) = world
        .get::<Cultist>(mob)
        .and_then(|c| c.spell(aura.spell).copied())
    else {
        despawn_aura(world, mob, item);
        return;
    };
    match spell.spell {
        Spell::Stun => cast_stun(world, mob, item, target),
        Spell::Teleport => {
            if world.get::<Cultist>(target).is_none() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "You can only teleport cultists with this spell!",
                );
                return;
            }
            let (items, values) = teleport_runes(world, None);
            if items.is_empty() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "There are no valid runes to teleport to!",
                );
                return;
            }
            open_list(
                world,
                client,
                mob,
                CultPrompt {
                    kind: PromptKind::Teleport {
                        from_rune: None,
                        target,
                    },
                    title: "Teleportation Target".into(),
                    message: "Rune to teleport to".into(),
                    items,
                    values,
                    spell_ids: Vec::new(),
                },
            );
        }
        Spell::Shackles => cast_shackles(world, client, mob, item, target),
        Spell::Construction => cast_construction(world, client, mob, item, target),
        Spell::Dagger | Spell::Emp => {}
    }
}

/// `blood_magic/stun/cast_spell`.
pub(super) fn cast_stun(world: &mut World, mob: Entity, aura: Entity, target: Entity) {
    if world.get::<Health>(target).is_none() || world.get::<Cultist>(target).is_some() {
        return;
    }
    whisper(world, mob, Spell::Stun.def().invocation);
    let (risen, ascendent) = {
        let t = world.resource::<CultTeam>();
        (t.cult_risen, t.cult_ascendent)
    };
    let coef = stun_coef(risen, ascendent);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!(
            "You attempt to stun {} with the spell!",
            name_of(world, target)
        ),
        &format!("{user} holds up their hand, which explodes in a flash of red light!"),
        Span::Warning,
    );
    if has_holy_water(world, target) {
        chat_to(world, mob, Span::Warning, "The spell had no effect!");
    } else {
        let victim = name_of(world, target);
        chat_to(
            world,
            mob,
            Span::CultItalic,
            format!("In a brilliant flash of red, {victim} falls to the ground!"),
        );
        crate::status_effects::apply(world, target, Incap::Paralyze, 16.0 * coef);
        // A carbon is silenced, stutters, slurs and jitters.
        if world
            .get::<crate::medical::organs::OrganHost>(target)
            .is_some()
        {
            use crate::status_effects::{SlurKind, Timed, adjust_timed, set_timed};
            adjust_timed(world, target, Timed::Silence, 12.0 * coef, None);
            adjust_timed(world, target, Timed::Stutter, 30.0 * coef, None);
            adjust_timed(
                world,
                target,
                Timed::Slurring(SlurKind::Cult),
                30.0 * coef,
                None,
            );
            set_timed(world, target, Timed::Jitter, 30.0 * coef, true);
        }
    }
    let t = now(world);
    world.entity_mut(mob).insert(NextMove(t + CLICK_CD_MELEE));
    spend_aura(world, mob, aura);
}

/// `blood_magic/shackles/cast_spell`: a 3 s `do_after`, then shadow shackles.
pub(super) fn cast_shackles(
    world: &mut World,
    client: Entity,
    mob: Entity,
    aura: Entity,
    target: Entity,
) {
    if world.get::<HumanLook>(target).is_none() || world.get::<Cultist>(target).is_some() {
        return;
    }
    let victim = name_of(world, target);
    if world.get::<crate::restraints::Handcuffed>(target).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            format!("{victim} is already bound."),
        );
        return;
    }
    whisper(world, mob, Spell::Shackles.def().invocation);
    let user = name_of(world, mob);
    visible(
        world,
        target,
        &format!("{user} begins shaping dark magic shackles around your wrists!"),
        &format!("{user} begins restraining {victim} with dark magic!"),
        Span::Danger,
    );
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(aura),
        },
        target,
        30.0,
        0,
        Job::Cult(CultJob::Shackle),
    );
}

pub(super) fn finish_shackle(world: &mut World, mob: Entity, action: &DoAfter) {
    let target = action.target;
    let Some(aura) = action.item.filter(|&a| world.get::<BloodAura>(a).is_some()) else {
        return;
    };
    let victim = name_of(world, target);
    if world.get::<crate::restraints::Handcuffed>(target).is_some() {
        chat_to(
            world,
            mob,
            Span::Warning,
            format!("{victim} is already bound."),
        );
        return;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let Some(cuffs) = crate::construction::spawn_obj(world, SHACKLES, pos) else {
        return;
    };
    crate::restraints::apply_cuffs(world, cuffs, target);
    crate::status_effects::adjust_timed(
        world,
        target,
        crate::status_effects::Timed::Silence,
        10.0,
        None,
    );
    chat_to(world, mob, Span::Notice, format!("You shackle {victim}."));
    spend_aura(world, mob, aura);
}

/// `blood_magic/construction/cast_spell`: plasteel into runed metal.
pub(super) fn cast_construction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    aura: Entity,
    target: Entity,
) {
    let path = path_of(world, target);
    if path.as_deref().is_some_and(|p| p.starts_with(PLASTEEL)) {
        let Some(&pos) = world.get::<TilePos>(target).or(world.get::<TilePos>(mob)) else {
            return;
        };
        let amount = world
            .get::<crate::tools::Stack>(target)
            .map_or(1, |s| s.amount);
        whisper(world, mob, Spell::Construction.def().invocation);
        if world.get::<CarriedBy>(target).is_some()
            && let Some(mut inv) = world.get_mut::<Inventory>(mob)
        {
            for h in &mut inv.hands {
                if *h == Some(target) {
                    *h = None;
                }
            }
        }
        world.despawn(target);
        crate::construction::spawn_stack(world, RUNED_METAL, pos, amount);
        tell(
            world,
            client,
            Span::Warning,
            "A dark cloud emanates from you hand and swirls around the plasteel, transforming it \
             into runed metal!",
        );
        spend_aura(world, mob, aura);
        return;
    }
    let name = name_of(world, target);
    tell(
        world,
        client,
        Span::Warning,
        format!("The spell will not work on {name}!"),
    );
}
