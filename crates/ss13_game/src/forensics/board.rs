//! The detective's notice board (server), after tg `/obj/structure/detectiveboard`
//! (`code/game/objects/structures/detectiveboard.dm`).
//!
//! - Up to 8 cases, each a name and a colour (asked when added). A paper or a photo clicked on
//!   the board is pinned to the current case as evidence, with a name and a description asked
//!   first (empty: the item's own). The board's sprite shows the current case's notices.
//! - The tgui `DetectiveBoard` window: case tabs (add, rename, remove: its items come back to the
//!   user's hands), evidence (look at it: the photo's window, the paper's text; take it down) and
//!   red strings between two evidences of a case.
//! - A wrench (right click, 6 s) takes the board off the wall: its frame, the pinned items out.
// shortcut: no canvas with dragged pins and drawn strings (the window lists evidence with their
// links; `set_evidence_cords` is not ported), a paper's text is shown in chat instead of a
// browser window, the board does not break (no `max_integrity`), papers lying under a mapped
// board stay there (tg pins them into a case that does not exist yet and runtimes).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::{
    BOARD_ADD_CASE, BOARD_CASE, BOARD_CASE_REF, BOARD_CONNECT, BOARD_EVIDENCE_REF, BOARD_FROM_REF,
    BOARD_LOOK, BOARD_REMOVE_CASE, BOARD_REMOVE_EVIDENCE, BOARD_RENAME_CASE, BOARD_SET_CASE,
    BOARD_TO_REF, BoardCase, BoardEvidence, DetectiveBoardData, DetectiveBoardUi,
};
use crate::construction::Job;
use crate::inventory::{Inventory, no_drop};
use crate::storage::{StoredIn, put_inside, take_out};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};
use crate::wallmount::WallJob;

pub const BOARD: &str = "/obj/structure/detectiveboard";
const FRAME: &str = "/obj/item/wallframe/detectiveboard";
const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
const PAPER: &str = "/obj/item/paper";
const PHOTO: &str = "/obj/item/photo";
/// `MAX_CASES`, `MAX_ICON_NOTICES`, `MAX_NAME_LEN`, `MAX_DESC_LEN`.
pub const MAX_CASES: usize = 8;
const MAX_ICON_NOTICES: u32 = 8;
const MAX_NAME_LEN: usize = 42;
const MAX_DESC_LEN: usize = 280;
/// `case_colors`.
pub const CASE_COLORS: [&str; 6] = ["red", "orange", "yellow", "green", "blue", "violet"];
/// `wrench_act_secondary`: `use_tool(src, user, 6 SECONDS)`.
const UNSECURE_DS: f32 = 60.0;
const DECONSTRUCT_SOUND: &str = "sound/items/deconstruct.ogg";
const TITLE: &str = "Detective's Board";

/// `/datum/evidence`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub photo: bool,
    pub item: Entity,
    pub connections: Vec<u32>,
}

/// `/datum/case`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    pub id: u32,
    pub name: String,
    pub color: String,
    pub notices: u32,
    pub evidences: Vec<Evidence>,
}

/// `/obj/structure/detectiveboard`: its cases, the current one (0-based) and whether someone is
/// pinning evidence (`attaching_evidence`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DetectiveBoard {
    pub cases: Vec<Case>,
    pub current: usize,
    pub attaching: bool,
    next_id: u32,
}

impl DetectiveBoard {
    fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// `add_case`: up to [`MAX_CASES`], the new one current.
    pub fn add_case(&mut self, name: String, color: String) -> bool {
        if self.cases.len() >= MAX_CASES {
            return false;
        }
        let id = self.new_id();
        self.cases.push(Case {
            id,
            name,
            color,
            notices: 0,
            evidences: Vec::new(),
        });
        self.current = self.cases.len() - 1;
        true
    }

