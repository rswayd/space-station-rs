//! Soulstones, shades and construct shells, after tg `/obj/item/soulstone` and
//! `/obj/structure/constructshell` (`code/modules/antagonists/wizard/equipment/soulstone.dm`),
//! `/datum/component/soulstoned`, `make_new_construct` and the cult's shade and construct datums
//! (`cult/datums/shade.dm`, `constructs.dm`).
//!
//! - A soulstone on a human in crit or unconscious (the dead only for `grab_sleeping = FALSE`
//!   stones) captures the soul: a shade "Shade of X" inside the stone takes the victim's player
//!   (its body or its ghost) and the victim is dusted. A soul that fled (no player) is offered to
//!   the ghosts for 20 s ([`crate::ghost_poll`]). The cult's sacrifice of a mindful body does
//!   the same, unconditionally ([`sacrifice_soul`]).
//! - The shade rides inside the stone ([`crate::closets::InCloset`]): hidden, unable to move, on
//!   the stone's turf. Using the stone in hand releases it; the stone on a free shade captures
//!   it again (healed). A cultist's shade is a `/datum/antagonist/cult/shade` that may not join
//!   rune invocations inside the stone or within a minute of its release.
//! - The stone with a shade on a construct shell: Juggernaut, Wraith or Artificer (a list for
//!   tg's radial menu); the shade's player becomes the construct (a cult one for a cultist's,
//!   else by the stone's theme), a mindless shade's construct is offered to the ghosts for 10 s,
//!   with nobody the shell comes back.
//! - Who may use it: a cult stone needs a cultist, a mystic one a wizard (else 10 s unconscious);
//!   a purified stone burns a cultist's hand. A destroyed stone kills its shade.
// shortcut: no exorcism with a bible or corruption of purified stones (the shade minion datum is
// `crate::antag::minions`), no soulful toolbox, no suicide, no seek-master action or
// blood sense alert, no examine lines, no language copying, no shade/construct `add_ally`; the
// shade inside is protected by being unreachable (no `TRAIT_GODMODE`).

use super::*;
use crate::ghost_poll::{self, Candidate, PollSpec};
use ss13_mapload::fidelity::is_under;

pub const SOULSTONE: &str = "/obj/item/soulstone";
pub const SHELL: &str = "/obj/structure/constructshell";
/// `invoke_delay` of `/datum/antagonist/cult/shade`.
pub const SHADE_INVOKE_DELAY_SECS: f64 = 60.0;
const STONE_DMI: &str = "icons/obj/mining_zones/artefacts.dmi";
const CONSTRUCT_FORM: &str = "sound/effects/constructform.ogg";
const SHADE_POLL_SECS: f64 = 20.0;
const CONSTRUCT_POLL_SECS: f64 = 10.0;
/// `POLL_IGNORE_SHADE`.
const POLL_IGNORE_SHADE: &str = "shade";
/// `GLOB.construct_radial_images`.
pub const CONSTRUCT_CLASSES: [&str; 3] = ["Juggernaut", "Wraith", "Artificer"];

/// `theme`: the stone's color and the constructs it makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Cult,
    Wizard,
    Holy,
}

/// `required_role`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Cult,
    Wizard,
    Anybody,
}

/// The vars of a soulstone type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StoneKind {
    theme: Theme,
    role: Role,
    one_use: bool,
    grab_sleeping: bool,
}

fn stone_kind(path: &str) -> Option<StoneKind> {
    if !is_under(path, SOULSTONE) {
        return None;
    }
    let sub = |s: &str| is_under(path, &format!("{SOULSTONE}/{s}"));
    let base = StoneKind {
        theme: Theme::Cult,
        role: Role::Anybody,
        one_use: false,
        grab_sleeping: true,
    };
    Some(if sub("mystic") {
        StoneKind {
            theme: Theme::Wizard,
            role: Role::Wizard,
            ..base
        }
    } else if sub("anybody/chaplain/sparring") {
        StoneKind {
            theme: Theme::Holy,
            one_use: true,
            grab_sleeping: false,
            ..base
        }
    } else if sub("anybody/chaplain") || sub("anybody/revolver") {
        StoneKind {
            one_use: true,
            grab_sleeping: false,
            ..base
        }
    } else if sub("anybody/purified") {
        StoneKind {
            theme: Theme::Holy,
            ..base
        }
    } else if sub("anybody/mining") {
        StoneKind {
            grab_sleeping: false,
            ..base
        }
    } else if sub("anybody") {
        base
    } else {
        StoneKind {
            role: Role::Cult,
            ..base
        }
    })
}

