//! The transformation diseases (`code/datums/diseases/transformation.dm`): stage messages, the
//! stage effects of each, and at stage 5 the host becomes a monkey (the same body), a cyborg, a
//! xenomorph hunter, a slime, a corgi, a morph or a gondola. Their sources are reagents
//! ([`reagent_expose`]).
// shortcut: no jobban replacement (`bantype`: no role bans yet); gondola meat and the slime
// extract's hand-held use do not infect yet (`crate::food`, `crate::science::xenobio`).

use super::*;

const SLIME: &str = "/mob/living/basic/slime";
const SLIME_RANDOM: &str = "/mob/living/basic/slime/random";
const CORGI: &str = "/mob/living/basic/pet/dog/corgi";
const GONDOLA: &str = "/mob/living/basic/pet/gondola";
const PAX: &str = "/datum/reagent/pax";

const ASLIMETOXIN: &str = "/datum/reagent/aslimetoxin";
const GLUTTONYTOXIN: &str = "/datum/reagent/gluttonytoxin";
const NANOMACHINES: &str = "/datum/reagent/cyborg_mutation_nanomachines";
const XENOMICROBES: &str = "/datum/reagent/xenomicrobes";
const FUNGALSPORES: &str = "/datum/reagent/fungalspores";

type Lines = &'static [(Span, &'static str)];

/// `stage1` ... `stage5` of a transformation.
fn stage_lines(kind: Kind, stage: u8) -> Lines {
    use Span::{Danger, Plain, Warning};
    match (kind, stage) {
        (Kind::JungleFlu, 4) => &[
            (Warning, "You breathe through your mouth."),
            (Warning, "You have a craving for bananas."),
            (Warning, "Your back hurts."),
            (Warning, "Your mind feels clouded."),
        ],
        (Kind::JungleFlu, 5) => &[(Warning, "You feel like monkeying around.")],
        (Kind::TransformRobot, 2) => &[
            (Danger, "Beep...boop.."),
            (Plain, "Your joints feel stiff."),
        ],
        (Kind::TransformRobot, 3) => &[
            (Danger, "You can feel something move...inside."),
            (Danger, "Your joints feel very stiff."),
            (Warning, "Your skin feels loose."),
        ],
        (Kind::TransformRobot, 4) => &[
            (Danger, "You can feel... something...inside you."),
            (Danger, "Your skin feels very loose."),
        ],
        (Kind::TransformRobot | Kind::TransformXeno, 5) => {
            &[(Danger, "Your skin feels as if it's about to burst off!")]
        }
        (Kind::TransformXeno, 2) => &[(Plain, "Your throat feels scratchy."), (Danger, "Kill...")],
        (Kind::TransformXeno, 3) => &[
            (Danger, "You can feel something move...inside."),
            (Danger, "Your throat feels very scratchy."),
            (Warning, "Your skin feels tight."),
        ],
        (Kind::TransformXeno, 4) => &[
            (Danger, "You can feel... something...inside you."),
            (Danger, "Your blood boils!"),
            (Danger, "Your skin feels very tight."),
        ],
        (Kind::TransformSlime, 1) => &[(Plain, "You don't feel very well.")],
        (Kind::TransformSlime, 2) => &[(Plain, "Your skin feels a little slimy.")],
        (Kind::TransformSlime | Kind::TransformMorph, 3) => &[
            (Danger, "Your appendages are melting away."),
            (Danger, "Your limbs begin to lose their shape."),
        ],
        (Kind::TransformSlime, 4) => &[(Danger, "You are turning into a slime.")],
        (Kind::TransformSlime, 5) => &[(Danger, "You have become a slime.")],
        (Kind::TransformCorgi, 1) => &[(Plain, "BARK.")],
        (Kind::TransformCorgi, 2) => &[(Plain, "You feel the need to wear silly hats.")],
        (Kind::TransformCorgi, 3) => &[(Danger, "Must... eat... chocolate...."), (Danger, "YAP")],
        (Kind::TransformCorgi, 4) => &[(Danger, "Visions of washing machines assail your mind!")],
        (Kind::TransformCorgi, 5) => &[(Danger, "AUUUUUU!!!")],
        (Kind::TransformMorph, 1) => &[(Plain, "Your stomach rumbles.")],
        (Kind::TransformMorph, 2) => &[(Plain, "Your skin feels saggy.")],
        (Kind::TransformMorph, 4) => &[(Danger, "You're ravenous.")],
        (Kind::TransformMorph, 5) => &[(Danger, "You have become a morph.")],
        (Kind::TransformGondola, 1) => &[(Plain, "You seem a little lighter in your step.")],
        (Kind::TransformGondola, 2) => &[(Plain, "You catch yourself smiling for no reason.")],
        (Kind::TransformGondola, 3) => &[
            (Danger, "A cruel sense of calm overcomes you."),
            (Danger, "You can't feel your arms!"),
            (Danger, "You let go of the urge to hurt clowns."),
        ],
        (Kind::TransformGondola, 4) => &[
            (
                Danger,
                "You can't feel your arms. It does not bother you anymore.",
            ),
            (Danger, "You forgive the clown for hurting you."),
        ],
        (Kind::TransformGondola, 5) => &[(Danger, "You have become a Gondola.")],
        _ => &[],
    }
}

