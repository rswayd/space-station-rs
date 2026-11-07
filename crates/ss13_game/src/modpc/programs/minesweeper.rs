//! Minesweeper (server), after BandaStation `/datum/computer_file/program/minesweeper`
//! (`modular_bandastation/minesweeper`): a field of hidden bombs on a PDA. The first click is
//! always safe (no bomb in the 3x3 around it); a click opens a cell (empty areas flood open),
//! a flag marks one. Every bomb flagged, no false flag and every other cell open wins: a ping,
//! a nickname prompt for the round's leaderboard and a fresh field 5 s later. A bomb loses: all
//! bombs shown, an explosion sound (a real explosion on an emagged PDA) and a fresh field 3 s
//! later. Before the first click the field's size and bomb count can be changed (9-25 a side,
//! 10 bombs up to a fifth of the field, at most 100).
//!
//! The program is installed by an emag (`/obj/item/modular_computer/pda/emag_act`); the port
//! has no software downloader.
// shortcut: no global leaderboard (BandaStation reads and writes the `minesweeper` SQL table;
// the port has no database), the round's leaderboard is shown sorted by time instead of the
// sortable table, and the window keeps one size (tg sizes it by the field).
// Deviations from the DM code (bugs there, not the intended game): the client never sees the
// bombs or numbers of closed cells (DM sends the whole matrix), the flood fill's visited set
// keys cells by both coordinates (DM's `"[x][y]"` mixes up 1,11 and 11,1), and 3BV is the
// standard count (each opening once, numbered cells only off an opening; DM also counts the
// numbers around openings and floods through bombs without neighbours).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{UiParams, UiReject};

use super::ProgramKind;
use crate::modpc::pda::ModularComputer;
use crate::modpc::{
    MINESWEEPER_CHANGE_SIZE, MINESWEEPER_SQUARE, MineCellUi, MinesweeperData, MinesweeperResult,
};

/// `DEFAULT_FIELD_HEIGHT`, `DEFAULT_FIELD_WIDTH`, `DEFAULT_BOMBS_AMOUNT`.
pub const DEFAULT_SIDE: u8 = 16;
pub const DEFAULT_BOMBS: u32 = 40;
/// `MIN_FIELD_SIDE_SIZE`, `MAX_FIELD_SIDE_SIZE`.
pub const MIN_SIDE: u8 = 9;
pub const MAX_SIDE: u8 = 25;
/// `MIN_BOMBS_AMOUNT`, `MAX_BOMBS_AMOUNT`, `FIELD_AREA_TO_BOMBS_MIN_RATIO`, `STARTING_AREA_SIZE`.
const MIN_BOMBS: u32 = 10;
const MAX_BOMBS: u32 = 100;
const AREA_PER_BOMB: u32 = 5;
const STARTING_AREA: u32 = 9;
/// The `addtimer(make_empty_matrix)` after a win and after a loss.
const WIN_RESET_SECS: f64 = 5.0;
const LOSS_RESET_SECS: f64 = 3.0;
/// The nickname prompt's `max_length`.
const NICKNAME_LEN: u32 = 10;
const TITLE: &str = "Настройки Сапёра";
const PING: &str = "sound/machines/ping.ogg";
const EXPLOSION: &str = "sound/effects/explosion/explosion1.ogg";

/// The 8 neighbours (`directions`).
const DIRECTIONS: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cell {
    open: bool,
    bomb: bool,
    flag: bool,
    around: u8,
    /// The bomb that went off.
    last: bool,
}

/// A click's `mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Open,
    Flag,
}

/// What a click did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Nothing (a flag before the first click or on an open cell).
    Ignored,
    Changed,
    Lost,
    Won,
}

/// The program's state, on the computer.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Minesweeper {
    /// `field_height` rows of `field_width` cells.
    pub height: u8,
    pub width: u8,
    pub bombs: u32,
    cells: Vec<Cell>,
    pub first_touch: bool,
    set_flags: u32,
    flagged_bombs: u32,
    opened_cells: u32,
    /// `current_3BV`.
    pub bv3: u32,
    /// `start_time` (`Time::elapsed_secs_f64`).
    pub start_time: f64,
    /// `ignore_touches`: the field is over and comes back empty at this time.
    pub reset_at: Option<f64>,
}

