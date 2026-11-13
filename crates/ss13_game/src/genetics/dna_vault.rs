//! The DNA vault station goal (server), after tg `code/modules/station_goals/dna_vault.dm` and
//! the DNA probe (`code/game/objects/items/dna_probe.dm`).
//!
//! - The goal ([`roll_targets`], [`goal_report`]) asks for 16-20 animals, 15-30% of the rare
//!   (non-standard) plants and 75-100% of the ready players' DNA; its board and probes come
//!   from cargo (`crate::station_goals::on_report`).
//! - A DNA probe links to the first vault it touches, then samples harvestable plants in trays,
//!   humans (organic, not lesser humanoids, by unique identity) and animals (basic mobs,
//!   xenomorphs, fish, monkeys; by type), once each per probe and vault; touching a vault again
//!   uploads what is new.
//! - The vault's `DnaVault` window shows the samples against the goal; once every count is met
//!   each viewer gets two of the vault's gene therapies to pick one from (once per body).
// shortcut: the vault's gene therapy mutations (`vault_mutation.dm`) are not ported yet
// (`docs/parity/genetics/mutations.md`): the choices show, but taking one only says so and
// keeps the choice; no 3x3 filler structures (only the vault's own tile is dense), no Ark
// shuttle unlock, no power use; the carp scanner is a plain probe.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, DmMob, DmObj};
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::dna_vault::{DnaVaultData, DnaVaultUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject};

use crate::obj_damage::is_under;
use crate::tools::tell;

pub const VAULT: &str = "/obj/machinery/dna_vault";
pub const PROBE: &str = "/obj/item/dna_probe";
/// "High defaults so it's not completed automatically if there's no station goal".
const DEFAULT_MAX: u32 = 100;
/// `animal_typecache`.
const ANIMALS: [&str; 4] = [
    "/mob/living/basic",
    "/mob/living/carbon/alien",
    "/mob/living/simple_animal",
    "/obj/item/fish",
];
/// Basic mobs with `MOB_ROBOTIC` biotypes (basic mobs carry no biotypes here).
const ROBOTIC_ANIMALS: [&str; 2] = ["/mob/living/basic/bot", "/mob/living/basic/drone"];
/// The monkey's type (`/mob/living/carbon/human/species/monkey`) for a monkey body.
const MONKEY: &str = "/mob/living/carbon/human/species/monkey";
const SCAN_SOUND: &str = "sound/machines/compiler/compiler-stage2.ogg";
const UPLOAD_SOUND: &str = "sound/machines/compiler/compiler-stage1.ogg";
const LINK_SOUND: &str = "sound/machines/terminal/terminal_success.ogg";
const FAIL_SOUND: &str = "sound/machines/buzz/buzz-sigh.ogg";

/// The gene therapies (`roll_powers`' `possible_powers` and `upgrade`'s names).
pub const POWERS: [(&str, &str); 7] = [
    ("Breathless", "/datum/mutation/breathless"),
    ("Dextrous", "/datum/mutation/dextrous"),
    ("Quick", "/datum/mutation/quick"),
    ("Fire Immunity", "/datum/mutation/fire_immunity"),
    ("Plasmocile", "/datum/mutation/plasmocile"),
    ("Quick Recovery", "/datum/mutation/quick_recovery"),
    ("Tough", "/datum/mutation/tough"),
];

/// `/datum/station_goal/dna_vault`'s sample counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaultTargets {
    pub animals: u32,
    pub plants: u32,
    pub humans: u32,
}

/// `/obj/machinery/dna_vault`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct DnaVault {
    pub animals_max: u32,
    pub plants_max: u32,
    pub dna_max: u32,
    pub animal_dna: Vec<String>,
    pub plant_dna: Vec<String>,
    pub human_dna: Vec<String>,
    pub completed: bool,
    /// `power_lottery`: each viewer's two [`POWERS`] (empty once used).
    pub lottery: Vec<(Entity, Vec<usize>)>,
}