fn tell_line(world: &mut World, mob: Entity, lines: Lines, rng: &mut CombatRng) {
    if lines.is_empty() {
        return;
    }
    let (span, text) = lines[rng.range(0, lines.len() as u32 - 1) as usize];
    say_to(world, mob, span, text);
}

/// `force_say`: the host says it out loud.
fn force_say(world: &mut World, mob: Entity, lines: &[&str], rng: &mut CombatRng) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let text = pick(rng, lines);
    let name = super::super::name_of(world, mob);
    let clients = crate::chem::clients_near(world, pos, None);
    world.write_message(ss13_net::chat::server::SendChat {
        clients,
        message: ss13_chat::server::say_message(&name, text),
    });
}

/// `/datum/disease/transformation/stage_act` and each subtype's after the base one let it act.
pub(super) fn act(
    world: &mut World,
    mob: Entity,
    disease: &mut Disease,
    secs: f32,
    rng: &mut CombatRng,
) -> Outcome {
    let kind = disease.kind;
    let stage = disease.stage;
    let stage_prob = disease.stage_prob;
    match stage {
        1 | 2 if spt_prob(rng, stage_prob, secs) => {
            tell_line(world, mob, stage_lines(kind, stage), rng);
        }
        3 | 4 if spt_prob(rng, stage_prob * 2.0, secs) => {
            tell_line(world, mob, stage_lines(kind, stage), rng);
        }
        5 => {
            if transform(world, mob, kind, rng) {
                return Outcome::Gone;
            }
        }
        _ => {}
    }
    if world.get_entity(mob).is_err() {
        return Outcome::Gone;
    }
    match (kind, stage) {
        (Kind::JungleFlu, 2) if spt_prob(rng, 1.0, secs) => {
            let part = pick(rng, &["arm", "back", "elbow", "head", "leg"]);
            say_to(world, mob, Span::Notice, format!("Your {part} itches."));
        }
        (Kind::JungleFlu, 3) if spt_prob(rng, 2.0, secs) => {
            say_to(
                world,
                mob,
                Span::Danger,
                "You feel a stabbing pain in your head.",
            );
            confusion_up_to(world, mob, 10.0, f32::MAX);
        }
        (Kind::JungleFlu, 4) if spt_prob(rng, 1.5, secs) => {
            force_say(
                world,
                mob,
                &["Eeee!", "Eeek, ook ook!", "Eee-eeek!", "Ungh, ungh."],
                rng,
            );
        }
        (Kind::TransformRobot, 3) => {
            if spt_prob(rng, 4.0, secs) {
                force_say(
                    world,
                    mob,
                    &["beep, beep!", "Beep, boop", "Boop...bop"],
                    rng,
                );
            }
            if spt_prob(rng, 2.0, secs) {
                stabbing_pain(world, mob);
            }
        }
        (Kind::TransformRobot, 4) if spt_prob(rng, 10.0, secs) => {
            force_say(
                world,
                mob,
                &[
                    "beep, beep!",
                    "Boop bop boop beep.",
                    "I wwwaaannntt tttoo dddiiieeee...",
                    "kkkiiiill mmme",
                ],
                rng,
            );
        }
        (Kind::TransformXeno, 3) if spt_prob(rng, 2.0, secs) => stabbing_pain(world, mob),
        (Kind::TransformXeno, 4) if spt_prob(rng, 10.0, secs) => {
            force_say(
                world,
                mob,
                &[
                    "Going to... devour you...",
                    "Hsssshhhhh!",
                    "You look delicious.",
                ],
                rng,
            );
        }
        (Kind::TransformSlime, 1) => {
            if is_jellyperson(world, mob) {
                disease.update_stage(5);
            }
        }
        (Kind::TransformSlime, 3) => {
            if !crate::species::has_trait(world, mob, crate::species::Trait::LesserHumanoid)
                && !is_jellyperson(world, mob)
            {
                crate::species::set_species(world, mob, crate::species::SpeciesId::Slime);
            }
        }
        (Kind::TransformCorgi, 3) if spt_prob(rng, 4.0, secs) => {
            force_say(world, mob, &["Woof!", "YAP"], rng)
        }
        (Kind::TransformCorgi, 4) if spt_prob(rng, 10.0, secs) => {
            force_say(world, mob, &["AUUUUUU", "Bark!"], rng)
        }
        (Kind::TransformGondola, 2..=4) => {
            if spt_prob(rng, 2.5, secs) {
                emote(world, mob, "smile");
            }
            if spt_prob(rng, 10.0, secs) {
                crate::medical::cyberimps::add_blood_reagent(world, mob, PAX, 5.0);
            }
            if stage == 4 && spt_prob(rng, 1.0, secs) && held_item(world, mob) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You let go of what you were holding.",
                );
                drop_held(world, mob, false, false);
            }
        }
        _ => {}
    }
    Outcome::Keep
}

