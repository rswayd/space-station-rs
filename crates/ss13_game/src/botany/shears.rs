//! Botanogenetic plant shears (tg `/obj/item/geneshears`, its branch of
//! `/obj/machinery/hydroponics/item_interaction`): on a plant with more than 15 health, a list of
//! its removable genes; the picked one is cut off (its reagent too) for 15 health.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_reagents::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface, UiStatus};

use super::genes::{Gene, PLANT_GENE_REMOVABLE};
use super::tray::Tray;
use super::{Botany, Plants, refresh_tray};
use crate::chem::Chem;
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::medical::surgery::ListAct;
use crate::tools::tell;

pub const GENESHEARS_ROOT: &str = "/obj/item/geneshears";
/// `GENE_SHEAR_MIN_HEALTH`.
pub const GENE_SHEAR_MIN_HEALTH: f32 = 15.0;
/// What a cut costs the plant.
const SHEAR_DAMAGE: f32 = 15.0;
const TITLE: &str = "Plant Trait Removal";

/// The genes offered to the shearer, waiting for the pick.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ShearChoice {
    pub tray: Entity,
    pub shears: Entity,
    pub plantname: String,
    pub names: Vec<String>,
}

/// The server side of `ListInputWindow` for the gene pick.
pub struct ShearListWindow;

impl UiInterface for ShearListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

impl ServerUi for ShearListWindow {
    type Source = &'static ShearChoice;
    type UserParam = ();
    type Act = ListAct;

    fn static_data(_: &ShearChoice) {}

    fn data(choice: &ShearChoice, _: Entity, _: &()) -> ListInputData {
        ListInputData {
            title: TITLE.into(),
            message: format!("Trait to remove from the {}", choice.plantname),
            init_value: choice.names.first().cloned().unwrap_or_default(),
            items: choice.names.clone(),
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ListAct, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| ListAct::Submit(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected an item".into())),
            "cancel" => Ok(ListAct::Cancel),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<ShearListWindow>().add_systems(
        Update,
        (
            pick_gene.in_set(UiSystems::Handle),
            close_stale_lists.in_set(UiSystems::StatusHook),
        ),
    );
}

/// tg `gene.name`: a trait's name, a reagent gene's reagent name.
pub fn gene_label(plants: &Plants, chem: Option<&ss13_reagents::Chemistry>, gene: &Gene) -> String {
    match gene {
        Gene::Trait { path, .. } => plants
            .trait_def(path)
            .map_or_else(|| path.clone(), |t| t.name.clone()),
        Gene::Reagent { path, .. } => chem
            .and_then(|c| c.id(path).map(|id| c.def(id).name.clone()))
            .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned()),
    }
}

/// The removable genes' names, sorted (`sort_list(current_traits)`), without duplicates.
pub fn removable_names(
    plants: &Plants,
    chem: Option<&ss13_reagents::Chemistry>,
    genes: &[Gene],
) -> Vec<String> {
    let mut names: Vec<String> = genes
        .iter()
        .filter(|g| g.mutability() & PLANT_GENE_REMOVABLE != 0)
        .map(|g| gene_label(plants, chem, g))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Gene shears on a tray: the list of removable genes. True when the click was theirs.
pub fn shears_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    tray: Entity,
) -> bool {
    let is_shears = world
        .get::<ss13_mapload::DmItem>(held)
        .is_some_and(|i| is_under(&i.path, GENESHEARS_ROOT));
    if !is_shears {
        return false;
    }
    let Some(state) = world.get::<Tray>(tray).cloned() else {
        return false;
    };
    let Some(seed) = state.myseed.as_ref() else {
        tell(world, client, Span::Notice, "The tray is empty.");
        return true;
    };
    if state.plant_health <= GENE_SHEAR_MIN_HEALTH {
        tell(
            world,
            client,
            Span::Notice,
            "This plant looks too unhealty to be sheared right now.",
        );
        return true;
    }
    let plants = world.resource::<Botany>().0.clone();
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let names = removable_names(&plants, chem.as_deref(), &seed.genes);
    world.entity_mut(mob).insert(ShearChoice {
        tray,
        shears: held,
        plantname: seed.plantname.clone(),
        names,
    });
    world.write_message(OpenUi::new::<ShearListWindow>(client, mob, mob, TITLE));
    true
}

/// The picked gene off the plant, after `can_perform_action` and the health check again.
fn pick_gene(world: &mut World, mut acts: Local<MessageCursor<UiActed<ShearListWindow>>>) {
    let picked: Vec<(Entity, ListAct)> = acts
        .read(world.resource::<Messages<UiActed<ShearListWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.actor, a.act.clone()))
        .collect();
    for (mob, act) in picked {
        let Some(choice) = world.get::<ShearChoice>(mob).cloned() else {
            continue;
        };
        world.entity_mut(mob).remove::<ShearChoice>();
        let ListAct::Submit(entry) = act else {
            continue;
        };
        if !choice.names.contains(&entry) || !can_shear(world, mob, &choice) {
            continue;
        }
        let Some(mut state) = world.get::<Tray>(choice.tray).cloned() else {
            continue;
        };
        if state.plant_health <= GENE_SHEAR_MIN_HEALTH {
            continue;
        }
        let plants = world.resource::<Botany>().0.clone();
        let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
        let Some(seed) = state.myseed.as_mut() else {
            continue;
        };
        let Some(i) = seed.genes.iter().position(|g| {
            g.mutability() & PLANT_GENE_REMOVABLE != 0
                && gene_label(&plants, chem.as_deref(), g) == entry
        }) else {
            continue;
        };
        seed.genes.remove(i);
        let plantname = seed.plantname.clone();
        state.adjust_plant_health(-SHEAR_DAMAGE);
        world.entity_mut(choice.tray).insert(state);
        if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "You carefully shear the genes off of the {plantname}, leaving the plant looking weaker."
                ),
            );
        }
        refresh_tray(world, choice.tray);
    }
}

/// `user.can_perform_action(tray)`: conscious, next to the tray, the shears still in hand.
fn can_shear(world: &World, mob: Entity, choice: &ShearChoice) -> bool {
    let (Some(a), Some(b)) = (
        crate::chem::map_pos(world, mob),
        world.get::<TilePos>(choice.tray),
    ) else {
        return false;
    };
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && a.z == b.z
        && (a.x - b.x).abs() <= 1
        && (a.y - b.y).abs() <= 1
        && world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(choice.shears)))
}

fn close_stale_lists(
    mut windows: Query<&mut UiWindow, With<ss13_tgui::server::UiKind<ShearListWindow>>>,
    choices: Query<(), With<ShearChoice>>,
) {
    for mut window in &mut windows {
        if !choices.contains(window.source) {
            window.cap_status(UiStatus::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_removable_genes_are_offered_sorted() {
        let plants = super::super::test_plants();
        let genes = vec![
            Gene::Trait {
                path: super::super::genes::REPEATED_HARVEST.into(),
                rate: 0.0,
                mutability: 7,
                ids: 0,
                flags: 0,
            },
            Gene::Trait {
                path: super::super::genes::CARNIVORY.into(),
                rate: 0.0,
                mutability: 7,
                ids: 0,
                flags: 0,
            },
            Gene::Trait {
                path: super::super::genes::FUNGAL.into(),
                rate: 0.0,
                mutability: 4,
                ids: 2,
                flags: 0,
            },
            Gene::new_reagent("/datum/reagent/consumable/nutriment", 0.1),
        ];
        assert_eq!(
            removable_names(&plants, None, &genes),
            vec!["carnivory", "nutriment", "repeated_harvest"]
        );
    }
}
