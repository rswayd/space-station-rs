//! Playing cards (server), after tg `code/modules/cards`: decks (`/obj/item/toy/cards/deck` and
//! its kinds: Syndicate, card shoe, tarot, haunted tarot, Cards Against Spess, Kotahi, Wiz-Off,
//! blank), hands of cards (`cardhand`) and single cards (`singlecard`).
//!
//! A card in a deck or a hand is data ([`Card`]), not an atom: tg makes the atoms lazily
//! (`fetch_card_atoms`) and only moves them around; here a [`SingleCard`] entity is spawned
//! when a card leaves its pile and despawned when it goes back.
//!
//! - A deck held in both hands (tg `two_handed`, [`DeckWielded`]) deals onto cards and hands and
//!   is shuffled by an alt click; held in one hand it takes them back.
//! - An empty hand draws the top card (a right click: face up); a deck is picked up by dragging
//!   it onto oneself (`drag_pickup`).
//! - A hand of cards in hand lists its cards to take one out (tg's radial menu); a card used on
//!   a card makes a hand.
//! - A card in hand flips; a pen or crayon marks a card's corner, or writes on a blank one.
// shortcut: no 52 card pickup on throws, no card game mood or memory (`CardgameEvent`), no
// x-ray or reagent scanner reading, no reversed tarot draws or cursed Death draws, no card
// rotation by alt click (overlays have no transform), the hand's fan is offsets only; the
// haunted tarot's sixth sense and ghost alert wait for those mechanics.

use bevy::prelude::*;
use ss13_mapload::DmAtom;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, ask_world};

use super::server::{PendingAsk, item_path, now, set_name, sound_at};
use crate::combat::CombatRng;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::tools::tell;

pub const DECK: &str = "/obj/item/toy/cards/deck";
pub const CARDHAND: &str = "/obj/item/toy/cards/cardhand";
pub const SINGLECARD: &str = "/obj/item/toy/singlecard";
pub const RULESET: &str = "/obj/item/toy/singlecard/wizoff_ruleset";
const SHUFFLE_SOUND: &str = "sound/items/cards/cardshuffle.ogg";
const FLIP_SOUND: &str = "sound/items/cards/cardflip.ogg";
/// `/obj/item/toy/cards` `card_limit`.
const CARD_LIMIT: usize = 21;
/// `DECK_SHUFFLE_TIME`, `DECK_SYNDIE_SHUFFLE_TIME`.
const SHUFFLE_SECS: f64 = 5.0;
const SYNDIE_SHUFFLE_SECS: f64 = 3.0;
/// `CARDS_MAX_DISPLAY_LIMIT`, `CARDS_PIXEL_X_OFFSET`.
const FAN_LIMIT: usize = 5;
const FAN_X: i16 = -5;
/// `stripped_input(..., 50)` of card writing.
const CARD_TEXT_MAX: u32 = 50;

/// `strings/wizoff.json` `wizard`.
const WIZOFF: [&str; 27] = [
    "O1: Spell Cards",
    "O2: Summon Bees",
    "O3: Polymorph",
    "O4: Tesla Blast",
    "O5: Rod Form",
    "O6: Mutate",
    "O7: Fireball",
    "O8: Mjolnir",
    "O9: Smite",
    "D1: Smoke",
    "D2: Battlemage Armor",
    "D3: Repulse",
    "D4: Magic Missile",
    "D5: Disable Technology",
    "D6: Spell Trap",
    "D7: Forcewall",
    "D8: Arcane Heal",
    "D9: Stop Time",
    "U1: Shapechange",
    "U2: Spacetime Distortion",
    "U3: Scrying Orb",
    "U4: Blink",
    "U5: Knock",
    "U6: Teleport",
    "U7: Bind Soul",
    "U8: Warp Whistle",
    "U9: Jaunt",
];

/// The rules on the Wiz-Off ruleset card (`wizoff_ruleset/examine`).
const WIZOFF_RULES: [(Span, &str); 9] = [
    (Span::Notice, "Remember the rules of Wiz-Off!"),
    (Span::Info, "Each player draws 5 cards."),
    (
        Span::Info,
        "There are five rounds. Each round, a player selects a card to play, and the winner is selected based on the following rules:",
    ),
    (Span::Info, "Defensive beats Offensive!"),
    (Span::Info, "Offensive beats Utility!"),
    (Span::Info, "Utility beats Defensive!"),
    (
        Span::Info,
        "If both players play the same type of spell, the higher number wins!",
    ),
    (
        Span::Info,
        "The player who wins the most of the 5 rounds wins the game!",
    ),
    (
        Span::Notice,
        "Now get ready to battle for the fate of the universe: Wiz-Off!",
    ),
];

/// The Cards Against Spess texts (`GLOB.card_decks`: `strings/cas_white.txt`,
/// `strings/cas_black.txt`, converted to `ss13_mapload::service::CARDS_FILE`).
#[derive(Resource, Default, Clone, Debug)]
pub struct CasCards {
    pub white: Vec<String>,
    pub black: Vec<String>,
}

/// One card (`/obj/item/toy/singlecard` vars).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    /// `cardname`.
    pub name: String,
    /// `deckstyle`.
    pub style: String,
    /// `has_unique_card_icons`.
    pub unique_icons: bool,
    /// `flipped`: `CARD_FACEUP`.
    pub face_up: bool,
    /// `blank`: written on with a pen.
    pub blank: bool,
    /// `marked_color` (`"invisible"` for white pens and mime crayons).
    pub marked: Option<String>,
    /// The Wiz-Off rules card (`wizoff_ruleset`).
    pub rules: bool,
}

