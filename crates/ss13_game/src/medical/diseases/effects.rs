//! The diseases' effects on the mob and each non-advance disease's `stage_act`.

use super::*;

pub(crate) fn say_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// `mob.emote(key)` from code: nothing while unconscious or dead.
pub(crate) fn emote(world: &mut World, mob: Entity, key: &str) {
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    // BandaStation's lines of the emotes the chat does not have yet.
    let text = match ss13_chat::emote::find(key) {
        Some(emote) => emote.message,
        None => match key {
            "drool" => "пускает слюни.",
            "gag" => "тужится.",
            "sniff" => "шмыгает носом.",
            "snore" => "храпит.",
            "stare" => "уставился.",
            "blink" => "моргает.",
            _ => return,
        },
    };
    crate::mobs::defense::emote(world, mob, text);
}

/// `amount` of `kind` over the whole body (tg `adjust_*_loss`, `take_overall_damage`; negative
/// heals).
pub(crate) fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage(&mut health, &mut new, kind, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

/// tg `take_bodypart_damage`: one random bodypart.
pub(super) fn bodypart_damage(
    world: &mut World,
    mob: Entity,
    kind: DamageType,
    amount: f32,
    rng: &mut CombatRng,
) {
    let zone = BodyZone::ALL[rng.range(0, 5) as usize];
    super::super::hurt_limb(world, mob, zone, kind, amount);
}

pub(crate) fn adjust_bodytemperature(world: &mut World, mob: Entity, amount: f64, min: f64) {
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
        if amount < 0.0 && r.bodytemperature <= min {
            return;
        }
        r.bodytemperature = (r.bodytemperature + amount).max(min);
    }
}

pub(crate) fn confusion_up_to(world: &mut World, mob: Entity, secs: f32, max: f32) {
    if let Some(mut effects) = crate::status_effects::effects_mut(world, mob)
        && effects.confusion < max
    {
        effects.confusion = (effects.confusion + secs).min(max);
    }
}

/// tg `adjust_nutrition` (and `overeatduration` for the starving symptoms).
pub(crate) fn adjust_nutrition(world: &mut World, mob: Entity, amount: f32, overeat_secs: f32) {
    if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(mob) {
        hunger.nutrition = (hunger.nutrition + amount).max(0.0);
        hunger.overeat_secs = (hunger.overeat_secs + overeat_secs).max(0.0);
    }
}

/// Drops the active hand's item (only a `WEIGHT_CLASS_TINY` one with `tiny_only`), or both
/// hands' (`drop_all_held_items`).
pub(crate) fn drop_held(world: &mut World, mob: Entity, all: bool, tiny_only: bool) {
    let (Some(inventory), Some(&pos)) = (
        world.get::<Inventory>(mob).cloned(),
        world.get::<TilePos>(mob),
    ) else {
        return;
    };
    let items: Vec<Entity> = if all {
        inventory.hands.iter().flatten().copied().collect()
    } else {
        inventory.active_item().into_iter().collect()
    };
    for item in items {
        if tiny_only && world.get::<DmWeight>(item).is_none_or(|w| w.w_class != 1) {
            continue;
        }
        if !crate::inventory::no_drop(world, item) {
            crate::inventory::put_down(world, item, pos);
        }
    }
}