impl DnaVault {
    fn new(targets: Option<VaultTargets>) -> Self {
        let t = targets.unwrap_or(VaultTargets {
            animals: DEFAULT_MAX,
            plants: DEFAULT_MAX,
            humans: DEFAULT_MAX,
        });
        Self {
            animals_max: t.animals,
            plants_max: t.plants,
            dna_max: t.humans,
            animal_dna: Vec::new(),
            plant_dna: Vec::new(),
            human_dna: Vec::new(),
            completed: false,
            lottery: Vec::new(),
        }
    }

    /// `check_goal`.
    fn check_goal(&mut self) {
        if self.plant_dna.len() as u32 >= self.plants_max
            && self.animal_dna.len() as u32 >= self.animals_max
            && self.human_dna.len() as u32 >= self.dna_max
        {
            self.completed = true;
        }
    }

    fn choices(&self, user: Entity) -> Option<&[usize]> {
        self.lottery
            .iter()
            .find(|(e, _)| *e == user)
            .map(|(_, c)| c.as_slice())
            .filter(|c| c.len() >= 2)
    }
}

/// `/obj/item/dna_probe`: its vault link and samples not uploaded yet.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DnaProbe {
    pub vault: Option<Entity>,
    pub animals: Vec<String>,
    pub plants: Vec<String>,
    pub humans: Vec<String>,
}

/// `TRAIT_USED_DNA_VAULT`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UsedDnaVault;

pub struct DnaVaultPlugin;

impl Plugin for DnaVaultPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `interact_with_atom`: the probe on a vault, a tray, a human or an animal.
        app.add_click_handler(ClickPhase::HeldOnTarget, 45, |w, c| {
            c.held
                .is_some_and(|probe| probe_interact(w, c.client, c.mob, probe, c.target))
        });
        // `ui_interact`.
        app.add_click_handler(ClickPhase::EmptyHand, 3605, |w, c| {
            open_vault(w, c.client, c.mob, c.target)
        });
        app.register_ui::<DnaVaultWindow>()
            .add_observer(on_vault_added)
            .add_observer(on_probe_added)
            .add_systems(
                Update,
                vault_acts.in_set(ss13_tgui::server::UiSystems::Handle),
            );
    }
}

/// `/datum/station_goal/dna_vault/New`.
pub fn roll_targets(world: &World, rng: &mut Rng) -> VaultTargets {
    let ready = world
        .get_resource::<crate::round::Ticker>()
        .map_or(0, |t| t.ready_players());
    let rare = world
        .get_resource::<crate::botany::Botany>()
        .map_or(0, |b| {
            b.0.data.seeds.iter().filter(|s| s.rarity > 0.0).count()
        });
    let round = |x: f64| x.round() as i64;
    VaultTargets {
        animals: rng.range(16, 20) as u32,
        humans: rng.range(round(0.75 * f64::from(ready)), i64::from(ready)) as u32,
        plants: rng.range(round(0.15 * rare as f64), round(0.3 * rare as f64)) as u32,
    }
}

/// `/datum/station_goal/dna_vault/get_report`.
pub fn goal_report(t: &VaultTargets) -> String {
    [
        "#### ДНК хранилище".to_owned(),
        "Наши системы долгосрочного прогнозирования указывают на 99% вероятность катастрофы в \
         масштабах всей системы в ближайшем будущем."
            .to_owned(),
        "Вам необходимо построить ДНК хранилище на борту вашей станции. Хранилище должно \
         содержать образцы:"
            .to_owned(),
        format!("- {} уникальных данных о животных", t.animals),
        format!("- {} уникальных данных о нестандартных растениях", t.plants),
        format!(
            "- {} уникальных данных о разумных гуманоидных существах",
            t.humans
        ),
        String::new(),
        "Базовые модули хранилища доступны для заказа через отдел снабжения.".to_owned(),
    ]
    .join("\n")
}