impl Card {
    fn new(name: impl Into<String>, style: &str, unique_icons: bool) -> Self {
        Self {
            name: name.into(),
            style: style.to_owned(),
            unique_icons,
            face_up: false,
            blank: false,
            marked: None,
            rules: false,
        }
    }

    /// `update_icon_state`.
    pub fn icon_state(&self) -> String {
        if !self.face_up {
            format!("singlecard_down_{}", self.style)
        } else if self.unique_icons {
            format!("sc_{}_{}", self.name, self.style)
        } else if self.blank {
            format!("sc_blank_{}", self.style)
        } else {
            format!("sc_scribble_{}", self.style)
        }
    }

    /// `update_name`.
    pub fn shown_name(&self) -> &str {
        if self.face_up { &self.name } else { "card" }
    }

    /// `getMarkedColor`: the mark a viewer sees.
    // shortcut: invisible marks need `TRAIT_REAGENT_SCANNER`, which is not ported.
    fn visible_mark(&self) -> Option<&str> {
        self.marked.as_deref().filter(|&m| m != "invisible")
    }
}

/// `/obj/item/toy/cards/deck`: its cards, top first.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Deck {
    pub cards: Vec<Card>,
    /// `card_limit`.
    pub limit: usize,
    /// `deckstyle`, `deck_base_icon`.
    pub style: String,
    pub base: String,
    /// `shuffle_time`, and when the `shuffle_cooldown` ends.
    pub shuffle_secs: f64,
    pub shuffle_ready: f64,
    /// `/obj/item/toy/cards/deck/tarot/haunted`.
    pub haunted: bool,
}

/// `/obj/item/toy/cards/cardhand`: its cards, the newest last.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CardHand {
    pub cards: Vec<Card>,
}

/// `/obj/item/toy/singlecard`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SingleCard(pub Card);

/// A deck held in both hands by the mob (tg `two_handed`, `TRAIT_WIELDED`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeckWielded(pub Entity);

/// The prompts of the cards.
#[derive(Clone, Debug)]
pub enum CardsAsk {
    /// A card out of a hand: the names as listed.
    Pick { hand: Entity, names: Vec<String> },
    /// Writing on a blank card with a pen.
    Write { card: Entity, pen: Entity },
}

/// What a deck kind is made of (`initialize_cards`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Contents {
    /// 52 cards and two jokers.
    Standard,
    /// The card shoe: empty.
    Empty,
    Tarot,
    Kotahi,
    Wizoff,
    /// `decksize` blank cards.
    Blank(usize),
    /// `decksize` texts of the white (false) or black deck.
    Cas {
        black: bool,
        size: usize,
    },
}

/// (style, base icon, unique card icons, shuffle secs, contents, fixed limit) of a deck type.
fn deck_spec(
    path: &str,
) -> (
    &'static str,
    &'static str,
    bool,
    f64,
    Contents,
    Option<usize>,
) {
    let under = |sub: &str| is_under(path, &format!("{DECK}/{sub}"));
    if under("syndicate") {
        (
            "syndicate",
            "deck_",
            true,
            SYNDIE_SHUFFLE_SECS,
            Contents::Standard,
            None,
        )
    } else if under("cardshoe") {
        (
            "",
            "deckshoe",
            true,
            SHUFFLE_SECS,
            Contents::Empty,
            Some(432),
        )
    } else if under("tarot") {
        ("tarot", "deck_", true, SHUFFLE_SECS, Contents::Tarot, None)
    } else if under("cas/black") {
        let cas = Contents::Cas {
            black: true,
            size: 50,
        };
        ("black", "deck_", false, SHUFFLE_SECS, cas, None)
    } else if under("cas") {
        let cas = Contents::Cas {
            black: false,
            size: 150,
        };
        ("white", "deck_", false, SHUFFLE_SECS, cas, None)
    } else if under("kotahi") {
        (
            "kotahi",
            "deck_",
            true,
            SHUFFLE_SECS,
            Contents::Kotahi,
            None,
        )
    } else if under("wizoff") {
        (
            "wizoff",
            "deck_",
            true,
            SHUFFLE_SECS,
            Contents::Wizoff,
            None,
        )
    } else if under("blank/black") {
        let blank = Contents::Blank(25);
        ("black", "deck_", false, SHUFFLE_SECS, blank, None)
    } else if under("blank") {
        let blank = Contents::Blank(25);
        ("white", "deck_", false, SHUFFLE_SECS, blank, None)
    } else {
        (
            "nanotrasen",
            "deck_",
            true,
            SHUFFLE_SECS,
            Contents::Standard,
            None,
        )
    }
}

