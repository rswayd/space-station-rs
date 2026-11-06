//! The slot machine (server), after tg `/obj/machinery/computer/slot_machine`
//! (`code/game/machinery/slotmachine.dm`): coins or holochips (a multitool switches, only with
//! an empty balance) buy spins of 5 credits; five reels of three symbols spin for 4 s, then the
//! longest run of 3+ equal symbols in a row pays (3: free spins, 4: 400, 5: 1000, from the
//! machine's bank `money`, the rest onto the balance), five jackpot symbols in the middle row pay
//! the bank plus 10000 and are announced, five trap symbols spring the machine's trap. An emag
//! rigs it: coin payouts are iron and thrown at someone near. The window is tgui `SlotMachine`
//! ([`crate::slot_machine`]).
// shortcut: no house bank account linking by ID, no EMP, no mood or memory events, no
// energy use (an unpowered machine refuses), no ray filter; the engineering trap's illusory
// shock (`electrocute_act`) is only its line; the jackpot's coin shower is five coins of a fixed
// list, dropped on the machine's tile (no `random_step`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmName, IconIndex, Template};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::server::{client_of, item_path, now, say_around, set_name, sound_at};
use crate::combat::CombatRng;
use crate::obj_damage::is_under;
use crate::slot_machine::{
    PAYOUT, SPIN, SlotMachineData, SlotMachineStatic, SlotMachineUi, SlotSymbol,
};
use crate::tools::tell;

pub const SLOT_MACHINE: &str = "/obj/machinery/computer/slot_machine";
const HOLOCHIP: &str = "/obj/item/holochip";
const SILVER_COIN: &str = "/obj/item/coin/silver";
const IRON_COIN: &str = "/obj/item/coin/iron";
const FLASHBANG: &str = "/obj/item/grenade/flashbang";
const HANDCUFFS: &str = "/obj/item/restraints/handcuffs";
/// The jackpot's coins (tg: `pick(subtypesof(/obj/item/coin))`).
const JACKPOT_COINS: [&str; 6] = [
    "/obj/item/coin/gold",
    "/obj/item/coin/silver",
    "/obj/item/coin/diamond",
    "/obj/item/coin/plasma",
    "/obj/item/coin/uranium",
    IRON_COIN,
];
const SPIN_PRICE: u32 = 5;
const PRIZE_SMALL: u32 = 400;
const PRIZE_BIG: u32 = 1000;
const PRIZE_JACKPOT: u32 = 10_000;
const SPIN_SECS: f64 = 4.0;
/// `addtimer(clear_winning, 3 SECONDS)`.
const WIN_SECS: f64 = 3.0;
/// `addtimer(playsound roulettewheel, 0.3 SECONDS)`.
const WHEEL_DELAY_SECS: f64 = 0.3;
const WINNING_NOTHING: u8 = 0;
const WINNING_FREESPIN: u8 = 1;
const WINNING_SMALL: u8 = 2;
const WINNING_BIG: u8 = 3;
const WINNING_JACKPOT: u8 = 4;

const ADJECTIVES: [&str; 12] = [
    "Blazing", "Bonus", "Grand", "Greedy", "Jumbo", "Platinum", "Lucky", "Mega", "Robust", "Super",
    "Turbo", "Wild",
];
const NOUNS: [&str; 14] = [
    "Bankroll", "Cashout", "Fortune", "Jackpot", "Luck", "Money", "Payday", "Reels", "Riches",
    "Spinner", "Spins", "Strike", "Treasure", "Spess",
];

/// A machine kind: its symbols, the jackpot's and the trap's index among them, its extra name
/// words.
struct Kind {
    path: &'static str,
    symbols: [&'static str; 7],
    jackpot: usize,
    trap: Option<usize>,
    adjectives: &'static [&'static str],
    nouns: &'static [&'static str],
}

