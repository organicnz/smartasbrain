//! Pure English draughts (checkers) rules — no TUI dependencies here.

/// Squares per board row/column.
pub const SIZE: usize = 8;

/// Playing sides. White sits at the bottom and marches up the board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    White,
    Black,
}

impl Side {
    /// The other side.
    pub fn opposite(self) -> Self {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }
}

/// A piece: a man until it reaches its last row, then a king.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PC {
    pub side: Side,
    pub king: bool,
}

/// Game outcome.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    Won(Side),
}

/// Full rules state. Squares are indexed `r * 8 + c` with row 0 at the top;
/// only dark squares (`(r + c) % 2 == 1`) ever hold pieces.
#[derive(Clone)]
pub struct Checkers {
    board: [Option<PC>; SIZE * SIZE],
    turn: Side,
    /// While a multi-jump is in progress, the index of the jumping piece.
    chain_from: Option<usize>,
    last_move: Option<(usize, usize)>,
    status: Status,
}

/// Diagonal directions a piece may travel; men only march forward.
fn dirs(king: bool, side: Side) -> &'static [(isize, isize)] {
    match king {
        true => &[(-1, -1), (-1, 1), (1, -1), (1, 1)],
        false => match side {
            Side::White => &[(-1, -1), (-1, 1)],
            Side::Black => &[(1, -1), (1, 1)],
        },
    }
}

impl Checkers {
    /// Standard starting position: Black men on top, White men below,
    /// White to move.
    pub fn new() -> Self {
        let mut board = [None; SIZE * SIZE];
        for r in 0..SIZE {
            for c in 0..SIZE {
                if (r + c) % 2 != 1 {
                    continue;
                }
                let side = match r {
                    0..=2 => Some(Side::Black),
                    5..=7 => Some(Side::White),
                    _ => None,
                };
                if let Some(side) = side {
                    board[r * SIZE + c] = Some(PC { side, king: false });
                }
            }
        }
        Self {
            board,
            turn: Side::White,
            chain_from: None,
            last_move: None,
            status: Status::Ongoing,
        }
    }

    /// Piece standing on `square`, if any.
    pub fn piece_at(&self, square: usize) -> Option<PC> {
        self.board[square]
    }

    /// Side whose turn it is.
    pub fn turn(&self) -> Side {
        self.turn
    }

    /// Current outcome.
    pub fn status(&self) -> Status {
        self.status
    }

    /// Origin square of an in-progress multi-jump, if any.
    pub fn chain_from(&self) -> Option<usize> {
        self.chain_from
    }

    /// Last executed move as `(from, to)`.
    pub fn last_move(&self) -> Option<(usize, usize)> {
        self.last_move
    }

    /// Sparse position builder for tests across the crate: listed pieces on
    /// dark squares, empty elsewhere.
    #[cfg(test)]
    pub(crate) fn test_position(turn: Side, pieces: &[(usize, Side, bool)]) -> Self {
        let mut board = [None; SIZE * SIZE];
        for &(square, side, is_king) in pieces {
            board[square] = Some(PC {
                side,
                king: is_king,
            });
        }
        Self {
            board,
            turn,
            chain_from: None,
            last_move: None,
            status: Status::Ongoing,
        }
    }

    /// Remaining men+kings as `(white, black)`.
    pub fn counts(&self) -> (usize, usize) {
        let mut white = 0;
        let mut black = 0;
        for pc in self.board.iter().flatten() {
            match pc.side {
                Side::White => white += 1,
                Side::Black => black += 1,
            }
        }
        (white, black)
    }

    /// Squares `from` may legally move to right now. Honours the mandatory
    /// capture rule (checked across all of the side's pieces) and, during a
    /// multi-jump, answers only for the chained piece.
    pub fn legal_targets(&self, from: usize) -> Vec<usize> {
        if self.status != Status::Ongoing {
            return Vec::new();
        }
        if let Some(chain) = self.chain_from {
            return if from == chain {
                self.capture_targets(chain)
            } else {
                Vec::new()
            };
        }
        let Some(pc) = self.board[from] else {
            return Vec::new();
        };
        if pc.side != self.turn {
            return Vec::new();
        }
        let captures = self.capture_targets(from);
        if !captures.is_empty() {
            return captures;
        }
        if self.any_capture(self.turn) {
            return Vec::new();
        }
        self.step_targets(from)
    }