/// The card names of a deck kind, top first (`initialize_cards`); CAS decks draw
/// `pick_n_take` from `cas`.
fn initial_names(contents: Contents, cas: &CasCards, rng: &mut CombatRng) -> Vec<String> {
    let mut out = Vec::new();
    match contents {
        Contents::Standard => {
            out.push("Joker Clown".to_owned());
            out.push("Joker Mime".to_owned());
            for suit in ["Hearts", "Spades", "Clubs", "Diamonds"] {
                out.push(format!("Ace of {suit}"));
                for i in 2..=10 {
                    out.push(format!("{i} of {suit}"));
                }
                for person in ["Jack", "Queen", "King"] {
                    out.push(format!("{person} of {suit}"));
                }
            }
        }
        Contents::Empty => {}
        Contents::Tarot => {
            for suit in ["Hearts", "Pikes", "Clovers", "Tiles"] {
                for i in 1..=10 {
                    out.push(format!("{i} of {suit}"));
                }
                for person in ["Valet", "Chevalier", "Dame", "Roi"] {
                    out.push(format!("{person} of {suit}"));
                }
            }
            for trump in [
                "The Magician",
                "The High Priestess",
                "The Empress",
                "The Emperor",
                "The Hierophant",
                "The Lover",
                "The Chariot",
                "Justice",
                "The Hermit",
                "The Wheel of Fortune",
                "Strength",
                "The Hanged Man",
                "Death",
                "Temperance",
                "The Devil",
                "The Tower",
                "The Star",
                "The Moon",
                "The Sun",
                "Judgement",
                "The World",
                "The Fool",
            ] {
                out.push(trump.to_owned());
            }
        }
        Contents::Kotahi => {
            for colour in ["Red", "Yellow", "Green", "Blue"] {
                out.push(format!("{colour} 0"));
                for _ in 0..2 {
                    out.push(format!("{colour} skip"));
                    out.push(format!("{colour} reverse"));
                    out.push(format!("{colour} draw 2"));
                    for i in 1..=9 {
                        out.push(format!("{colour} {i}"));
                    }
                }
            }
            for _ in 0..4 {
                out.push("Wildcard".to_owned());
                out.push("Draw 4".to_owned());
            }
        }
        Contents::Wizoff => {
            out.push("Wizoff Ruleset".to_owned());
            out.extend(WIZOFF.iter().map(|&s| s.to_owned()));
        }
        Contents::Blank(size) => {
            out.extend((0..size).map(|_| "blank card".to_owned()));
        }
        Contents::Cas { black, size } => {
            let mut pool = if black {
                cas.black.clone()
            } else {
                cas.white.clone()
            };
            for _ in 0..size {
                if pool.is_empty() {
                    break;
                }
                let i = rng.range(0, pool.len() as u32 - 1) as usize;
                out.push(pool.swap_remove(i));
            }
        }
    }
    out
}

/// A new deck of `path`'s kind.
pub fn new_deck(path: &str, cas: &CasCards, rng: &mut CombatRng) -> Deck {
    let (style, base, unique, shuffle_secs, contents, limit) = deck_spec(path);
    let cards: Vec<Card> = initial_names(contents, cas, rng)
        .into_iter()
        .map(|name| {
            let mut card = Card::new(name, style, unique);
            match contents {
                Contents::Blank(_) => card.blank = true,
                Contents::Wizoff if card.name == "Wizoff Ruleset" => {
                    // `wizoff_ruleset`: a black scribble card.
                    card.style = "black".to_owned();
                    card.unique_icons = false;
                    card.rules = true;
                }
                _ => {}
            }
            card
        })
        .collect();
    // `if(length(initial_cards)) card_limit = length(initial_cards)`.
    let limit = limit.unwrap_or(if cards.is_empty() {
        CARD_LIMIT
    } else {
        cards.len()
    });
    Deck {
        cards,
        limit,
        style: style.to_owned(),
        base: base.to_owned(),
        shuffle_secs,
        shuffle_ready: 0.0,
        haunted: is_under(path, &format!("{DECK}/tarot/haunted")),
    }
}

/// `insert`: as many of `cards` as fit under `limit` go to the bottom of `pile`, none when it
/// is full; how many went in.
pub fn take_in(pile: &mut Vec<Card>, limit: usize, cards: &[Card]) -> usize {
    if pile.len() >= limit {
        return 0;
    }
    let n = cards.len().min(limit - pile.len());
    pile.extend_from_slice(&cards[..n]);
    n
}

/// `update_icon_state` of a deck.
pub fn deck_icon_state(deck: &Deck) -> String {
    let fill = match deck.cards.len() {
        27.. => "full",
        11.. => "half",
        1.. => "low",
        0 => "empty",
    };
    format!("{}{}_{fill}", deck.base, deck.style)
}

/// `shuffle_inplace`.
fn shuffle(rng: &mut CombatRng, cards: &mut [Card]) {
    for i in (1..cards.len()).rev() {
        let j = rng.range(0, i as u32) as usize;
        cards.swap(i, j);
    }
}

/// `Initialize` of mapped decks and cards (a spawned card or hand gets its data first).
pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, DECK) && world.get::<Deck>(e).is_none() {
        let cas = cas_cards(world);
        let deck = match world.get_resource_mut::<CombatRng>() {
            Some(mut rng) => new_deck(path, &cas, &mut rng),
            None => new_deck(path, &cas, &mut CombatRng(1)),
        };
        world.entity_mut(e).insert(deck);
        refresh_deck(world, e);
    } else if is_under(path, CARDHAND) && world.get::<CardHand>(e).is_none() {
        world.entity_mut(e).insert(CardHand::default());
    } else if is_under(path, SINGLECARD) && world.get::<SingleCard>(e).is_none() {
        // A mapped card lies face up (`if(mapload) Flip(CARD_FACEUP)`).
        let mut card = if path == RULESET {
            let mut card = Card::new("Wizoff Ruleset", "black", false);
            card.rules = true;
            card
        } else {
            Card::new("Ace of Spades", "nanotrasen", true)
        };
        card.face_up = true;
        world.entity_mut(e).insert(SingleCard(card));
        refresh_card(world, e);
    }
}