static KINDS: [Kind; 11] = [
    Kind {
        path: "/obj/machinery/computer/slot_machine/command",
        symbols: [
            "/obj/item/disk/nuclear",
            "/obj/item/clothing/accessory/medal/gold",
            "/obj/item/hand_tele",
            "/mob/living/basic/pet/dog/corgi",
            "/obj/item/card/id/advanced/gold",
            "/obj/item/melee/sabre",
            "/obj/item/grenade/syndieminibomb",
        ],
        jackpot: 1,
        trap: Some(6),
        adjectives: &["Royal", "Regal", "Golden", "Captain's", "Glorious"],
        nouns: &["Medal", "Ransom", "Authority", "Command"],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/security",
        symbols: [
            "/obj/item/food/donut/berry",
            "/mob/living/basic/bot/secbot/beepsky",
            "/obj/item/melee/baton/security/loaded",
            "/obj/item/gun/energy/disabler",
            "/obj/vehicle/sealed/mecha/ripley/paddy",
            "/obj/item/book/manual/wiki/security_space_law",
            "/obj/item/grown/bananapeel",
        ],
        jackpot: 0,
        trap: Some(6),
        adjectives: &["Stunned", "Flashed", "Confiscated", "Loyal", "Arrested"],
        nouns: &["Baton", "Donut", "Contraband", "Brig", "Security"],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/medical",
        symbols: [
            "/obj/item/storage/medkit/brute",
            "/obj/vehicle/sealed/mecha/odysseus",
            "/obj/item/clothing/glasses/hud/health",
            "/mob/living/basic/pet/cat/runtime",
            "/obj/item/plunger",
            "/obj/item/clothing/neck/stethoscope",
            "/obj/machinery/syndicatebomb",
        ],
        jackpot: 5,
        trap: Some(6),
        adjectives: &["Mutated", "Overdosed", "Infectious", "Healing", "Husked"],
        nouns: &["Medkit", "Defib", "Patient", "Doctor", "Cure"],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/engineering",
        symbols: [
            "/obj/item/storage/toolbox/mechanical",
            "/obj/item/blueprints",
            "/obj/item/clothing/gloves/color/yellow",
            "/obj/item/clothing/head/utility/welding",
            "/obj/item/clothing/glasses/meson",
            "/mob/living/basic/parrot/poly",
            "/obj/singularity",
        ],
        jackpot: 1,
        trap: Some(6),
        adjectives: &[
            "Supercharged",
            "Pressurized",
            "Radioactive",
            "Overloaded",
            "Delaminating",
            "Insulated",
        ],
        nouns: &["Toolbox", "Emitter", "Supermatter"],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/cargo",
        symbols: [
            "/obj/item/bounty_cube",
            "/obj/item/clipboard",
            "/obj/item/universal_scanner",
            "/mob/living/basic/sloth",
            "/obj/item/multitool",
            "/obj/vehicle/sealed/mecha/ripley",
            "/obj/item/suspiciousphone",
        ],
        jackpot: 0,
        trap: Some(6),
        adjectives: &[
            "Express",
            "Smuggled",
            "Stolen",
            "Overdue",
            "Subsidized",
            "Manifested",
        ],
        nouns: &["Bounty", "Crate", "Manifest", "MULE", "Profit"],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/service",
        symbols: [
            "/obj/item/clothing/head/hats/tophat",
            "/obj/item/reagent_containers/cup/watering_can",
            "/obj/item/clothing/shoes/galoshes",
            "/mob/living/basic/goat/pete",
            "/obj/item/book/bible",
            "/obj/item/kitchen/rollingpin",
            "/obj/item/seeds/random",
        ],
        jackpot: 6,
        trap: Some(3),
        adjectives: &[
            "Fermented",
            "Seasoned",
            "Tipsy",
            "Cleaned",
            "Organic",
            "Culinary",
            "Refreshing",
            "Divine",
            "Holy",
        ],
        nouns: &["Recipe", "Cocktail", "Harvest", "Scrubber"],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/science",
        symbols: [
            "/obj/item/stack/sheet/mineral/gold",
            "/obj/item/stack/sheet/mineral/silver",
            "/obj/item/stack/sheet/mineral/uranium",
            "/obj/item/stack/sheet/mineral/plasma",
            "/obj/item/stack/sheet/mineral/runite",
            "/obj/item/clothing/mask/facehugger/lamarr",
            "/obj/item/gibtonite",
        ],
        jackpot: 4,
        trap: Some(6),
        adjectives: &[
            "Atomic",
            "Bluespace",
            "Cosmic",
            "Golden",
            "Diamond",
            "Silver",
            "Uranium",
            "Quantum",
            "Anomalous",
            "Plasma",
            "Experimental",
            "Robotic",
        ],
        nouns: &[
            "Anomaly",
            "Artifact",
            "Slime",
            "Extract",
            "Circuit",
            "Discovery",
            "Explosion",
        ],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/clown",
        symbols: [
            "/obj/item/food/grown/banana",
            "/obj/item/toy/crayon/spraycan/lubecan",
            "/obj/item/card/id/advanced/rainbow",
            "/obj/vehicle/sealed/mecha/honker",
            "/obj/item/clothing/mask/gas/clown_hat",
            "/obj/item/storage/backpack/clown",
            HANDCUFFS,
        ],
        jackpot: 3,
        trap: Some(6),
        adjectives: &[
            "Honking",
            "Slippery",
            "Pranked",
            "Squeaky",
            "Hilarious",
            "Giggling",
        ],
        nouns: &[
            "Banana",
            "Peel",
            "Prank",
            "Joke",
            "Punchline",
            "Candy",
            "Honk",
        ],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/mime",
        symbols: [
            "/obj/item/book/granter/action/spell/mime/mimery",
            "/obj/item/clothing/mask/gas/mime",
            "/obj/item/toy/crayon/spraycan/mimecan",
            "/obj/item/reagent_containers/cup/glass/bottle/bottleofnothing",
            "/obj/item/clothing/gloves/color/white",
            "/obj/item/storage/backpack/mime",
            HANDCUFFS,
        ],
        jackpot: 0,
        trap: Some(6),
        adjectives: &[
            "Silent",
            "Transparent",
            "Unspeakable",
            "Voiceless",
            "Quiet",
            "Hushed",
            "Invisible",
            "Imaginary",
            "Empty",
        ],
        nouns: &["Silence", "Mute", "Introvert", "Nothing", "Baguette"],
    },
    Kind {
        path: "/obj/machinery/computer/slot_machine/syndicate",
        symbols: [
            "/obj/machinery/nuclearbomb",
            "/obj/item/card/emag",
            "/obj/item/storage/toolbox/syndicate",
            "/obj/vehicle/sealed/mecha/gygax/dark",
            "/obj/item/soap/syndie",
            "/obj/item/gun/energy/recharge/ebow",
            HANDCUFFS,
        ],
        jackpot: 0,
        trap: Some(6),
        adjectives: &[
            "Covert",
            "Nuclear",
            "Suspicious",
            "Bloody",
            "Syndie",
            "Sabotaged",
            "Clandestine",
            "Illicit",
            "Traitorous",
        ],
        nouns: &[
            "Telecrystal",
            "Uplink",
            "Bomb",
            "Operative",
            "Disk",
            "Nuke",
            "Syndicate",
            "Traitor",
        ],
    },
    // The base machine last: every kind is under it.
    Kind {
        path: SLOT_MACHINE,
        symbols: [
            "/obj/item/storage/bag/money",
            "/obj/item/food/grown/cherry_bomb",
            FLASHBANG,
            "/obj/item/poker_chip",
            "/obj/item/food/grown/chili",
            "/obj/item/clothing/neck/necklace/dope",
            "/obj/item/stack/spacecash/c20",
        ],
        jackpot: 1,
        trap: Some(2),
        adjectives: &[],
        nouns: &[],
    },
];

