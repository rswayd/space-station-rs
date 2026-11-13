//! Invoking runes, and Nar'Sie's round end countdown.

use super::*;

/// An empty hand on a rune (`attack_hand`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(rune) = world.get::<Rune>(target).cloned() else {
        return false;
    };
    if world.get::<Cultist>(mob).is_none() {
        let name = name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("You aren't able to understand the words of {name}."),
        );
        return true;
    }
    let invokers = can_invoke(world, target, mob, rune.kind);
    let req = rune.kind.def().req_cultists;
    if invokers.len() >= req {
        invoke(world, client, target, &invokers);
    } else {
        tell(
            world,
            client,
            Span::Danger,
            format!(
                "You need {} more adjacent cultists to use this rune in such a manner.",
                req - invokers.len()
            ),
        );
        fail_invoke(world, target);
    }
    true
}

/// `can_invoke`: the user, then (for multi-invoker runes and Offer) every conscious cultist
/// next to the rune.
pub fn can_invoke(world: &mut World, rune: Entity, user: Entity, kind: RuneKind) -> Vec<Entity> {
    let mut invokers = vec![user];
    if kind.def().req_cultists <= 1 && kind != RuneKind::Offer {
        return invokers;
    }
    let Some(&at) = world.get::<TilePos>(rune) else {
        return invokers;
    };
    let mut query = world.query_filtered::<(Entity, &TilePos, &Stat), With<Cultist>>();
    let mut others: Vec<Entity> = query
        .iter(world)
        .filter(|(e, p, s)| {
            *e != user
                && p.z == at.z
                && (p.x - at.x).abs() <= 1
                && (p.y - at.y).abs() <= 1
                && **s == Stat::Conscious
        })
        .map(|(e, ..)| e)
        .collect();
    others.sort();
    others.retain(|&e| check_invoke_validity(world, e));
    invokers.extend(others);
    invokers
}

/// `cult/shade/check_invoke_validity`: not inside a soulstone, and a minute after leaving it.
fn check_invoke_validity(world: &mut World, mob: Entity) -> bool {
    let Some(CultForm::Shade { released_at }) = world.get::<Cultist>(mob).map(|c| c.form) else {
        return true;
    };
    let text = match released_at {
        None => "Вы не можете вызывать руны, находясь в камне душ!",
        Some(at) if at + SHADE_INVOKE_DELAY_SECS > now(world) => {
            "Вы не накопили достаточно сил для вызова рун. Вам нужно некоторое время с момента \
             выхода из камня душ!"
        }
        Some(_) => return true,
    };
    chat_to(world, mob, Span::Warning, text);
    false
}

/// The base `invoke`: every invoker says the words and takes `invoke_damage`.
pub(super) fn chant(world: &mut World, kind: RuneKind, invokers: &[Entity], invocation: &str) {
    let def = kind.def();
    for &mob in invokers {
        say(world, mob, invocation);
        if def.invoke_damage > 0.0 {
            hurt(world, mob, None, def.invoke_damage);
            chat_to(world, mob, Span::CultItalic, "rune saps your strength!");
        }
    }
}

/// `fail_invoke`.
pub(super) fn fail_invoke(world: &mut World, rune: Entity) {
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    let clients = crate::chem::clients_near(world, pos, None);
    world.write_message(SendChat {
        clients,
        message: crate::chem::line(
            Span::Warning,
            "The markings pulse with a small flash of red light, then fall dark.",
        ),
    });
}

pub(super) fn invoke(world: &mut World, client: Entity, rune: Entity, invokers: &[Entity]) {
    let Some(kind) = world.get::<Rune>(rune).map(|r| r.kind) else {
        return;
    };
    match kind {
        RuneKind::Offer => invoke_offer(world, rune, invokers),
        RuneKind::Empower => {
            chant(world, kind, invokers, kind.def().invocation);
            prepare_magic(world, client, invokers[0]);
        }
        RuneKind::Teleport => invoke_teleport(world, client, rune, invokers[0]),
        RuneKind::Revive => invoke_revive(world, rune, invokers),
        RuneKind::Barrier => invoke_barrier(world, rune, invokers),
        RuneKind::Summon => invoke_summon(world, client, rune, invokers),
        RuneKind::NarSie => invoke_narsie(world, rune, invokers),
    }
}