/// A soulstone's state: its shade (tg `contents`), `spent` and `base_name`.
#[derive(Component, Clone, Debug, Default)]
pub struct Soulstone {
    pub shade: Option<Entity>,
    pub spent: bool,
    base_name: String,
}

/// A construct choice of the cultist (tg `show_radial_menu`), on the prompt entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct ConstructPrompt {
    user: Entity,
    stone: Entity,
    shell: Entity,
}

fn kind_of(world: &World, stone: Entity) -> Option<StoneKind> {
    path_of(world, stone).and_then(|p| stone_kind(&p))
}

fn state(world: &mut World, stone: Entity) -> Soulstone {
    if let Some(s) = world.get::<Soulstone>(stone) {
        return s.clone();
    }
    let s = Soulstone {
        base_name: name_of(world, stone),
        ..default()
    };
    world.entity_mut(stone).insert(s.clone());
    s
}

fn is_wizard(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::antag::wizard::server::Wizard>(mob)
        .is_some()
}

/// `role_check`.
fn role_check(world: &World, kind: StoneKind, who: Entity) -> bool {
    match kind.role {
        Role::Cult => is_cultist(world, who),
        Role::Wizard => is_wizard(world, who),
        Role::Anybody => true,
    }
}

/// The failed `role_check`: `Unconscious(10 SECONDS)`.
fn wracked(world: &mut World, user: Entity) {
    crate::status_effects::apply(world, user, crate::status_effects::Incap::Unconscious, 10.0);
    chat_to(
        world,
        user,
        Span::Danger,
        "Your body is wracked with debilitating pain!",
    );
}

/// `hot_potato`: holy magic burns a cultist's hand and the stone falls.
fn hot_potato(world: &mut World, user: Entity, stone: Entity) {
    let name = name_of(world, stone);
    chat_to(
        world,
        user,
        Span::Danger,
        format!("Holy magics residing in {name} burn your hand!"),
    );
    let zone = active_arm(world, user);
    if let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(user).cloned(),
        world.get::<Stat>(user).copied(),
    ) {
        apply_damage_zone(&mut health, &mut stat, zone, DamageType::Burn, 10.0);
        world.entity_mut(user).insert((health, stat));
    }
    if let Some(pos) = world.get::<TilePos>(user).copied() {
        crate::inventory::put_down(world, stone, pos);
    }
}

/// `update_appearance`: the icon by theme and contents, the name with the shade's.
fn refresh_stone(world: &mut World, stone: Entity) {
    let (Some(kind), Some(s)) = (
        kind_of(world, stone),
        world.get::<Soulstone>(stone).cloned(),
    ) else {
        return;
    };
    let base = match kind.theme {
        Theme::Holy => "purified_soulstone",
        Theme::Cult => "soulstone",
        Theme::Wizard => "mystic_soulstone",
    };
    let full = s.shade.is_some();
    let icon_state = if full {
        format!("{base}2")
    } else {
        base.to_owned()
    };
    if let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(STONE_DMI, &icon_state))
        && let Some(mut a) = world.get_mut::<ss13_shared::Appearance>(stone)
    {
        a.icon_state = id;
    }
    let mut name = if s.spent {
        format!("dull {}", s.base_name)
    } else {
        s.base_name.clone()
    };
    if let Some(shade) = s.shade {
        name = format!("{name}: {}", name_of(world, shade));
    }
    world.entity_mut(stone).insert(Name::new(name));
}

