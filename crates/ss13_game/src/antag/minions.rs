//! Minions and awakened creatures after tg `code/modules/antagonists/` `sentient_creature`,
//! `shade/shade_minion.dm`, `venus_human_trap`, `pyro_slime`, and the sentience potion that
//! makes the first (`/obj/item/slimepotion/sentience` in `research/xenobiology/xenobiology.dm`).
//!
//! - [`Minion`]: the antagonist datum of each kind — the greeting with its objective, the panel
//!   (and its info button), no round-end report (all four have `show_in_roundend = FALSE`).
//! - The intelligence potion ([`potion_interaction`]) on a conscious mindless organic basic mob
//!   asks the ghosts for 20 s; the chosen one takes the mob, which serves the user
//!   (`enslave_mind_to_creator`: the user's factions, the user a friend) as a sentient creature;
//!   the dangerous pony potion takes ponies (`SENTIENCE_PONY`) instead.
//! - A soulstone's master binds a non-cult shade ([`assign_shade_master`], from
//!   `crate::antag::cult::server::soulstone`).
//! - The venus human trap ghost role (`crate::mobs::jungle::take_bud`) and the pyroclastic
//!   anomaly's slime (`crate::events::anomalies`) call [`add_minion`].
// shortcut: the panels are the generic one (`AntagInfoSentient` and `AntagInfoShade` are not
// drawn; their text goes to chat), no potion offer reason (alt-click), no language copy,
// `TRAIT_UNCONVERTABLE` or cult/revolution/nuclear enslavement, and the Syndicate potions'
// radio implant and access are not given. Magic servant, Syndicate monkey, wishgranter avatar
// and greentext holder are missing with their sources (servant spell, monkey agent beacon,
// wish granter, greentext event).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::SENTIENCE_ORGANIC;
use ss13_mapload::{DmItem, DmMob};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};

use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::ghost_poll::{self, PollSpec};
use crate::health::Stat;
use crate::mobs::BasicMob;
use crate::server::PlayerMob;
use crate::tools::{client_of, tell};

pub const SENTIENCE_POTION: &str = "/obj/item/slimepotion/sentience";
/// `/obj/item/slimepotion/sentience/nuclear/dangerous_horse` `sentience_type`.
const DANGEROUS_HORSE: &str = "/obj/item/slimepotion/sentience/nuclear/dangerous_horse";
/// `SENTIENCE_PONY`.
const SENTIENCE_PONY: u8 = 6;
const POTION_POLL_SECS: f64 = 20.0;
/// `POLL_IGNORE_SENTIENCE_POTION`.
const POLL_IGNORE_SENTIENCE_POTION: &str = "sentience_potion";

/// Which antagonist datum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinionKind {
    /// `/datum/antagonist/sentient_creature`.
    Sentient,
    /// `/datum/antagonist/shade_minion`.
    Shade,
    /// `/datum/antagonist/venus_human_trap`.
    VenusTrap,
    /// `/datum/antagonist/pyro_slime`.
    PyroSlime,
}

impl MinionKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sentient => "Sentient Creature",
            Self::Shade => "Loyal Shade",
            Self::VenusTrap => "Venus Human Trap",
            Self::PyroSlime => "Pyroclastic Anomaly",
        }
    }

    /// The objective text, if the datum forges one.
    pub fn objective(self) -> Option<&'static str> {
        match self {
            Self::VenusTrap => Some("Spread the infestation."),
            Self::PyroSlime => Some(
                "I am fire. I am hunger. The cold is agony. The living pulse with energy; their \
                 warmth fuels me. The dead are husks, their embers long faded. Water is death. \
                 Fire... fire is freedom.",
            ),
            Self::Sentient | Self::Shade => None,
        }
    }

    /// `silent`: no greeting.
    fn silent(self) -> bool {
        self == Self::Shade
    }
}

/// On the minion's body.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Minion {
    pub kind: MinionKind,
    /// `enslaved_to` / `master_name`.
    pub master: Option<Entity>,
    pub master_name: Option<String>,
}

impl Minion {
    /// The text of the tgui panel (`AntagInfoSentient`, `AntagInfoShade`).
    pub fn panel_text(&self) -> Option<String> {
        match self.kind {
            MinionKind::Sentient => {
                let mut text = "All at once it makes sense: you know what you are and who you \
                                are! Self awareness is yours!"
                    .to_owned();
                if let Some(master) = &self.master_name {
                    text.push_str(&format!(
                        " You are grateful to be self aware and owe {master} a great debt. Serve \
                         {master}, and assist them in completing their goals at any cost."
                    ));
                }
                Some(text)
            }
            MinionKind::Shade => Some(format!(
                "You are a shade bound to {}. Serve your master and help them succeed in their \
                 goals at all costs.",
                self.master_name.as_deref().unwrap_or("nobody?")
            )),
            _ => None,
        }
    }