/// `convert/invoke`: a non-cultist on the rune is converted (alive, convertible, two invokers)
/// or sacrificed.
pub(super) fn invoke_offer(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    if world.get::<Rune>(rune).is_some_and(|r| r.in_use) {
        return;
    }
    let mut targets: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<Health>(e).is_some()
                && world.get::<Cultist>(e).is_none()
                && !crate::mobs::cult_pets::spared_by_offer(world, e)
        })
        .collect();
    targets.sort();
    let Some(&target) = targets.first() else {
        fail_invoke(world, rune);
        return;
    };
    let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
    if alive && is_convertable(world, target) {
        chant(
            world,
            RuneKind::Offer,
            invokers,
            "Mah'weyh pleggh at e'ntrath!",
        );
        do_convert(world, rune, target, invokers);
    } else {
        chant(world, RuneKind::Offer, invokers, "Barhah hra zar'garis!");
        do_sacrifice(world, target, invokers);
    }
    check_size(world);
}

pub(super) fn do_convert(
    world: &mut World,
    rune: Entity,
    target: Entity,
    invokers: &[Entity],
) -> bool {
    let victim = name_of(world, target);
    if invokers.len() < offer_invokers_needed(OfferTarget::Convert) {
        for &i in invokers {
            chat_to(
                world,
                i,
                Span::Warning,
                format!("You need at least two invokers to convert {victim}!"),
            );
        }
        return false;
    }
    if has_holy_water(world, target) {
        for &i in invokers {
            chat_to(
                world,
                i,
                Span::Warning,
                format!("Something is shielding {victim}'s mind!"),
            );
        }
        return false;
    }
    let health = world.get::<Health>(target).cloned().unwrap_or_default();
    let (brute, burn) = (health.brute, health.burn);
    if brute > 0.0 || burn > 0.0 {
        let mut stat = world.get::<Stat>(target).copied().unwrap_or_default();
        let mut health = health;
        health.adjust(DamageType::Brute, -(brute * 0.75));
        health.adjust(DamageType::Burn, -(burn * 0.75));
        crate::health::update_stat(&health, &mut stat);
        world.entity_mut(target).insert((health, stat));
    }
    let wounds = brute > 0.0 || burn > 0.0;
    visible(
        world,
        target,
        "AAAAAAAAAAAAAA-",
        &format!(
            "{victim} writhes in pain {}!",
            if wounds {
                "even as their wounds heal and close"
            } else {
                "as the markings below them glow a bloody red"
            }
        ),
        Span::Warning,
    );
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::restraints::uncuff(world, target, pos);
    }
    // `Unconscious(10 SECONDS)`.
    crate::status_effects::apply(world, target, Incap::Unconscious, 10.0);
    if let Some(&pos) = world.get::<TilePos>(rune) {
        crate::construction::spawn_obj(world, DAGGER, pos);
    }
    gain(world, target, false, true);
    greet(world, target);
    chat_to(
        world,
        target,
        Span::CultItalic,
        "Your blood pulses. Your head throbs. The world goes red. All at once you are aware of a \
         horrible, horrible, truth. The veil of reality has been ripped away and something evil \
         takes root.",
    );
    chat_to(
        world,
        target,
        Span::CultItalic,
        "Assist your new compatriots in their dark dealings. Your goal is theirs, and theirs is \
         yours. You serve the Geometer above all else. Bring it back.",
    );
    info!("{target} was converted to the cult by {invokers:?}");
    true
}