/// tg `/mob/living/basic/shade` `theme`: its sprite.
fn theme_shade(world: &mut World, shade: Entity, theme: Theme) {
    let state = match theme {
        Theme::Cult => return,
        Theme::Wizard => "shade_wizard",
        Theme::Holy => "shade_holy",
    };
    let Some(dmi) = world
        .get::<crate::mobs::BasicMob>(shade)
        .map(|b| b.def.icon.clone())
    else {
        return;
    };
    if let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&dmi, state))
        && let Some(mut a) = world.get_mut::<ss13_shared::Appearance>(shade)
    {
        a.icon_state = id;
    }
}

fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
    match (
        crate::explosions::turf_of(world, a),
        crate::explosions::turf_of(world, b),
    ) {
        (Some(p), Some(q)) => p.chebyshev(q).is_some_and(|d| d <= 1),
        _ => false,
    }
}

fn holds(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)))
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn unconscious_or_crit(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob).is_none_or(|s| *s >= Stat::SoftCrit)
        || crate::status_effects::knocked_out(world, mob)
}

/// The stone in hand on `target` (`attack`, `capture_shade` from the shade's
/// `item_interaction`, the shell's `item_interaction`). False when it is none of these.
pub fn soulstone_interaction(
    world: &mut World,
    user: Entity,
    stone: Entity,
    target: Entity,
) -> bool {
    let Some(kind) = kind_of(world, stone) else {
        return false;
    };
    if !adjacent(world, user, target) {
        return false;
    }
    // A shade is a basic mob: its path is its definition's.
    let target_path = path_of(world, target)
        .or_else(|| {
            world
                .get::<crate::mobs::BasicMob>(target)
                .map(|m| m.def.path.clone())
        })
        .unwrap_or_default();
    if is_under(&target_path, crate::mobs::constructs::SHADE) {
        capture_shade(world, kind, stone, target, Some(user));
        return true;
    }
    if is_under(&target_path, SHELL) {
        shell_interaction(world, kind, user, stone, target);
        return true;
    }
    if world.get::<HumanLook>(target).is_none() {
        return false;
    }
    if !role_check(world, kind, user) {
        wracked(world, user);
        return true;
    }
    if state(world, stone).spent {
        let name = name_of(world, stone);
        chat_to(
            world,
            user,
            Span::Warning,
            format!("There is no power left in {name}."),
        );
        return true;
    }
    if target == user {
        return true;
    }
    if is_cultist(world, target) && is_cultist(world, user) {
        chat_to(
            world,
            user,
            Span::CultLarge,
            "\"Come now, do not capture your bretheren's soul.\"",
        );
        return true;
    }
    if kind.theme == Theme::Holy && is_cultist(world, user) {
        hot_potato(world, user, stone);
        return true;
    }
    info!("{user} captured the soul of {target} with {stone}");
    capture_soul(world, stone, target, Some(user), false);
    true
}

/// Who takes the shade: the victim's own player (in its body or its ghost), or another ghost.
enum Controller {
    Body,
    /// A ghost's player; `mind_from`: the body whose mind it is (the victim's own ghost).
    Ghost {
        client: Entity,
        ghost: Entity,
        mind_from: Option<Entity>,
    },
}