/// tg carbon `vomit(flags, lost_nutrition, distance)`.
// shortcut: no vomit decal (blood vomit splatters blood), no mood event, no purge.
pub(crate) fn vomit(
    world: &mut World,
    mob: Entity,
    lost_nutrition: f32,
    stun: bool,
    blood: bool,
    distance: u32,
) {
    let name = super::super::name_of(world, mob);
    let nutrition = world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(f32::MAX, |h| h.nutrition);
    if !blood && nutrition < 100.0 {
        super::super::visible(
            world,
            mob,
            "Вы пытаетесь блевать, но в желудке ничего нет!",
            &format!("{} всухую тужится!", ss13_chat::capitalize(&name)),
            Span::Warning,
        );
        if stun {
            crate::status_effects::apply(world, mob, Incap::Stun, 20.0);
        }
        return;
    }
    super::super::visible(
        world,
        mob,
        "Вы блюете!",
        &format!("{} блюет!", ss13_chat::capitalize(&name)),
        Span::Danger,
    );
    if stun {
        crate::status_effects::apply(world, mob, Incap::Stun, 8.0);
    }
    if blood {
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::janitor::add_splatter_floor(world, pos, false);
        }
        damage(world, mob, DamageType::Brute, 3.0 * (distance + 1) as f32);
    } else {
        adjust_nutrition(world, mob, -lost_nutrition, 0.0);
        damage(world, mob, DamageType::Tox, -3.0);
    }
}

pub(super) fn pick<'a>(rng: &mut CombatRng, lines: &[&'a str]) -> &'a str {
    lines[rng.range(0, lines.len() as u32 - 1) as usize]
}

pub(super) fn has_resistance(world: &World, mob: Entity, path: &str) -> bool {
    world
        .get::<Diseases>(mob)
        .is_some_and(|d| d.resistances.iter().any(|r| r == path))
}

