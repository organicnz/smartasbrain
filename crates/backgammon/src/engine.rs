//! Pure backgammon rules engine: no ratatui/crossterm, only `rand` for dice.
//!
//! Model: points indexed 0..23; WHITE travels 0 -> 23 (home 18..=23, bears off
//! past 23), BLACK travels 23 -> 0 (home 0..=5, bears off past 0). The board is
//! an `i8` per point (positive = white count, negative = black count) plus bar
//! and borne-off counters.
//!
//! Documented simplifications:
//! - the must-play-max-die rule is not enforced;
//! - "no moves at all" detection right after a roll approximates forced-line
//!   analysis with a union of single legal steps across the distinct dice;
//! - the same union check runs after each play, so a roll whose leftover dice
//!   (partial doubles) admit no step passes the turn instead of deadlocking.

use rand::Rng;

/// Side marker for a checker on its way back in from the bar.
pub const BAR: usize = 24;

/// Pseudo-slot for a borne-off destination.
pub const OFF: usize = 25;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Roll,
    Move,
    GameOver,
}

/// Where a move lands: an interior point or off the board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dest {
    Point(usize),
    Off,
}

const START: [i8; 24] = {
    let mut p = [0i8; 24];
    p[0] = 2;
    p[5] = -5;
    p[7] = -3;
    p[11] = 5;
    p[12] = -5;
    p[16] = 3;
    p[18] = 5;
    p[23] = -2;
    p
};

#[derive(Clone)]
pub struct Backgammon {
    points: [i8; 24],
    /// (white, black) checkers waiting to re-enter from the bar.
    bar: (u8, u8),
    /// (white, black) checkers borne off.
    off: (u8, u8),
    turn: Side,
    phase: Phase,
    dice: [u8; 2],
    /// Remaining die values to play this turn (four entries on doubles).
    left: Vec<u8>,
}

impl Default for Backgammon {
    fn default() -> Self {
        Self::new()
    }
}

impl Backgammon {
    pub fn new() -> Self {
        Self {
            points: START,
            bar: (0, 0),
            off: (0, 0),
            turn: Side::Black,
            phase: Phase::Roll,
            dice: [1, 1],
            left: Vec::new(),
        }
    }

    pub fn points(&self) -> &[i8; 24] {
        &self.points
    }

    pub fn bar(&self) -> (u8, u8) {
        self.bar
    }