    fn info(&self) -> BrotherInfo {
        BrotherInfo {
            antag_name: self.kind.name().to_owned(),
            objectives: self
                .kind
                .objective()
                .map(|o| ObjectiveInfo {
                    count: 1,
                    explanation: o.to_owned(),
                    complete: false,
                })
                .into_iter()
                .collect(),
        }
    }
}

/// The minion's info panel (tg's default `AntagInfoGeneric`; see the shortcut).
pub struct MinionInfoWindow;

impl UiInterface for MinionInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for MinionInfoWindow {
    type Source = &'static Minion;
    type UserParam = ();
    type Act = ();

    fn static_data(minion: &Minion) -> BrotherInfo {
        minion.info()
    }

    fn data(_: &Minion, _: Entity, _: &()) {}
}

/// On a potion while its ghost poll runs (`being_used`).
#[derive(Component, Clone, Copy, Debug)]
pub struct PotionOffered;

pub struct MinionsPlugin;

impl Plugin for MinionsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The intelligence potion on a creature (before the slime potions of `xenobio`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 1771, |w, c| {
            c.held
                .is_some_and(|h| potion_interaction(w, c.mob, h, c.target))
        });
        app.register_ui::<MinionInfoWindow>()
            .add_systems(Update, open_info.in_set(UiSystems::Handle));
    }
}

/// `add_antag_datum` of a minion kind: the greeting (with the objective), the panel. `false`
/// when it already is a minion of that kind (a shade's master is updated instead).
pub fn add_minion(
    world: &mut World,
    mob: Entity,
    kind: MinionKind,
    master: Option<Entity>,
) -> bool {
    if world.get_entity(mob).is_err() {
        return false;
    }
    let master_name = master.map(|m| crate::security::name_of(world, m));
    if let Some(existing) = world.get::<Minion>(mob).cloned()
        && existing.kind == kind
    {
        // `update_master`: the panel again for a new master.
        if existing.master_name == master_name {
            return false;
        }
        world.entity_mut(mob).insert(Minion {
            master,
            master_name,
            ..existing
        });
        open_panel(world, mob);
        return false;
    }
    let minion = Minion {
        kind,
        master,
        master_name,
    };
    world.entity_mut(mob).insert(minion.clone());
    if world.get::<AntagHudData>(mob).is_none() {
        world.entity_mut(mob).insert(AntagHudData::default());
    }
    info!("{mob} became a {} (master {master:?})", kind.name());
    let Some(client) = client_of(world, mob) else {
        return true;
    };
    if !kind.silent() {
        // shortcut: `span_big` is drawn bold.
        tell(world, client, Span::Bold, format!("Вы - {}.", kind.name()));
        // `announce_objectives`.
        if let Some(objective) = kind.objective() {
            tell(world, client, Span::Notice, "Ваши текущие задачи:");
            world.write_message(SendChat {
                clients: vec![client],
                message: ChatMessage {
                    channel: ChatChannel::System,
                    segments: vec![
                        ChatSegment::new(Span::Bold, "Задача #1"),
                        ChatSegment::new(Span::Plain, format!(": {objective}")),
                    ],
                },
            });
        }
    }
    world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    open_panel(world, mob);
    true
}

fn open_panel(world: &mut World, mob: Entity) {
    let Some(client) = client_of(world, mob) else {
        return;
    };
    if let Some(text) = world.get::<Minion>(mob).and_then(Minion::panel_text) {
        tell(world, client, Span::Notice, text);
    }
    let name = world.get::<Minion>(mob).map_or("", |m| m.kind.name());
    world.write_message(OpenUi::new::<MinionInfoWindow>(client, mob, mob, name));
}

/// `/datum/action/antag_info/Trigger` of a minion.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<Entity> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| a.source)
        .collect();
    for source in pending {
        if world.get::<Minion>(source).is_some() {
            open_panel(world, source);
        }
    }
}

/// `/obj/item/soulstone/proc/assign_master` past the cult: a shade with a player serves the
/// stone's bearer.
pub fn assign_shade_master(world: &mut World, shade: Entity, user: Entity) {
    if world.get::<PlayerMob>(shade).is_none() {
        return;
    }
    add_minion(world, shade, MinionKind::Shade, Some(user));
}