/// `HOLOCHIP` / `COIN` paymode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayMode {
    Holochip,
    Coin,
}

/// One slot machine.
#[derive(Component, Clone, Debug)]
pub struct SlotMachine {
    kind: usize,
    /// The bank: what the machine has consumed.
    pub money: u32,
    pub plays: u32,
    pub jackpots: u32,
    /// What is in the machine, ready to be spent.
    pub balance: u32,
    pub working: bool,
    pub winning: u8,
    pub paymode: PayMode,
    pub emagged: bool,
    /// Symbol indices, five reels of three.
    pub reels: [[usize; 3]; 5],
    /// When the spin ends, and who pulled the lever.
    spin_end: f64,
    spinner: Option<Entity>,
    /// When the wheel sound plays, and when the win banner clears.
    wheel_at: Option<f64>,
    clear_at: Option<f64>,
    /// `process`: half a credit a second into the bank.
    bank_frac: f64,
    /// The symbols as the window shows them (sprites filled once the window opens).
    symbols: Vec<SlotSymbol>,
    icons_ready: bool,
}

fn kind_of(path: &str) -> usize {
    KINDS
        .iter()
        .position(|k| is_under(path, k.path))
        .unwrap_or(KINDS.len() - 1)
}

impl SlotMachine {
    fn kind(&self) -> &'static Kind {
        &KINDS[self.kind]
    }

    fn new(path: &str, rng: &mut CombatRng) -> Self {
        let kind = kind_of(path);
        let symbols = KINDS[kind]
            .symbols
            .iter()
            .map(|&id| SlotSymbol {
                id: id.to_owned(),
                name: id.rsplit('/').next().unwrap_or(id).to_owned(),
                icon: None,
            })
            .collect();
        let mut machine = Self {
            kind,
            money: 3000,
            plays: rng.range(75, 200),
            jackpots: rng.range(1, 4),
            balance: 0,
            working: false,
            winning: WINNING_NOTHING,
            paymode: PayMode::Holochip,
            emagged: false,
            reels: [[0; 3]; 5],
            spin_end: 0.0,
            spinner: None,
            wheel_at: None,
            clear_at: None,
            bank_frac: 0.0,
            symbols,
            icons_ready: false,
        };
        machine.randomize(rng);
        machine
    }

    /// `randomize_reels`.
    fn randomize(&mut self, rng: &mut CombatRng) {
        let n = self.kind().symbols.len() as u32;
        for reel in &mut self.reels {
            for cell in reel.iter_mut() {
                *cell = rng.range(0, n - 1) as usize;
            }
        }
    }

    /// `check_middle_row_all`.
    pub fn middle_row_all(&self, symbol: usize) -> bool {
        self.reels.iter().all(|reel| reel[1] == symbol)
    }

    /// `get_lines`: the longest run of 3+ equal symbols along a row (0 without one).
    pub fn lines(&self) -> usize {
        let mut best = 0;
        for row in 0..3 {
            let mut current = None;
            let mut run = 0;
            for reel in &self.reels {
                if Some(reel[row]) == current {
                    run += 1;
                } else {
                    current = Some(reel[row]);
                    run = 1;
                }
                if run >= 3 {
                    best = best.max(run);
                }
            }
        }
        best
    }
}