    /// Execute one move step (a plain step or a single jump). Returns false
    /// for anything [`Checkers::legal_targets`] would not offer.
    pub fn play(&mut self, from: usize, to: usize) -> bool {
        if !self.legal_targets(from).contains(&to) {
            return false;
        }
        let Some(mut pc) = self.board[from] else {
            return false;
        };

        let (fr, fc) = ((from / SIZE) as isize, (from % SIZE) as isize);
        let (tr, tc) = ((to / SIZE) as isize, (to % SIZE) as isize);
        let is_jump = fr.abs_diff(tr) == 2 && fc.abs_diff(tc) == 2;
        if is_jump {
            let mid = ((fr + tr) / 2 * SIZE as isize + (fc + tc) / 2) as usize;
            self.board[mid] = None;
        }
        self.board[from] = None;
        self.board[to] = Some(pc);
        self.last_move = Some((from, to));

        // Crowning ends the turn at once, even if further jumps would exist.
        let crown_row = match pc.side {
            Side::White => 0,
            Side::Black => SIZE - 1,
        };
        if !pc.king && to / SIZE == crown_row {
            pc.king = true;
            self.board[to] = Some(pc);
            self.chain_from = None;
            self.advance_turn();
            return true;
        }

        if is_jump && !self.capture_targets(to).is_empty() {
            // The same piece must keep jumping; the turn does not switch.
            self.chain_from = Some(to);
        } else {
            self.chain_from = None;
            self.advance_turn();
        }
        true
    }

    fn advance_turn(&mut self) {
        self.turn = self.turn.opposite();
        if self.status == Status::Ongoing && !self.side_has_moves(self.turn) {
            self.status = Status::Won(self.turn.opposite());
        }
    }

    /// Empty diagonal neighbours this piece could step to.
    fn step_targets(&self, sq: usize) -> Vec<usize> {
        let Some(pc) = self.board[sq] else {
            return Vec::new();
        };
        let (r, c) = ((sq / SIZE) as isize, (sq % SIZE) as isize);
        let mut out = Vec::new();
        for &(dr, dc) in dirs(pc.king, pc.side) {
            let (nr, nc) = (r + dr, c + dc);
            if !(0..SIZE as isize).contains(&nr) || !(0..SIZE as isize).contains(&nc) {
                continue;
            }
            let to = (nr * SIZE as isize + nc) as usize;
            if self.board[to].is_none() {
                out.push(to);
            }
        }
        out
    }

    /// Landing squares of legal jumps over adjacent enemies.
    fn capture_targets(&self, sq: usize) -> Vec<usize> {
        let Some(pc) = self.board[sq] else {
            return Vec::new();
        };
        let (r, c) = ((sq / SIZE) as isize, (sq % SIZE) as isize);
        let mut out = Vec::new();
        for &(dr, dc) in dirs(pc.king, pc.side) {
            let (mr, mc) = (r + dr, c + dc);
            let (lr, lc) = (r + 2 * dr, c + 2 * dc);
            let inside = |v: isize| (0..SIZE as isize).contains(&v);
            if !inside(mr) || !inside(mc) || !inside(lr) || !inside(lc) {
                continue;
            }
            let mid = (mr * SIZE as isize + mc) as usize;
            let land = (lr * SIZE as isize + lc) as usize;
            if let Some(victim) = self.board[mid]
                && victim.side == pc.side.opposite()
                && self.board[land].is_none()
            {
                out.push(land);
            }
        }
        out
    }

    fn any_capture(&self, side: Side) -> bool {
        (0..SIZE * SIZE).any(|sq| {
            matches!(self.board[sq], Some(pc) if pc.side == side)
                && !self.capture_targets(sq).is_empty()
        })
    }

    /// True when `side` has at least one legal move (captures first, as they
    /// are mandatory). Also false when the side has no pieces at all.
    fn side_has_moves(&self, side: Side) -> bool {
        if self.any_capture(side) {
            return true;
        }
        (0..SIZE * SIZE).any(|sq| {
            matches!(self.board[sq], Some(pc) if pc.side == side)
                && !self.step_targets(sq).is_empty()
        })
    }
}