pub(super) fn do_sacrifice(world: &mut World, target: Entity, invokers: &[Entity]) -> bool {
    let victim = name_of(world, target);
    let human = world.get::<HumanLook>(target).is_some();
    let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
    let is_target = world.resource::<CultTeam>().is_sacrifice_target(target);
    let linked = (human && alive) || is_target;
    let needed = offer_invokers_needed(if linked {
        OfferTarget::LinkedSacrifice
    } else {
        OfferTarget::Sacrifice
    });
    if invokers.len() < needed {
        for &i in invokers {
            chat_to(
                world,
                i,
                Span::CultItalic,
                format!("{victim} is too greatly linked to the world! You need three acolytes!"),
            );
        }
        return false;
    }
    {
        let mut team = world.resource_mut::<CultTeam>();
        team.sacrificed += 1;
        for o in &mut team.objectives {
            if let CultObjective::Sacrifice {
                target: t, sacced, ..
            } = o
                && *t == Some(target)
            {
                *sacced = true;
                *t = None;
            }
        }
    }
    // `COMSIG_LIVING_CULT_SACRIFICED`: a pet turns cultist instead (`STOP_SACRIFICE`).
    let stop = crate::mobs::cult_pets::become_cultist(world, target);
    for &i in invokers {
        let text = if is_target {
            "\"Yes! This is the one I desire! You have done well.\""
        } else if human {
            "\"I accept this sacrifice.\""
        } else {
            "\"I accept this meager sacrifice.\""
        };
        chat_to(world, i, Span::CultLarge, text);
    }
    if stop {
        update_info(world);
        return false;
    }
    if let Some(pos) = world.get::<TilePos>(target).copied()
        && let Some(&invoker) = invokers.first()
    {
        sacrifice_soul(world, target, invoker, pos);
    }
    if world.get_entity(target).is_ok() {
        gib(world, target);
    }
    update_info(world);
    info!("{target} was sacrificed to Nar'Sie (target: {is_target})");
    true
}

/// `gib(DROP_ALL_REMAINS)` of a sacrifice: out of the cult first.
pub(super) fn gib(world: &mut World, mob: Entity) {
    if world.get::<Cultist>(mob).is_some() {
        remove_cultist(world, mob, true);
    }
    crate::health::gib(world, mob);
}

/// `teleport/invoke`: the list of the other teleport runes.
pub(super) fn invoke_teleport(world: &mut World, client: Entity, rune: Entity, user: Entity) {
    let (items, values) = teleport_runes(world, Some(rune));
    if items.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "There are no valid runes to teleport to!",
        );
        fail_invoke(world, rune);
        return;
    }
    open_list(
        world,
        client,
        user,
        CultPrompt {
            kind: PromptKind::Teleport {
                from_rune: Some(rune),
                target: user,
            },
            title: "Teleportation Target".into(),
            message: "Rune to teleport to".into(),
            items,
            values,
            spell_ids: Vec::new(),
        },
    );
}

/// `GLOB.teleport_runes` but `except`, by `listkey` (`avoid_assoc_duplicate_keys`).
pub(super) fn teleport_runes(
    world: &mut World,
    except: Option<Entity>,
) -> (Vec<String>, Vec<Entity>) {
    let mut query = world.query::<(Entity, &Rune)>();
    let mut runes: Vec<(Entity, String)> = query
        .iter(world)
        .filter(|(e, r)| r.kind == RuneKind::Teleport && Some(*e) != except)
        .map(|(e, r)| (e, r.listkey.clone()))
        .collect();
    runes.sort_by_key(|(e, _)| *e);
    let mut items: Vec<String> = Vec::new();
    let mut values = Vec::new();
    for (e, key) in runes {
        let mut k = key.clone();
        let mut n = 1;
        while items.contains(&k) {
            n += 1;
            k = format!("{key} ({n})");
        }
        items.push(k);
        values.push(e);
    }
    (items, values)
}