/// `isjellyperson`: a jellyperson or any of its subspecies.
fn is_jellyperson(world: &World, mob: Entity) -> bool {
    use crate::species::SpeciesId::*;
    matches!(
        crate::species::species_of(world, mob),
        Jelly | Slime | Luminescent | Stargazer
    )
}

fn held_item(world: &World, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.active_item().is_some())
}

fn stabbing_pain(world: &mut World, mob: Entity) {
    say_to(
        world,
        mob,
        Span::Danger,
        "You feel a stabbing pain in your head.",
    );
    crate::status_effects::apply(world, mob, Incap::Unconscious, 4.0);
}

/// `do_disease_transformation`: true when the host is gone (a new mob took its place).
fn transform(world: &mut World, mob: Entity, kind: Kind, rng: &mut CombatRng) -> bool {
    if kind == Kind::JungleFlu {
        crate::transform::monkeyize(world, mob, false);
        return false;
    }
    if world.get::<Stat>(mob) == Some(&Stat::Dead) || crate::transform::is_transforming(world, mob)
    {
        return false;
    }
    tell_line(world, mob, stage_lines(kind, 5), rng);
    let name = world.get::<Name>(mob).cloned();
    let new = match kind {
        Kind::TransformRobot => robot(world, mob),
        Kind::TransformXeno => {
            let path = crate::antag::xeno::Caste::Hunter.path();
            crate::transform::transform_into_mob(world, mob, path, false)
        }
        Kind::TransformSlime => {
            let sentient = world.get::<PlayerMob>(mob).is_some()
                && !crate::species::has_trait(world, mob, crate::species::Trait::LesserHumanoid);
            let path = if sentient { SLIME_RANDOM } else { SLIME };
            crate::transform::transform_into_mob(world, mob, path, false)
        }
        Kind::TransformCorgi => crate::transform::transform_into_mob(world, mob, CORGI, false),
        Kind::TransformMorph => {
            let new = crate::transform::transform_into_mob(
                world,
                mob,
                crate::antag::morph::MORPH_MOB,
                false,
            );
            if let Some(new) = new {
                crate::antag::morph::make_morph(world, new);
            }
            new
        }
        Kind::TransformGondola => crate::transform::transform_into_mob(world, mob, GONDOLA, false),
        _ => None,
    };
    let Some(new) = new else {
        return false;
    };
    if let Some(mut combat) = world.get_mut::<crate::combat::CombatMode>(new) {
        combat.0 = true;
    }
    if kind == Kind::TransformXeno {
        crate::antag::xeno::make_xeno(world, new, false);
    }
    if let Some(name) = name {
        world.entity_mut(new).insert(name);
    }
    true
}