/// `slimepotion/sentience/interact_with_atom`. `true` when handled.
pub fn potion_interaction(world: &mut World, user: Entity, potion: Entity, target: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(potion).map(|i| i.path.clone()) else {
        return false;
    };
    if !is_under(&path, SENTIENCE_POTION) {
        return false;
    }
    // `isliving`.
    if world.get::<Stat>(target).is_none() {
        return false;
    }
    if world.get::<PotionOffered>(potion).is_some() {
        return true;
    }
    if world.get::<PlayerMob>(target).is_some() {
        crate::balloon::balloon_alert(world, user, target, "already sentient!");
        return true;
    }
    if world.get::<Stat>(target) != Some(&Stat::Conscious) {
        crate::balloon::balloon_alert(world, user, target, "it's dead!");
        return true;
    }
    let wanted = if is_under(&path, DANGEROUS_HORSE) {
        SENTIENCE_PONY
    } else {
        SENTIENCE_ORGANIC
    };
    // `compare_sentience_type`: basic (and simple) mobs only.
    if world
        .get::<BasicMob>(target)
        .is_none_or(|b| b.def.sentience_type != wanted)
    {
        crate::balloon::balloon_alert(world, user, target, "invalid creature!");
        return true;
    }
    crate::balloon::balloon_alert(world, user, target, "offering...");
    world.entity_mut(potion).insert(PotionOffered);
    let user_name = crate::security::name_of(world, user);
    let target_name = crate::security::name_of(world, target);
    let spec = PollSpec {
        question: format!("{user_name} is offering {target_name} an intelligence potion!"),
        role_name: "intelligence potion".to_owned(),
        secs: POTION_POLL_SECS,
        ignore: Some(POLL_IGNORE_SENTIENCE_POTION),
        target: Some(target),
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        let chosen = ghost_poll::pick_candidate(world, &mut signed_up);
        on_poll_concluded(world, user, potion, target, chosen);
    });
    true
}

/// `on_poll_concluded`: the chosen ghost takes the creature, which serves the user. Checked
/// again now: the potion and the creature still there, the creature still mindless and alive.
fn on_poll_concluded(
    world: &mut World,
    user: Entity,
    potion: Entity,
    target: Entity,
    chosen: Option<ghost_poll::Candidate>,
) {
    if world.get_entity(potion).is_err() {
        return;
    }
    world.entity_mut(potion).remove::<PotionOffered>();
    let valid = world.get::<PlayerMob>(target).is_none()
        && world.get::<Stat>(target).is_some_and(|s| *s != Stat::Dead);
    let Some(chosen) = chosen.filter(|_| valid) else {
        if world.get_entity(user).is_ok() {
            crate::balloon::balloon_alert(world, user, user, "try again later!");
        }
        return;
    };
    if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, target) {
        return;
    }
    world.try_despawn(chosen.mob).ok();
    let master = world.get_entity(user).is_ok().then_some(user);
    if let Some(master) = master {
        enslave_to_creator(world, target, master);
    }
    add_minion(world, target, MinionKind::Sentient, master);
    if let Some(master) = master {
        crate::balloon::balloon_alert(world, master, target, "success");
    }
    let mob_path = world.get::<DmMob>(target).map(|m| m.path.clone());
    info!("{target} ({mob_path:?}) was made sentient by {user:?}");
    world.despawn(potion);
}

/// `enslave_mind_to_creator`: the creator's factions, the creator a friend.
fn enslave_to_creator(world: &mut World, mob: Entity, creator: Entity) {
    let theirs = world
        .get::<crate::mobs::Faction>(creator)
        .map(|f| f.0.clone())
        .unwrap_or_default();
    match world.get_mut::<crate::mobs::Faction>(mob) {
        Some(mut f) => {
            for faction in theirs {
                if !f.0.contains(&faction) {
                    f.0.push(faction);
                }
            }
        }
        None => {
            world.entity_mut(mob).insert(crate::mobs::Faction(theirs));
        }
    }
    crate::mobs::pets::befriend(world, mob, creator);
    if world.get::<AntagHudData>(creator).is_some() {
        let name = crate::security::name_of(world, creator);
        crate::tools::tell_mob(
            world,
            mob,
            Span::Danger,
            format!(
                "Несмотря на текущие убеждения вашего создателя, вашим истинным хозяином остаётся \
                 {name}. Если его верность сменит сторону, ваша сменится вслед за ней. Это правило \
                 останется неизменным до тех пор, пока тело вашего создателя не будет уничтожено."
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minion_panels_and_objectives() {
        let sentient = Minion {
            kind: MinionKind::Sentient,
            master: None,
            master_name: Some("Bob".into()),
        };
        assert!(
            sentient
                .panel_text()
                .unwrap()
                .contains("owe Bob a great debt")
        );
        assert!(sentient.info().objectives.is_empty());
        let trap = Minion {
            kind: MinionKind::VenusTrap,
            master: None,
            master_name: None,
        };
        assert_eq!(
            trap.info().objectives[0].explanation,
            "Spread the infestation."
        );
        assert!(trap.panel_text().is_none());
    }

    #[test]
    fn a_shade_gets_its_master_once() {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        let client = world.spawn_empty().id();
        let shade = world.spawn((Stat::Conscious, PlayerMob { client })).id();
        let master = world.spawn(Name::new("Ann")).id();
        assign_shade_master(&mut world, shade, master);
        let m = world.get::<Minion>(shade).unwrap();
        assert_eq!(m.kind, MinionKind::Shade);
        assert_eq!(m.master_name.as_deref(), Some("Ann"));
        // The same master again changes nothing.
        assert!(!add_minion(
            &mut world,
            shade,
            MinionKind::Shade,
            Some(master)
        ));
        // A mindless shade gets no datum.
        let empty = world.spawn(Stat::Conscious).id();
        assign_shade_master(&mut world, empty, master);
        assert!(world.get::<Minion>(empty).is_none());
    }
}