/// `capture_soul(victim, user, forced)`: false when nothing happened.
pub fn capture_soul(
    world: &mut World,
    stone: Entity,
    victim: Entity,
    user: Option<Entity>,
    forced: bool,
) -> bool {
    let Some(kind) = kind_of(world, stone) else {
        return false;
    };
    if world.get::<HumanLook>(victim).is_none() || state(world, stone).shade.is_some() {
        return false;
    }
    if !forced {
        let user = user.expect("an unforced capture has a user");
        if is_cultist(world, user)
            && world
                .get_resource::<CultTeam>()
                .is_some_and(|t| t.is_sacrifice_target(victim))
        {
            chat_to(
                world,
                user,
                Span::CultLarge,
                "\"This soul is mine. SACRIFICE THEM!\"",
            );
            return false;
        }
        let ready = if kind.grab_sleeping {
            unconscious_or_crit(world, victim)
        } else {
            world.get::<Stat>(victim) == Some(&Stat::Dead)
        };
        if !ready {
            chat_to(world, user, Span::Danger, "Capture failed!");
            chat_to(world, user, Span::Danger, "Kill or maim the victim first!");
            return false;
        }
    }
    // `grab_ghost`: the victim's player, in the body or its ghost.
    if world.get::<PlayerMob>(victim).is_some() {
        init_shade(world, stone, victim, user, Controller::Body, false);
        return true;
    }
    if let Some(ghost) = crate::medical::brainmob::ghost_of(world, victim)
        && let Some(client) = world.get::<PlayerMob>(ghost).map(|p| p.client)
    {
        init_shade(
            world,
            stone,
            victim,
            user,
            Controller::Ghost {
                client,
                ghost,
                mind_from: Some(victim),
            },
            false,
        );
        return true;
    }
    if let Some(user) = user {
        chat_to(world, user, Span::Danger, "Capture failed!");
        chat_to(
            world,
            user,
            Span::Warning,
            "The soul has already fled its mortal frame. You attempt to bring it back...",
        );
    }
    let group = ghost_poll::ghosts(world);
    let spec = PollSpec {
        question: "Хотите сыграть за тень?".to_owned(),
        role_name: "a shade".to_owned(),
        secs: SHADE_POLL_SECS,
        ignore: Some(POLL_IGNORE_SHADE),
        target: Some(stone),
        ..default()
    };
    ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        let chosen = ghost_poll::pick_candidate(world, &mut signed_up);
        on_poll_concluded(world, stone, victim, user, chosen);
    });
    true
}

/// `on_poll_concluded`: the master still holds the stone next to the body, the stone is
/// still empty; then a ghost takes the shade.
fn on_poll_concluded(
    world: &mut World,
    stone: Entity,
    victim: Entity,
    master: Option<Entity>,
    chosen: Option<Candidate>,
) {
    if world.get_entity(victim).is_err() || world.get_entity(stone).is_err() {
        return;
    }
    if let Some(master) = master
        && (world.get_entity(master).is_err()
            || !can_act(world, master)
            || !holds(world, master, stone)
            || !adjacent(world, master, victim))
    {
        return;
    }
    let Some(chosen) = chosen else {
        if let Some(master) = master {
            chat_to(
                world,
                master,
                Span::Danger,
                "There were no spirits willing to become a shade.",
            );
        }
        return;
    };
    if world
        .get::<Soulstone>(stone)
        .is_some_and(|s| s.shade.is_some())
    {
        return;
    }
    if let Some(master) = master {
        let (name, victim_name) = (name_of(world, stone), name_of(world, victim));
        chat_to(
            world,
            master,
            Span::Info,
            format!(
                "Capture successful!: A spirit has entered {name}, taking upon the identity of \
                 {victim_name}."
            ),
        );
    }
    init_shade(
        world,
        stone,
        victim,
        master,
        Controller::Ghost {
            client: chosen.client,
            ghost: chosen.mob,
            mind_from: None,
        },
        false,
    );
}