impl Default for Minesweeper {
    fn default() -> Self {
        Self::new(DEFAULT_SIDE, DEFAULT_SIDE, DEFAULT_BOMBS)
    }
}

impl Minesweeper {
    /// An empty field (`make_empty_matrix`).
    pub fn new(height: u8, width: u8, bombs: u32) -> Self {
        Self {
            height,
            width,
            bombs,
            cells: vec![Cell::default(); usize::from(height) * usize::from(width)],
            first_touch: true,
            set_flags: 0,
            flagged_bombs: 0,
            opened_cells: 0,
            bv3: 0,
            start_time: 0.0,
            reset_at: None,
        }
    }

    /// `make_empty_matrix`.
    pub fn reset(&mut self) {
        *self = Self::new(self.height, self.width, self.bombs);
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < i32::from(self.height) && y < i32::from(self.width))
            .then(|| x as usize * usize::from(self.width) + y as usize)
    }

    fn neighbours(&self, x: i32, y: i32) -> impl Iterator<Item = (i32, i32, usize)> + '_ {
        DIRECTIONS.iter().filter_map(move |&(dx, dy)| {
            let (nx, ny) = (x + dx, y + dy);
            self.index(nx, ny).map(|i| (nx, ny, i))
        })
    }

    /// `generate_field`: the bombs anywhere but the 3x3 around the first click. `pick(n)` is a
    /// uniform index below `n` (`pick_n_take`).
    fn generate(&mut self, sx: i32, sy: i32, now: f64, mut pick: impl FnMut(usize) -> usize) {
        self.set_flags = 0;
        self.flagged_bombs = 0;
        self.opened_cells = 0;
        let mut free: Vec<(i32, i32)> = (0..i32::from(self.height))
            .flat_map(|x| (0..i32::from(self.width)).map(move |y| (x, y)))
            .filter(|&(x, y)| (x - sx).abs() > 1 || (y - sy).abs() > 1)
            .collect();
        for _ in 0..self.bombs {
            if free.is_empty() {
                break;
            }
            let (x, y) = free.swap_remove(pick(free.len()) % free.len());
            let i = self.index(x, y).expect("in bounds");
            self.cells[i].bomb = true;
            let around: Vec<usize> = self.neighbours(x, y).map(|(_, _, i)| i).collect();
            for n in around {
                self.cells[n].around += 1;
            }
        }
        self.first_touch = false;
        self.bv3 = self.count_3bv();
        self.start_time = now;
    }

    /// `open_cell`: true when the cell was opened and has no bomb around (the flood goes on).
    fn open_cell(&mut self, i: usize) -> bool {
        let cell = &mut self.cells[i];
        if cell.open {
            return false;
        }
        cell.open = true;
        self.opened_cells += 1;
        if cell.flag {
            cell.flag = false;
            self.set_flags -= 1;
            if cell.bomb {
                self.flagged_bombs -= 1;
            }
        }
        cell.around == 0
    }

    /// `update_zeros`: the cell, and the flood of empty cells around it.
    fn update_zeros(&mut self, x: i32, y: i32) {
        let mut visited = vec![false; self.cells.len()];
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            let i = self.index(cx, cy).expect("in bounds");
            if !self.open_cell(i) {
                continue;
            }
            let next: Vec<(i32, i32, usize)> = self.neighbours(cx, cy).collect();
            for (nx, ny, n) in next {
                if !visited[n] {
                    visited[n] = true;
                    stack.push((nx, ny));
                }
            }
        }
    }

    /// 3BV: the clicks the field needs, one per opening and one per numbered cell no opening
    /// reaches.
    fn count_3bv(&self) -> u32 {
        let mut marked = vec![false; self.cells.len()];
        let mut bv = 0;
        // The openings and the cells they open.
        for start in 0..self.cells.len() {
            let c = self.cells[start];
            if marked[start] || c.bomb || c.around > 0 {
                continue;
            }
            bv += 1;
            marked[start] = true;
            let mut stack = vec![start];
            while let Some(i) = stack.pop() {
                let (x, y) = (
                    (i / usize::from(self.width)) as i32,
                    (i % usize::from(self.width)) as i32,
                );
                for (_, _, n) in self.neighbours(x, y) {
                    if marked[n] || self.cells[n].bomb {
                        continue;
                    }
                    marked[n] = true;
                    if self.cells[n].around == 0 {
                        stack.push(n);
                    }
                }
            }
        }
        bv + self
            .cells
            .iter()
            .zip(&marked)
            .filter(|(c, m)| !**m && !c.bomb)
            .count() as u32
    }

    fn won(&self) -> bool {
        self.flagged_bombs == self.bombs
            && self.set_flags == self.bombs
            && self.opened_cells == self.cells.len() as u32 - self.bombs
    }

    /// `handle_square_click` at row `x`, column `y` (0-based); `None` off the field.
    pub fn click(
        &mut self,
        x: i32,
        y: i32,
        mode: Mode,
        now: f64,
        pick: impl FnMut(usize) -> usize,
    ) -> Option<Click> {
        let i = self.index(x, y)?;
        match mode {
            Mode::Open => {
                if self.first_touch {
                    self.generate(x, y, now, pick);
                }
                if self.cells[i].bomb {
                    // `on_loose`: the bomb marked, every bomb shown.
                    self.cells[i].last = true;
                    for c in self.cells.iter_mut().filter(|c| c.bomb) {
                        c.open = true;
                    }
                    return Some(Click::Lost);
                }
                self.update_zeros(x, y);
            }
            Mode::Flag => {
                let cell = &mut self.cells[i];
                if self.first_touch || cell.open {
                    return Some(Click::Ignored);
                }
                cell.flag = !cell.flag;
                if cell.flag {
                    self.set_flags += 1;
                    self.flagged_bombs += u32::from(cell.bomb);
                } else {
                    self.set_flags -= 1;
                    self.flagged_bombs -= u32::from(cell.bomb);
                }
            }
        }
        Some(if self.won() {
            Click::Won
        } else {
            Click::Changed
        })
    }

    /// `ui_data`: closed cells show only their flag.
    pub fn ui_data(&self, leaderboard: &[MinesweeperResult]) -> MinesweeperData {
        let mut board = leaderboard.to_vec();
        board.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
        MinesweeperData {
            width: self.width,
            height: self.height,
            cells: self
                .cells
                .iter()
                .map(|c| MineCellUi {
                    open: c.open,
                    bomb: c.open && c.bomb,
                    flag: !c.open && c.flag,
                    around: if c.open { c.around } else { 0 },
                    final_bomb: c.last,
                })
                .collect(),
            flags: self.set_flags,
            bombs: self.bombs,
            first_touch: self.first_touch,
            leaderboard: board,
        }
    }

    /// The result of a game won at `now` (`add_into_leaders` without the name).
    fn result(&self, now: f64) -> MinesweeperResult {
        let seconds = ((now - self.start_time) * 10.0).round() / 10.0;
        let per_sec = if seconds > 0.0 {
            (f64::from(self.bv3) / seconds * 10.0).round() / 10.0
        } else {
            0.0
        };
        MinesweeperResult {
            name: String::new(),
            seconds,
            points: self.bv3,
            points_per_sec: per_sec,
            field_params: format!("{}X{}({})", self.width, self.height, self.bombs),
        }
    }
}