    pub fn off(&self) -> (u8, u8) {
        self.off
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn dice(&self) -> [u8; 2] {
        self.dice
    }

    pub fn left(&self) -> &[u8] {
        &self.left
    }

    pub fn winner(&self) -> Option<Side> {
        if self.off.0 == 15 {
            Some(Side::White)
        } else if self.off.1 == 15 {
            Some(Side::Black)
        } else {
            None
        }
    }

    /// Distance-to-off summed over a side's checkers; bar checkers count 25
    /// (a full board traverse) for either side.
    pub fn pip(&self, side: Side) -> usize {
        let mut total = 0usize;
        for (idx, &v) in self.points.iter().enumerate() {
            let owned = match side {
                Side::White => v > 0,
                Side::Black => v < 0,
            };
            if owned {
                total += side_count(v) * distance_to_off(side, idx);
            }
        }
        let bar_count = match side {
            Side::White => self.bar.0,
            Side::Black => self.bar.1,
        };
        total + bar_count as usize * 25
    }

    /// Roll the dice and enter the Move phase. Returns false unless the game
    /// is awaiting a roll. If neither player has any legal step, the turn
    /// auto-passes straight back to the opponent's Roll phase.
    pub fn roll(&mut self) -> bool {
        if self.phase != Phase::Roll {
            return false;
        }
        let mut rng = rand::thread_rng();
        let d1 = rng.gen_range(1..=6);
        let d2 = rng.gen_range(1..=6);
        self.roll_with(d1, d2);
        true
    }

    /// Deterministic roll used by tests and setup code.
    pub(crate) fn roll_with(&mut self, d1: u8, d2: u8) {
        debug_assert!((1..=6).contains(&d1) && (1..=6).contains(&d2));
        self.dice = [d1, d2];
        if d1 == d2 {
            self.left = vec![d1, d1, d1, d1];
        } else {
            self.left = vec![d1, d2];
        }
        self.phase = Phase::Move;
        // Auto-pass approximation: if no distinct die admits even one step,
        // the whole roll is dead — hand over immediately.
        if !self.left.iter().any(|&d| !self.steps_for_die(d).is_empty()) {
            self.end_turn();
        }
    }

    /// Sources (0..=BAR pseudo-slots) from which the current player may move.
    pub fn movable(&self) -> Vec<usize> {
        if self.phase != Phase::Move {
            return Vec::new();
        }
        let mut out: Vec<usize> = Vec::new();
        for d in self.unique_dice() {
            for src in 0..=BAR {
                // Bar forces entry-only; otherwise the source must be ours.
                let usable = if src == BAR {
                    self.bar_count(self.turn) > 0
                } else {
                    self.bar_count(self.turn) == 0 && self.owns(self.turn, src)
                };
                if usable && !out.contains(&src) && !self.step_from(src, d).is_empty() {
                    out.push(src);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// Legal `(destination, consumed die)` pairs from `from` (a point index or
    /// [`BAR`]) for the current player mid-move.
    pub fn destinations(&self, from: usize) -> Vec<(Dest, u8)> {
        if self.phase != Phase::Move || from > BAR {
            return Vec::new();
        }
        if from == BAR {
            if self.bar_count(self.turn) == 0 {
                return Vec::new();
            }
        } else if !self.owns(self.turn, from) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut seen = Vec::new();
        for &d in &self.left {
            if seen.contains(&d) {
                continue;
            }
            seen.push(d);
            for dest in self.step_from(from, d) {
                out.push((dest, d));
            }
        }
        out
    }

    /// Validate, apply, consume one die, and auto-end the turn when the move
    /// list empties. Errors are static strings; nothing panics.
    pub fn move_checker(&mut self, from: usize, dest: Dest) -> Result<(), &'static str> {
        if self.phase != Phase::Move {
            return Err("not in move phase");
        }
        if from > BAR {
            return Err("bad source slot");
        }
        if from == BAR && self.bar_count(self.turn) == 0 {
            return Err("no checker on your bar");
        }
        let candidates = self.destinations(from);
        let die = candidates
            .iter()
            .find(|(d, _)| *d == dest)
            .map(|(_, die)| *die)
            .ok_or("illegal destination")?;

        // A lone ENEMY blot on the landing point gets sent to its bar; landing
        // on a lone own checker just stacks (v carries over untouched).
        if let Dest::Point(idx) = dest {
            let v = self.points[idx];
            if v == -side_sign(self.turn) {
                self.points[idx] = 0;
                if v == 1 {
                    self.bar.0 += 1;
                } else {
                    self.bar.1 += 1;
                }
            }
        }

        if from == BAR {
            match self.turn {
                Side::White => self.bar.0 -= 1,
                Side::Black => self.bar.1 -= 1,
            }
        } else {
            self.points[from] -= side_sign(self.turn);
        }

        match dest {
            Dest::Point(idx) => self.points[idx] += side_sign(self.turn),
            Dest::Off => match self.turn {
                Side::White => self.off.0 += 1,
                Side::Black => self.off.1 += 1,
            },
        }

        // Playing consumes exactly ONE entry (matters on doubles).
        if let Some(pos) = self.left.iter().position(|&d| d == die) {
            self.left.remove(pos);
        }

        if self.winner().is_some() {
            self.phase = Phase::GameOver;
        } else if self.left.is_empty()
            || self.left.iter().all(|&d| self.steps_for_die(d).is_empty())
        {
            self.end_turn();
        }
        Ok(())
    }

    // -- internals -----------------------------------------------------------

    fn end_turn(&mut self) {
        if self.phase == Phase::GameOver {
            return;
        }
        self.left.clear();
        self.phase = Phase::Roll;
        self.turn = self.turn.opposite();
    }

    fn unique_dice(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for &d in &self.left {
            if !out.contains(&d) {
                out.push(d);
            }
        }
        out
    }

    fn bar_count(&self, side: Side) -> u8 {
        match side {
            Side::White => self.bar.0,
            Side::Black => self.bar.1,
        }
    }

    fn owns(&self, side: Side, idx: usize) -> bool {
        match side {
            Side::White => self.points[idx] > 0,
            Side::Black => self.points[idx] < 0,
        }
    }

    /// Landing target of one die from `src`, if legal. Empty when blocked.
    fn step_from(&self, src: usize, die: u8) -> Vec<Dest> {
        if die == 0 {
            return Vec::new();
        }
        // While your bar holds checkers, bar entry is the only source.
        if src != BAR && self.bar_count(self.turn) > 0 {
            return Vec::new();
        }
        match self.turn {
            Side::White => self.white_step(src, die),
            Side::Black => self.black_step(src, die),
        }
    }

    fn entry_blocked(&self, idx: usize) -> bool {
        match self.turn {
            Side::White => self.points[idx] <= -2,
            Side::Black => self.points[idx] >= 2,
        }
    }

    /// Union of legal single steps for every distinct remaining die — the
    /// auto-pass / movable machinery builds on this.
    fn steps_for_die(&self, die: u8) -> Vec<Dest> {
        if self.bar_count(self.turn) > 0 {
            return self.step_from(BAR, die);
        }
        let mut out = Vec::new();
        for idx in 0..24 {
            if self.owns(self.turn, idx) {
                out.extend(self.step_from(idx, die));
            }
        }
        out
    }

    fn can_land(&self, idx: usize) -> bool {
        !self.entry_blocked(idx)
    }

    fn all_home(&self, side: Side) -> bool {
        if self.bar_count(side) > 0 {
            return false;
        }
        let home = match side {
            Side::White => 18..=23,
            Side::Black => 0..=5,
        };
        // All checkers home <=> no checker owned outside the home range.
        (0..24).all(|i| home.contains(&i) || !self.owns(side, i))
    }

    fn white_step(&self, src: usize, die: u8) -> Vec<Dest> {
        if src == BAR {
            let idx = die as usize - 1;
            return if self.can_land(idx) {
                vec![Dest::Point(idx)]
            } else {
                Vec::new()
            };
        }
        let target = src + die as usize;
        if target <= 23 {
            return if self.can_land(target) {
                vec![Dest::Point(target)]
            } else {
                Vec::new()
            };
        }
        // Bear-off territory: needs everything home.
        if !self.all_home(Side::White) {
            return Vec::new();
        }
        let exact = die as usize == 24 - src;
        if exact {
            return vec![Dest::Off];
        }
        // Overshoot allowed only from the highest-indexed white checker.
        let furthest = (0..24).rev().find(|&i| self.owns(Side::White, i));
        if furthest == Some(src) {
            return vec![Dest::Off];
        }
        Vec::new()
    }

    fn black_step(&self, src: usize, die: u8) -> Vec<Dest> {
        if src == BAR {
            let idx = 24 - die as usize;
            return if self.can_land(idx) {
                vec![Dest::Point(idx)]
            } else {
                Vec::new()
            };
        }
        if let Some(target) = src.checked_sub(die as usize) {
            return if self.can_land(target) {
                vec![Dest::Point(target)]
            } else {
                Vec::new()
            };
        }
        if !self.all_home(Side::Black) {
            return Vec::new();
        }
        let exact = die as usize == src + 1;
        if exact {
            return vec![Dest::Off];
        }
        // Overshoot allowed only from the lowest-indexed black checker.
        let furthest = (0..24).find(|&i| self.owns(Side::Black, i));
        if furthest == Some(src) {
            return vec![Dest::Off];
        }
        Vec::new()
    }
}

fn side_sign(side: Side) -> i8 {
    match side {
        Side::White => 1,
        Side::Black => -1,
    }
}

/// Test-only constructor so sibling modules (e.g. `ai`) can stage exact
/// positions without reaching into private fields.
#[cfg(test)]
impl Backgammon {
    pub(crate) fn staged(
        points: [i8; 24],
        bar: (u8, u8),
        off: (u8, u8),
        turn: Side,
        d1: u8,
        d2: u8,
    ) -> Self {
        let mut g = Self::new();
        g.points = points;
        g.bar = bar;
        g.off = off;
        g.turn = turn;
        g.roll_with(d1, d2);
        g
    }
}

fn side_count(v: i8) -> usize {
    v.unsigned_abs() as usize
}

fn distance_to_off(side: Side, idx: usize) -> usize {
    match side {
        Side::White => 24 - idx,
        Side::Black => idx + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_with(mut setup: impl FnMut(&mut Backgammon)) -> Backgammon {
        let mut g = Backgammon::new();
        setup(&mut g);
        g
    }

    #[test]
    fn start_layout_matches_spec_and_pips_are_equal() {
        let g = Backgammon::new();
        assert_eq!(g.points()[0], 2);
        assert_eq!(g.points()[5], -5);
        assert_eq!(g.points()[7], -3);
        assert_eq!(g.points()[11], 5);
        assert_eq!(g.points()[12], -5);
        assert_eq!(g.points()[16], 3);
        assert_eq!(g.points()[18], 5);
        assert_eq!(g.points()[23], -2);
        assert_eq!(g.pip(Side::White), 167);
        assert_eq!(g.pip(Side::Black), 167);
        assert_eq!(g.turn(), Side::Black);
        assert_eq!(g.phase(), Phase::Roll);
    }

    #[test]
    fn blots_hit_to_correct_bar() {
        // White blot at 3; black checker at 7 rolls a 4 and lands on it.
        let mut g = game_with(|g| {
            g.points = START;
            g.points[3] = 1;
            g.points[7] += 1; // now -4, still black
        });
        g.roll_with(4, 4);
        let dests = g.destinations(7);
        assert!(
            dests.contains(&(Dest::Point(3), 4)),
            "expected hit available: {dests:?}"
        );
        g.move_checker(7, Dest::Point(3)).unwrap();
        assert_eq!(g.bar().0, 1, "white blot sent to white bar");
        assert_eq!(g.points()[3], -1);
    }

    #[test]
    fn blocked_destination_excluded() {
        let mut g = game_with(|g| {
            g.points = START;
            g.points[2] = -2; // white cannot land here
            g.turn = Side::White;
        });
        g.roll_with(1, 2);
        let dests = g.destinations(0);
        assert!(dests.contains(&(Dest::Point(1), 1)));
        assert!(!dests.iter().any(|(d, _)| *d == Dest::Point(2)));
    }

    #[test]
    fn bar_forces_entry_only_sources() {
        let mut g = game_with(|g| {
            g.points = START;
            g.bar = (1, 0);
            g.turn = Side::White; // white sits on the bar
        });
        g.roll_with(3, 5);
        assert_eq!(g.movable(), vec![BAR], "bar must be sole source");
        assert!(g.move_checker(11, Dest::Point(14)).is_err());
        g.move_checker(BAR, Dest::Point(2)).unwrap(); // die 3 enters at 3-1
        assert_eq!(g.bar().0, 0);
        assert_eq!(g.points()[2], 1);
    }

    #[test]
    fn entering_blocked_point_excluded() {
        let mut g = game_with(|g| {
            g.points = START;
            g.bar = (1, 0);
            g.turn = Side::White;
        });
        g.roll_with(6, 3); // entry targets: die 6 -> point 5 (blocked), die 3 -> point 2 (open)
        let dests = g.destinations(BAR);
        assert!(!dests.iter().any(|(d, _)| *d == Dest::Point(5)));
        assert_eq!(dests, vec![(Dest::Point(2), 3)]);
    }

    #[test]
    fn exact_bear_off_works() {
        let mut g = game_with(|g| {
            g.points = [0; 24];
            g.points[20] = 2;
            g.off = (13, 0);
            g.turn = Side::White;
        });
        g.roll_with(4, 2); // white: 20 + 4 = 24 -> exact off
        let dests = g.destinations(20);
        assert!(dests.contains(&(Dest::Off, 4)));
        g.move_checker(20, Dest::Off).unwrap();
        assert_eq!(g.off().0, 14);
        assert_eq!(g.points()[20], 1);
    }

    #[test]
    fn overshoot_bear_off_only_from_furthest_checker() {
        let mut g = game_with(|g| {
            g.points = [0; 24];
            g.points[19] = 1; // lower than 21
            g.points[21] = 1; // highest white checker
            g.off = (13, 0);
            g.turn = Side::White;
        });
        g.roll_with(5, 5);
        // From 19 with die 5: 19+5=24 exact? 24-19=5 yes — exact always fine.
        assert!(g.destinations(19).contains(&(Dest::Off, 5)));
        // From 21 with die 5: overshoot (needs 3), allowed since 21 is furthest.
        assert!(g.destinations(21).contains(&(Dest::Off, 5)));
        // Now make 21 not furthest by adding a checker at 22 via another route:
        g.points[22] = 1;
        g.off = (12, 0);
        assert!(
            !g.destinations(21).contains(&(Dest::Off, 5)),
            "overshoot must be reserved for the rearmost... foremost checker"
        );
        assert!(g.destinations(22).contains(&(Dest::Off, 5)));
    }

    #[test]
    fn doubles_yield_four_entries_and_four_moves() {
        let mut g = Backgammon::new();
        g.roll_with(3, 3);
        assert_eq!(g.left(), &[3, 3, 3, 3]);
        for _ in 0..4 {
            let m = g.movable();
            assert!(!m.is_empty());
            let src = m[0];
            let (dest, _) = g.destinations(src)[0];
            g.move_checker(src, dest).unwrap();
        }
        assert!(g.left().is_empty());
        assert_eq!(g.phase(), Phase::Roll);
        assert_eq!(g.turn(), Side::White, "turn flipped after four plays");
    }

    #[test]
    fn winner_when_fifteen_off() {
        let mut g = game_with(|g| {
            g.points = [0; 24];
            g.points[23] = 1;
            g.off = (14, 0);
            g.turn = Side::White;
        });
        g.roll_with(1, 1);
        g.move_checker(23, Dest::Off).unwrap();
        assert_eq!(g.winner(), Some(Side::White));
        assert_eq!(g.phase(), Phase::GameOver);
        assert!(g.movable().is_empty());
        assert!(!g.roll(), "roll refused after game over");
    }

    #[test]
    fn auto_pass_when_no_legal_steps() {
        // Black checkers at 6 (home edge) and 10 (outside home, so no bear-off).
        // White pairs block every downward landing square for dice 5 and 3.
        let mut g = game_with(|g| {
            g.points = [0; 24];
            g.points[6] = -1;
            g.points[10] = -1;
            for idx in [1usize, 3, 5, 7] {
                g.points[idx] = 2;
            }
        });
        g.roll_with(5, 3);
        assert_eq!(g.phase(), Phase::Roll, "auto-passed");
        assert_eq!(g.turn(), Side::White, "handed to opponent");
        assert!(g.left().is_empty());
    }

    #[test]
    fn move_checker_rejects_illegal_dest() {
        let mut g = Backgammon::new();
        g.roll_with(1, 2);
        // Not your turn pieces / wrong direction: black moving upward is illegal.
        assert!(g.move_checker(12, Dest::Point(13)).is_err());
        // Off while not all home is illegal.
        assert!(g.move_checker(23, Dest::Off).is_err());
        // Point too far for the dice from a real source:
        assert!(g.move_checker(23, Dest::Point(15)).is_err());
        assert_eq!(g.phase(), Phase::Move);
    }

    #[test]
    fn landing_on_own_blot_stacks_instead_of_hitting_itself() {
        // Black owns a lone blot on 18; a bar checker re-enters right onto it.
        let mut g = game_with(|g| {
            g.points = START;
            g.points[18] = -1;
            g.bar = (0, 1);
            g.turn = Side::Black;
        });
        g.roll_with(6, 3); // die 6 enters at 24-6 = 18
        assert!(g.destinations(BAR).contains(&(Dest::Point(18), 6)));
        g.move_checker(BAR, Dest::Point(18)).unwrap();
        assert_eq!(g.points()[18], -2, "own blot becomes a made point");
        assert_eq!(g.bar(), (0, 0), "no self-hitting");
        assert_eq!(g.left(), &[3]);

        // Same rule mid-board: white moving onto its own lone checker.
        let mut g = game_with(|g| {
            g.points = START;
            g.points[16] = 4;
            g.points[19] = 1;
            g.turn = Side::White;
        });
        g.roll_with(3, 5);
        g.move_checker(16, Dest::Point(19)).unwrap();
        assert_eq!(g.points()[19], 2);
        assert_eq!(g.bar(), (0, 0));
    }

    #[test]
    fn playing_consumes_one_die_entry() {
        let mut g = Backgammon::new();
        g.roll_with(2, 5);
        assert_eq!(g.left().len(), 2);
        g.move_checker(23, Dest::Point(21)).unwrap(); // consumes a 2
        assert_eq!(g.left(), &[5]);
        assert_eq!(g.dice(), [2, 5]);
    }
}