/// `init_shade`: a shade of the victim inside the stone, its controller's, bound to the user;
/// the victim dusted with its items dropped.
fn init_shade(
    world: &mut World,
    stone: Entity,
    victim: Entity,
    user: Option<Entity>,
    controller: Controller,
    message_user: bool,
) {
    let (Some(kind), Some(pos)) = (
        kind_of(world, stone),
        crate::explosions::turf_of(world, stone),
    ) else {
        return;
    };
    let Some(shade) = crate::mobs::spawn::spawn_mob(world, crate::mobs::constructs::SHADE, pos)
    else {
        warn!("no shade prototype");
        return;
    };
    let victim_name = name_of(world, victim);
    world
        .entity_mut(shade)
        .insert(Name::new(format!("Shade of {victim_name}")));
    theme_shade(world, shade, kind.theme);
    crate::closets::put_mob_inside(world, shade, stone);
    state(world, stone);
    if let Some(mut s) = world.get_mut::<Soulstone>(stone) {
        s.shade = Some(shade);
    }
    let moved = match controller {
        Controller::Body => crate::medical::brainmob::transfer_mind(world, victim, shade),
        Controller::Ghost {
            client,
            ghost,
            mind_from,
        } => {
            if let Some(from) = mind_from {
                crate::mind::transfer(world, from, shade);
            }
            let moved = crate::medical::brainmob::possess(world, client, ghost, shade);
            if moved {
                world.try_despawn(ghost).ok();
            }
            moved
        }
    };
    if !moved {
        warn!("{shade}: the soul of {victim} did not move into its shade");
    }
    if let Some(user) = user {
        let cult_user = is_cultist(world, user);
        if cult_user {
            make_shade_cultist(world, shade, None);
            chat_to(
                world,
                shade,
                Span::Bold,
                "Your soul has been captured! You are now bound to the cult's will. Help them \
                 succeed in their goals at all costs.",
            );
        } else if role_check(world, kind, user) {
            let master = name_of(world, user);
            chat_to(
                world,
                shade,
                Span::Bold,
                format!(
                    "Your soul has been captured! You are now bound to {master}'s will. Help \
                     them succeed in their goals at all costs."
                ),
            );
            crate::antag::minions::assign_shade_master(world, shade, user);
        }
        if message_user {
            chat_to(
                world,
                user,
                Span::Info,
                format!(
                    "Capture successful!: {victim_name}'s soul has been ripped from their body \
                     and stored within {}.",
                    name_of(world, stone)
                ),
            );
        }
    }
    refresh_stone(world, stone);
    crate::health::dust_with(world, victim, false, true, true);
    info!("{victim} became the shade {shade} in {stone}");
}

/// `add_antag_datum(/datum/antagonist/cult/shade)` (or the form of a shade that already serves),
/// with the time of its release.
fn make_shade_cultist(world: &mut World, shade: Entity, released_at: Option<f64>) {
    let form = CultForm::Shade { released_at };
    if let Some(mut c) = world.get_mut::<Cultist>(shade) {
        c.form = form;
        return;
    }
    add_cult_minion(world, shade, form);
}

/// `capture_shade`: a free shade back into the stone, healed.
fn capture_shade(
    world: &mut World,
    kind: StoneKind,
    stone: Entity,
    shade: Entity,
    user: Option<Entity>,
) {
    if let Some(user) = user
        && !role_check(world, kind, user)
    {
        wracked(world, user);
        return;
    }
    if state(world, stone).shade.is_some() {
        if let Some(user) = user {
            let name = name_of(world, stone);
            chat_to(world, user, Span::Danger, "Capture failed!");
            chat_to(
                world,
                user,
                Span::Danger,
                format!("{name} is full! Free an existing soul to make room."),
            );
        }
        return;
    }
    crate::closets::put_mob_inside(world, shade, stone);
    if let Some(pos) = crate::explosions::turf_of(world, stone) {
        world.entity_mut(shade).insert(pos);
    }
    crate::spells::effects::full_heal(world, shade);
    if let Some(mut s) = world.get_mut::<Soulstone>(stone) {
        s.shade = Some(shade);
    }
    let name = name_of(world, stone);
    chat_to(
        world,
        shade,
        Span::Notice,
        format!(
            "Your soul has been captured by {name}. Its arcane energies are reknitting your \
             ethereal form."
        ),
    );
    if let Some(mut c) = world.get_mut::<Cultist>(shade)
        && matches!(c.form, CultForm::Shade { .. })
    {
        c.form = CultForm::Shade { released_at: None };
    }
    if let Some(user) = user
        && user != shade
    {
        let shade_name = name_of(world, shade);
        chat_to(
            world,
            user,
            Span::Info,
            format!(
                "Capture successful!: {shade_name}'s soul has been captured and stored within \
                 {name}."
            ),
        );
        // `assign_master`: a cultist's stone binds the shade to the cult, anyone else's to them.
        if is_cultist(world, user) && world.get::<PlayerMob>(shade).is_some() {
            make_shade_cultist(world, shade, None);
        } else {
            crate::antag::minions::assign_shade_master(world, shade, user);
        }
    }
    refresh_stone(world, stone);
}