/// `make_machine_name`: "[adjective] [noun]" from the base words and the kind's.
fn machine_name(kind: &Kind, rng: &mut CombatRng) -> String {
    let adjectives: Vec<&str> = ADJECTIVES.iter().chain(kind.adjectives).copied().collect();
    let nouns: Vec<&str> = NOUNS.iter().chain(kind.nouns).copied().collect();
    let a = adjectives[rng.range(0, adjectives.len() as u32 - 1) as usize];
    let n = nouns[rng.range(0, nouns.len() as u32 - 1) as usize];
    format!("{a} {n}")
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    use ss13_tgui::server::{AppTguiExt, UiSystems};
    // `ui_interact` with an empty hand.
    app.add_click_handler(ClickPhase::EmptyHand, 3221, |w, c| {
        open(w, c.client, c.mob, c.target)
    });
    // Coins, holochips, the multitool and the emag (before `machine_frame` takes tools).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1591, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    app.register_ui::<SlotMachineUi>().add_systems(
        Update,
        (
            acts.in_set(UiSystems::Handle),
            run_machines.run_if(any_with_component::<SlotMachine>),
        ),
    );
}

/// `Initialize` of a slot machine.
pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if !is_under(path, SLOT_MACHINE) || world.get::<SlotMachine>(e).is_some() {
        return;
    }
    let (machine, name) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let machine = SlotMachine::new(path, &mut rng);
        let name = machine_name(machine.kind(), &mut rng);
        (machine, name)
    };
    world.entity_mut(e).insert(machine);
    set_name(world, e, &name);
}

/// The symbols' sprites from their prototypes (those the conversion has).
fn fill_icons(world: &mut World, e: Entity) {
    if world.get::<SlotMachine>(e).is_none_or(|m| m.icons_ready) {
        return;
    }
    let icons: Vec<Option<ProductIcon>> = {
        let index: std::collections::HashMap<(&str, &str), ss13_shared::IconStateId> = world
            .get_resource::<IconIndex>()
            .map(|i| {
                i.0.iter()
                    .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                    .collect()
            })
            .unwrap_or_default();
        let protos = world.get_resource::<ss13_proto::Prototypes>();
        let kind = &KINDS[world.get::<SlotMachine>(e).map_or(0, |m| m.kind)];
        kind.symbols
            .iter()
            .map(|&path| {
                let proto = protos.and_then(|p| p.get(path)).cloned()?;
                Template::new(proto, &index)
                    .appearance
                    .map(|a| ProductIcon {
                        icon_state: a.icon_state,
                        color: a.color,
                    })
            })
            .collect()
    };
    if let Some(mut m) = world.get_mut::<SlotMachine>(e) {
        for (symbol, icon) in m.symbols.iter_mut().zip(icons) {
            symbol.icon = icon;
        }
        m.icons_ready = true;
    }
}