/// The `stage_act` of each simple disease after the base one let it act.
pub(super) fn kind_act(
    world: &mut World,
    mob: Entity,
    disease: &mut Disease,
    secs: f32,
    rng: &mut CombatRng,
) -> Outcome {
    let stage = disease.stage;
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let mut p = |percent: f32| spt_prob(rng, percent, secs);
    match disease.kind {
        Kind::Cold => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            if p(0.5) {
                emote(world, mob, "sneeze");
            }
            if p(0.5) {
                emote(world, mob, "cough");
            }
            if p(0.5) {
                say_to(world, mob, Span::Danger, "Your throat feels sore.");
            }
            if p(0.5) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "Mucous runs down the back of your throat.",
                );
            }
            if stage == 3 && p(0.25) && !has_resistance(world, mob, FLU.path) {
                force_contract(world, mob, &Disease::new(Kind::Flu));
                return Outcome::Cure { resistance: true };
            }
            let (rest, chance) = if stage == 2 {
                (23.0, 0.025)
            } else {
                (12.5, 0.005)
            };
            if (lying && p(rest)) || p(chance) {
                say_to(world, mob, Span::Notice, "You feel better.");
                return Outcome::Cure { resistance: true };
            }
        }
        Kind::Cold9 => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            let cold = if stage == 2 { -5.0 } else { -10.0 };
            adjust_bodytemperature(world, mob, cold * f64::from(secs), 0.0);
            if p(0.5) {
                emote(world, mob, "sneeze");
            }
            if p(0.5) {
                emote(world, mob, "cough");
            }
            if p(0.5) {
                say_to(world, mob, Span::Danger, "Your throat feels sore.");
            }
            if p(if stage == 2 { 2.5 } else { 5.0 }) {
                say_to(world, mob, Span::Danger, "You feel stiff.");
            }
            if stage == 2 && p(0.05) {
                say_to(world, mob, Span::Notice, "You feel better.");
                return Outcome::Cure { resistance: true };
            }
        }
        Kind::Flu => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            if p(0.5) {
                emote(world, mob, "sneeze");
            }
            if p(0.5) {
                emote(world, mob, "cough");
            }
            if p(0.5) {
                say_to(world, mob, Span::Danger, "Your muscles ache.");
                if rng.chance(20.0) {
                    bodypart_damage(world, mob, DamageType::Brute, 1.0, rng);
                }
            }
            if spt_prob(rng, 0.5, secs) {
                say_to(world, mob, Span::Danger, "Your stomach hurts.");
                if rng.chance(20.0) {
                    damage(world, mob, DamageType::Tox, 1.0);
                }
            }
            let rest = if stage == 2 { 10.0 } else { 7.5 };
            if lying && spt_prob(rng, rest, secs) {
                say_to(world, mob, Span::Notice, "You feel better.");
                disease.stage -= 1;
            }
        }
        Kind::FluSpanish => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            let heat = if stage == 2 { 5.0 } else { 10.0 };
            adjust_bodytemperature(world, mob, heat * f64::from(secs), 0.0);
            if p(2.5) {
                emote(world, mob, "sneeze");
            }
            if p(2.5) {
                emote(world, mob, "cough");
            }
            if p(if stage == 2 { 0.5 } else { 2.5 }) {
                say_to(world, mob, Span::Danger, "You're burning in your own skin!");
                bodypart_damage(world, mob, DamageType::Burn, 5.0, rng);
            }
        }
        Kind::Gbs | Kind::GbsNoTransmission => match stage {
            2 => {
                if p(2.5) {
                    emote(world, mob, "cough");
                }
            }
            3 => {
                if p(2.5) {
                    emote(world, mob, "gasp");
                }
                if p(5.0) {
                    say_to(world, mob, Span::Danger, "Your body hurts all over!");
                }
            }
            4 => {
                say_to(
                    world,
                    mob,
                    Span::BoldWarning,
                    "Your body feels as if it's trying to rip itself apart!",
                );
                if p(30.0) {
                    crate::health::gib(world, mob);
                    return Outcome::Gone;
                }
            }
            _ => {}
        },
        Kind::FakeGbs => match stage {
            2 => {
                if p(0.5) {
                    emote(world, mob, "sneeze");
                }
            }
            3 => {
                if p(2.5) {
                    emote(world, mob, "cough");
                } else if p(2.5) {
                    emote(world, mob, "gasp");
                }
                if p(5.0) {
                    say_to(
                        world,
                        mob,
                        Span::Danger,
                        "You're starting to feel very weak...",
                    );
                }
            }
            4 | 5 if p(5.0) => emote(world, mob, "cough"),
            _ => {}
        },
        Kind::Brainrot => brainrot(world, mob, stage, secs, rng),
        Kind::Tuberculosis => tuberculosis(world, mob, stage, secs, rng),
        Kind::Anxiety => anxiety(world, mob, stage, secs, rng),
        Kind::AdrenalCrisis => match stage {
            1 => {
                if p(2.5) {
                    let line = pick(rng, &["You feel lightheaded.", "You feel lethargic."]);
                    say_to(world, mob, Span::Warning, line);
                }
            }
            2 => {
                if p(5.0) {
                    crate::status_effects::apply(world, mob, Incap::Unconscious, 4.0);
                }
                if p(10.0) {
                    adjust_timed(world, mob, Timed::Slurring(SlurKind::Generic), 14.0, None);
                }
                if p(7.0) {
                    set_timed(world, mob, Timed::Dizziness, 20.0, true);
                }
                if spt_prob(rng, 2.5, secs) {
                    let line = pick(
                        rng,
                        &[
                            "You feel pain shoot down your legs!",
                            "You feel like you are going to pass out at any moment.",
                            "You feel really dizzy.",
                        ],
                    );
                    say_to(world, mob, Span::Warning, line);
                }
            }
            _ => {}
        },
        Kind::DeathSandwich => death_sandwich(world, mob, stage, secs, rng),
        Kind::Anaphylaxis => anaphylaxis(world, mob, stage, secs, rng),
        Kind::Decloning => {
            if world.get::<Stat>(mob) == Some(&Stat::Dead) {
                return Outcome::Cure { resistance: true };
            }
            if stage >= 3 && p(1.0) {
                emote(world, mob, "drool");
            }
            if stage == 2 && p(1.0) {
                emote(world, mob, "yawn");
            }
            match stage {
                3 => {
                    if p(1.0) {
                        say_to(world, mob, Span::Danger, "Your skin feels strange.");
                    }
                }
                4 => {
                    if p(2.5) {
                        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 1.0, 170.0);
                    }
                    if p(7.5) {
                        adjust_timed(world, mob, Timed::Stutter, 6.0, None);
                    }
                }
                5 => {
                    if p(2.5) {
                        say_to(world, mob, Span::Danger, "Your skin starts degrading!");
                    }
                    if spt_prob(rng, 5.0, secs) {
                        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 2.0, 170.0);
                    }
                }
                _ => {}
            }
        }
        Kind::Revblight => {
            return crate::antag::revenant::revblight_act(world, mob, stage, secs, rng);
        }
        Kind::Advance => {}
    }
    Outcome::Keep
}