/// `/datum/station_goal/dna_vault/check_completion`: a vault holds enough of everything.
pub fn goal_met(world: &mut World, t: &VaultTargets) -> bool {
    world.query::<&DnaVault>().iter(world).any(|v| {
        v.animal_dna.len() as u32 >= t.animals
            && v.plant_dna.len() as u32 >= t.plants
            && v.human_dna.len() as u32 >= t.humans
    })
}

/// `/obj/machinery/dna_vault/Initialize`: the goal's counts, or the high defaults.
fn on_vault_added(
    add: On<Add<DmObj>>,
    objs: Query<&DmObj>,
    goals: Option<Res<crate::station_goals::StationGoals>>,
    mut commands: Commands,
) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&obj.path, VAULT) {
        return;
    }
    let targets = goals.and_then(|g| g.goals.iter().find_map(|goal| goal.vault));
    commands
        .entity(add.entity)
        .try_insert(DnaVault::new(targets));
}

fn on_probe_added(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|item| is_under(&item.path, PROBE))
    {
        commands.entity(add.entity).try_insert(DnaProbe::default());
    }
}

fn sound(world: &mut World, at: Entity, file: &str) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, 50.0, true));
    }
}

fn alert(world: &mut World, user: Entity, probe: Entity, text: &str) {
    crate::balloon::balloon_alert(world, user, probe, text);
}

/// A sample a probe takes, by its key (seed type, unique identity, mob or item type).
enum Sample {
    Plant(String),
    Human(String),
    Animal(String),
}

/// `valid_scan_target` and the sample `scan_dna` would take; `Err` with the balloon text when
/// the target is valid but gives nothing.
fn sample_of(world: &World, target: Entity) -> Option<Result<Sample, &'static str>> {
    if let Some(tray) = world.get::<crate::botany::tray::Tray>(target) {
        let Some(seed) = &tray.myseed else {
            return Some(Err(""));
        };
        if tray.plant_status != crate::botany::tray::PlantStatus::Harvestable {
            // "So it's bit harder".
            return Some(Err("plant is not harvestable!"));
        }
        return Some(Ok(Sample::Plant(seed.path.clone())));
    }
    let lesser = crate::species::has_trait(world, target, crate::species::Trait::LesserHumanoid);
    if world.get::<crate::human::HumanLook>(target).is_some() {
        let organic =
            crate::species::def_of(world, target).biotypes & crate::species::biotype::ORGANIC != 0;
        if lesser {
            return Some(if organic {
                Ok(Sample::Animal(MONKEY.to_owned()))
            } else {
                Err("no compatible dna!")
            });
        }
        if !organic {
            return Some(Err("no compatible dna!"));
        }
        let identity = world
            .get::<super::Dna>(target)
            .map(|d| d.unique_identity.clone())?;
        return Some(Ok(Sample::Human(identity)));
    }
    let path = world
        .get::<DmMob>(target)
        .map(|m| m.path.as_str())
        .or_else(|| world.get::<DmItem>(target).map(|i| i.path.as_str()))?;
    if !ANIMALS.iter().any(|root| is_under(path, root)) {
        return None;
    }
    if ROBOTIC_ANIMALS.iter().any(|root| is_under(path, root)) {
        return Some(Err("no compatible dna!"));
    }
    Some(Ok(Sample::Animal(path.to_owned())))
}