/// `attack_self`: the shades out onto the user's turf.
pub fn soulstone_attack_self(world: &mut World, user: Entity, stone: Entity) -> bool {
    let Some(kind) = kind_of(world, stone) else {
        return false;
    };
    if !role_check(world, kind, user) {
        wracked(world, user);
        return true;
    }
    if kind.theme == Theme::Holy && is_cultist(world, user) {
        hot_potato(world, user, stone);
        return true;
    }
    release_shades(world, kind, user, stone);
    true
}

/// `release_shades`.
fn release_shades(world: &mut World, kind: StoneKind, user: Entity, stone: Entity) {
    let Some(shade) = state(world, stone).shade else {
        return;
    };
    let Some(pos) = world.get::<TilePos>(user).copied() else {
        return;
    };
    crate::closets::let_out(world, shade);
    world.entity_mut(shade).insert(pos);
    let spent = kind.one_use;
    if let Some(mut s) = world.get_mut::<Soulstone>(stone) {
        s.shade = None;
        s.spent |= spent;
    }
    if is_cultist(world, user) {
        chat_to(
            world,
            shade,
            Span::Bold,
            "You have been released from your prison, but you are still bound to the cult's \
             will. Help them succeed in their goals at all costs.",
        );
    } else if role_check(world, kind, user) {
        let master = name_of(world, user);
        chat_to(
            world,
            shade,
            Span::Bold,
            format!(
                "You have been released from your prison, but you are still bound to {master}'s \
                 will. Help them succeed in their goals at all costs."
            ),
        );
    }
    let t = now(world);
    if let Some(mut c) = world.get_mut::<Cultist>(shade)
        && matches!(c.form, CultForm::Shade { .. })
    {
        c.form = CultForm::Shade {
            released_at: Some(t),
        };
    }
    refresh_stone(world, stone);
}

/// The shell's `item_interaction` with a soulstone.
fn shell_interaction(
    world: &mut World,
    kind: StoneKind,
    user: Entity,
    stone: Entity,
    shell: Entity,
) {
    let gifted = is_cultist(world, user) || is_wizard(world, user);
    if !gifted && kind.theme != Theme::Holy {
        let name = name_of(world, stone);
        chat_to(
            world,
            user,
            Span::Danger,
            format!(
                "An overwhelming feeling of dread comes over you as you attempt to place {name} \
                 into the shell. It would be wise to be rid of this quickly."
            ),
        );
        crate::status_effects::set_timed(
            world,
            user,
            crate::status_effects::Timed::Dizziness,
            60.0,
            true,
        );
        return;
    }
    if kind.theme == Theme::Holy && is_cultist(world, user) {
        hot_potato(world, user, stone);
        return;
    }
    // `transfer_to_construct`.
    if state(world, stone).shade.is_none() {
        let name = name_of(world, stone);
        chat_to(
            world,
            user,
            Span::Danger,
            format!("Creation failed!: {name} is empty! Go kill someone!"),
        );
        return;
    }
    let Some(client) = world.get::<PlayerMob>(user).map(|p| p.client) else {
        return;
    };
    let title = name_of(world, shell);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            user,
            title,
            "",
            prompt::PromptKind::List {
                items: CONSTRUCT_CLASSES.iter().map(|c| (*c).to_owned()).collect(),
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(ConstructPrompt { user, stone, shell });
}

/// The class picked: `check_menu` again, then `make_new_construct_from_class`; the shell and the
/// stone are used up.
pub(super) fn construct_picked(
    world: &mut World,
    mut answers: Local<MessageCursor<PromptAnswered>>,
) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(asked) = world.get::<ConstructPrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&class) = CONSTRUCT_CLASSES.get(i) else {
            continue;
        };
        let ConstructPrompt { user, stone, shell } = asked;
        if world.get_entity(shell).is_err()
            || world.get_entity(user).is_err()
            || !can_act(world, user)
            || !holds(world, user, stone)
            || !adjacent(world, user, shell)
        {
            continue;
        }
        let (Some(kind), Some(shade)) = (
            kind_of(world, stone),
            world.get::<Soulstone>(stone).and_then(|s| s.shade),
        ) else {
            continue;
        };
        let Some(pos) = world.get::<TilePos>(shell).copied() else {
            continue;
        };
        // `shade.mind?.remove_antag_datum(/datum/antagonist/cult)`.
        if world.get::<Cultist>(shade).is_some() {
            remove_cultist(world, shade, true);
        }
        let path = construct_path(class, kind.theme, is_cultist(world, user));
        make_construct(world, path, shade, Some(user), pos);
        world.despawn(shell);
        world.despawn(stone);
    }
}