    /// Pins `item` to the current case.
    pub fn pin(&mut self, item: Entity, name: String, description: String, photo: bool) -> bool {
        let id = self.new_id();
        let Some(case) = self.cases.get_mut(self.current) else {
            return false;
        };
        case.notices += 1;
        case.evidences.push(Evidence {
            id,
            name,
            description,
            photo,
            item,
            connections: Vec::new(),
        });
        true
    }

    /// `remove_evidence`: off the current case with its strings; the item it held.
    pub fn unpin(&mut self, evidence: u32) -> Option<Entity> {
        let case = self.cases.get_mut(self.current)?;
        let index = case.evidences.iter().position(|e| e.id == evidence)?;
        let removed = case.evidences.remove(index);
        for other in &mut case.evidences {
            other.connections.retain(|&c| c != evidence);
        }
        case.notices = case.notices.saturating_sub(1);
        Some(removed.item)
    }

    /// `add_connection` between two evidences of the current case, both ways, once.
    pub fn connect(&mut self, from: u32, to: u32) -> bool {
        let Some(case) = self.cases.get_mut(self.current) else {
            return false;
        };
        if from == to
            || !case.evidences.iter().any(|e| e.id == from)
            || !case.evidences.iter().any(|e| e.id == to)
        {
            return false;
        }
        for e in &mut case.evidences {
            let other = if e.id == from {
                to
            } else if e.id == to {
                from
            } else {
                continue;
            };
            if !e.connections.contains(&other) {
                e.connections.push(other);
            }
        }
        true
    }

    /// `remove_case`: its items; the last case becomes current.
    pub fn remove_case(&mut self, case: u32) -> Option<Vec<Entity>> {
        let index = self.cases.iter().position(|c| c.id == case)?;
        let removed = self.cases.remove(index);
        self.current = self.cases.len().saturating_sub(1);
        Some(removed.evidences.into_iter().map(|e| e.item).collect())
    }

    /// The notices drawn on the board (`notices_N`), `None` without cases.
    pub fn shown_notices(&self) -> Option<u32> {
        self.cases
            .get(self.current)
            .map(|c| c.notices.min(MAX_ICON_NOTICES))
    }
}

/// On a prompt: what the board asked and for whom.
#[derive(Component, Clone, Debug)]
enum BoardAsk {
    EvidenceName {
        board: Entity,
        item: Entity,
    },
    EvidenceDesc {
        board: Entity,
        item: Entity,
        name: String,
    },
    CaseName {
        board: Entity,
    },
    CaseColor {
        board: Entity,
        name: String,
    },
    Rename {
        board: Entity,
        case: u32,
    },
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `ui_interact` of the board with an empty hand.
    app.add_click_handler(ClickPhase::EmptyHand, 3309, |w, c| {
        crate::forensics::board::attack_hand(w, c.client, c.mob, c.target)
    });
    // Papers and photos pinned to the board, its wrench.
    app.add_click_handler(ClickPhase::HeldOnTarget, 809, |w, c| {
        c.held.is_some_and(|h| {
            crate::forensics::board::item_interaction(
                w,
                c.client,
                c.mob,
                h,
                c.target,
                c.modifiers.right,
            )
        })
    });
    app.register_ui::<DetectiveBoardUi>()
        .add_observer(add_board)
        .add_systems(
            Update,
            (
                board_acts.in_set(UiSystems::Handle),
                answers
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer)
                    .after(board_acts),
            ),
        );
}

fn add_board(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs.get(add.entity).is_ok_and(|o| is_under(&o.path, BOARD)) {
        commands
            .entity(add.entity)
            .insert_if_new(DetectiveBoard::default());
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// `update_overlays`: the current case's notices.
pub fn refresh_look(world: &mut World, board: Entity) {
    let Some(shown) = world
        .get::<DetectiveBoard>(board)
        .map(|b| b.shown_notices())
    else {
        return;
    };
    let icon = world
        .get::<DmAtom>(board)
        .map(|a| a.icon.clone())
        .unwrap_or_default();
    let overlay = shown.and_then(|n| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, &format!("notices_{n}")))
    });
    if let Some(mut look) = world.get_mut::<Appearance>(board) {
        let overlays: Vec<Overlay> = overlay.map(Overlay::new).into_iter().collect();
        if look.overlays != overlays {
            look.overlays = overlays;
        }
    }
}