/// The CAS texts, read from `<assets>/cards.ron` once (none without it).
fn cas_cards(world: &mut World) -> CasCards {
    if let Some(cas) = world.get_resource::<CasCards>() {
        return cas.clone();
    }
    let path = world
        .get_resource::<ss13_mapload::MapLoadPlugin>()
        .map(|m| m.assets.join(ss13_mapload::service::CARDS_FILE));
    let data = match path
        .as_deref()
        .map(ss13_mapload::service::CardStrings::read)
    {
        Some(Ok(data)) => data,
        Some(Err(e)) => {
            warn!("{e:#} (rerun `ss13-mapload convert`): no CAS cards");
            ss13_mapload::service::CardStrings::default()
        }
        None => ss13_mapload::service::CardStrings::default(),
    };
    let cas = CasCards {
        white: data.cas_white,
        black: data.cas_black,
    };
    world.insert_resource(cas.clone());
    cas
}

fn icon_of(world: &World, e: Entity) -> Option<String> {
    world.get::<DmAtom>(e).map(|a| a.icon.clone())
}

fn state_id(world: &World, icon: &str, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(icon, state))
}

fn set_state(world: &mut World, e: Entity, state: &str) {
    let Some(icon) = icon_of(world, e) else {
        return;
    };
    if let Some(id) = state_id(world, &icon, state)
        && let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

fn refresh_deck(world: &mut World, deck: Entity) {
    if let Some(state) = world.get::<Deck>(deck).map(deck_icon_state) {
        set_state(world, deck, &state);
    }
}

fn refresh_card(world: &mut World, e: Entity) {
    let Some(card) = world.get::<SingleCard>(e).map(|c| c.0.clone()) else {
        return;
    };
    set_state(world, e, &card.icon_state());
    set_name(world, e, card.shown_name());
}

/// `cardhand/update_overlays`: the top five cards fanned out.
fn refresh_hand(world: &mut World, hand: Entity) {
    let (Some(cards), Some(icon)) = (
        world.get::<CardHand>(hand).map(|h| h.cards.clone()),
        icon_of(world, hand),
    ) else {
        return;
    };
    let shown = cards.len().min(FAN_LIMIT);
    // `round(10 / (cards_to_display - 1))`.
    let step = if shown > 1 {
        (10.0 / (shown - 1) as f32).round() as i16
    } else {
        0
    };
    let wanted: Vec<Overlay> = cards[cards.len() - shown..]
        .iter()
        .enumerate()
        .filter_map(|(i, card)| {
            let id = state_id(world, &icon, &card.icon_state())?;
            Some(Overlay {
                pixel_offset: [FAN_X + i as i16 * step, 0],
                ..Overlay::new(id)
            })
        })
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(hand)
        && look.overlays != wanted
    {
        look.overlays = wanted;
    }
}

/// A new card entity for `card` (nowhere yet).
fn spawn_card(world: &mut World, card: Card) -> Option<Entity> {
    let path = if card.rules { RULESET } else { SINGLECARD };
    let e = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(e).insert(SingleCard(card));
    refresh_card(world, e);
    Some(e)
}

fn spawn_hand(world: &mut World, cards: Vec<Card>) -> Option<Entity> {
    let e = crate::construction::spawn_detached(world, CARDHAND)?;
    world.entity_mut(e).insert(CardHand { cards });
    refresh_hand(world, e);
    Some(e)
}

/// `put_in_hands`: the active hand first, else the other, else the floor under the mob.
fn give(world: &mut World, mob: Entity, item: Entity) {
    let Some(pos) = crate::chem::map_pos(world, mob) else {
        return;
    };
    crate::storage::take_out(world, item, pos);
    if let Some(inventory) = world.get::<Inventory>(mob).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, mob, item, hand);
    }
}

/// The mob holds `deck` in both hands (the other hand still free).
pub fn wielded(world: &World, mob: Entity, deck: Entity) -> bool {
    world.get::<DeckWielded>(deck) == Some(&DeckWielded(mob))
        && world
            .get::<Inventory>(mob)
            .is_some_and(|inv| inv.hands.contains(&Some(deck)) && inv.hands.contains(&None))
}

/// `two_handed` unwields a deck that left its wielder's hands or lost the free hand.
pub(super) fn drop_unheld_wield(world: &mut World) {
    let mut q = world.query::<(Entity, &DeckWielded)>();
    let all: Vec<(Entity, Entity)> = q.iter(world).map(|(e, w)| (e, w.0)).collect();
    for (deck, mob) in all {
        let held = world.get::<CarriedBy>(deck).is_some_and(|c| c.0 == mob);
        if !held || !wielded(world, mob, deck) {
            world.entity_mut(deck).remove::<DeckWielded>();
            unwield_effects(world, mob, deck);
        }
    }
}

fn unwield_effects(world: &mut World, mob: Entity, deck: Entity) {
    if world.get::<Deck>(deck).is_some_and(|d| d.haunted)
        && let Some(client) = super::server::client_of(world, mob)
    {
        tell(
            world,
            client,
            Span::Notice,
            "The veil to the underworld closes shut. You feel your senses returning to normal.",
        );
    }
}

/// `two_handed` `attack_self`: wield or let go.
fn toggle_wield(world: &mut World, client: Entity, mob: Entity, deck: Entity) {
    let name = crate::chem::name_of(world, deck);
    if world.get::<DeckWielded>(deck).is_some() {
        world.entity_mut(deck).remove::<DeckWielded>();
        tell(
            world,
            client,
            Span::Notice,
            format!("You are now carrying {name} with one hand."),
        );
        unwield_effects(world, mob, deck);
        return;
    }
    let free = world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands.contains(&None));
    if !free {
        tell(
            world,
            client,
            Span::Warning,
            "You need your other hand to be empty!",
        );
        return;
    }
    world.entity_mut(deck).insert(DeckWielded(mob));
    tell(
        world,
        client,
        Span::Notice,
        format!("You grab {name} with both hands."),
    );
    // `haunted/on_wield`.
    if world.get::<Deck>(deck).is_some_and(|d| d.haunted) {
        tell(
            world,
            client,
            Span::Notice,
            "The veil to the underworld is opened. You can sense the dead souls calling out...",
        );
    }
}

