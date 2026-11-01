//! Disposal pipe routing, without the world: BYOND direction bits, a pipe's `dpdir` from its
//! type and `dir` (`/obj/structure/disposalpipe/Initialize`), and each pipe kind's `nextdir`
//! (`pipe.dm`, `pipe_sorting.dm`).

use ss13_mapload::fidelity::is_under;

pub const NORTH: u8 = 1;
pub const SOUTH: u8 = 2;
pub const EAST: u8 = 4;
pub const WEST: u8 = 8;
/// BYOND `UP`/`DOWN`: a holder entering a trunk from the bin above it moves `DOWN`.
pub const UP: u8 = 16;
pub const DOWN: u8 = 32;
/// `GLOB.cardinals` order.
pub const CARDINALS: [u8; 4] = [NORTH, SOUTH, EAST, WEST];

/// `DISP_DIR_LEFT`, `DISP_DIR_RIGHT`, `DISP_DIR_FLIP`, `DISP_DIR_NONE`
/// (`code/__DEFINES/pipe_construction.dm`).
const DISP_DIR_LEFT: u8 = 1;
const DISP_DIR_RIGHT: u8 = 2;
const DISP_DIR_FLIP: u8 = 4;
const DISP_DIR_NONE: u8 = 8;

/// BYOND `REVERSE_DIR`.
pub fn reverse(dir: u8) -> u8 {
    let swap = |a: u8, b: u8| {
        let (ha, hb) = (dir & a != 0, dir & b != 0);
        (if ha { b } else { 0 }) | (if hb { a } else { 0 })
    };
    swap(NORTH, SOUTH) | swap(EAST, WEST) | swap(UP, DOWN)
}

/// Clockwise from north, as BYOND numbers the eight directions for `turn`.
const CIRCLE: [u8; 8] = [
    NORTH,
    NORTH | EAST,
    EAST,
    SOUTH | EAST,
    SOUTH,
    SOUTH | WEST,
    WEST,
    NORTH | WEST,
];

/// BYOND `turn(dir, angle)`: positive angles turn counterclockwise, in steps of 45°. A value
/// that is not one of the eight directions turns into 0.
pub fn turn(dir: u8, angle: i32) -> u8 {
    let Some(i) = CIRCLE.iter().position(|&d| d == dir) else {
        return 0;
    };
    let steps = (-angle / 45).rem_euclid(8) as usize;
    CIRCLE[(i + steps) % 8]
}

pub fn is_diagonal(dir: u8) -> bool {
    dir & (NORTH | SOUTH) != 0 && dir & (EAST | WEST) != 0
}

/// BYOND `get_step` offset of a direction (conflicting bits cancel).
pub fn offset(dir: u8) -> (i32, i32) {
    let dx = i32::from(dir & EAST != 0) - i32::from(dir & WEST != 0);
    let dy = i32::from(dir & NORTH != 0) - i32::from(dir & SOUTH != 0);
    (dx, dy)
}

/// What a mail or wrap sorting junction looks at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SortFilter {
    /// `/obj/structure/disposalpipe/sorting/mail`: `destinationTag in sortTypes`.
    Mail(Vec<u8>),
    /// `/obj/structure/disposalpipe/sorting/wrap`: `tomail`.
    Wrap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipeKind {
    Segment,
    Junction,
    Sorting(SortFilter),
    /// Under a bin or outlet (`linked`), or the end of a run.
    Trunk,
    /// `dpdir` 0: holders never find it.
    Broken,
    /// `direction_angle`.
    Rotator(i32),
}

/// One pipe: its kind, `dir` and `dpdir`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipe {
    pub kind: PipeKind,
    pub dir: u8,
    pub dpdir: u8,
}

/// What `nextdir` reads from the holder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cargo {
    pub destination_tag: u8,
    pub tomail: bool,
}

