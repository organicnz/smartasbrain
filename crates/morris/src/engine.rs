//! Pure Nine Men's Morris rules: alternating placement, sliding along board
//! lines, flight at three stones, mill capture with the protected-stone
//! exemption, and wins by reduction or blockade.
//!
//! Points are indexed `0..`[`POINTS`] on three nested squares:
//!
//! ```text
//! 0-----------1-----------2
//! |           |           |
//! |   3-------4-------5   |
//! |   |       |       |   |
//! |   |   6---7---8   |   |
//! |   |   |       |   |   |
//! 9---10--11      12--13--14
//! |   |   |       |   |   |
//! |   |   15--16--17  |   |
//! |   |       |       |   |
//! |   18------19------20  |
//! |           |           |
//! 21----------22----------23
//! ```

/// Number of playable points on the board.
pub const POINTS: usize = 24;

/// Neighbours along board lines for every point index.
pub const ADJACENCY: [&[usize]; POINTS] = [
    &[1, 9],
    &[0, 2, 4],
    &[1, 14],
    &[4, 10],
    &[1, 3, 5, 7],
    &[4, 13],
    &[7, 11],
    &[4, 6, 8],
    &[7, 12],
    &[0, 10, 21],
    &[3, 9, 11, 18],
    &[6, 10, 15],
    &[8, 13, 17],
    &[5, 12, 14, 20],
    &[2, 13, 23],
    &[11, 16],
    &[15, 17, 19],
    &[12, 16],
    &[10, 19],
    &[16, 18, 20, 22],
    &[13, 19],
    &[9, 22],
    &[19, 21, 23],
    &[14, 22],
];

/// Every mill line: eight across, eight down.
pub const MILLS: [[usize; 3]; 16] = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8],
    [9, 10, 11],
    [12, 13, 14],
    [15, 16, 17],
    [18, 19, 20],
    [21, 22, 23],
    [0, 9, 21],
    [3, 10, 18],
    [6, 11, 15],
    [1, 4, 7],
    [16, 19, 22],
    [8, 12, 17],
    [5, 13, 20],
    [2, 14, 23],
];

/// Board seat. White opens the game.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    /// Lowercase name for UI text.
    pub fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
        }
    }
}

/// Coarse game stage; an outstanding capture rides on
/// [`Morris::removal_pending`] instead of its own phase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Place,
    Move,
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    Won(Side),
}

/// One resolved board action, shared by the UI highlight layer and the AI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Place(usize),
    Move(usize, usize),
    Remove(usize),
}

/// Nine Men's Morris state machine.
#[derive(Clone)]
pub struct Morris {
    board: [Option<Side>; POINTS],
    turn: Side,
    /// Stones still waiting to be placed, indexed by [`Side`].
    to_place: [u8; 2],
    on_board: [u8; 2],
    /// True while the mover owes a capture off the opposing mill-exempt pool.
    pending_remove: bool,
    status: Status,
    last: Option<Action>,
}

fn idx(side: Side) -> usize {
    match side {
        Side::White => 0,
        Side::Black => 1,
    }
}

impl Morris {
    pub fn new() -> Self {
        Self {
            board: [None; POINTS],
            turn: Side::White,
            to_place: [9; 2],
            on_board: [0; 2],
            pending_remove: false,
            status: Status::Ongoing,
            last: None,
        }
    }

    /// Occupant of `i`, or `None` when empty or out of range.
    pub fn point(&self, i: usize) -> Option<Side> {
        self.board.get(i).copied().flatten()
    }