pub(super) fn brainrot(world: &mut World, mob: Entity, stage: u8, secs: f32, rng: &mut CombatRng) {
    match stage {
        2 => {
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "blink");
            }
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "yawn");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "You don't feel like yourself.");
            }
            if spt_prob(rng, 2.5, secs) {
                organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 1.0, 170.0);
            }
        }
        3 | 4 => {
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "stare");
            }
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "drool");
            }
            let (chance, amount, line) = if stage == 3 {
                (
                    5.0,
                    2.0,
                    "Your try to remember something important...but can't.",
                )
            } else {
                (
                    7.5,
                    3.0,
                    "Strange buzzing fills your head, removing all thoughts.",
                )
            };
            if spt_prob(rng, chance, secs) {
                organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, amount, 170.0);
                if rng.chance(2.0) {
                    say_to(world, mob, Span::Danger, line);
                }
            }
            if stage == 4 && spt_prob(rng, 1.5, secs) {
                say_to(world, mob, Span::Danger, "You lose consciousness...");
                let name = super::super::name_of(world, mob);
                super::super::visible(
                    world,
                    mob,
                    "You suddenly collapse!",
                    &format!("{name} suddenly collapses!"),
                    Span::Warning,
                );
                let knocked = rng.range(10, 20) as f32;
                crate::status_effects::apply(world, mob, Incap::Unconscious, knocked);
            }
            if stage == 4 && spt_prob(rng, 7.5, secs) {
                adjust_timed(world, mob, Timed::Stutter, 6.0, None);
            }
        }
        _ => {}
    }
}

pub(super) fn tuberculosis(
    world: &mut World,
    mob: Entity,
    stage: u8,
    secs: f32,
    rng: &mut CombatRng,
) {
    if spt_prob(rng, f32::from(stage) * 2.0, secs) {
        emote(world, mob, "cough");
        say_to(world, mob, Span::Danger, "Your chest hurts.");
    }
    match stage {
        2 => {
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your stomach violently rumbles!");
            }
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Danger, "You feel a cold sweat form.");
            }
        }
        4 => {
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::BoldWarning, "You see four of everything!");
                set_timed(world, mob, Timed::Dizziness, 10.0, true);
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You feel a sharp pain from your lower chest!",
                );
                damage(world, mob, DamageType::Oxy, 5.0);
                emote(world, mob, "gasp");
            }
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You feel air escape from your lungs painfully.",
                );
                damage(world, mob, DamageType::Oxy, 25.0);
                emote(world, mob, "gasp");
            }
        }
        5 => {
            if spt_prob(rng, 1.0, secs) {
                let line = pick(
                    rng,
                    &[
                        "You feel your heart slowing...",
                        "You relax and slow your heartbeat.",
                    ],
                );
                say_to(world, mob, Span::BoldWarning, line);
                crate::status_effects::adjust_stamina(world, mob, 70.0);
            }
            if spt_prob(rng, 5.0, secs) {
                crate::status_effects::adjust_stamina(world, mob, 100.0);
                let name = super::super::name_of(world, mob);
                super::super::visible(
                    world,
                    mob,
                    "You surrender yourself and feel at peace...",
                    &format!("{name} faints!"),
                    Span::Warning,
                );
                crate::status_effects::adjust(world, mob, Incap::Sleeping, 10.0);
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::BoldWarning,
                    "You feel your mind relax and your thoughts drift!",
                );
                confusion_up_to(world, mob, 8.0, 100.0);
            }
            if spt_prob(rng, 5.0, secs) {
                vomit(world, mob, 20.0, true, false, 1);
            }
            if spt_prob(rng, 1.5, secs) {
                let line = pick(
                    rng,
                    &[
                        "Your stomach silently rumbles...",
                        "Your stomach seizes up and falls limp, muscles dead and lifeless.",
                        "You could eat a crayon",
                    ],
                );
                say_to(world, mob, Span::Warning, line);
                adjust_nutrition(world, mob, -100.0, -200.0);
            }
            if spt_prob(rng, 7.5, secs) {
                let dressed = world
                    .get::<Inventory>(mob)
                    .is_some_and(|i| i.uniform.is_some());
                let line = if dressed {
                    pick(
                        rng,
                        &[
                            "You feel uncomfortably hot...",
                            "You feel like unzipping your jumpsuit...",
                            "You feel like taking off some clothes...",
                        ],
                    )
                } else {
                    "You feel uncomfortably hot..."
                };
                say_to(world, mob, Span::Danger, line);
                adjust_bodytemperature(world, mob, 40.0, 0.0);
            }
        }
        _ => {}
    }
}