/// `draw`: the top card off the deck, with the flip sound.
fn draw(world: &mut World, client: Entity, deck: Entity) -> Option<Card> {
    let card = match world.get_mut::<Deck>(deck) {
        Some(mut d) if !d.cards.is_empty() => d.cards.remove(0),
        Some(_) => {
            tell(
                world,
                client,
                Span::Warning,
                "There are no more cards to draw!",
            );
            return None;
        }
        None => return None,
    };
    refresh_deck(world, deck);
    sound_at(world, deck, FLIP_SOUND, 50.0, true);
    Some(card)
}

/// tg `user.balloon_alert_to_viewers(text)`.
fn balloon(world: &mut World, mob: Entity, text: &str) {
    crate::balloon::balloon_alert_to_viewers(world, mob, text, None, &[]);
}

/// The held card thing's tg `attack_self`. True when `item` is one.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Deck>(item).is_some() {
        toggle_wield(world, client, mob, item);
        true
    } else if world.get::<CardHand>(item).is_some() {
        ask_pick(world, client, mob, item);
        true
    } else if world.get::<SingleCard>(item).is_some() {
        flip(world, mob, item);
        true
    } else {
        false
    }
}

/// `singlecard/attack_self` -> `Flip`.
fn flip(world: &mut World, mob: Entity, card: Entity) {
    if let Some(mut c) = world.get_mut::<SingleCard>(card) {
        c.0.face_up = !c.0.face_up;
    }
    refresh_card(world, card);
    if world.get::<TilePos>(card).is_some() {
        balloon(world, mob, "flips a card");
    }
}

/// `deck/attack_hand` (a right click: face up) and `singlecard/attack_hand_secondary` (flip).
/// True when the click was theirs.
pub(super) fn attack_hand(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<Deck>(target).is_none() {
        if right && world.get::<SingleCard>(target).is_some() {
            flip(world, mob, target);
            return true;
        }
        return false;
    }
    let Some(mut card) = draw(world, client, target) else {
        return true;
    };
    if right {
        card.face_up = !card.face_up;
    }
    if let Some(e) = spawn_card(world, card) {
        give(world, mob, e);
    }
    balloon(world, mob, "draws a card");
    true
}

/// `deck/click_alt`: shuffled while held in both hands.
pub(super) fn click_alt(world: &mut World, client: Entity, mob: Entity, deck: Entity) -> bool {
    let Some(state) = world.get::<Deck>(deck).cloned() else {
        return false;
    };
    if !wielded(world, mob, deck) {
        let name = crate::chem::name_of(world, deck);
        tell(
            world,
            client,
            Span::Notice,
            format!("You must hold the {name} with both hands to shuffle."),
        );
        return true;
    }
    let t = now(world);
    if t < state.shuffle_ready {
        return true;
    }
    let mut cards = state.cards;
    shuffle(&mut world.resource_mut::<CombatRng>(), &mut cards);
    if let Some(mut d) = world.get_mut::<Deck>(deck) {
        d.cards = cards;
        d.shuffle_ready = t + d.shuffle_secs;
    }
    sound_at(world, deck, SHUFFLE_SOUND, 50.0, true);
    balloon(world, mob, "shuffles the deck");
    true
}

/// What a card thing holds: a card's one card, a hand's cards.
fn cards_of(world: &World, e: Entity) -> Option<Vec<Card>> {
    if let Some(c) = world.get::<SingleCard>(e) {
        return Some(vec![c.0.clone()]);
    }
    world.get::<CardHand>(e).map(|h| h.cards.clone())
}

/// What is left of a card or hand after `taken` of its cards went elsewhere: a card or an
/// emptied hand is gone.
fn after_taken(world: &mut World, e: Entity, taken: usize) {
    if taken == 0 {
        return;
    }
    let left = match world.get_mut::<CardHand>(e) {
        Some(mut hand) => {
            let n = taken.min(hand.cards.len());
            hand.cards.drain(..n);
            hand.cards.len()
        }
        None => 0,
    };
    if left == 0 {
        world.despawn(e);
    } else {
        refresh_hand(world, e);
    }
}

/// `deck/insert` of a card or hand: face down at the bottom. False when the deck is full.
fn into_deck(world: &mut World, deck: Entity, item: Entity) -> bool {
    let Some(mut cards) = cards_of(world, item) else {
        return false;
    };
    for card in &mut cards {
        card.face_up = false;
    }
    let taken = match world.get_mut::<Deck>(deck) {
        Some(mut d) => {
            let limit = d.limit;
            take_in(&mut d.cards, limit, &cards)
        }
        None => 0,
    };
    if taken == 0 {
        return false;
    }
    after_taken(world, item, taken);
    refresh_deck(world, deck);
    true
}

/// `cardhand/insert` of one card. False when the hand is full.
fn into_hand(world: &mut World, hand: Entity, card: Card) -> bool {
    let taken = match world.get_mut::<CardHand>(hand) {
        Some(mut h) => take_in(&mut h.cards, CARD_LIMIT, &[card]),
        None => 0,
    };
    refresh_hand(world, hand);
    taken > 0
}