/// `max_bombs_amount` of a field.
pub fn max_bombs(width: u8, height: u8) -> u32 {
    (u32::from(width) * u32::from(height) / AREA_PER_BOMB).clamp(MIN_BOMBS, MAX_BOMBS)
}

/// The round's leaderboard (`leaderboard`, static to the program type).
#[derive(Resource, Clone, Debug, Default)]
pub struct MinesweeperLeaderboard(pub Vec<MinesweeperResult>);

/// The program's `ui_data` for a computer's state (an empty default field before any).
pub fn ui_data(
    state: Option<&Minesweeper>,
    leaderboard: Option<&MinesweeperLeaderboard>,
) -> MinesweeperData {
    let board = leaderboard.map_or(&[][..], |l| &l.0[..]);
    match state {
        Some(s) => s.ui_data(board),
        None => Minesweeper::default().ui_data(board),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MinesweeperAct {
    /// `Square`: row `X`, column `Y` (0-based), `mode`.
    Square(i32, i32, Mode),
    /// `ChangeSize`.
    ChangeSize,
}

/// `None` for an action that is not the minesweeper's.
pub fn parse(action: &str, params: &UiParams) -> Option<Result<MinesweeperAct, UiReject>> {
    Some(match action {
        MINESWEEPER_SQUARE => parse_square(params),
        MINESWEEPER_CHANGE_SIZE => Ok(MinesweeperAct::ChangeSize),
        _ => return None,
    })
}

fn parse_square(params: &UiParams) -> Result<MinesweeperAct, UiReject> {
    let coord = |key: &str| -> Result<i32, UiReject> {
        let v = params.number(key)?;
        if v.fract() != 0.0 || !(0.0..f64::from(MAX_SIDE)).contains(&v) {
            return Err(UiReject::BadParams(format!("{key}: not a cell")));
        }
        Ok(v as i32)
    };
    let mode = match params.get("mode") {
        Some("bomb") => Mode::Open,
        Some("flag") => Mode::Flag,
        _ => return Err(UiReject::BadParams("mode: bomb or flag".into())),
    };
    Ok(MinesweeperAct::Square(coord("X")?, coord("Y")?, mode))
}

/// The open question of a minesweeper, on its prompt entity.
#[derive(Component, Clone, Debug, PartialEq)]
enum Asked {
    Confirm(Entity),
    Width(Entity),
    Height(Entity, u8),
    Bombs(Entity, u8, u8),
    /// The won game's result, waiting for its name.
    Nickname(MinesweeperResult),
}

use crate::tools::now;

fn sound(world: &mut World, pc: Entity, file: &str, vol: f32) {
    if let Some(pos) = crate::modpc::pda::turf_of(world, pc) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, vol, true));
    }
}