/// `/obj/item/dna_probe/interact_with_atom`.
fn probe_interact(
    world: &mut World,
    client: Entity,
    user: Entity,
    probe: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<DnaProbe>(probe).cloned() else {
        return false;
    };
    let linked = state.vault.filter(|&v| world.get::<DnaVault>(v).is_some());
    if world.get::<DnaVault>(target).is_some() {
        match linked {
            // Any vault takes the upload once the probe is linked to one.
            Some(_) => upload(world, client, probe, target),
            None => {
                if let Some(mut p) = world.get_mut::<DnaProbe>(probe) {
                    p.vault = Some(target);
                }
                alert(world, user, probe, "vault linked");
                sound(world, probe, LINK_SOUND);
            }
        }
        return true;
    }
    let Some(sample) = sample_of(world, target) else {
        return false;
    };
    let Some(vault) = linked else {
        sound(world, user, FAIL_SOUND);
        alert(world, user, probe, "need database!");
        return true;
    };
    let sample = match sample {
        Ok(sample) => sample,
        Err(text) => {
            if !text.is_empty() {
                alert(world, user, probe, text);
            }
            return true;
        }
    };
    let v = world.get::<DnaVault>(vault).expect("linked vault");
    let (in_vault, in_probe, key) = match &sample {
        Sample::Plant(k) => (v.plant_dna.contains(k), state.plants.contains(k), k),
        Sample::Human(k) => (v.human_dna.contains(k), state.humans.contains(k), k),
        Sample::Animal(k) => (v.animal_dna.contains(k), state.animals.contains(k), k),
    };
    if in_vault {
        alert(world, user, probe, "data already in vault!");
        return true;
    }
    if in_probe {
        alert(world, user, probe, "data already in scanner!");
        return true;
    }
    let key = key.clone();
    if let Some(mut p) = world.get_mut::<DnaProbe>(probe) {
        match sample {
            Sample::Plant(_) => p.plants.push(key),
            Sample::Human(_) => p.humans.push(key),
            Sample::Animal(_) => p.animals.push(key),
        }
    }
    sound(world, probe, SCAN_SOUND);
    alert(world, user, probe, "data added");
    true
}

/// `try_upload_dna`: what the vault lacks moves in; the rest is dropped from the probe.
fn upload(world: &mut World, client: Entity, probe: Entity, vault: Entity) {
    let Some(mut p) = world.get::<DnaProbe>(probe).cloned() else {
        return;
    };
    let Some(mut v) = world.get::<DnaVault>(vault).cloned() else {
        return;
    };
    let mut uploaded = 0;
    for (from, into) in [
        (&mut p.plants, &mut v.plant_dna),
        (&mut p.humans, &mut v.human_dna),
        (&mut p.animals, &mut v.animal_dna),
    ] {
        from.retain(|k| !into.contains(k));
        uploaded += from.len();
        into.append(from);
    }
    v.check_goal();
    world.entity_mut(probe).insert(p);
    world.entity_mut(vault).insert(v);
    sound(world, vault, UPLOAD_SOUND);
    tell(
        world,
        client,
        Span::Notice,
        format!("{uploaded} new datapoints uploaded."),
    );
}

/// `ui_interact`: `roll_powers` for a living viewer without a roll, then the window.
fn open_vault(world: &mut World, client: Entity, user: Entity, vault: Entity) -> bool {
    if world.get::<DnaVault>(vault).is_none() {
        return false;
    }
    let alive = world.get::<crate::health::Stat>(user) != Some(&crate::health::Stat::Dead);
    let rolled = world
        .get::<DnaVault>(vault)
        .is_some_and(|v| v.lottery.iter().any(|(e, _)| *e == user));
    if alive && !rolled {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        let first = rng.range(0, POWERS.len() as u32 - 1) as usize;
        let second = {
            let r = rng.range(0, POWERS.len() as u32 - 2) as usize;
            if r >= first { r + 1 } else { r }
        };
        if let Some(mut v) = world.get_mut::<DnaVault>(vault) {
            v.lottery.push((user, vec![first, second]));
        }
    }
    world.write_message(OpenUi::new::<DnaVaultWindow>(
        client,
        user,
        vault,
        "DNA Vault",
    ));
    true
}

pub struct DnaVaultWindow;

impl UiInterface for DnaVaultWindow {
    const NAME: &'static str = DnaVaultUi::NAME;
    type Data = DnaVaultData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneChoice(pub String);

impl ServerUi for DnaVaultWindow {
    type Source = &'static DnaVault;
    type UserParam = ();
    type Act = GeneChoice;

    fn static_data(_: &DnaVault) {}