/// `make_new_construct_from_class`: a cultist's construct ignores the theme.
pub fn construct_path(class: &str, theme: Theme, cultist: bool) -> String {
    let class = class.to_lowercase();
    let base = format!("{}/{class}", crate::mobs::constructs::CONSTRUCT);
    if cultist {
        return base;
    }
    match theme {
        Theme::Wizard => format!("{base}/mystic"),
        Theme::Holy => format!("{base}/angelic"),
        Theme::Cult if class == "artificer" => format!("{base}/noncult"),
        Theme::Cult => base,
    }
}

/// `make_new_construct`: the construct on `pos`, `target`'s player in it (or a polled ghost's);
/// bound to the creator.
pub fn make_construct(
    world: &mut World,
    path: String,
    target: Entity,
    creator: Option<Entity>,
    pos: TilePos,
) {
    let Some(construct) = crate::mobs::spawn::spawn_mob(world, &path, pos) else {
        warn!("no prototype for {path}");
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, CONSTRUCT_FORM, 50.0, true),
    );
    if world.get::<PlayerMob>(target).is_some() {
        crate::medical::brainmob::transfer_mind(world, target, construct);
        bind_construct(world, construct, creator);
        return;
    }
    if let Some(creator) = creator {
        chat_to(world, creator, Span::Danger, "Shell imbuement failed!");
        chat_to(
            world,
            creator,
            Span::Warning,
            "The soul has already fled its mortal frame. You attempt to bring it back...",
        );
    }
    let name = name_of(world, construct);
    let group = ghost_poll::ghosts(world);
    let spec = PollSpec {
        question: format!("Хотите сыграть за {name}?"),
        role_name: "inactive construct".to_owned(),
        secs: CONSTRUCT_POLL_SECS,
        ignore: Some(POLL_IGNORE_SHADE),
        target: Some(construct),
        ..default()
    };
    ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        if world.get_entity(construct).is_err() {
            return;
        }
        let Some(chosen) = ghost_poll::pick_candidate(world, &mut signed_up) else {
            if let Some(creator) = creator
                && world.get_entity(creator).is_ok()
            {
                chat_to(
                    world,
                    creator,
                    Span::Danger,
                    "There were no spirits willing to become a construct.",
                );
            }
            let at = crate::explosions::turf_of(world, construct).unwrap_or(pos);
            crate::construction::spawn_obj(world, SHELL, at);
            world.despawn(construct);
            return;
        };
        if let Some(creator) = creator
            && world.get_entity(creator).is_ok()
        {
            chat_to(
                world,
                creator,
                Span::Notice,
                format!("A new soul has possessed {name}!"),
            );
        }
        if crate::medical::brainmob::possess(world, chosen.client, chosen.mob, construct) {
            world.try_despawn(chosen.mob).ok();
        }
        bind_construct(world, construct, creator);
    });
}

/// The end of `make_new_construct`: a cultist creator's construct joins the cult.
fn bind_construct(world: &mut World, construct: Entity, creator: Option<Entity>) {
    let Some(creator) = creator.filter(|&c| world.get_entity(c).is_ok()) else {
        return;
    };
    let master = name_of(world, creator);
    if is_cultist(world, creator) {
        if world.get::<PlayerMob>(construct).is_some() && !is_cultist(world, construct) {
            add_cult_minion(world, construct, CultForm::Construct);
        }
        chat_to(
            world,
            construct,
            Span::Cult,
            format!(
                "You are still bound to serve the cult and {master}, follow their orders and help \
                 them complete their goals at all costs."
            ),
        );
    } else {
        chat_to(
            world,
            construct,
            Span::BoldWarning,
            format!(
                "You are still bound to serve your creator, {master}, follow their orders and \
                 help them complete their goals at all costs."
            ),
        );
    }
}