/// `ui_interact`. True when `target` is a slot machine.
fn open(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<SlotMachine>(target).is_none() {
        return false;
    }
    fill_icons(world, target);
    let name = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<SlotMachineUi>(client, mob, target, name));
    true
}

fn machine_say(world: &mut World, machine: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, machine, text);
}

/// `item_interaction`, `multitool_act`, `emag_act`. True when the click was the machine's.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    machine: Entity,
) -> bool {
    let Some(state) = world.get::<SlotMachine>(machine).cloned() else {
        return false;
    };
    let path = item_path(world, held);
    if crate::emag::is_emag(world, held) {
        if !state.emagged {
            if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
                m.emagged = true;
            }
            sound_at(world, machine, "sparks", 50.0, true);
            tell(world, client, Span::Notice, "machine rigged");
        }
        return true;
    }
    if matches!(
        crate::tools::tool_of(world, held),
        Some((crate::tools::ToolKind::Multitool, _))
    ) {
        if state.balance > 0 {
            machine_say(
                world,
                machine,
                "ОШИБКА! Пожалуйста, опустошите машину перед изменением способа оплаты.",
            );
            return true;
        }
        let (mode, text) = match state.paymode {
            PayMode::Holochip => (PayMode::Coin, "переключено на монеты"),
            PayMode::Coin => (PayMode::Holochip, "переключено на голочипы"),
        };
        if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
            m.paymode = mode;
        }
        tell(world, client, Span::Notice, text);
        return true;
    }
    if is_under(&path, crate::cargo::money::COIN) {
        if state.paymode != PayMode::Coin {
            tell(world, client, Span::Warning, "принимаются только голочипы!");
            return true;
        }
        let spit = world.resource_mut::<CombatRng>().chance(2.0);
        if spit {
            // The coin flies back at its owner; sometimes a spin is lost with it.
            if let (Some(at), Some(user_at)) = (
                world.get::<TilePos>(machine).copied(),
                crate::chem::map_pos(world, mob),
            ) {
                crate::inventory::put_down(world, held, at);
                crate::throw::throw_at(world, held, user_at, 3, 10.0);
            }
            if world.resource_mut::<CombatRng>().chance(10.0)
                && let Some(mut m) = world.get_mut::<SlotMachine>(machine)
            {
                m.balance = m.balance.saturating_sub(SPIN_PRICE);
            }
            let name = crate::chem::name_of(world, machine);
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} выплевывает монету обратно!"),
            );
            return true;
        }
        let value = crate::cargo::money::coin_value(&path);
        world.despawn(held);
        if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
            m.balance += value;
        }
        tell(world, client, Span::Notice, "монета вставлена");
        return true;
    }
    if let Some(chip) = world.get::<crate::cargo::money::Holochip>(held).copied() {
        if state.paymode != PayMode::Holochip {
            tell(world, client, Span::Warning, "принимаются только монеты!");
            return true;
        }
        world.despawn(held);
        if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
            m.balance += chip.credits;
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("вставлено {} кр.", chip.credits),
        );
        return true;
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotAct {
    Spin,
    Payout,
}

impl ServerUi for SlotMachineUi {
    type Source = &'static SlotMachine;
    type UserParam = ();
    type Act = SlotAct;

    fn static_data(m: &SlotMachine) -> SlotMachineStatic {
        let kind = m.kind();
        SlotMachineStatic {
            symbols: m.symbols.clone(),
            cost: SPIN_PRICE,
            jackpot: PRIZE_JACKPOT,
            jackpot_id: kind.symbols[kind.jackpot].to_owned(),
            trap_id: kind.trap.map(|t| kind.symbols[t].to_owned()),
        }
    }