    pub fn phase(&self) -> Phase {
        match self.status {
            Status::Won(_) => Phase::Over,
            Status::Ongoing if self.to_place[0] > 0 || self.to_place[1] > 0 => Phase::Place,
            Status::Ongoing => Phase::Move,
        }
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// Stones `side` has yet to place.
    pub fn stones_left(&self, side: Side) -> u8 {
        self.to_place[idx(side)]
    }

    /// Stones `side` currently has standing on the board.
    pub fn on_board(&self, side: Side) -> u8 {
        self.on_board[idx(side)]
    }

    /// True while the side to move must capture before anything else.
    pub fn removal_pending(&self) -> bool {
        self.status == Status::Ongoing && self.pending_remove
    }

    pub fn last_action(&self) -> Option<Action> {
        self.last
    }

    /// True when `side` is down to three placed stones and may fly.
    pub fn flying(&self, side: Side) -> bool {
        self.to_place[idx(side)] == 0 && self.on_board[idx(side)] == 3
    }

    /// True when `side` has at least one legal action right now.
    fn has_any_action(&self, side: Side) -> bool {
        if self.to_place[idx(side)] > 0 {
            return (0..POINTS).any(|i| self.board[i].is_none());
        }
        if self.flying(side) {
            return (0..POINTS).any(|i| self.board[i].is_none());
        }
        (0..POINTS).any(|i| {
            self.board[i] == Some(side) && ADJACENCY[i].iter().any(|&j| self.board[j].is_none())
        })
    }

    /// True when a mill of `side` passes through `at`.
    fn is_mill_at(&self, at: usize, side: Side) -> bool {
        MILLS
            .iter()
            .any(|m| m.contains(&at) && m.iter().all(|&p| self.board[p] == Some(side)))
    }

    fn in_any_mill(&self, at: usize) -> bool {
        self.board[at].is_some_and(|s| self.is_mill_at(at, s))
    }

    /// Enemy stones the mover may take: unprotected ones, or every stone when
    /// the whole opposing force sits inside mills.
    pub fn removable_points(&self) -> Vec<usize> {
        if self.removal_pending() {
            self.capturable_points()
        } else {
            Vec::new()
        }
    }

    /// Capture candidates ignoring the pending flag, so [`Self::after_action`]
    /// can consult the pool before arming it.
    fn capturable_points(&self) -> Vec<usize> {
        let foe = self.turn.opposite();
        let free: Vec<usize> = (0..POINTS)
            .filter(|&i| self.board[i] == Some(foe) && !self.in_any_mill(i))
            .collect();
        if free.is_empty() {
            (0..POINTS)
                .filter(|&i| self.board[i] == Some(foe))
                .collect()
        } else {
            free
        }
    }

    /// Empty destinations from `i` for the side to move; respects adjacency
    /// unless the stone may fly. Empty unless `i` holds a movable own stone.
    pub fn legal_moves_from(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        if self.removal_pending() || self.phase() != Phase::Move || i >= POINTS {
            return out;
        }
        if self.board[i] != Some(self.turn) {
            return out;
        }
        if self.flying(self.turn) {
            out.extend((0..POINTS).filter(|&j| self.board[j].is_none()));
        } else {
            out.extend(
                ADJACENCY[i]
                    .iter()
                    .copied()
                    .filter(|&j| self.board[j].is_none()),
            );
        }
        out
    }

    /// Put a stone on empty `i` for the mover; a new mill arms the capture.
    pub fn place(&mut self, i: usize) -> bool {
        if self.removal_pending() || self.phase() != Phase::Place {
            return false;
        }
        if i >= POINTS || self.board[i].is_some() {
            return false;
        }
        let side = self.turn;
        if self.to_place[idx(side)] == 0 {
            return false;
        }
        self.board[i] = Some(side);
        self.to_place[idx(side)] -= 1;
        self.on_board[idx(side)] += 1;
        self.last = Some(Action::Place(i));
        self.after_action(i);
        true
    }

    /// Slide `from`->`to` along a line (anywhere when flying); a new mill at
    /// `to` arms the capture.
    pub fn move_stone(&mut self, from: usize, to: usize) -> bool {
        if self.removal_pending() || self.phase() != Phase::Move {
            return false;
        }
        if from >= POINTS || to >= POINTS || from == to {
            return false;
        }
        if self.board[from] != Some(self.turn) || self.board[to].is_some() {
            return false;
        }
        if !self.flying(self.turn) && !ADJACENCY[from].contains(&to) {
            return false;
        }
        self.board[from] = None;
        self.board[to] = Some(self.turn);
        self.last = Some(Action::Move(from, to));
        self.after_action(to);
        true
    }

    /// Take the opposing stone on `i`, honouring the mill-exemption rule.
    pub fn remove(&mut self, i: usize) -> bool {
        if !self.removal_pending() {
            return false;
        }
        if i >= POINTS || self.board[i] != Some(self.turn.opposite()) {
            return false;
        }
        if !self.removable_points().contains(&i) {
            return false;
        }
        self.on_board[idx(self.turn.opposite())] -= 1;
        self.board[i] = None;
        self.pending_remove = false;
        self.last = Some(Action::Remove(i));
        self.pass_turn();
        true
    }

    /// Shared tail of `place`/`move_stone`: arm the capture on a fresh mill,
    /// otherwise hand the turn over.
    fn after_action(&mut self, dest: usize) {
        if self.is_mill_at(dest, self.turn) && !self.capturable_points().is_empty() {
            self.pending_remove = true;
        } else {
            self.pass_turn();
        }
    }

    fn pass_turn(&mut self) {
        self.turn = self.turn.opposite();
        self.refresh_status();
    }

    /// The side to move loses when reduced under three stones overall or
    /// completely blocked; the previous mover takes the win.
    fn refresh_status(&mut self) {
        if self.status != Status::Ongoing {
            return;
        }
        let winner = self.turn.opposite();
        let now = idx(self.turn);
        let total = u16::from(self.on_board[now]) + u16::from(self.to_place[now]);
        if total < 3 || !self.has_any_action(self.turn) {
            self.status = Status::Won(winner);
        }
    }

    /// Test constructor: raw position assembly for AI and rules fixtures.
    #[cfg(test)]
    pub(crate) fn set_position(
        pieces: &[(usize, Side)],
        to_place: [u8; 2],
        turn: Side,
        pending_remove: bool,
    ) -> Self {
        let mut g = Self::new();
        g.board = [None; POINTS];
        for &(i, side) in pieces {
            g.board[i] = Some(side);
        }
        g.to_place = to_place;
        g.on_board = [
            pieces.iter().filter(|(_, s)| *s == Side::White).count() as u8,
            pieces.iter().filter(|(_, s)| *s == Side::Black).count() as u8,
        ];
        g.turn = turn;
        g.pending_remove = pending_remove;
        g
    }
}

impl Default for Morris {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_alternates_and_rejects_bad_targets() {
        let mut g = Morris::new();
        assert_eq!(g.phase(), Phase::Place);
        assert_eq!(g.turn(), Side::White);
        assert!(g.place(0));
        assert_eq!(g.turn(), Side::Black);
        assert_eq!(g.stones_left(Side::White), 8);
        assert!(g.place(23));
        assert_eq!(g.turn(), Side::White);
        assert!(!g.place(0), "occupied point");
        assert!(!g.place(POINTS), "out of range");
        assert!(!g.move_stone(0, 1), "cannot slide while placing");
        assert!(!g.remove(23), "nothing pending");
    }