/// `ui_act` of the minesweeper in front of `computer`.
pub fn apply(
    world: &mut World,
    client: Entity,
    actor: Entity,
    pc: Entity,
    computer: &ModularComputer,
    act: MinesweeperAct,
) -> Result<(), &'static str> {
    if computer.active != Some(ProgramKind::Minesweeper) {
        return Err("minesweeper not in front");
    }
    let mut state = world.get::<Minesweeper>(pc).cloned().unwrap_or_default();
    if state.reset_at.is_some() {
        return Err("ignoring touches");
    }
    match act {
        MinesweeperAct::Square(x, y, mode) => {
            let t = now(world);
            let mut rng = world.get_resource_or_insert_with(crate::combat::CombatRng::default);
            let click = state.click(x, y, mode, t, |n| rng.range(0, n as u32 - 1) as usize);
            match click {
                None => return Err("off the field"),
                Some(Click::Ignored) => return Ok(()),
                Some(Click::Changed) => {}
                Some(Click::Won) => {
                    state.reset_at = Some(t + WIN_RESET_SECS);
                    let result = state.result(t);
                    sound(world, pc, PING, 20.0);
                    let ask = Ask::new(
                        client,
                        actor,
                        "Сапёр",
                        format!(
                            "Вы справились за {} секунд!\nНапишите ваш никнейм, чтобы сохранить \
                             результат в рейтинговой таблице.",
                            result.seconds
                        ),
                        PromptKind::Text {
                            default: String::new(),
                            max_length: NICKNAME_LEN,
                            multiline: false,
                        },
                    );
                    let prompt = ask_world(world, ask);
                    world.entity_mut(prompt).insert(Asked::Nickname(result));
                }
                Some(Click::Lost) => {
                    state.reset_at = Some(t + LOSS_RESET_SECS);
                    sound(world, pc, EXPLOSION, 50.0);
                    if computer.emagged
                        && let Some(pos) = crate::modpc::pda::turf_of(world, pc)
                    {
                        world.entity_mut(pc).insert(state);
                        crate::explosions::explosion(
                            world,
                            pos,
                            crate::explosions::Explosion::new(-1, 1, 3)
                                .flame(2)
                                .cause("emagged minesweeper"),
                        );
                        return Ok(());
                    }
                }
            }
            if let Ok(mut e) = world.get_entity_mut(pc) {
                e.insert(state);
            }
            Ok(())
        }
        MinesweeperAct::ChangeSize => {
            if !can_change(world, actor, pc) {
                return Err("cannot change the field now");
            }
            let ask = Ask::new(
                client,
                actor,
                TITLE,
                "Вы хотите изменить параметры поля?",
                PromptKind::Alert {
                    buttons: vec!["Да".into(), "Нет".into()],
                    autofocus: true,
                },
            );
            let prompt = ask_world(world, ask);
            world.entity_mut(prompt).insert(Asked::Confirm(pc));
            Ok(())
        }
    }
}