/// The teleport pick: from a rune (everything unanchored on it) or by an aura (one cultist).
pub(super) fn teleport_to(
    world: &mut World,
    client: Entity,
    user: Entity,
    from_rune: Option<Entity>,
    target: Entity,
    dest_rune: Entity,
) {
    let Some(&dest) = world.get::<TilePos>(dest_rune) else {
        fail_rune(world, from_rune);
        return;
    };
    if world.get::<Rune>(dest_rune).is_none() {
        fail_rune(world, from_rune);
        return;
    }
    if world
        .resource::<ss13_world::SpatialIndex>()
        .dense_on_tile(dest)
        .is_empty()
        .not()
    {
        tell(
            world,
            client,
            Span::Warning,
            "The target rune is blocked. Attempting to teleport to it would be massively unwise.",
        );
        fail_rune(world, from_rune);
        return;
    }
    match from_rune {
        Some(rune) => {
            let Some(&here) = world.get::<TilePos>(rune) else {
                return;
            };
            let user_pos = world.get::<TilePos>(user).copied();
            if user_pos.is_none_or(|p| {
                (p.x - here.x).abs() > 1 || (p.y - here.y).abs() > 1 || p.z != here.z
            }) {
                fail_invoke(world, rune);
                return;
            }
            let movables: Vec<Entity> = world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(here)
                .iter()
                .copied()
                .filter(|&e| {
                    e != rune
                        && world.get::<CarriedBy>(e).is_none()
                        && (world.get::<Health>(e).is_some()
                            || world
                                .get::<ss13_mapload::DmItem>(e)
                                .is_some_and(|i| i.pickable()))
                })
                .collect();
            if movables.is_empty() {
                fail_invoke(world, rune);
                return;
            }
            chant(
                world,
                RuneKind::Teleport,
                &[user],
                RuneKind::Teleport.def().invocation,
            );
            let moved_user = movables.contains(&user);
            for e in movables {
                world.entity_mut(e).insert(dest);
            }
            let clients = crate::chem::clients_near(world, here, None);
            world.write_message(SendChat {
                clients,
                message: crate::chem::line(
                    Span::Warning,
                    "There is a sharp crack of inrushing air, and everything above the rune \
                     disappears!",
                ),
            });
            chat_to(
                world,
                user,
                Span::Cult,
                if moved_user {
                    "Your vision blurs, and you suddenly appear somewhere else."
                } else {
                    "You send everything above the rune away."
                },
            );
        }
        None => {
            // The aura: revalidated, then used.
            if world.get::<Cultist>(target).is_none() {
                return;
            }
            let Some(aura) = held_aura(world, user, Spell::Teleport) else {
                return;
            };
            let Some(&origin) = world.get::<TilePos>(user) else {
                return;
            };
            if let Some(&tp) = world.get::<TilePos>(target)
                && ((tp.x - origin.x).abs() > 1 || (tp.y - origin.y).abs() > 1 || tp.z != origin.z)
            {
                return;
            }
            whisper(world, user, Spell::Teleport.def().invocation);
            world.entity_mut(target).insert(dest);
            let name = name_of(world, user);
            visible(
                world,
                user,
                "You speak the words of the talisman and find yourself somewhere else!",
                &format!("Dust flows from {name}'s hand, and they disappear with a sharp crack!"),
                Span::Warning,
            );
            spend_aura(world, user, aura);
        }
    }
}

pub(super) fn fail_rune(world: &mut World, rune: Option<Entity>) {
    if let Some(rune) = rune {
        fail_invoke(world, rune);
    }
}

