//! Brand Intelligence (`code/modules/events/brand_intelligence.dm`): one station vending
//! machine starts shooting its stock at people and, every 4 s, infects another; silencing it
//! (its speaker wire) or cutting all its wires ends the event and cures the rest.
// shortcut: infected machines never deny poor customers (`deny_vending_interact`) and do not
// come alive when every machine is infected (`/datum/ai_controller/vending_machine`): no mob AI.

use bevy::prelude::*;
use ss13_mapload::DmName;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_world::Density;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::vending::Vendor;
use crate::wires::{ShootsInventory, VendorHack, Wires};

pub const NAME: &str = "Brand Intelligence";
/// `/obj/machinery` `verb_say` ("beeps"), BandaStation's translation.
const MACHINE_VERB: &str = "сигналит";

const RAMPANT_SPEECHES: [&str; 7] = [
    "Попробуйте нашу новую АГРЕССИВНУЮ стратегию маркетинга!",
    "Вам стоит что-нибудь купить, дабы утолить ваши ПОТРЕБНОСТИ!",
    "Потребляй!",
    "За ваши деньги можно купить счастье!",
    "Методика ПРЯМОГО маркетинга!",
    "Реклама узаконила ложь! Но не позвольте ей отвлечь вас от наших замечательных предложений!",
    "Не хочешь платить? Я твоей мамке тоже платить не хотел.",
];

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(BrandIntelligence::default())
}

#[derive(Default)]
struct BrandIntelligence {
    vending_machines: Vec<Entity>,
    infected_machines: Vec<Entity>,
    origin_machine: Option<Entity>,
}

fn station_vendors(world: &mut World) -> Vec<Entity> {
    let levels = station_levels(world);
    let mut q = world.query_filtered::<(Entity, &TilePos), (With<Vendor>, With<Density>)>();
    q.iter(world)
        .filter(|(_, pos)| levels.contains(&pos.z))
        .map(|(e, _)| e)
        .collect()
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// `shut_up = FALSE; shoot_inventory = <on>`.
fn set_rampant(world: &mut World, vendor: Entity, on: bool) {
    let Some(mut hack) = world.get_mut::<VendorHack>(vendor) else {
        return;
    };
    if on {
        hack.shut_up = false;
    }
    hack.shoot_inventory = on;
    if on {
        world.entity_mut(vendor).insert(ShootsInventory);
    } else {
        world.entity_mut(vendor).remove::<ShootsInventory>();
    }
}

/// `/obj/machinery/vending/speak(message)`: said to everyone in view.
fn speak(world: &mut World, vendor: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(vendor) else {
        return;
    };
    let clients = crate::construction::clients_near(world, pos, 7, &[]);
    let name = name_of(world, vendor);
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![
                ChatSegment::new(Span::Name, name),
                ChatSegment::new(Span::Say, format!(" {MACHINE_VERB}, \"")),
                ChatSegment::new(Span::Say, text),
                ChatSegment::new(Span::Say, "\""),
            ],
        },
    });
}

impl RoundEvent for BrandIntelligence {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.announce_when = 21;
        ev.end_when = 1000;
        self.vending_machines = station_vendors(world);
        if self.vending_machines.is_empty() {
            ev.killed = true;
            return;
        }
        let rng = &mut world.resource_mut::<EventRng>().0;
        let i = rng.range(0, self.vending_machines.len() as i64 - 1) as usize;
        self.origin_machine = Some(self.vending_machines.swap_remove(i));
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let machine_name = if fake {
            // shortcut: a random vendor of the map instead of any `/obj/machinery/vending`
            // subtype.
            let vendors = station_vendors(world);
            let picked = world.resource_mut::<EventRng>().pick(&vendors).copied();
            picked.map(|v| name_of(world, v)).unwrap_or_default()
        } else {
            self.origin_machine
                .map(|v| name_of(world, v))
                .unwrap_or_default()
        };
        let text = format!(
            "На борту станции {} зафиксировано распространение цифрового торгового вируса. \
             Проверьте все торговые автоматы типа {machine_name} на наличие агрессивной \
             маркетинговой тактики и при необходимости перезагрузите их.",
            super::station_name(world)
        );
        super::priority_announce(world, "Обнаружен цифровой вирус", &text);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        if let Some(origin) = self.origin_machine {
            set_rampant(world, origin, true);
        }
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        // Deleted machines leave the lists (`clear_from_lists` on `COMSIG_QDELETING`).
        self.vending_machines
            .retain(|&v| world.get::<Vendor>(v).is_some());
        self.infected_machines
            .retain(|&v| world.get::<Vendor>(v).is_some());
        let origin = self
            .origin_machine
            .filter(|&o| world.get::<Vendor>(o).is_some());
        let silenced = origin.is_none_or(|o| {
            world.get::<VendorHack>(o).is_some_and(|h| h.shut_up)
                || world
                    .get::<Wires>(o)
                    .is_some_and(|w| w.cut.len() >= w.colors.len())
        });
        if silenced {
            for saved in std::mem::take(&mut self.infected_machines) {
                set_rampant(world, saved, false);
            }
            if let Some(origin) = origin {
                // The origin keeps its own `shoot_inventory`, as in tg.
                speak(
                    world,
                    origin,
                    "I am... vanquished. My people will remem...ber...meeee.",
                );
            }
            ev.killed = true;
            return;
        }
        if self.vending_machines.is_empty() {
            ev.killed = true;
            return;
        }
        if ev.active_for.is_multiple_of(2) {
            let rng = &mut world.resource_mut::<EventRng>().0;
            let i = rng.range(0, self.vending_machines.len() as i64 - 1) as usize;
            let rebel = self.vending_machines.swap_remove(i);
            self.infected_machines.push(rebel);
            set_rampant(world, rebel, true);
            if ev.active_for.is_multiple_of(4)
                && let Some(origin) = origin
            {
                let speech = *world
                    .resource_mut::<EventRng>()
                    .pick(&RAMPANT_SPEECHES)
                    .expect("non-empty");
                speak(world, origin, speech);
            }
        }
    }
}