/// `change_field_params`'s checks, again after every answer: the PDA in the user's hands, on,
/// the minesweeper in front, no game started.
fn can_change(world: &World, actor: Entity, pc: Entity) -> bool {
    crate::modpc::pda::holder_mob(world, pc) == Some(actor)
        && world
            .get::<ModularComputer>(pc)
            .is_some_and(|c| c.enabled && c.active == Some(ProgramKind::Minesweeper))
        && world
            .get::<Minesweeper>(pc)
            .is_none_or(|s| s.first_touch && s.reset_at.is_none())
}

fn ask_number(
    world: &mut World,
    client: Entity,
    actor: Entity,
    message: &str,
    (default, min, max): (u32, u32, u32),
    asked: Asked,
) {
    let ask = Ask::new(
        client,
        actor,
        TITLE,
        message,
        PromptKind::Number {
            default: f64::from(default),
            min: f64::from(min),
            max: f64::from(max),
            round: true,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(asked);
}

/// The answers to the field and nickname prompts.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(asked) = world.get::<Asked>(prompt).cloned() else {
            continue;
        };
        let number = |a: &Option<PromptAnswer>, min: u32, max: u32| match a {
            Some(PromptAnswer::Number(n)) => Some((n.max(0.0).round() as u32).clamp(min, max)),
            _ => None,
        };
        match asked {
            Asked::Nickname(mut result) => {
                let Some(PromptAnswer::Text(name)) = answer else {
                    continue;
                };
                result.name = name.trim().chars().take(NICKNAME_LEN as usize).collect();
                if result.name.is_empty() {
                    continue;
                }
                world
                    .get_resource_or_insert_with(MinesweeperLeaderboard::default)
                    .0
                    .push(result);
            }
            Asked::Confirm(pc) => {
                if answer == Some(PromptAnswer::Button(0)) && can_change(world, actor, pc) {
                    let width = world
                        .get::<Minesweeper>(pc)
                        .map_or(DEFAULT_SIDE, |s| s.width);
                    let range = (u32::from(width), u32::from(MIN_SIDE), u32::from(MAX_SIDE));
                    ask_number(
                        world,
                        client,
                        actor,
                        "Выставите ширину",
                        range,
                        Asked::Width(pc),
                    );
                }
            }
            Asked::Width(pc) => {
                if let Some(width) = number(&answer, u32::from(MIN_SIDE), u32::from(MAX_SIDE))
                    && can_change(world, actor, pc)
                {
                    let height = world
                        .get::<Minesweeper>(pc)
                        .map_or(DEFAULT_SIDE, |s| s.height);
                    let range = (u32::from(height), u32::from(MIN_SIDE), u32::from(MAX_SIDE));
                    let asked = Asked::Height(pc, width as u8);
                    ask_number(world, client, actor, "Выставите длину", range, asked);
                }
            }
            Asked::Height(pc, width) => {
                if let Some(height) = number(&answer, u32::from(MIN_SIDE), u32::from(MAX_SIDE))
                    && can_change(world, actor, pc)
                {
                    let height = height as u8;
                    let max = max_bombs(width, height);
                    // Never true for a 9x9 field or bigger; kept from `change_field_params`.
                    if u32::from(width) * u32::from(height) - STARTING_AREA < MIN_BOMBS {
                        crate::tools::tell(
                            world,
                            client,
                            ss13_shared::chat::Span::Warning,
                            "Поле слишком маленькое!",
                        );
                        continue;
                    }
                    let bombs = world
                        .get::<Minesweeper>(pc)
                        .map_or(DEFAULT_BOMBS, |s| s.bombs);
                    let range = (bombs.min(max), MIN_BOMBS, max);
                    let asked = Asked::Bombs(pc, width, height);
                    ask_number(world, client, actor, "Выставите кол-во бомб", range, asked);
                }
            }
            Asked::Bombs(pc, width, height) => {
                if let Some(bombs) = number(&answer, MIN_BOMBS, max_bombs(width, height))
                    && can_change(world, actor, pc)
                {
                    world
                        .entity_mut(pc)
                        .insert(Minesweeper::new(height, width, bombs));
                }
            }
        }
    }
}