/// `raise_dead/invoke`: a dead cultist on the rune is revived for three sacrifices.
pub(crate) fn invoke_revive(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    let mut dead: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<Cultist>(e).is_some()
                && (world.get::<Stat>(e) == Some(&Stat::Dead)
                    || world.get::<PlayerMob>(e).is_none())
        })
        .collect();
    dead.sort();
    // shortcut: the first one on the rune (tg asks which when several lie there).
    let Some(&target) = dead.first() else {
        chat_to(
            world,
            user,
            Span::CultItalic,
            "There are no dead cultists on the rune!",
        );
        fail_invoke(world, rune);
        return;
    };
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        let team = world.resource::<CultTeam>();
        if !can_revive_dead(team.sacrificed, team.sacrifices_used) {
            let missing = SOULS_TO_REVIVE + team.sacrifices_used - team.sacrificed;
            chat_to(
                world,
                user,
                Span::Warning,
                format!(
                    "Your cult must carry out {missing} more sacrifice{} before it can revive \
                     another cultist!",
                    if missing == 1 { "" } else { "s" }
                ),
            );
            fail_invoke(world, rune);
            return;
        }
        world.resource_mut::<CultTeam>().sacrifices_used += SOULS_TO_REVIVE;
        revive_fully(world, target);
    }
    chant(
        world,
        RuneKind::Revive,
        invokers,
        RuneKind::Revive.def().invocation,
    );
    chat_to(
        world,
        target,
        Span::CultLarge,
        "\"PASNAR SAVRAE YAM'TOTH. Arise.\"",
    );
    let name = name_of(world, target);
    visible(
        world,
        target,
        "You awaken suddenly from the void. You're alive!",
        &format!("{name} draws in a huge breath, red light shining from their eyes."),
        Span::Warning,
    );
}

/// `revive(ADMIN_HEAL_ALL)`.
pub(super) fn revive_fully(world: &mut World, mob: Entity) {
    crate::medical::heal_all(world, mob);
    let (max, mods) = world
        .get::<Health>(mob)
        .map_or((100.0, Default::default()), |h| (h.max, h.mods));
    world.entity_mut(mob).insert((
        Health {
            max,
            mods,
            ..Health::default()
        },
        Stat::Conscious,
    ));
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
    if let Some(mut alerts) = world.get_mut::<crate::breath::Alerts>(mob) {
        alerts.0.clear();
    }
    // The ghost goes back in (tg `revive` + the ghost reentering).
    let ghost = {
        let mut q = world.query::<(Entity, &Ghost)>();
        q.iter(world)
            .find(|(_, g)| g.body == Some(mob))
            .map(|(e, _)| e)
    };
    if let Some(ghost) = ghost {
        let _ = ghost;
        crate::ghost::return_to_body(world, mob);
    }
}

/// `wall/invoke`: the barrier goes up or down; 2 brute to an arm.
pub(super) fn invoke_barrier(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    chant(
        world,
        RuneKind::Barrier,
        invokers,
        RuneKind::Barrier.def().invocation,
    );
    let barrier = match world.get::<Rune>(rune).and_then(|r| r.barrier) {
        Some(b) if world.get_entity(b).is_ok() => Some(b),
        _ => {
            let b = crate::construction::spawn_obj(world, BARRIER, pos);
            if let Some(b) = b {
                world.entity_mut(b).insert(CultBarrier { rune, up: false });
                if let Some(mut r) = world.get_mut::<Rune>(rune) {
                    r.barrier = Some(b);
                }
            }
            b
        }
    };
    if let Some(b) = barrier {
        let up = !world.get::<CultBarrier>(b).is_some_and(|c| c.up);
        if let Some(mut c) = world.get_mut::<CultBarrier>(b) {
            c.up = up;
        }
        // shortcut: the shield is always drawn; up is dense, down lets everything through.
        if up {
            world.entity_mut(b).insert(ss13_world::Density);
        } else {
            world.entity_mut(b).remove::<ss13_world::Density>();
        }
        crate::construction::tile_changed(world, pos);
    }
    let zone = arm(world);
    hurt(world, user, Some(zone), 2.0);
}

/// `summon/invoke`: two invokers pick a living cultist to call to the rune.
pub(super) fn invoke_summon(world: &mut World, client: Entity, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let members = world.resource::<CultTeam>().members.clone();
    let cultists: Vec<Entity> = members
        .into_iter()
        .filter(|m| !invokers.contains(m) && world.get::<Stat>(*m) != Some(&Stat::Dead))
        .collect();
    if cultists.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "There are no cultists to summon!",
        );
        fail_invoke(world, rune);
        return;
    }
    let items: Vec<String> = cultists.iter().map(|&c| name_of(world, c)).collect();
    open_list(
        world,
        client,
        user,
        CultPrompt {
            kind: PromptKind::Summon { rune },
            title: "Followers of the Geometer".into(),
            message: "Who do you wish to call to rune?".into(),
            items,
            values: cultists,
            spell_ids: Vec::new(),
        },
    );
}