/// `attack_hand` -> `ui_interact`. True when `target` is a board.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<DetectiveBoard>(target).is_none() {
        return false;
    }
    let title = crate::examine::name_of(world, target);
    world.write_message(OpenUi::new::<DetectiveBoardUi>(client, mob, target, title));
    true
}

/// `item_interaction` (a paper or a photo pinned) and `wrench_act_secondary`. True when the
/// click was the board's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    let Some(board) = world.get::<DetectiveBoard>(target).cloned() else {
        return false;
    };
    if right && tool_of(world, held).is_some_and(|(k, _)| k == ToolKind::Wrench) {
        let text = if crate::obj_damage::anchored(world, target) {
            "unsecuring..."
        } else {
            "securing..."
        };
        crate::balloon::balloon_alert(world, mob, target, text);
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        use_tool(
            world,
            actor,
            target,
            UNSECURE_DS,
            0,
            Job::WallMount(WallJob::UnsecureBoard),
        );
        return true;
    }
    let path = path_of(world, held);
    if !is_under(&path, PAPER) && !is_under(&path, PHOTO) {
        return false;
    }
    if board.cases.is_empty() {
        tell(world, client, Span::Plain, "There are no cases!");
        return true;
    }
    if board.attaching {
        tell(
            world,
            client,
            Span::Plain,
            "You already attaching evidence!",
        );
        return true;
    }
    if let Some(mut b) = world.get_mut::<DetectiveBoard>(target) {
        b.attaching = true;
    }
    ask_text(
        world,
        client,
        mob,
        "Please enter the evidence name",
        MAX_NAME_LEN,
        BoardAsk::EvidenceName {
            board: target,
            item: held,
        },
    );
    true
}

fn ask_text(
    world: &mut World,
    client: Entity,
    mob: Entity,
    message: &str,
    max: usize,
    ask: BoardAsk,
) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            TITLE,
            message,
            PromptKind::Text {
                default: String::new(),
                max_length: max as u32,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(ask);
}

/// The end of the wrench: `deconstruct(disassembled = TRUE)`, the frame and the pinned items on
/// the tile.
pub fn finish_unsecure(world: &mut World, board: Entity) {
    let (Some(state), Some(&pos)) = (
        world.get::<DetectiveBoard>(board).cloned(),
        world.get::<TilePos>(board),
    ) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, DECONSTRUCT_SOUND, 50.0, true),
    );
    let frame = crate::construction::spawn_obj(world, FRAME, pos);
    if frame.is_none() {
        crate::construction::spawn_stack(world, WOOD, pos, 1);
    }
    for case in state.cases {
        for evidence in case.evidences {
            if world
                .get::<StoredIn>(evidence.item)
                .is_some_and(|s| s.0 == board)
            {
                take_out(world, evidence.item, pos);
            }
        }
    }
    world.despawn(board);
    crate::construction::tile_changed(world, pos);
}

/// `remove_item(item, user)`: out of the board into the user's hands (or under the board).
fn remove_item(world: &mut World, board: Entity, item: Entity, user: Entity) {
    let Some(&pos) = world.get::<TilePos>(board) else {
        return;
    };
    if !world.get::<StoredIn>(item).is_some_and(|s| s.0 == board) {
        return;
    }
    take_out(world, item, pos);
    if let Some(inventory) = world.get::<Inventory>(user).cloned() {
        crate::storage::put_in_hands(world, user, &inventory, item);
    }
    crate::balloon::balloon_alert(world, user, board, "removed from board");
}

impl ServerUi for DetectiveBoardUi {
    type Source = &'static DetectiveBoard;
    type UserParam = Query<'static, 'static, &'static crate::service::Paper>;
    type Act = BoardAct;