fn stacked_too_high(world: &mut World, client: Entity, deck: Entity) {
    let name = crate::chem::name_of(world, deck);
    tell(
        world,
        client,
        Span::Warning,
        format!("The {name} is stacked too high!"),
    );
}

/// tg `item_interaction` of decks, hands and cards with `held` (a right click deals or adds
/// face up). True when the click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    let held_deck = world.get::<Deck>(held).is_some();
    let held_card = world.get::<SingleCard>(held).is_some();
    let held_hand = world.get::<CardHand>(held).is_some();
    // `deck/item_interaction`: cards and hands go back in.
    if world.get::<Deck>(target).is_some() {
        if !held_card && !held_hand {
            return false;
        }
        if into_deck(world, target, held) {
            let what = if held_card { "card" } else { "cards" };
            balloon(world, mob, &format!("puts {what} in deck"));
        } else {
            stacked_too_high(world, client, target);
        }
        return true;
    }
    if world.get::<CardHand>(target).is_some() {
        return hand_interaction(world, client, mob, held, target, right);
    }
    if world.get::<SingleCard>(target).is_none() {
        return false;
    }
    // `singlecard/item_interaction`.
    if held_deck && !wielded(world, mob, held) {
        if into_deck(world, held, target) {
            balloon(world, mob, "puts card in deck");
        } else {
            stacked_too_high(world, client, held);
        }
        return true;
    }
    let card = if held_deck {
        let Some(card) = draw(world, client, held) else {
            return true;
        };
        Some(card)
    } else if held_card {
        world.get::<SingleCard>(held).map(|c| c.0.clone())
    } else {
        None
    };
    if let Some(mut card) = card {
        if right {
            card.face_up = !card.face_up;
        }
        if held_deck {
            balloon(world, mob, "deals a card");
        } else {
            world.despawn(held);
        }
        combine(world, mob, target, card);
        return true;
    }
    if held_hand {
        return hand_interaction(world, client, mob, target, held, right);
    }
    mark_or_write(world, client, mob, held, target)
}

/// Card + card: a new hand where the clicked card was (in the active hand if it was held).
fn combine(world: &mut World, mob: Entity, target: Entity, card: Card) {
    let Some(first) = world.get::<SingleCard>(target).map(|c| c.0.clone()) else {
        return;
    };
    let at = world.get::<TilePos>(target).copied();
    let offset = world.get::<Appearance>(target).map(|a| a.pixel_offset);
    world.despawn(target);
    let Some(hand) = spawn_hand(world, vec![first, card]) else {
        return;
    };
    match at {
        Some(pos) => {
            crate::storage::take_out(world, hand, pos);
            if let (Some(offset), Some(mut look)) = (offset, world.get_mut::<Appearance>(hand)) {
                look.pixel_offset = offset;
            }
        }
        None => give(world, mob, hand),
    }
}

/// `cardhand/item_interaction` with `tool` (a card, or a deck: back in when held in one hand,
/// else its top card dealt).
fn hand_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    hand: Entity,
    right: bool,
) -> bool {
    let card = if world.get::<Deck>(tool).is_some() {
        if !wielded(world, mob, tool) {
            if into_deck(world, tool, hand) {
                balloon(world, mob, "puts card in deck");
            } else {
                stacked_too_high(world, client, tool);
            }
            return true;
        }
        let full = world
            .get::<CardHand>(hand)
            .is_some_and(|h| h.cards.len() >= CARD_LIMIT);
        if full {
            tell(
                world,
                client,
                Span::Warning,
                "You can't hold any more cards in your hand!",
            );
            return true;
        }
        match draw(world, client, tool) {
            Some(card) => card,
            None => return true,
        }
    } else if let Some(c) = world.get::<SingleCard>(tool) {
        c.0.clone()
    } else {
        return false;
    };
    let mut card = card;
    if right {
        card.face_up = !card.face_up;
    }
    let from_deck = world.get::<Deck>(tool).is_some();
    if into_hand(world, hand, card) {
        if !from_deck {
            world.despawn(tool);
        }
    } else {
        tell(
            world,
            client,
            Span::Warning,
            "You can't hold any more cards in your hand!",
        );
    }
    true
}

/// The cheating mark a writing tool leaves (`pen.colour`, `crayon.crayon_color`; white pens
/// and mime crayons leave an invisible one). `None`: not a writing tool.
pub fn mark_colour(path: &str) -> Option<String> {
    let colour = if is_under(path, "/obj/item/pen") {
        match path {
            "/obj/item/pen/blue" | "/obj/item/pen/survival" => "blue",
            "/obj/item/pen/red" => "red",
            "/obj/item/pen/invisible" => "white",
            "/obj/item/pen/charcoal" => "grey",
            _ => "black",
        }
    } else if is_under(path, "/obj/item/toy/crayon") && !path.contains("spraycan") {
        match path.rsplit('/').next().unwrap_or_default() {
            "crayon" => "red",
            other => other,
        }
    } else {
        return None;
    };
    Some(match colour {
        "white" | "mime" => "invisible".to_owned(),
        other => other.to_owned(),
    })
}

/// A pen or crayon on a card: a mark in its corner, or the text of a blank card.
fn mark_or_write(
    world: &mut World,
    client: Entity,
    mob: Entity,
    pen: Entity,
    card: Entity,
) -> bool {
    let Some(colour) = mark_colour(&item_path(world, pen)) else {
        return false;
    };
    let blank = world.get::<SingleCard>(card).is_some_and(|c| c.0.blank);
    if !blank {
        let (name, tool) = (
            crate::chem::name_of(world, card),
            crate::chem::name_of(world, pen),
        );
        if let Some(mut c) = world.get_mut::<SingleCard>(card) {
            c.0.marked = Some(colour.clone());
        }
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "You put a {colour} mark in the corner of {name} with the {tool}. Cheat to win!"
            ),
        );
        return true;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Card Writing",
            "What do you wish to write on the card?",
            PromptKind::Text {
                default: String::new(),
                max_length: CARD_TEXT_MAX,
                multiline: false,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Cards(CardsAsk::Write { card, pen }));
    true
}