/// `new /mob/living/silicon/robot` taking the host's mind: the cyborg job's body, its radio,
/// AI link and model choice (`crate::silicon::cyborg::activate`); the host's items drop.
fn robot(world: &mut World, mob: Entity) -> Option<Entity> {
    let pos = crate::explosions::turf_of(world, mob)?;
    let borg = crate::silicon::cyborg::spawn_borg(world, pos, "Cyborg")?;
    for item in crate::health::carried_items(world, mob) {
        crate::inventory::put_down(world, item, pos);
    }
    crate::mind::transfer(world, mob, borg);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        // `attach_client` removes the old body.
        crate::silicon::cyborg::activate(world, borg, client, pos);
    }
    if world.get_entity(mob).is_ok() {
        world.despawn(mob);
    }
    crate::construction::tile_changed(world, pos);
    Some(borg)
}

/// The `expose_mob` of the reagents that infect with a disease (`aslimetoxin`,
/// `gluttonytoxin`, `cyborg_mutation_nanomachines`, `xenomicrobes`, `fungalspores`):
/// swallowed, injected, breathed or patched always; touch and vapor at `min(volume, 100)%`
/// through `touch_protection`.
pub fn reagent_expose(
    world: &mut World,
    mob: Entity,
    path: &str,
    volume: f32,
    methods: u32,
    touch_protection: f32,
) {
    use crate::janitor::{INGEST, INHALE, INJECT, PATCH, TOUCH, VAPOR};
    let kind = match path {
        ASLIMETOXIN => Kind::TransformSlime,
        // "This prevents microdosing from infecting masses of people".
        GLUTTONYTOXIN if volume > 1.0 => Kind::TransformMorph,
        NANOMACHINES => Kind::TransformRobot,
        XENOMICROBES => Kind::TransformXeno,
        FUNGALSPORES => Kind::Tuberculosis,
        _ => return,
    };
    let always = methods & (PATCH | INGEST | INJECT | INHALE) != 0;
    let chance = volume.min(100.0) * (1.0 - touch_protection);
    if !always
        && !(methods & (VAPOR | TOUCH) != 0 && world.resource_mut::<CombatRng>().chance(chance))
    {
        return;
    }
    force_contract(world, mob, &Disease::new(kind));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transformation_says_something_at_stage_five() {
        for kind in [
            Kind::TransformRobot,
            Kind::TransformXeno,
            Kind::TransformSlime,
            Kind::TransformCorgi,
            Kind::TransformMorph,
            Kind::TransformGondola,
        ] {
            assert!(!stage_lines(kind, 5).is_empty(), "{kind:?}");
            assert_eq!(kind.def().max_stages, 5);
            assert!(kind.def().bypasses_immunity);
        }
        assert_eq!(Kind::JungleFlu.def().disease_flags & CURABLE, 0);
        assert_eq!(
            Kind::from_path("/datum/disease/transformation/gondola"),
            Some(Kind::TransformGondola)
        );
    }
}