    #[test]
    fn mill_sets_pending_then_removal_passes_turn() {
        let mut g = Morris::new();
        for (side, pt) in [
            (Side::White, 0usize),
            (Side::Black, 9),
            (Side::White, 1),
            (Side::Black, 10),
            (Side::White, 2),
        ] {
            assert_eq!(g.turn(), side);
            assert!(g.place(pt));
        }
        assert!(g.removal_pending(), "0-1-2 completed");
        assert_eq!(g.turn(), Side::White, "mover keeps the turn for capture");
        assert_eq!(g.removable_points(), vec![9, 10]);
        assert!(!g.remove(0), "own stone");
        assert!(g.remove(10));
        assert!(!g.removal_pending());
        assert_eq!(g.turn(), Side::Black);
        assert_eq!(g.last_action(), Some(Action::Remove(10)));
    }

    #[test]
    fn removal_refuses_protected_unless_all_protected() {
        // White just closed 0-1-2; black owns the 3-4-5 mill plus point 6.
        let mut g = Morris::set_position(
            &[
                (0, Side::White),
                (1, Side::White),
                (2, Side::White),
                (3, Side::Black),
                (4, Side::Black),
                (5, Side::Black),
                (6, Side::Black),
            ],
            [0, 0],
            Side::White,
            true,
        );
        assert_eq!(g.removable_points(), vec![6], "mill stones shielded");
        assert!(!g.remove(3));
        assert!(g.remove(6));

        // Everything black has lives in mills: the shield drops.
        let mut g = Morris::set_position(
            &[
                (0, Side::White),
                (1, Side::White),
                (2, Side::White),
                (3, Side::Black),
                (4, Side::Black),
                (5, Side::Black),
            ],
            [0, 0],
            Side::White,
            true,
        );
        assert_eq!(g.removable_points(), vec![3, 4, 5]);
        assert!(g.remove(4));
    }

    #[test]
    fn flying_activates_at_exactly_three_placed_stones() {
        let mut g = Morris::set_position(
            &[
                (0, Side::White),
                (4, Side::White),
                (7, Side::White),
                (9, Side::Black),
                (10, Side::Black),
                (11, Side::Black),
            ],
            [0, 0],
            Side::White,
            false,
        );
        assert!(g.flying(Side::White));
        assert!(g.flying(Side::Black));
        assert!(g.legal_moves_from(0).contains(&21), "flight skips lines");
        assert!(g.move_stone(0, 21));

        // Four stones: strictly adjacency-bound again.
        let mut g = Morris::set_position(
            &[
                (0, Side::White),
                (4, Side::White),
                (7, Side::White),
                (16, Side::White),
                (9, Side::Black),
                (10, Side::Black),
                (11, Side::Black),
            ],
            [0, 0],
            Side::White,
            false,
        );
        assert!(!g.flying(Side::White));
        assert_eq!(g.legal_moves_from(0), vec![1], "point 9 is black's");
        assert!(!g.move_stone(0, 21));
    }