impl Default for Checkers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(r: usize, c: usize) -> usize {
        r * SIZE + c
    }

    fn man(side: Side) -> Option<PC> {
        Some(PC { side, king: false })
    }

    fn king(side: Side) -> Option<PC> {
        Some(PC { side, king: true })
    }

    /// Sparse test position: listed pieces on dark squares, empty elsewhere.
    fn pos(turn: Side, pieces: &[(usize, Side, bool)]) -> Checkers {
        Checkers::test_position(turn, pieces)
    }

    #[test]
    fn initial_counts_12_v_12() {
        let g = Checkers::new();
        assert_eq!(g.counts(), (12, 12));
        assert_eq!(g.turn(), Side::White);
        assert_eq!(g.status(), Status::Ongoing);
        assert_eq!(g.chain_from(), None);
        assert_eq!(g.piece_at(sq(0, 1)), man(Side::Black));
        assert_eq!(g.piece_at(sq(7, 6)), man(Side::White));
        assert_eq!(g.piece_at(sq(0, 0)), None, "light square is never played");
    }

    #[test]
    fn plain_move_refused_while_any_jump_exists() {
        // White man at (5,6) has no capture of its own, but (5,2) does —
        // the mandatory-capture rule is global across the side.
        let mut g = pos(
            Side::White,
            &[
                (sq(5, 2), Side::White, false),
                (sq(5, 6), Side::White, false),
                (sq(4, 3), Side::Black, false),
            ],
        );
        assert!(g.legal_targets(sq(5, 6)).is_empty());
        assert!(!g.play(sq(5, 6), sq(4, 5)));
        assert!(!g.play(sq(5, 2), sq(4, 1)));
        assert_eq!(g.piece_at(sq(5, 2)), man(Side::White), "board untouched");
        assert_eq!(g.turn(), Side::White);
        assert!(g.play(sq(5, 2), sq(3, 4)), "the forced jump goes through");
    }

    #[test]
    fn multi_jump_chain_keeps_turn_until_exhausted() {
        // White man at (5,2) jumps (4,3) landing (3,4), then (2,3) landing (1,2).
        let mut g = pos(
            Side::White,
            &[
                (sq(5, 2), Side::White, false),
                (sq(4, 3), Side::Black, false),
                (sq(2, 3), Side::Black, false),
            ],
        );
        assert_eq!(g.legal_targets(sq(5, 2)), vec![sq(3, 4)]);
        assert!(g.play(sq(5, 2), sq(3, 4)));
        assert_eq!(g.piece_at(sq(4, 3)), None, "first victim removed");
        assert_eq!(g.turn(), Side::White, "chain keeps the turn");
        assert_eq!(g.chain_from(), Some(sq(3, 4)));
        assert_eq!(g.last_move(), Some((sq(5, 2), sq(3, 4))));

        assert_eq!(g.legal_targets(sq(3, 4)), vec![sq(1, 2)]);
        assert!(g.play(sq(3, 4), sq(1, 2)));
        assert_eq!(g.piece_at(sq(2, 3)), None, "second victim removed");
        assert_eq!(g.turn(), Side::Black, "chain done, turn flips");
        assert_eq!(g.chain_from(), None);
        assert_eq!(g.counts(), (1, 0));
        assert_eq!(g.status(), Status::Won(Side::White));
    }

    #[test]
    fn crowning_ends_turn_even_if_more_jumps_exist() {
        // White man (2,3) jumps (1,4) to (0,5) and crowns; a king on (0,5)
        // could still jump (1,6) to (2,7), but the turn must end regardless.
        let mut g = pos(
            Side::White,
            &[
                (sq(2, 3), Side::White, false),
                (sq(1, 4), Side::Black, false),
                (sq(1, 6), Side::Black, false),
            ],
        );
        assert_eq!(g.legal_targets(sq(2, 3)), vec![sq(0, 5)]);
        assert!(g.play(sq(2, 3), sq(0, 5)));
        assert_eq!(
            g.piece_at(sq(0, 5)),
            king(Side::White),
            "crowned on the last row"
        );
        assert_eq!(g.turn(), Side::Black);
        assert_eq!(g.chain_from(), None);
        assert_eq!(g.status(), Status::Ongoing);
    }

    #[test]
    fn kings_step_and_capture_backward() {
        // Plain backward step when no captures exist anywhere.
        let mut g = pos(
            Side::White,
            &[
                (sq(4, 5), Side::White, true),
                (sq(0, 1), Side::Black, false),
            ],
        );
        let targets = g.legal_targets(sq(4, 5));
        assert!(targets.contains(&sq(5, 4)) && targets.contains(&sq(5, 6)));
        assert!(g.play(sq(4, 5), sq(5, 6)));
        assert_eq!(g.turn(), Side::Black);

        // Backward capture is forced over forward steps.
        let mut g = pos(
            Side::White,
            &[
                (sq(4, 5), Side::White, true),
                (sq(5, 4), Side::Black, false),
            ],
        );
        assert_eq!(g.legal_targets(sq(4, 5)), vec![sq(6, 3)]);
        assert!(g.play(sq(4, 5), sq(6, 3)));
        assert_eq!(g.piece_at(sq(5, 4)), None, "victim removed");
        assert_eq!(g.turn(), Side::Black);
    }

    #[test]
    fn men_cannot_move_backward() {
        let mut g = pos(
            Side::White,
            &[
                (sq(4, 5), Side::White, false),
                (sq(0, 1), Side::Black, false),
            ],
        );
        assert_eq!(g.legal_targets(sq(4, 5)), vec![sq(3, 4), sq(3, 6)]);
        assert!(!g.play(sq(4, 5), sq(5, 4)));
        assert!(!g.play(sq(4, 5), sq(5, 6)));
        assert!(g.play(sq(4, 5), sq(3, 4)), "forward step still works");
    }

    #[test]
    fn no_legal_moves_loses() {
        // Black man at (0,7) is boxed in: its step (1,6) and jump landing
        // (2,5) are both occupied by white men.
        let mut g = pos(
            Side::White,
            &[
                (sq(0, 7), Side::Black, false),
                (sq(1, 6), Side::White, false),
                (sq(2, 5), Side::White, false),
                (sq(5, 2), Side::White, false),
            ],
        );
        assert!(g.play(sq(5, 2), sq(4, 3)));
        assert_eq!(g.counts(), (3, 1), "lost on mobility, not material");
        assert_eq!(g.status(), Status::Won(Side::White));
        assert!(g.legal_targets(sq(0, 7)).is_empty());
        assert!(!g.play(sq(0, 7), sq(1, 6)));
    }
}