pub(super) fn anxiety(world: &mut World, mob: Entity, stage: u8, secs: f32, rng: &mut CombatRng) {
    match stage {
        2 => {
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Notice, "You feel anxious.");
            }
        }
        3 => {
            if spt_prob(rng, 5.0, secs) {
                say_to(world, mob, Span::Notice, "Your stomach flutters.");
            }
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Notice, "You feel panicky.");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "You're overtaken with panic!");
                let secs = rng.range(2, 3) as f32;
                confusion_up_to(world, mob, secs, f32::MAX);
            }
        }
        4 => {
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You feel butterflies in your stomach.",
                );
            }
            let name = super::super::name_of(world, mob);
            if spt_prob(rng, 2.5, secs) {
                super::super::visible(
                    world,
                    mob,
                    "You have a panic attack!",
                    &format!("{name} stumbles around in a panic."),
                    Span::Danger,
                );
                let secs = rng.range(6, 8) as f32;
                confusion_up_to(world, mob, secs, f32::MAX);
                let jitter = rng.range(12, 16) as f32;
                adjust_timed(world, mob, Timed::Jitter, jitter, None);
            }
            if spt_prob(rng, 1.0, secs) {
                super::super::visible(
                    world,
                    mob,
                    "You cough up butterflies!",
                    &format!("{name} coughs up butterflies!"),
                    Span::Danger,
                );
            }
        }
        _ => {}
    }
}

pub(super) fn death_sandwich(
    world: &mut World,
    mob: Entity,
    stage: u8,
    secs: f32,
    rng: &mut CombatRng,
) {
    match stage {
        1 => {
            if spt_prob(rng, 1.5, secs) {
                emote(world, mob, "cough");
            }
            if spt_prob(rng, 0.5, secs) {
                emote(world, mob, "gag");
            }
            if spt_prob(rng, 0.5, secs) {
                damage(world, mob, DamageType::Tox, 5.0);
            }
        }
        2 => {
            if spt_prob(rng, 5.0, secs) {
                emote(world, mob, "cough");
            }
            if spt_prob(rng, 2.5, secs) {
                emote(world, mob, "gag");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your body feels hot!");
                if rng.chance(20.0) {
                    bodypart_damage(world, mob, DamageType::Burn, 1.0, rng);
                }
            }
            if spt_prob(rng, 3.0, secs) {
                damage(world, mob, DamageType::Tox, 10.0);
            }
        }
        3 => {
            if spt_prob(rng, 5.0, secs) {
                emote(world, mob, "gag");
            }
            if spt_prob(rng, 10.0, secs) {
                emote(world, mob, "gasp");
            }
            if spt_prob(rng, 2.5, secs) {
                vomit(world, mob, 20.0, true, true, 1);
            }
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Danger, "Your body feels hot!");
                if rng.chance(60.0) {
                    bodypart_damage(world, mob, DamageType::Burn, 2.0, rng);
                }
            }
            if spt_prob(rng, 6.0, secs) {
                damage(world, mob, DamageType::Tox, 15.0);
            }
            if spt_prob(rng, 1.5, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You try to scream, but nothing comes out!",
                );
                set_timed(world, mob, Timed::Silence, 5.0, true);
            }
        }
        _ => {}
    }
}