    #[test]
    fn moving_phase_rejects_non_adjacent_slide() {
        // Four white stones keep flight off so adjacency bites.
        let mut g = Morris::set_position(
            &[
                (0, Side::White),
                (3, Side::White),
                (6, Side::White),
                (19, Side::White),
                (21, Side::Black),
                (22, Side::Black),
                (23, Side::Black),
            ],
            [0, 0],
            Side::White,
            false,
        );
        assert_eq!(g.phase(), Phase::Move);
        assert!(!g.move_stone(0, 16), "no line joins 0 and 16");
        assert!(!g.move_stone(21, 0), "enemy stone");
        assert!(!g.move_stone(5, 4), "source empty");
        assert!(g.move_stone(0, 1));
    }

    #[test]
    fn fully_blocked_side_to_move_loses() {
        // Four black stones (0, 2, 21, 23) whose every neighbour is white,
        // so flight never kicks in; white 6 sits clear of the siege and its
        // slide opens no gaps.
        let mut g = Morris::set_position(
            &[
                (1, Side::White),
                (9, Side::White),
                (14, Side::White),
                (22, Side::White),
                (6, Side::White),
                (0, Side::Black),
                (2, Side::Black),
                (21, Side::Black),
                (23, Side::Black),
            ],
            [0, 0],
            Side::White,
            false,
        );
        assert!(g.move_stone(6, 7), "white steps aside, mill-free");
        assert_eq!(g.status(), Status::Won(Side::White), "black cannot act");
        assert_eq!(g.phase(), Phase::Over);
    }

    #[test]
    fn reduction_below_three_total_loses_immediately() {
        // White holds mill 0-1-2 and owes a capture; black has three total.
        let mut g = Morris::set_position(
            &[
                (0, Side::White),
                (1, Side::White),
                (2, Side::White),
                (9, Side::White),
                (21, Side::Black),
                (18, Side::Black),
                (15, Side::Black),
            ],
            [0, 0],
            Side::White,
            true,
        );
        assert!(g.remove(21));
        assert_eq!(
            g.status(),
            Status::Won(Side::White),
            "black down to two stones"
        );
        assert_eq!(g.phase(), Phase::Over);
    }

    #[test]
    fn scripted_game_tracks_phases_counts_and_actions() {
        let mut g = Morris::new();
        // Interleaved placements; white closes 0-1-2 on its fifth drop.
        let script: [(Side, usize); 18] = [
            (Side::White, 0),
            (Side::Black, 21),
            (Side::White, 3),
            (Side::Black, 18),
            (Side::White, 6),
            (Side::Black, 15),
            (Side::White, 1),
            (Side::Black, 22),
            (Side::White, 2), // mill
            (Side::Black, 19),
            (Side::White, 4),
            (Side::Black, 10),
            (Side::White, 5),  // mill 3-4-5
            (Side::Black, 17), // off the 16-19-22 rail, so black never mills
            (Side::White, 7),  // mill 1-4-7
            (Side::Black, 11),
            (Side::White, 20),
            (Side::Black, 13),
        ];
        let mut captures = 0;
        for &(side, pt) in &script {
            assert_eq!(g.turn(), side, "alternation broke before {pt}");
            assert!(g.place(pt), "{side:?} place {pt}");
            if g.removal_pending() {
                captures += 1;
                let target = g.removable_points()[0];
                assert!(g.remove(target));
            }
        }
        assert_eq!(captures, 3, "white closed three mills");
        assert_eq!(g.phase(), Phase::Move);
        assert_eq!(g.stones_left(Side::White), 0);
        assert_eq!(g.stones_left(Side::Black), 0);
        assert_eq!(g.status(), Status::Ongoing);

        let whites = (0..POINTS)
            .filter(|&i| g.point(i) == Some(Side::White))
            .count();
        let blacks = (0..POINTS)
            .filter(|&i| g.point(i) == Some(Side::Black))
            .count();
        assert_eq!(whites, 9, "nine white stones, never touched");
        assert_eq!(blacks, 9 - captures, "one stone per capture");

        // Point 9 is never played in the script, so this slide always fits.
        assert!(g.move_stone(0, 9), "slide along the left rail");
        assert!(!g.removal_pending());
        assert_eq!(g.last_action(), Some(Action::Move(0, 9)));
        assert_eq!(g.turn(), Side::Black);
        assert_eq!(g.status(), Status::Ongoing);
    }
}