/// The `addtimer(make_empty_matrix)` of finished fields.
fn reset_fields(time: Res<Time>, mut fields: Query<&mut Minesweeper>) {
    let t = time.elapsed_secs_f64();
    for mut field in &mut fields {
        if field.reset_at.is_some_and(|at| t >= at) {
            field.reset();
        }
    }
}

/// `store_file(new /datum/computer_file/program/minesweeper)`: once per computer.
pub fn install(world: &mut World, pc: Entity) {
    let Some(mut computer) = world.get_mut::<ModularComputer>(pc) else {
        return;
    };
    if !computer.programs.contains(&ProgramKind::Minesweeper) {
        computer.programs.push(ProgramKind::Minesweeper);
    }
    if world.get::<Minesweeper>(pc).is_none() {
        world.entity_mut(pc).insert(Minesweeper::default());
    }
}

pub fn build(app: &mut App) {
    app.init_resource::<MinesweeperLeaderboard>().add_systems(
        Update,
        (
            answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
            reset_fields.run_if(any_with_component::<Minesweeper>),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Always the first free cell: bombs fill the field in row order.
    fn first(_: usize) -> usize {
        0
    }

    fn field(height: u8, width: u8, bombs: u32) -> Minesweeper {
        Minesweeper::new(height, width, bombs)
    }

    #[test]
    fn the_first_click_is_safe_and_floods_open() {
        let mut f = field(9, 9, 10);
        // `swap_remove(0)` takes the first free cell and moves the last one in: never the 3x3.
        assert_eq!(f.click(4, 4, Mode::Open, 0.0, first), Some(Click::Changed));
        assert!(!f.first_touch);
        assert_eq!(f.cells.iter().filter(|c| c.bomb).count(), 10);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let i = f.index(4 + dx, 4 + dy).unwrap();
                assert!(!f.cells[i].bomb && f.cells[i].open);
            }
        }
        assert_eq!(
            f.opened_cells as usize,
            f.cells.iter().filter(|c| c.open).count()
        );
        // The client sees no bomb under a closed cell.
        let ui = f.ui_data(&[]);
        assert!(
            ui.cells
                .iter()
                .all(|c| !c.bomb && (c.open || c.around == 0))
        );
        assert!(f.bv3 >= 1);
    }

    #[test]
    fn flags_and_opening_everything_wins() {
        let mut f = field(9, 9, 10);
        assert_eq!(f.click(0, 0, Mode::Flag, 0.0, first), Some(Click::Ignored));
        f.click(8, 8, Mode::Open, 0.0, first).unwrap();
        let bombs: Vec<(i32, i32)> = (0..9)
            .flat_map(|x| (0..9).map(move |y| (x, y)))
            .filter(|&(x, y)| f.cells[f.index(x, y).unwrap()].bomb)
            .collect();
        let safe: Vec<(i32, i32)> = (0..9)
            .flat_map(|x| (0..9).map(move |y| (x, y)))
            .filter(|&(x, y)| !f.cells[f.index(x, y).unwrap()].bomb)
            .collect();
        for &(x, y) in &safe {
            assert_ne!(f.click(x, y, Mode::Open, 1.0, first), Some(Click::Lost));
        }
        let (last, rest) = bombs.split_last().unwrap();
        for &(x, y) in rest {
            assert_eq!(f.click(x, y, Mode::Flag, 2.0, first), Some(Click::Changed));
        }
        // A flag taken back and put again.
        f.click(last.0, last.1, Mode::Flag, 2.0, first);
        f.click(last.0, last.1, Mode::Flag, 2.0, first);
        assert_eq!(
            f.click(last.0, last.1, Mode::Flag, 2.0, first),
            Some(Click::Won)
        );
        let r = f.result(12.0);
        assert_eq!(r.seconds, 12.0);
        assert_eq!(r.field_params, "9X9(10)");
    }

    #[test]
    fn a_bomb_loses_and_shows_every_bomb() {
        let mut f = field(9, 9, 10);
        f.click(8, 8, Mode::Open, 0.0, first).unwrap();
        let bomb = f.cells.iter().position(|c| c.bomb).unwrap();
        let (x, y) = ((bomb / 9) as i32, (bomb % 9) as i32);
        assert_eq!(f.click(x, y, Mode::Open, 1.0, first), Some(Click::Lost));
        let ui = f.ui_data(&[]);
        assert!(ui.cells[bomb].final_bomb);
        assert_eq!(ui.cells.iter().filter(|c| c.bomb).count(), 10);
        assert_eq!(f.click(9, 0, Mode::Open, 1.0, first), None);
        f.reset();
        assert!(f.first_touch && f.cells.iter().all(|c| *c == Cell::default()));
    }

    #[test]
    fn three_bv_counts_openings_once() {
        // One bomb in a corner of a 9x9: the rest is one opening plus its 3 border numbers,
        // all reached by the opening.
        let mut f = field(9, 9, 1);
        f.generate(8, 8, 0.0, first);
        assert!(f.cells[0].bomb);
        assert_eq!(f.bv3, 1);
        assert_eq!(max_bombs(9, 9), 16);
        assert_eq!(max_bombs(25, 25), 100);
    }

    #[test]
    fn acts_parse_hostile_params() {
        let parse_with = |action: &str, pairs: &[(&str, &str)]| {
            let params: Vec<(String, String)> = pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect();
            parse(action, &UiParams::new(&params))
        };
        assert_eq!(
            parse_with(
                MINESWEEPER_SQUARE,
                &[("X", "3"), ("Y", "0"), ("mode", "flag")]
            ),
            Some(Ok(MinesweeperAct::Square(3, 0, Mode::Flag)))
        );
        for bad in ["-1", "25", "1.5", "NaN", "1e9"] {
            assert!(matches!(
                parse_with(
                    MINESWEEPER_SQUARE,
                    &[("X", bad), ("Y", "0"), ("mode", "bomb")]
                ),
                Some(Err(_))
            ));
        }
        assert!(matches!(
            parse_with(
                MINESWEEPER_SQUARE,
                &[("X", "1"), ("Y", "1"), ("mode", "nuke")]
            ),
            Some(Err(_))
        ));
        assert_eq!(
            parse_with(MINESWEEPER_CHANGE_SIZE, &[]),
            Some(Ok(MinesweeperAct::ChangeSize))
        );
        assert_eq!(parse_with("PC_exit", &[]), None);
    }
}