    fn data(v: &DnaVault, user: Entity, _: &SystemParamItem<'_, '_, ()>) -> DnaVaultData {
        let choices = v.completed.then(|| v.choices(user)).flatten();
        let name = |i: Option<usize>| i.map_or(String::new(), |i| POWERS[i].0.to_owned());
        DnaVaultData {
            plants: v.plant_dna.len() as u32,
            plants_max: v.plants_max,
            animals: v.animal_dna.len() as u32,
            animals_max: v.animals_max,
            dna: v.human_dna.len() as u32,
            dna_max: v.dna_max,
            completed: v.completed,
            used: choices.is_none(),
            choice_a: name(choices.map(|c| c[0])),
            choice_b: name(choices.map(|c| c[1])),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<GeneChoice, UiReject> {
        match action {
            "gene" => Ok(GeneChoice(params.text("choice")?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `upgrade(usr, choice)`: one of the viewer's own rolled therapies, once per body.
fn vault_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<DnaVaultWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<DnaVaultWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.0.clone()))
        .collect();
    for (client, user, vault, choice) in pending {
        let Some(v) = world.get::<DnaVault>(vault) else {
            continue;
        };
        let Some(index) = POWERS.iter().position(|(name, _)| *name == choice) else {
            continue;
        };
        let offered = v.completed && v.choices(user).is_some_and(|c| c.contains(&index));
        if !offered || world.get::<UsedDnaVault>(user).is_some() {
            continue;
        }
        let Some(id) = super::mutation_id(POWERS[index].1) else {
            tell(
                world,
                client,
                Span::Warning,
                "Эта генная терапия пока недоступна.",
            );
            continue;
        };
        super::add_mutation(world, user, id, super::SOURCE_ACTIVATED);
        world.entity_mut(user).insert(UsedDnaVault);
        if let Some(mut v) = world.get_mut::<DnaVault>(vault)
            && let Some(entry) = v.lottery.iter_mut().find(|(e, _)| *e == user)
        {
            entry.1.clear();
        }
    }
}

/// A probe's samples not uploaded yet: plants, humans, animals.
#[cfg(test)]
pub(crate) fn probe_samples(world: &World, probe: Entity) -> (usize, usize, usize) {
    world.get::<DnaProbe>(probe).map_or((0, 0, 0), |p| {
        (p.plants.len(), p.humans.len(), p.animals.len())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_goal_is_met_when_every_count_is() {
        let mut v = DnaVault::new(Some(VaultTargets {
            animals: 1,
            plants: 1,
            humans: 2,
        }));
        v.animal_dna.push("/mob/living/basic/cow".into());
        v.plant_dna.push("/obj/item/seeds/ambrosia/gaia".into());
        v.human_dna.push("abc".into());
        v.check_goal();
        assert!(!v.completed);
        v.human_dna.push("def".into());
        v.check_goal();
        assert!(v.completed);
    }

    #[test]
    fn no_goal_means_high_defaults() {
        let v = DnaVault::new(None);
        assert_eq!((v.animals_max, v.plants_max, v.dna_max), (100, 100, 100));
    }

    #[test]
    fn upload_skips_what_the_vault_has() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let probe = world
            .spawn(DnaProbe {
                vault: None,
                animals: vec![
                    "/mob/living/basic/cow".into(),
                    "/mob/living/basic/pig".into(),
                ],
                plants: Vec::new(),
                humans: vec!["abc".into()],
            })
            .id();
        let mut vault = DnaVault::new(None);
        vault.animal_dna.push("/mob/living/basic/cow".into());
        let vault = world.spawn(vault).id();
        let client = world.spawn_empty().id();
        upload(&mut world, client, probe, vault);
        let v = world.get::<DnaVault>(vault).unwrap();
        assert_eq!(v.animal_dna.len(), 2);
        assert_eq!(v.human_dna, vec!["abc".to_owned()]);
        assert_eq!(probe_samples(&world, probe), (0, 0, 0));
    }

    #[test]
    fn report_lists_the_counts() {
        let text = goal_report(&VaultTargets {
            animals: 17,
            plants: 5,
            humans: 12,
        });
        assert!(text.contains("- 17 уникальных данных о животных"));
        assert!(text.contains("- 12 уникальных данных о разумных"));
    }
}