/// `cardhand/attack_self`: the cards to take one out (tg's radial menu).
fn ask_pick(world: &mut World, client: Entity, mob: Entity, hand: Entity) {
    let Some(names) = world
        .get::<CardHand>(hand)
        .map(|h| h.cards.iter().map(|c| c.name.clone()).collect::<Vec<_>>())
    else {
        return;
    };
    if names.is_empty() {
        return;
    }
    let title = crate::chem::name_of(world, hand);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "Which card?",
            PromptKind::List {
                items: names.clone(),
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Cards(CardsAsk::Pick { hand, names }));
}

/// A card prompt's answer, checked again: the hand still held and unchanged, the pen in the
/// active hand and the card in reach.
pub(super) fn answer(
    world: &mut World,
    actor: Entity,
    ask: CardsAsk,
    answer: Option<PromptAnswer>,
) {
    match (ask, answer) {
        (CardsAsk::Pick { hand, names }, Some(PromptAnswer::Item(i))) => {
            let held = world
                .get::<Inventory>(actor)
                .is_some_and(|inv| inv.holds(hand));
            let same = world
                .get::<CardHand>(hand)
                .is_some_and(|h| h.cards.iter().map(|c| &c.name).eq(names.iter()));
            if !held || !same || i >= names.len() {
                return;
            }
            pick(world, actor, hand, i);
        }
        (CardsAsk::Write { card, pen }, Some(PromptAnswer::Text(text))) => {
            let text = text.trim();
            let pen_held = world
                .get::<Inventory>(actor)
                .is_some_and(|inv| inv.active_item() == Some(pen));
            if text.is_empty()
                || !pen_held
                || !super::paper::reachable(world, actor, card)
                || !world.get::<SingleCard>(card).is_some_and(|c| c.0.blank)
            {
                return;
            }
            let text: String = text.chars().take(CARD_TEXT_MAX as usize).collect();
            if let Some(mut c) = world.get_mut::<SingleCard>(card) {
                c.0.name = text;
                c.0.blank = false;
            }
            refresh_card(world, card);
        }
        _ => {}
    }
}

/// Card `i` out of the hand into the hands; the last one left follows and the hand is gone.
pub fn pick(world: &mut World, mob: Entity, hand: Entity, i: usize) {
    let Some(card) = world
        .get_mut::<CardHand>(hand)
        .and_then(|mut h| (i < h.cards.len()).then(|| h.cards.remove(i)))
    else {
        return;
    };
    sound_at(world, hand, FLIP_SOUND, 50.0, true);
    if let Some(e) = spawn_card(world, card) {
        give(world, mob, e);
    }
    let left = world.get::<CardHand>(hand).map_or(0, |h| h.cards.len());
    if left == 1 {
        let last = world
            .get_mut::<CardHand>(hand)
            .and_then(|mut h| h.cards.pop());
        world.despawn(hand);
        if let Some(e) = last.and_then(|card| spawn_card(world, card)) {
            give(world, mob, e);
        }
    } else {
        refresh_hand(world, hand);
    }
}

/// `examine` of decks, hands and cards as `examiner` sees them.
pub fn examine_lines(
    world: &World,
    examiner: Entity,
    target: Entity,
    lines: &mut Vec<crate::examine::Line>,
) {
    let holds = world
        .get::<Inventory>(examiner)
        .is_some_and(|inv| inv.holds(target));
    if let Some(deck) = world.get::<Deck>(target) {
        if let Some(mark) = deck.cards.first().and_then(Card::visible_mark) {
            lines.push((
                Span::Notice,
                format!("The top card of the deck has a {mark} mark on the corner!"),
            ));
        }
        lines.push((
            Span::Notice,
            "Click and drag the deck to yourself to pickup.".to_owned(),
        ));
    } else if let Some(hand) = world.get::<CardHand>(target) {
        lines.push((
            Span::Notice,
            format!("There are {} cards.", hand.cards.len()),
        ));
        for card in &hand.cards {
            if holds || card.face_up {
                lines.push((Span::Notice, format!("The hand contains a: {}", card.name)));
            }
            if let Some(mark) = card.visible_mark() {
                lines.push((
                    Span::Notice,
                    format!("There is a {mark} mark on the corner of a card in the cardhand!"),
                ));
            }
        }
    } else if let Some(SingleCard(card)) = world.get::<SingleCard>(target) {
        if holds {
            lines.push((Span::Notice, format!("The card reads: {}.", card.name)));
            if card.blank {
                lines.push((
                    Span::Notice,
                    "The card is blank. Write on it with a pen.".to_owned(),
                ));
            }
        } else {
            lines.push((
                Span::Warning,
                "You need to have the card in your hand to check it!".to_owned(),
            ));
        }
        if let Some(mark) = card.visible_mark() {
            lines.push((
                Span::Notice,
                format!("The card has a {mark} mark on the corner!"),
            ));
        }
        if card.rules {
            lines.extend(
                WIZOFF_RULES
                    .iter()
                    .map(|&(span, text)| (span, text.to_owned())),
            );
        }
    }
}