pub(super) fn summon_cultist(world: &mut World, user: Entity, rune: Entity, target: Entity) {
    let Some(&at) = world.get::<TilePos>(rune) else {
        return;
    };
    let invokers = can_invoke(world, rune, user, RuneKind::Summon);
    if invokers.len() < RuneKind::Summon.def().req_cultists {
        fail_invoke(world, rune);
        return;
    }
    let name = name_of(world, target);
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        chat_to(world, user, Span::CultItalic, format!("{name} has died!"));
        fail_invoke(world, rune);
        return;
    }
    if world.get::<Cultist>(target).is_none() {
        chat_to(
            world,
            user,
            Span::CultItalic,
            format!("{name} is not a follower of the Geometer!"),
        );
        fail_invoke(world, rune);
        return;
    }
    let pulled = {
        let mut q = world.query::<&crate::pull::Pulling>();
        q.iter(world).any(|p| p.0 == target)
    };
    if pulled {
        chat_to(
            world,
            user,
            Span::CultItalic,
            format!("{name} is being held in place!"),
        );
        fail_invoke(world, rune);
        return;
    }
    visible(
        world,
        target,
        "Overwhelming vertigo consumes you as you are hurled through the air!",
        &format!("{name} suddenly disappears in a flash of red light!"),
        Span::Warning,
    );
    chant(
        world,
        RuneKind::Summon,
        &invokers,
        RuneKind::Summon.def().invocation,
    );
    world.entity_mut(target).insert(at);
    let clients = crate::chem::clients_near(world, at, None);
    world.write_message(SendChat {
        clients,
        message: crate::chem::line(
            Span::Warning,
            &format!("A foggy shape materializes atop rune and solidifies into {name}!"),
        ),
    });
    delete_rune(world, rune);
}