/// The Offer rune's `do_sacrifice`: a new soulstone on the rune; a body with a mind leaves its
/// shade in it (`capture_soul(forced = TRUE)`, the body is dust then).
pub(super) fn sacrifice_soul(world: &mut World, victim: Entity, invoker: Entity, pos: TilePos) {
    let Some(stone) = crate::construction::spawn_obj(world, SOULSTONE, pos) else {
        return;
    };
    if world.get::<crate::mind::MindData>(victim).is_some() {
        capture_soul(world, stone, victim, Some(invoker), true);
    }
}

/// The shade follows its stone's turf; a shade gone empties the stone (`Exited`).
// Cost: one pass over the soulstones holding a shade (a handful) per tick.
pub(super) fn follow_stones(world: &mut World) {
    let stones: Vec<(Entity, Entity)> = world
        .query::<(Entity, &Soulstone)>()
        .iter(world)
        .filter_map(|(e, s)| s.shade.map(|m| (e, m)))
        .collect();
    for (stone, shade) in stones {
        let inside = world
            .get::<crate::closets::InCloset>(shade)
            .is_some_and(|c| c.closet == stone);
        if !inside {
            if let Some(mut s) = world.get_mut::<Soulstone>(stone) {
                s.shade = None;
            }
            refresh_stone(world, stone);
            continue;
        }
        if let Some(pos) = crate::explosions::turf_of(world, stone)
            && world.get::<TilePos>(shade) != Some(&pos)
        {
            world.entity_mut(shade).insert(pos);
        }
    }
}

/// `soulstone/Destroy`: the shade inside dies where the stone was.
pub(super) fn stone_destroyed(
    remove: On<Remove<Soulstone>>,
    stones: Query<&Soulstone>,
    mut commands: Commands,
) {
    let Ok(Soulstone {
        shade: Some(shade), ..
    }) = stones.get(remove.entity)
    else {
        return;
    };
    let (shade, stone) = (*shade, remove.entity);
    commands.queue(move |world: &mut World| {
        if !world
            .get::<crate::closets::InCloset>(shade)
            .is_some_and(|c| c.closet == stone)
        {
            return;
        }
        crate::closets::let_out(world, shade);
        if let Some(mut stat) = world.get_mut::<Stat>(shade) {
            *stat = Stat::Dead;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_kinds_follow_their_types() {
        let cult = stone_kind(SOULSTONE).unwrap();
        assert_eq!((cult.theme, cult.role), (Theme::Cult, Role::Cult));
        assert!(cult.grab_sleeping && !cult.one_use);
        let mystic = stone_kind("/obj/item/soulstone/mystic").unwrap();
        assert_eq!((mystic.theme, mystic.role), (Theme::Wizard, Role::Wizard));
        let purified = stone_kind("/obj/item/soulstone/anybody/purified").unwrap();
        assert_eq!(
            (purified.theme, purified.role),
            (Theme::Holy, Role::Anybody)
        );
        let revolver = stone_kind("/obj/item/soulstone/anybody/revolver").unwrap();
        assert!(revolver.one_use && !revolver.grab_sleeping);
        assert!(
            !stone_kind("/obj/item/soulstone/anybody/mining")
                .unwrap()
                .grab_sleeping
        );
        assert!(stone_kind("/obj/item/ectoplasm").is_none());
    }

    #[test]
    fn constructs_follow_the_creator_and_theme() {
        let c = crate::mobs::constructs::CONSTRUCT;
        assert_eq!(
            construct_path("Artificer", Theme::Holy, true),
            format!("{c}/artificer")
        );
        assert_eq!(
            construct_path("Artificer", Theme::Cult, false),
            format!("{c}/artificer/noncult")
        );
        assert_eq!(
            construct_path("Juggernaut", Theme::Wizard, false),
            format!("{c}/juggernaut/mystic")
        );
        assert_eq!(
            construct_path("Wraith", Theme::Holy, false),
            format!("{c}/wraith/angelic")
        );
    }
}