/// `drag_pickup`: a deck dragged onto its user goes into a free hand. True when it was a deck.
pub fn drag_pickup(world: &mut World, mob: Entity, dragged: Entity) -> bool {
    if world.get::<Deck>(dragged).is_none() || world.get::<TilePos>(dragged).is_none() {
        return false;
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, mob, dragged, hand);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(path: &str) -> Deck {
        new_deck(path, &CasCards::default(), &mut CombatRng(7))
    }

    #[test]
    fn decks_hold_their_kinds_cards() {
        let standard = deck(DECK);
        assert_eq!(standard.cards.len(), 54);
        assert_eq!(standard.limit, 54);
        assert_eq!(standard.cards[0].name, "Joker Clown");
        assert_eq!(standard.cards[2].name, "Ace of Hearts");
        assert!(standard.cards.iter().all(|c| !c.face_up));
        assert_eq!(deck_icon_state(&standard), "deck_nanotrasen_full");

        let syndie = deck("/obj/item/toy/cards/deck/syndicate");
        assert_eq!(syndie.shuffle_secs, SYNDIE_SHUFFLE_SECS);
        assert_eq!(syndie.cards[0].icon_state(), "singlecard_down_syndicate");

        let shoe = deck("/obj/item/toy/cards/deck/cardshoe");
        assert!(shoe.cards.is_empty());
        assert_eq!(shoe.limit, 432);
        assert_eq!(deck_icon_state(&shoe), "deckshoe_empty");
    }

    #[test]
    fn special_decks_follow_tg() {
        let tarot = deck("/obj/item/toy/cards/deck/tarot");
        assert_eq!(tarot.cards.len(), 78);
        assert!(!tarot.haunted);
        assert!(deck("/obj/item/toy/cards/deck/tarot/haunted").haunted);

        let kotahi = deck("/obj/item/toy/cards/deck/kotahi");
        assert_eq!(kotahi.cards.len(), 108);
        assert_eq!(kotahi.cards.iter().filter(|c| c.name == "Red 0").count(), 1);

        let wizoff = deck("/obj/item/toy/cards/deck/wizoff");
        assert_eq!(wizoff.cards.len(), 28);
        assert!(wizoff.cards[0].rules, "the ruleset is the top card");
        assert_eq!(wizoff.cards[0].style, "black");

        let blank = deck("/obj/item/toy/cards/deck/blank/black");
        assert_eq!(blank.cards.len(), 25);
        assert!(blank.cards.iter().all(|c| c.blank && c.style == "black"));
        let mut card = blank.cards[0].clone();
        card.face_up = true;
        assert_eq!(card.icon_state(), "sc_blank_black");
        card.blank = false;
        assert_eq!(card.icon_state(), "sc_scribble_black");

        let cas = CasCards {
            white: (0..200).map(|i| format!("white {i}")).collect(),
            black: (0..10).map(|i| format!("black {i}")).collect(),
        };
        let mut rng = CombatRng(3);
        let white = new_deck("/obj/item/toy/cards/deck/cas", &cas, &mut rng);
        assert_eq!(white.cards.len(), 150);
        let mut names: Vec<&str> = white.cards.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 150, "pick_n_take never repeats");
        let black = new_deck("/obj/item/toy/cards/deck/cas/black", &cas, &mut rng);
        assert_eq!(black.cards.len(), 10, "a short list gives what it has");
        assert!(!black.cards[0].unique_icons);
    }

    #[test]
    fn piles_take_what_fits_and_shuffle_keeps_cards() {
        let mut pile = vec![Card::new("A", "nanotrasen", true); 20];
        let two = [
            Card::new("B", "nanotrasen", true),
            Card::new("C", "nanotrasen", true),
        ];
        assert_eq!(take_in(&mut pile, CARD_LIMIT, &two), 1);
        assert_eq!(pile.last().unwrap().name, "B");
        assert_eq!(take_in(&mut pile, CARD_LIMIT, &two), 0, "full");

        let mut cards = deck(DECK).cards;
        let before = cards.clone();
        shuffle(&mut CombatRng(11), &mut cards);
        assert_ne!(cards, before);
        let mut a: Vec<String> = cards.into_iter().map(|c| c.name).collect();
        let mut b: Vec<String> = before.into_iter().map(|c| c.name).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }

    #[test]
    fn cards_show_their_face_and_marks() {
        let mut card = Card::new("Ace of Spades", "nanotrasen", true);
        assert_eq!(card.shown_name(), "card");
        assert_eq!(card.icon_state(), "singlecard_down_nanotrasen");
        card.face_up = true;
        assert_eq!(card.shown_name(), "Ace of Spades");
        assert_eq!(card.icon_state(), "sc_Ace of Spades_nanotrasen");
        assert_eq!(mark_colour("/obj/item/pen").as_deref(), Some("black"));
        assert_eq!(
            mark_colour("/obj/item/pen/invisible").as_deref(),
            Some("invisible")
        );
        assert_eq!(
            mark_colour("/obj/item/toy/crayon/mime").as_deref(),
            Some("invisible")
        );
        assert_eq!(
            mark_colour("/obj/item/toy/crayon/blue").as_deref(),
            Some("blue")
        );
        assert_eq!(mark_colour("/obj/item/toy/crayon/spraycan"), None);
        assert_eq!(mark_colour("/obj/item/stamp"), None);
        card.marked = Some("invisible".to_owned());
        assert_eq!(card.visible_mark(), None);
        card.marked = Some("red".to_owned());
        assert_eq!(card.visible_mark(), Some("red"));
    }
}