pub(super) fn anaphylaxis(
    world: &mut World,
    mob: Entity,
    stage: u8,
    secs: f32,
    rng: &mut CombatRng,
) {
    adjust_bodytemperature(
        world,
        mob,
        -10.0 * f64::from(secs) * f64::from(stage),
        crate::breath::BODYTEMP_COLD_DAMAGE_LIMIT - 70.0,
    );
    match stage {
        1 => {
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your hands start to shake.",
                );
                adjust_timed(world, mob, Timed::Jitter, 4.0 * secs, Some(60.0));
            }
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your legs start to shake.",
                );
                adjust_timed(world, mob, Timed::Jitter, 4.0 * secs, Some(60.0));
            }
            if spt_prob(rng, 2.0, secs) {
                adjust_timed(world, mob, Timed::Dizziness, 5.0 * secs, Some(60.0));
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your throat itches.");
            }
        }
        2 => {
            damage(world, mob, DamageType::Tox, 0.33 * secs);
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your hands shake violently.",
                );
                adjust_timed(world, mob, Timed::Jitter, 8.0 * secs, Some(60.0));
                if rng.chance(20.0) {
                    drop_held(world, mob, true, false);
                }
            }
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your legs shake violently.",
                );
                adjust_timed(world, mob, Timed::Jitter, 8.0 * secs, Some(60.0));
                let stamina = world
                    .get::<crate::status_effects::StatusEffects>(mob)
                    .map_or(0.0, |e| e.stamina);
                if rng.chance(40.0) && stamina < 75.0 {
                    crate::status_effects::adjust_stamina(world, mob, 15.0);
                }
            }
            if has_organ(world, mob, Slot::Eyes) && spt_prob(rng, 4.0, secs) {
                crate::vision::set_eye_blur_if_lower(world, mob, 4.0 * secs);
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "It's getting harder to see clearly.",
                );
            }
            if spt_prob(rng, 4.0, secs) {
                damage(world, mob, DamageType::Oxy, 2.0 * secs);
                if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
                    r.losebreath += 2.0 * secs;
                }
                say_to(world, mob, Span::Warning, "It's getting harder to breathe.");
            }
            if spt_prob(rng, 2.0, secs) {
                adjust_timed(world, mob, Timed::Drowsiness, 3.0 * secs, Some(30.0));
            }
            if spt_prob(rng, 2.0, secs) {
                adjust_timed(world, mob, Timed::Dizziness, 5.0 * secs, Some(60.0));
                confusion_up_to(world, mob, secs, 10.0);
            }
            if spt_prob(rng, 2.0, secs) {
                vomit(world, mob, 10.0, false, false, 1);
                crate::status_effects::apply(world, mob, Incap::Stun, 2.0);
            }
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "cough");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your throat feels sore.");
            }
        }
        3 => {
            damage(world, mob, DamageType::Tox, 3.0 * secs);
            damage(world, mob, DamageType::Oxy, secs);
            crate::status_effects::apply(world, mob, Incap::Unconscious, 3.0 * secs);
        }
        _ => {}
    }
}