    fn data(m: &SlotMachine, _: Entity, _: &()) -> SlotMachineData {
        let symbols = &m.kind().symbols;
        SlotMachineData {
            reels: m
                .reels
                .iter()
                .map(|reel| reel.iter().map(|&i| symbols[i].to_owned()).collect())
                .collect(),
            balance: m.balance,
            working: m.working,
            winning: m.winning,
            money: m.money,
            plays: m.plays,
            jackpots: m.jackpots,
            paymode: match m.paymode {
                PayMode::Holochip => 1,
                PayMode::Coin => 2,
            },
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<SlotAct, UiReject> {
        match action {
            SPIN => Ok(SlotAct::Spin),
            PAYOUT => Ok(SlotAct::Payout),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`, at the moment of effect.
fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<SlotMachineUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, SlotAct)> = cursor
        .read(world.resource::<Messages<UiActed<SlotMachineUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, machine, act) in pending {
        let Some(balance) = world.get::<SlotMachine>(machine).map(|m| m.balance) else {
            continue;
        };
        match act {
            SlotAct::Spin => spin(world, client, actor, machine),
            SlotAct::Payout if balance > 0 => {
                let left = give_payout(world, machine, balance);
                if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
                    // tg drops what the coins cannot make up; it stays in the machine here.
                    m.balance = left;
                }
            }
            SlotAct::Payout => {}
        }
    }
}

/// `spin` after `can_spin`.
pub fn spin(world: &mut World, client: Entity, user: Entity, machine: Entity) {
    let Some(state) = world.get::<SlotMachine>(machine).cloned() else {
        return;
    };
    let refusal = if world.get::<crate::power::Unpowered>(machine).is_some() {
        Some("нет энергии!")
    } else if state.working {
        Some("машина уже крутится!")
    } else if state.balance < SPIN_PRICE {
        Some("недостаточно кредитов!")
    } else {
        None
    };
    if let Some(text) = refusal {
        tell(world, client, Span::Warning, text);
        return;
    }
    let who = crate::chem::name_of(world, user);
    say_around(
        world,
        machine,
        Span::Notice,
        format!("{who} тянет рычаг и слот-машина начинает крутиться!"),
        &[],
    );
    let t = now(world);
    world.resource_scope(|world, mut rng: Mut<CombatRng>| {
        if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
            m.balance -= SPIN_PRICE;
            m.money += SPIN_PRICE;
            m.plays += 1;
            m.working = true;
            m.spinner = Some(user);
            m.spin_end = t + SPIN_SECS;
            m.wheel_at = Some(t + WHEEL_DELAY_SECS);
            m.randomize(&mut rng);
        }
    });
    sound_at(
        world,
        machine,
        "sound/machines/lever/lever_start.ogg",
        50.0,
        false,
    );
}

/// Spins ending, the wheel sound, the win banner clearing, and `process` (the bank grows by
/// `round(seconds_per_tick / 2)` per 2 s machine tick: half a credit a second while powered).
fn run_machines(world: &mut World) {
    let t = now(world);
    let dt = f64::from(world.resource::<Time>().delta_secs());
    let mut q = world.query::<(Entity, &SlotMachine)>();
    let all: Vec<(Entity, bool, f64, Option<f64>, Option<f64>)> = q
        .iter(world)
        .map(|(e, m)| (e, m.working, m.spin_end, m.wheel_at, m.clear_at))
        .collect();
    for (e, working, spin_end, wheel_at, clear_at) in all {
        let powered = world.get::<crate::power::Unpowered>(e).is_none();
        if let Some(mut m) = world.get_mut::<SlotMachine>(e)
            && powered
        {
            m.bank_frac += dt * 0.5;
            if m.bank_frac >= 1.0 {
                let whole = m.bank_frac.floor();
                m.money += whole as u32;
                m.bank_frac -= whole;
            }
        }
        if wheel_at.is_some_and(|at| t >= at) {
            if let Some(mut m) = world.get_mut::<SlotMachine>(e) {
                m.wheel_at = None;
            }
            sound_at(
                world,
                e,
                "sound/machines/roulette/roulettewheel.ogg",
                20.0,
                false,
            );
        }
        if clear_at.is_some_and(|at| t >= at)
            && let Some(mut m) = world.get_mut::<SlotMachine>(e)
        {
            m.clear_at = None;
            m.winning = WINNING_NOTHING;
        }
        if working && t >= spin_end {
            finish_spinning(world, e);
        }
    }
}

/// `finish_spinning` -> `give_prizes`.
pub fn finish_spinning(world: &mut World, machine: Entity) {
    let Some(state) = world.get::<SlotMachine>(machine).cloned() else {
        return;
    };
    let user = state.spinner.filter(|&u| world.get_entity(u).is_ok());
    let client = user.and_then(|u| client_of(world, u));
    let kind = state.kind();
    let lines = state.lines();
    let symbol = crate::cargo::MONEY_SYMBOL;
    let mut won = true;
    let mut winning = WINNING_NOTHING;
    if let Some(trap) = kind.trap
        && state.middle_row_all(trap)
    {
        activate_trap(world, machine, kind.symbols[trap], user);
    } else if state.middle_row_all(kind.jackpot) {
        winning = WINNING_JACKPOT;
        let prize = state.money + PRIZE_JACKPOT;
        machine_say(
            world,
            machine,
            &format!("ДЖЕКПОТ! Вы выиграли {prize} {symbol}!"),
        );
        let who = user.map_or_else(
            || "Exaybachay".to_owned(),
            |u| crate::chem::name_of(world, u),
        );
        let area = area_name(world, machine);
        crate::events::priority_announce(
            world,
            "",
            &format!("Поздравляем {who} с крупным выигрышем в {area}!"),
        );
        if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
            m.jackpots += 1;
            m.money = 0;
        }
        match state.paymode {
            PayMode::Holochip => {
                spawn_chip(world, machine, prize, None);
            }
            PayMode::Coin => {
                for _ in 0..5 {
                    let i = world.resource_mut::<CombatRng>().range(0, 5) as usize;
                    spawn_at_machine(world, machine, JACKPOT_COINS[i]);
                }
                coin_sound(world, machine);
            }
        }
    } else if lines == 5 {
        winning = WINNING_BIG;
        machine_say(
            world,
            machine,
            &format!("Большой выигрыш! Вы выиграли {PRIZE_BIG}{symbol}!"),
        );
        give_money(world, machine, PRIZE_BIG);
    } else if lines == 4 {
        winning = WINNING_SMALL;
        machine_say(
            world,
            machine,
            &format!("Выигрыш! Вы выиграли {PRIZE_SMALL}{symbol}!"),
        );
        give_money(world, machine, PRIZE_SMALL);
    } else if lines == 3 {
        winning = WINNING_FREESPIN;
        if let Some(client) = client {
            tell(world, client, Span::Notice, "получено 3 фриспина!");
        }
        if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
            m.balance += SPIN_PRICE * 4;
            // tg `max(money - SPIN_PRICE * 4, money)` never takes anything; the free spins
            // come out of the bank as intended.
            m.money = m.money.saturating_sub(SPIN_PRICE * 4);
        }
    } else {
        won = false;
        if let Some(client) = client {
            tell(world, client, Span::Warning, "неудача!");
        }
    }
    let t = now(world);
    if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
        m.working = false;
        m.spinner = None;
        m.winning = winning;
        m.clear_at = Some(t + WIN_SECS);
    }
    sound_at(
        world,
        machine,
        "sound/machines/lever/lever_stop.ogg",
        50.0,
        false,
    );
    if won {
        sound_at(
            world,
            machine,
            "sound/machines/roulette/roulettejackpot.ogg",
            50.0,
            true,
        );
    }
}

fn area_name(world: &World, machine: Entity) -> String {
    let Some(pos) = world.get::<TilePos>(machine).copied() else {
        return String::new();
    };
    world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|area| world.get::<DmName>(area))
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// `activate_trap`.
fn activate_trap(world: &mut World, machine: Entity, trap: &str, user: Option<Entity>) {
    machine_say(world, machine, "Big Loser! Prepare for your special prize!");
    match trap {
        HANDCUFFS => {
            let Some(user) =
                user.filter(|&u| world.get::<crate::inventory::Inventory>(u).is_some())
            else {
                return;
            };
            if world.get::<crate::restraints::Handcuffed>(user).is_some() {
                return;
            }
            sound_at(
                world,
                machine,
                "sound/items/weapons/handcuffs.ogg",
                30.0,
                true,
            );
            if let Some(cuffs) = crate::construction::spawn_detached(world, HANDCUFFS) {
                world.entity_mut(cuffs).insert(ss13_net::server::Networked);
                crate::restraints::apply_cuffs(world, cuffs, user);
            }
        }
        "/obj/item/suspiciousphone" => {
            sound_at(world, machine, "sound/items/dump_it.ogg", 30.0, true);
            if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
                m.balance = 0;
            }
        }
        "/obj/singularity" => {
            // `electrocute_act(80, SHOCK_ILLUSION)`: an illusory shock.
            if let Some(client) = user.and_then(|u| client_of(world, u)) {
                tell(world, client, Span::Danger, "Вас бьёт током!");
            }
        }
        _ => {
            if let Some(bang) = spawn_at_machine(world, machine, FLASHBANG) {
                crate::grenades::arm(world, bang, None, Some(1.0));
            }
        }
    }
}

/// `give_money`: what the bank covers is paid out, the rest goes onto the balance.
fn give_money(world: &mut World, machine: Entity, amount: u32) {
    let money = world.get::<SlotMachine>(machine).map_or(0, |m| m.money);
    let to_give = amount.min(money);
    let left = give_payout(world, machine, to_give);
    if let Some(mut m) = world.get_mut::<SlotMachine>(machine) {
        m.money -= to_give;
        // tg adds `amount - give_payout(...)`: right for holochips (which return the whole sum),
        // double for coins (which return the change); the intended surplus is the uncovered
        // part plus the change.
        m.balance += amount - to_give + left;
    }
}

/// `give_payout` / `dispense`: a holochip of `amount`, or coins of it (silver; iron and thrown
/// at someone within two tiles when emagged). The change the coins cannot make.
pub fn give_payout(world: &mut World, machine: Entity, amount: u32) -> u32 {
    let Some(state) = world.get::<SlotMachine>(machine).cloned() else {
        return amount;
    };
    if amount == 0 {
        return 0;
    }
    let target = state.emagged.then(|| near_living(world, machine)).flatten();
    let left = match state.paymode {
        PayMode::Holochip => {
            spawn_chip(world, machine, amount, target);
            0
        }
        PayMode::Coin => {
            let coin = if state.emagged {
                IRON_COIN
            } else {
                SILVER_COIN
            };
            let value = crate::cargo::money::coin_value(coin).max(1);
            for _ in 0..amount / value {
                if let Some(c) = spawn_at_machine(world, machine, coin)
                    && let Some(to) = target
                {
                    crate::throw::throw_at(world, c, to, 3, 10.0);
                }
            }
            amount % value
        }
    };
    coin_sound(world, machine);
    left
}

fn coin_sound(world: &mut World, machine: Entity) {
    let second = world.resource_mut::<CombatRng>().chance(50.0);
    let sound = if second {
        "sound/machines/coindrop2.ogg"
    } else {
        "sound/machines/coindrop.ogg"
    };
    sound_at(world, machine, sound, 50.0, true);
}

fn spawn_at_machine(world: &mut World, machine: Entity, path: &str) -> Option<Entity> {
    let pos = world.get::<TilePos>(machine).copied()?;
    crate::construction::spawn_obj(world, path, pos)
}

fn spawn_chip(world: &mut World, machine: Entity, credits: u32, target: Option<TilePos>) {
    if let Some(chip) = spawn_at_machine(world, machine, HOLOCHIP) {
        crate::cargo::money::make_holochip(world, chip, credits);
        if let Some(to) = target {
            crate::throw::throw_at(world, chip, to, 3, 10.0);
        }
    }
}

/// `locate(/mob/living) in range(2, src)`.
fn near_living(world: &mut World, machine: Entity) -> Option<TilePos> {
    let at = world.get::<TilePos>(machine).copied()?;
    let mut q = world.query_filtered::<&TilePos, With<crate::health::Health>>();
    q.iter(world)
        .copied()
        .find(|p| p.chebyshev(at).is_some_and(|d| d <= 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> SlotMachine {
        SlotMachine::new(SLOT_MACHINE, &mut CombatRng(5))
    }

    #[test]
    fn lines_count_the_longest_row_run() {
        let mut m = machine();
        m.reels = [[0, 1, 2], [3, 1, 4], [5, 1, 6], [0, 2, 1], [1, 3, 2]];
        assert_eq!(m.lines(), 3);
        m.reels[3][1] = 1;
        assert_eq!(m.lines(), 4);
        m.reels[4][1] = 1;
        assert_eq!(m.lines(), 5);
        assert!(m.middle_row_all(1));
        m.reels = [[0, 1, 2], [1, 2, 3], [2, 3, 4], [3, 4, 5], [4, 5, 6]];
        assert_eq!(m.lines(), 0);
    }

    #[test]
    fn kinds_follow_tg() {
        let m = machine();
        let kind = m.kind();
        assert_eq!(
            kind.symbols[kind.jackpot],
            "/obj/item/food/grown/cherry_bomb"
        );
        assert_eq!(kind.trap.map(|t| kind.symbols[t]), Some(FLASHBANG));
        assert_eq!(m.money, 3000);
        assert!((75..=200).contains(&m.plays) && (1..=4).contains(&m.jackpots));
        let clown = SlotMachine::new(
            "/obj/machinery/computer/slot_machine/clown",
            &mut CombatRng(1),
        );
        let kind = clown.kind();
        assert_eq!(
            kind.symbols[kind.jackpot],
            "/obj/vehicle/sealed/mecha/honker"
        );
        assert_eq!(kind.trap.map(|t| kind.symbols[t]), Some(HANDCUFFS));
        let name = machine_name(kind, &mut CombatRng(2));
        assert_eq!(name.split(' ').count(), 2);
    }
}