    fn static_data(_: &DetectiveBoard) {}

    fn data(
        board: &DetectiveBoard,
        _: Entity,
        papers: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> DetectiveBoardData {
        let cases = board
            .cases
            .iter()
            .map(|c| BoardCase {
                id: c.id,
                name: c.name.clone(),
                color: c.color.clone(),
                evidences: c
                    .evidences
                    .iter()
                    .map(|e| BoardEvidence {
                        id: e.id,
                        name: e.name.clone(),
                        description: e.description.clone(),
                        photo: e.photo,
                        text: papers
                            .get(e.item)
                            .ok()
                            .and_then(|p| p.inputs.first())
                            .map(|i| i.raw_text.clone())
                            .unwrap_or_default(),
                        connections: e.connections.clone(),
                    })
                    .collect(),
            })
            .collect();
        DetectiveBoardData {
            cases,
            current_case: board.current,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<BoardAct, UiReject> {
        let id = |key: &str| {
            let n = params.number(key)?;
            if n < 0.0 || n.fract() != 0.0 || n > f64::from(u32::MAX) {
                return Err(UiReject::BadParams(format!("{key}: {n}")));
            }
            Ok(n as u32)
        };
        Ok(match action {
            BOARD_ADD_CASE => BoardAct::AddCase,
            BOARD_SET_CASE => BoardAct::SetCase(id(BOARD_CASE)? as usize),
            BOARD_REMOVE_CASE => BoardAct::RemoveCase(id(BOARD_CASE_REF)?),
            BOARD_RENAME_CASE => BoardAct::RenameCase(id(BOARD_CASE_REF)?),
            BOARD_LOOK => BoardAct::Look(id(BOARD_CASE_REF)?, id(BOARD_EVIDENCE_REF)?),
            BOARD_REMOVE_EVIDENCE => BoardAct::RemoveEvidence(id(BOARD_EVIDENCE_REF)?),
            BOARD_CONNECT => BoardAct::Connect(id(BOARD_FROM_REF)?, id(BOARD_TO_REF)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardAct {
    AddCase,
    /// 0-based case index.
    SetCase(usize),
    RemoveCase(u32),
    RenameCase(u32),
    Look(u32, u32),
    RemoveEvidence(u32),
    Connect(u32, u32),
}

/// `ui_act`, every reference looked up in the board's own lists at the moment of effect.
fn board_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<DetectiveBoardUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, BoardAct)> = acts
        .read(world.resource::<Messages<UiActed<DetectiveBoardUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, user, board, act) in pending {
        let Some(state) = world.get::<DetectiveBoard>(board).cloned() else {
            continue;
        };
        match act {
            BoardAct::AddCase => {
                if state.cases.len() >= MAX_CASES {
                    continue;
                }
                ask_text(
                    world,
                    client,
                    user,
                    "Please enter the case name",
                    MAX_NAME_LEN,
                    BoardAsk::CaseName { board },
                );
            }
            BoardAct::SetCase(index) => {
                if index < state.cases.len()
                    && let Some(mut b) = world.get_mut::<DetectiveBoard>(board)
                {
                    b.current = index;
                }
                refresh_look(world, board);
            }
            BoardAct::RemoveCase(case) => {
                let items = world
                    .get_mut::<DetectiveBoard>(board)
                    .and_then(|mut b| b.remove_case(case));
                for item in items.into_iter().flatten() {
                    remove_item(world, board, item, user);
                }
                refresh_look(world, board);
            }
            BoardAct::RenameCase(case) => {
                if state.cases.iter().any(|c| c.id == case) {
                    ask_text(
                        world,
                        client,
                        user,
                        "Please enter the new name for the case",
                        MAX_NAME_LEN,
                        BoardAsk::Rename { board, case },
                    );
                }
            }
            BoardAct::Look(case, evidence) => {
                let Some(e) = state
                    .cases
                    .iter()
                    .find(|c| c.id == case)
                    .and_then(|c| c.evidences.iter().find(|e| e.id == evidence))
                else {
                    continue;
                };
                if e.photo {
                    crate::service::photo_show(world, client, user, e.item);
                    continue;
                }
                let text = world
                    .get::<crate::service::Paper>(e.item)
                    .map(|p| {
                        p.inputs
                            .iter()
                            .map(|i| i.raw_text.as_str())
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                let name = world
                    .get::<DmName>(e.item)
                    .map(|n| crate::vending::plain_name(&n.name).to_owned())
                    .unwrap_or_default();
                tell(world, client, Span::Info, format!("{name}\n{text}"));
            }
            BoardAct::RemoveEvidence(evidence) => {
                // `if(!istype(item) || item.loc != src) return`.
                let inside = state
                    .cases
                    .get(state.current)
                    .and_then(|c| c.evidences.iter().find(|e| e.id == evidence))
                    .is_some_and(|e| world.get::<StoredIn>(e.item).is_some_and(|s| s.0 == board));
                if !inside {
                    continue;
                }
                let item = world
                    .get_mut::<DetectiveBoard>(board)
                    .and_then(|mut b| b.unpin(evidence));
                if let Some(item) = item {
                    remove_item(world, board, item, user);
                }
                refresh_look(world, board);
            }
            BoardAct::Connect(from, to) => {
                if let Some(mut b) = world.get_mut::<DetectiveBoard>(board) {
                    b.connect(from, to);
                }
            }
        }
    }
}

/// The prompts' answers, after their sleep: the board is still there and in reach.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answered: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, user, answer) in answered {
        let Some(ask) = world.get::<BoardAsk>(prompt).cloned() else {
            continue;
        };
        let text = match &answer {
            Some(PromptAnswer::Text(t)) => crate::records::console::sanitize(t, MAX_DESC_LEN),
            _ => String::new(),
        };
        match ask {
            BoardAsk::EvidenceName { board, item } => {
                let name = if text.is_empty() {
                    crate::examine::name_of(world, item)
                } else {
                    text.chars().take(MAX_NAME_LEN).collect()
                };
                if world.get::<DetectiveBoard>(board).is_none() {
                    continue;
                }
                ask_text(
                    world,
                    client,
                    user,
                    "Please enter the evidence description",
                    MAX_DESC_LEN,
                    BoardAsk::EvidenceDesc { board, item, name },
                );
            }
            BoardAsk::EvidenceDesc { board, item, name } => {
                let desc = if text.is_empty() {
                    world
                        .get::<DmName>(item)
                        .map(|n| n.desc.clone())
                        .unwrap_or_default()
                } else {
                    text
                };
                pin_evidence(world, client, user, board, item, name, desc);
            }
            BoardAsk::CaseName { board } => {
                if text.is_empty() || !usable(world, user, board) {
                    continue;
                }
                let name: String = text.chars().take(MAX_NAME_LEN).collect();
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        user,
                        TITLE,
                        "Please choose case color",
                        PromptKind::List {
                            items: CASE_COLORS.iter().map(|c| (*c).to_owned()).collect(),
                            default: None,
                        },
                    ),
                );
                world
                    .entity_mut(prompt)
                    .insert(BoardAsk::CaseColor { board, name });
            }
            BoardAsk::CaseColor { board, name } => {
                let Some(PromptAnswer::Item(i)) = answer else {
                    continue;
                };
                let Some(color) = CASE_COLORS.get(i) else {
                    continue;
                };
                if !usable(world, user, board) {
                    continue;
                }
                if let Some(mut b) = world.get_mut::<DetectiveBoard>(board) {
                    b.add_case(name, (*color).to_owned());
                }
                refresh_look(world, board);
            }
            BoardAsk::Rename { board, case } => {
                if text.is_empty() || !usable(world, user, board) {
                    continue;
                }
                let name: String = text.chars().take(MAX_NAME_LEN).collect();
                if let Some(mut b) = world.get_mut::<DetectiveBoard>(board)
                    && let Some(c) = b.cases.iter_mut().find(|c| c.id == case)
                {
                    c.name = name;
                }
            }
        }
    }
}

/// The user can still use the board after a prompt: it exists and is in reach.
fn usable(world: &World, user: Entity, board: Entity) -> bool {
    world.get::<DetectiveBoard>(board).is_some() && crate::storage::in_reach(world, user, board)
}

/// The end of `item_interaction`: `transferItemToLoc` from the user's hands, then the evidence.
/// `attaching_evidence` is cleared whatever happens.
fn pin_evidence(
    world: &mut World,
    client: Entity,
    user: Entity,
    board: Entity,
    item: Entity,
    name: String,
    desc: String,
) {
    let Some(mut b) = world.get_mut::<DetectiveBoard>(board) else {
        return;
    };
    b.attaching = false;
    let held = world
        .get::<Inventory>(user)
        .is_some_and(|i| i.hands.contains(&Some(item)));
    if !held || !usable(world, user, board) || no_drop(world, item) {
        return;
    }
    if world
        .get::<DetectiveBoard>(board)
        .is_none_or(|b| b.cases.get(b.current).is_none())
    {
        return;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(user) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
    put_inside(world, item, board);
    let photo = is_under(&path_of(world, item), PHOTO);
    if let Some(mut b) = world.get_mut::<DetectiveBoard>(board) {
        b.pin(item, name, desc, photo);
    }
    let what = crate::examine::name_of(world, item);
    tell(
        world,
        client,
        Span::Notice,
        format!("You pin the {what} to the detective board."),
    );
    refresh_look(world, board);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize) -> Vec<Entity> {
        let mut world = World::new();
        (0..n).map(|_| world.spawn_empty().id()).collect()
    }

    #[test]
    fn cases_are_capped_and_the_new_one_is_current() {
        let mut b = DetectiveBoard::default();
        for i in 0..MAX_CASES {
            assert!(b.add_case(format!("case {i}"), "red".into()));
        }
        assert!(!b.add_case("one too many".into(), "red".into()));
        assert_eq!(b.current, MAX_CASES - 1);
    }

    #[test]
    fn strings_go_both_ways_once_and_leave_with_the_evidence() {
        let e = items(2);
        let mut b = DetectiveBoard::default();
        assert!(!b.pin(e[0], "x".into(), "y".into(), false), "no case");
        b.add_case("a".into(), "red".into());
        b.pin(e[0], "knife".into(), "".into(), false);
        b.pin(e[1], "photo".into(), "".into(), true);
        let ids: Vec<u32> = b.cases[0].evidences.iter().map(|e| e.id).collect();
        assert!(b.connect(ids[0], ids[1]));
        assert!(b.connect(ids[1], ids[0]));
        assert!(!b.connect(ids[0], ids[0]), "no string to itself");
        assert_eq!(b.cases[0].evidences[0].connections, vec![ids[1]]);
        assert_eq!(b.cases[0].evidences[1].connections, vec![ids[0]]);
        assert_eq!(b.shown_notices(), Some(2));
        assert_eq!(b.unpin(ids[0]), Some(e[0]));
        assert!(b.cases[0].evidences[0].connections.is_empty());
        assert_eq!(b.shown_notices(), Some(1));
    }

    #[test]
    fn removing_a_case_returns_its_items() {
        let e = items(1);
        let mut b = DetectiveBoard::default();
        b.add_case("a".into(), "red".into());
        b.pin(e[0], "n".into(), "d".into(), false);
        b.add_case("b".into(), "blue".into());
        let first = b.cases[0].id;
        assert_eq!(b.remove_case(first), Some(vec![e[0]]));
        assert_eq!(b.current, 0);
        assert_eq!(b.cases[0].name, "b");
        assert_eq!(b.remove_case(999), None);
    }
}