impl Pipe {
    /// A pipe of DM type `path` facing `dir` (BYOND bits; 0, which BYOND does not keep on an
    /// atom, is south). `None` for types that are not pipes this port routes through.
    // shortcut: multi-z trunks (`trunk/multiz`) route like plain trunks; MetaStation has none.
    pub fn from_type(path: &str, dir: u8, sort_types: &[u8]) -> Option<Self> {
        const ROOT: &str = "/obj/structure/disposalpipe";
        if !is_under(path, ROOT) {
            return None;
        }
        let dir = if dir == 0 { SOUTH } else { dir };
        let sub = |s: &str| is_under(path, &format!("{ROOT}/{s}"));
        let (kind, init) = if sub("segment") {
            (PipeKind::Segment, DISP_DIR_FLIP)
        } else if sub("junction/yjunction") {
            (PipeKind::Junction, DISP_DIR_LEFT | DISP_DIR_RIGHT)
        } else if sub("junction/flip") {
            (PipeKind::Junction, DISP_DIR_LEFT | DISP_DIR_FLIP)
        } else if sub("junction") {
            (PipeKind::Junction, DISP_DIR_RIGHT | DISP_DIR_FLIP)
        } else if sub("sorting/mail/flip") {
            (
                PipeKind::Sorting(SortFilter::Mail(sort_types.to_vec())),
                DISP_DIR_LEFT | DISP_DIR_FLIP,
            )
        } else if sub("sorting/mail") {
            (
                PipeKind::Sorting(SortFilter::Mail(sort_types.to_vec())),
                DISP_DIR_RIGHT | DISP_DIR_FLIP,
            )
        } else if sub("sorting/wrap/flip") {
            (
                PipeKind::Sorting(SortFilter::Wrap),
                DISP_DIR_LEFT | DISP_DIR_FLIP,
            )
        } else if sub("sorting/wrap") {
            (
                PipeKind::Sorting(SortFilter::Wrap),
                DISP_DIR_RIGHT | DISP_DIR_FLIP,
            )
        } else if sub("trunk") {
            // `/obj/structure/disposalpipe::initialize_dirs` is `NONE`, so only `dir` is set.
            (PipeKind::Trunk, 0)
        } else if sub("broken") {
            (PipeKind::Broken, DISP_DIR_NONE)
        } else if sub("rotator/flip") {
            (
                PipeKind::Rotator(90),
                DISP_DIR_LEFT | DISP_DIR_RIGHT | DISP_DIR_FLIP,
            )
        } else if sub("rotator") {
            (
                PipeKind::Rotator(-90),
                DISP_DIR_LEFT | DISP_DIR_RIGHT | DISP_DIR_FLIP,
            )
        } else {
            return None;
        };
        Some(Self {
            dpdir: dpdir(dir, init),
            kind,
            dir,
        })
    }

    /// The direction a holder moving `holder_dir` leaves this pipe by (`nextdir`); `coin` is
    /// one `prob(50)`.
    pub fn next_dir(&self, holder_dir: u8, cargo: Cargo, coin: impl FnOnce() -> bool) -> u8 {
        match &self.kind {
            PipeKind::Segment | PipeKind::Broken => self.dpdir & !reverse(holder_dir),
            PipeKind::Junction => {
                // From a secondary side: out through the primary one.
                if reverse(holder_dir) != self.dir {
                    return self.dir;
                }
                let mask = self.dpdir & !self.dir;
                let secdir = CARDINALS.into_iter().find(|&d| d & mask != 0).unwrap_or(0);
                if coin() { secdir } else { mask & !secdir }
            }
            PipeKind::Sorting(filter) => {
                let sortdir = self.dpdir & !(self.dir | reverse(self.dir));
                // "probably came from the negdir"
                if holder_dir != sortdir {
                    let sorted = match filter {
                        SortFilter::Mail(tags) => tags.contains(&cargo.destination_tag),
                        SortFilter::Wrap => cargo.tomail,
                    };
                    if sorted {
                        return sortdir;
                    }
                }
                self.dir
            }
            PipeKind::Trunk => {
                if holder_dir == DOWN {
                    self.dir
                } else {
                    0
                }
            }
            PipeKind::Rotator(angle) => turn(holder_dir, *angle),
        }
    }
}