/// `narsie/invoke`: nine invokers in a summoning area; Nar'Sie rises 4 s later.
pub(super) fn invoke_narsie(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    if world.get::<Rune>(rune).is_some_and(|r| r.used) {
        return;
    }
    let team = world.resource::<CultTeam>().clone();
    let spots = team.summon_spots();
    let here = area_of(world, pos);
    if !spots.iter().any(|(a, _)| Some(*a) == here) {
        let names = english_list(spots.iter().map(|(_, n)| n.as_str()));
        chat_to(
            world,
            user,
            Span::CultLarge,
            format!("The Geometer can only be summoned where the veil is weak - in {names}!"),
        );
        return;
    }
    if team.narsie.is_some() {
        for &i in invokers {
            chat_to(world, i, Span::Warning, "Nar'Sie is already on this plane!");
        }
        return;
    }
    if let Some(mut r) = world.get_mut::<Rune>(rune) {
        r.used = true;
    }
    let true_cultists: Vec<(Entity, String)> = team
        .members
        .iter()
        .map(|&m| {
            (
                m,
                world
                    .get::<Cultist>(m)
                    .map(|c| c.key.clone())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let at = now(world) + 4.0;
    {
        let mut team = world.resource_mut::<CultTeam>();
        team.narsie_summoned = true;
        team.true_cultists = true_cultists;
        team.ending = Some(Ending::Rising { at, pos });
    }
    chant(
        world,
        RuneKind::NarSie,
        invokers,
        RuneKind::NarSie.def().invocation,
    );
    // BandaStation's cult overhaul summoning sound for everyone.
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::to_playing_players(
            ss13_mapload::sound::files::NARSIE_SUMMON,
            100.0,
            false,
        ),
    );
    info!("Nar'Sie is being summoned at {pos:?} by {invokers:?}");
}

/// The steps of `begin_the_end` while Nar'Sie stands.
// Cost: one comparison per tick while the cult exists.
pub(super) fn tick_ending(world: &mut World) {
    let Some(ending) = world.resource::<CultTeam>().ending else {
        return;
    };
    let t = now(world);
    let sender = format!(
        "{}: Отдел паранормальных явлений",
        crate::events::COMMAND_NAME
    );
    let next = match ending {
        Ending::Rising { at, pos } if t >= at => {
            narsie_rises(world, pos);
            Ending::BeginCheck { at: t + 15.0 }
        }
        Ending::BeginCheck { at } if t >= at => {
            crate::shuttle::priority_announce(
                world,
                &sender,
                "",
                "Внимание, это приоритетное оповещение. Сектор Эпсилон Эридани был подвергнут \
                 вторжению древней богоподобной враждебной сущности, у нас есть подтвержденная \
                 информация о массовых порабощениях по всему сектору. Положению присвоен сценарий \
                 \"ПОЛНОЕ ИСТРЕБЛЕНИЕ\". Приказ на принятие мер получен и авторизован. Ожидаемое \
                 время готовности: 60 секунд.",
            );
            Ending::SecondCheck { at: t + 50.0 }
        }
        Ending::SecondCheck { at } if t >= at => {
            crate::shuttle::priority_announce(
                world,
                &sender,
                "",
                "Моделирование беспричинного изменения пространства завершено. Начинаем \
                 развертывание.",
            );
            Ending::DestroyStation { at: t + 5.0 }
        }
        Ending::DestroyStation { at } if t >= at => {
            // `narsie_start_destroy_station`: the shuttle stays until Nar'Sie is gone.
            if let Some(narsie) = world.resource::<CultTeam>().narsie {
                crate::shuttle::register_hostile_environment(
                    world,
                    crate::shuttle::Blocker::Entity(narsie),
                );
            }
            Ending::Apocalypse { at: t + 60.0 }
        }
        Ending::Apocalypse { at } if t >= at => {
            crate::shuttle::priority_announce(
                world,
                "Оповещение безопасности",
                "",
                "Тревога! Обнаружен запуск орбитального эммитера. Протоколы безопасности станции \
                 обнулены, возобновление нево@#$%^-...",
            );
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::to_playing_players(
                    ss13_mapload::sound::files::NUKE_ALARM,
                    70.0,
                    false,
                ),
            );
            Ending::End { at: t + 12.0 }
        }
        Ending::End { at } if t >= at => {
            // `ending_helper`: `SSticker.force_ending`.
            world.resource_mut::<crate::round::Ticker>().force_ending = true;
            info!("Nar'Sie has consumed the station: the round ends");
            Ending::Done
        }
        other => other,
    };
    if next != ending {
        world.resource_mut::<CultTeam>().ending = Some(next);
    }
}

/// `new /obj/narsie(rune_turf)` + `start_ending_the_round`.
pub(super) fn narsie_rises(world: &mut World, pos: TilePos) {
    let narsie = crate::construction::spawn_obj(world, NARSIE, pos);
    {
        let mut team = world.resource_mut::<CultTeam>();
        team.narsie = narsie;
        for o in &mut team.objectives {
            if let CultObjective::Eldergod { summoned, .. } = o {
                *summoned = true;
            }
        }
    }
    update_info(world);
    let mut q = world.query_filtered::<Entity, With<ClientToken>>();
    let clients: Vec<Entity> = q.iter(world).collect();
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::System, Span::CultLarge, "NAR'SIE HAS RISEN"),
    });
    // BandaStation: `/obj/narsie/Initialize` to the world, the cult overhaul's risen sound to
    // the players.
    crate::sound::send_sound_to_world(world, ss13_mapload::sound::files::NARSIE_RISES, 100.0);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::to_playing_players(
            ss13_mapload::sound::files::NARSIE_RISEN,
            100.0,
            false,
        ),
    );
    info!("Nar'Sie has risen at {pos:?}");
}