/// `dpdir` from `Initialize`: bent pipes (a diagonal `dir`) have both their dirs already.
fn dpdir(dir: u8, init: u8) -> u8 {
    let init = if is_diagonal(dir) { 0 } else { init };
    if init == DISP_DIR_NONE {
        return 0;
    }
    let mut dpdir = dir;
    if init & DISP_DIR_LEFT != 0 {
        dpdir |= turn(dir, 90);
    }
    if init & DISP_DIR_RIGHT != 0 {
        dpdir |= turn(dir, -90);
    }
    if init & DISP_DIR_FLIP != 0 {
        dpdir |= reverse(dir);
    }
    dpdir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipe(path: &str, dir: u8) -> Pipe {
        Pipe::from_type(&format!("/obj/structure/disposalpipe/{path}"), dir, &[9]).unwrap()
    }

    #[test]
    fn byond_turn_and_reverse() {
        assert_eq!(turn(NORTH, 90), WEST);
        assert_eq!(turn(NORTH, -90), EAST);
        assert_eq!(turn(EAST, 180), WEST);
        assert_eq!(turn(SOUTH | EAST, 45), EAST);
        assert_eq!(turn(NORTH | SOUTH, 90), 0);
        assert_eq!(reverse(NORTH | EAST), SOUTH | WEST);
        assert_eq!(reverse(DOWN), UP);
    }

    #[test]
    fn dpdir_follows_initialize_dirs() {
        assert_eq!(pipe("segment", NORTH).dpdir, NORTH | SOUTH);
        assert_eq!(
            pipe("segment", 0).dpdir,
            NORTH | SOUTH,
            "unset dir is south"
        );
        assert_eq!(pipe("segment", NORTH | EAST).dpdir, NORTH | EAST, "bent");
        // Junction: dir, right (clockwise) and flip.
        assert_eq!(pipe("junction", NORTH).dpdir, NORTH | EAST | SOUTH);
        assert_eq!(pipe("junction/flip", NORTH).dpdir, NORTH | WEST | SOUTH);
        assert_eq!(pipe("junction/yjunction", NORTH).dpdir, NORTH | WEST | EAST);
        assert_eq!(pipe("sorting/mail", EAST).dpdir, EAST | SOUTH | WEST);
        assert_eq!(pipe("sorting/mail/flip", EAST).dpdir, EAST | NORTH | WEST);
        assert_eq!(pipe("trunk", WEST).dpdir, WEST);
        assert_eq!(pipe("broken", WEST).dpdir, 0);
        assert!(Pipe::from_type("/obj/structure/disposaloutlet", 2, &[]).is_none());
    }

    #[test]
    fn segments_pass_through_and_bend() {
        let c = Cargo::default();
        assert_eq!(pipe("segment", NORTH).next_dir(NORTH, c, || true), NORTH);
        assert_eq!(pipe("segment", NORTH).next_dir(SOUTH, c, || true), SOUTH);
        // A bend from the south side up and to the east: entering moving north leaves east.
        let bend = pipe("segment", SOUTH | EAST);
        assert_eq!(bend.next_dir(NORTH, c, || true), EAST);
        assert_eq!(bend.next_dir(WEST, c, || true), SOUTH);
    }

    #[test]
    fn junctions_merge_to_primary_and_split_at_random() {
        let c = Cargo::default();
        let j = pipe("junction", NORTH); // N primary, E secondary, S flip
        // From the east side (moving west) or from the north side (moving south): out north
        // unless it came from the primary side.
        assert_eq!(j.next_dir(WEST, c, || true), NORTH);
        // Moving south = entered from the north (primary) side: south or east at random.
        assert_eq!(j.next_dir(SOUTH, c, || true), SOUTH, "first of N,S,E,W");
        assert_eq!(j.next_dir(SOUTH, c, || false), EAST);
    }

    #[test]
    fn sorting_junctions_divert_matching_tags() {
        let s = pipe("sorting/mail", EAST); // dir E, sortdir S, negdir W
        let medbay = Cargo {
            destination_tag: 9,
            tomail: true,
        };
        let cargo_bay = Cargo {
            destination_tag: 2,
            tomail: true,
        };
        // Coming in moving east (from the west, the negdir): tag 9 goes south.
        assert_eq!(s.next_dir(EAST, medbay, || true), SOUTH);
        assert_eq!(s.next_dir(EAST, cargo_bay, || true), EAST);
        // Coming up from the sort side (moving north): on east, unless the tag matches, which
        // tg sends back down the sort side (`H.dir != sortdir` holds for it too).
        assert_eq!(s.next_dir(NORTH, cargo_bay, || true), EAST);
        assert_eq!(s.next_dir(NORTH, medbay, || true), SOUTH);
        let wrap = Pipe::from_type("/obj/structure/disposalpipe/sorting/wrap", EAST, &[]).unwrap();
        assert_eq!(wrap.next_dir(EAST, medbay, || true), SOUTH);
        assert_eq!(wrap.next_dir(EAST, Cargo::default(), || true), EAST);
    }

    #[test]
    fn trunks_and_rotators() {
        let c = Cargo::default();
        let t = pipe("trunk", WEST);
        assert_eq!(t.next_dir(DOWN, c, || true), WEST, "from the bin above");
        assert_eq!(
            t.next_dir(EAST, c, || true),
            0,
            "arrived: the linked machine takes it"
        );
        assert_eq!(pipe("rotator", NORTH).next_dir(NORTH, c, || true), EAST);
        assert_eq!(
            pipe("rotator/flip", NORTH).next_dir(NORTH, c, || true),
            WEST
        );
    }
}
