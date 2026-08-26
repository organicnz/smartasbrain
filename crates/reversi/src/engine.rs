//! Pure reversi rules: placement, line flips, auto-pass, final scoring.
//!
//! Squares are indexed `r * 8 + c` with `(0, 0)` the top-left corner (d3-style
//! algebraic would be row 2, column 3). Black moves first; a side that has no
//! legal move is skipped automatically, and when neither side can move — or
//! the board is full — the game ends by majority or draw.

/// Disc colour; the board only stores which side owns each square.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Black,
    White,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
        }
    }

    /// Lowercase name for UI text ("black" / "white").
    pub fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::White => "white",
        }
    }
}

/// Terminal states: majority winner or a tied board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    Won(Side),
    Draw,
}

/// Square index from row/column: `r * 8 + c`.
pub const fn sq(r: usize, c: usize) -> usize {
    r * 8 + c
}

/// Row of a square index, 0 = top of the board.
pub const fn row(i: usize) -> usize {
    i / 8
}

/// Column of a square index, 0 = left of the board.
pub const fn col(i: usize) -> usize {
    i % 8
}

/// The eight ray directions used for flip detection.
const DIRS: [(isize, isize); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
];

/// Squares that would be flipped if `side` played `idx` on `board`; empty
/// when the square is occupied or brackets no opposing run.
fn flips_for(board: &[Option<Side>; 64], idx: usize, side: Side) -> Vec<usize> {
    let mut out = Vec::new();
    if board.get(idx).is_none() || board[idx].is_some() {
        return out;
    }
    let (r, c) = (row(idx) as isize, col(idx) as isize);
    let foe = side.opposite();
    for &(dr, dc) in &DIRS {
        let mut rr = r + dr;
        let mut cc = c + dc;
        let mut run = Vec::new();
        while (0..8).contains(&rr)
            && (0..8).contains(&cc)
            && board[sq(rr as usize, cc as usize)] == Some(foe)
        {
            run.push(sq(rr as usize, cc as usize));
            rr += dr;
            cc += dc;
        }
        if !run.is_empty()
            && (0..8).contains(&rr)
            && (0..8).contains(&cc)
            && board[sq(rr as usize, cc as usize)] == Some(side)
        {
            out.extend(run);
        }
    }
    out
}

#[derive(Clone)]
pub struct Reversi {
    board: [Option<Side>; 64],
    turn: Side,
    status: Status,
    last_move: Option<usize>,
    flipped_last: Vec<usize>,
    /// True when the current side to move arrived via an automatic pass.
    passed_last: bool,
}

impl Reversi {
    pub fn new() -> Self {
        let mut board = [None; 64];
        // Standard centre: d4/e5 white, e4/d5 black (rows 3-4, cols 3-4).
        board[sq(3, 3)] = Some(Side::White);
        board[sq(4, 4)] = Some(Side::White);
        board[sq(3, 4)] = Some(Side::Black);
        board[sq(4, 3)] = Some(Side::Black);
        Self {
            board,
            turn: Side::Black,
            status: Status::Ongoing,
            last_move: None,
            flipped_last: Vec::new(),
            passed_last: false,
        }
    }

    /// Test constructor: place discs directly, then run the same
    /// pass/settle logic as a completed move so mid-game positions work.
    #[cfg(test)]
    pub(crate) fn set_board(pieces: &[((usize, usize), Side)], turn: Side) -> Self {
        let mut state = Self {
            board: [None; 64],
            turn,
            status: Status::Ongoing,
            last_move: None,
            flipped_last: Vec::new(),
            passed_last: false,
        };
        for &((r, c), side) in pieces {
            state.board[sq(r, c)] = Some(side);
        }
        state.settle();
        state
    }

    pub fn board(&self) -> &[Option<Side>; 64] {
        &self.board
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// Occupied-square counts as `(black, white)`.
    pub fn score(&self) -> (usize, usize) {
        self.board.iter().fold((0, 0), |(b, w), cell| match cell {
            Some(Side::Black) => (b + 1, w),
            Some(Side::White) => (b, w + 1),
            None => (b, w),
        })
    }

    pub fn last_move(&self) -> Option<usize> {
        self.last_move
    }

    /// Squares flipped by the most recent play, in walk order.
    pub fn flipped_last(&self) -> &[usize] {
        &self.flipped_last
    }

    /// True when the side to move was auto-passed into position.
    pub fn passed_last(&self) -> bool {
        self.passed_last
    }

    /// Empty squares where placing a disc would bracket at least one line.
    pub fn legal_moves(&self) -> Vec<usize> {
        if self.status != Status::Ongoing {
            return Vec::new();
        }
        (0..64)
            .filter(|&i| !flips_for(&self.board, i, self.turn).is_empty())
            .collect()
    }

    /// Apply a move iff it is legal: place the disc, flip the bracketed
    /// lines, then hand over — auto-passing or ending the game as needed.
    /// Returns false when rejected.
    pub fn play(&mut self, idx: usize) -> bool {
        if self.status != Status::Ongoing {
            return false;
        }
        let flips = flips_for(&self.board, idx, self.turn);
        if flips.is_empty() {
            return false;
        }
        self.board[idx] = Some(self.turn);
        for &f in &flips {
            self.board[f] = Some(self.turn);
        }
        self.last_move = Some(idx);
        self.flipped_last = flips;
        self.passed_last = false;
        self.turn = self.turn.opposite();
        self.settle();
        true
    }

    /// After a handover: skip a stranded side, end on double-pass/full board.
    fn settle(&mut self) {
        if self.board.iter().all(Option::is_some) {
            self.finish();
            return;
        }
        if !self.legal_moves().is_empty() {
            return; // incoming side can move normally
        }
        let mover = self.turn.opposite();
        self.turn = mover;
        if self.legal_moves().is_empty() {
            self.finish(); // neither side can move
        } else {
            self.passed_last = true;
        }
    }

    /// Record the result from current disc counts.
    fn finish(&mut self) {
        let (b, w) = self.score();
        self.status = match b.cmp(&w) {
            std::cmp::Ordering::Greater => Status::Won(Side::Black),
            std::cmp::Ordering::Less => Status::Won(Side::White),
            std::cmp::Ordering::Equal => Status::Draw,
        };
    }
}

impl Default for Reversi {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_four_legal_squares() {
        let state = Reversi::new();
        let mut moves = state.legal_moves();
        moves.sort_unstable();
        assert_eq!(moves, vec![sq(2, 3), sq(3, 2), sq(4, 5), sq(5, 4)]);
        assert_eq!(state.score(), (2, 2));
        assert_eq!(state.turn(), Side::Black);
    }

    #[test]
    fn play_flips_known_lines_with_counts() {
        let mut state = Reversi::new();
        // Black d3 flips exactly d4.
        assert!(state.play(sq(2, 3)));
        assert_eq!(state.flipped_last(), &[sq(3, 3)]);
        assert_eq!(state.score(), (4, 1));
        assert_eq!(state.turn(), Side::White);
        // White c5 flips exactly d5.
        assert!(state.play(sq(4, 2)));
        assert_eq!(state.flipped_last(), &[sq(4, 3)]);
        // Black c6 flips exactly d5 back.
        assert!(state.play(sq(5, 2)));
        assert_eq!(state.flipped_last(), &[sq(4, 3)]);
        // White e2 brackets two black lines at once.
        assert!(state.play(sq(2, 4)));
        assert_eq!(state.flipped_last().len(), 2);
        assert!(state.flipped_last().contains(&sq(3, 3)));
        assert!(state.flipped_last().contains(&sq(3, 4)));
        assert_eq!(state.score(), (3, 5));
    }

    #[test]
    fn rejects_occupied_and_non_flipping_squares() {
        let mut state = Reversi::new();
        assert!(!state.play(sq(3, 3)), "occupied");
        assert!(!state.play(sq(0, 0)), "no line to flip");
        assert!(!state.play(64), "off-board index");
        assert_eq!(state.last_move(), None);
        assert_eq!(state.turn(), Side::Black);
    }

    #[test]
    fn corner_capture_scenario() {
        let mut state = Reversi::set_board(
            &[
                ((7, 4), Side::Black),
                ((7, 5), Side::White),
                ((7, 6), Side::White),
                ((4, 7), Side::Black),
                ((5, 7), Side::White),
                ((6, 7), Side::White),
            ],
            Side::Black,
        );
        assert!(state.legal_moves().contains(&sq(7, 7)));
        assert!(state.play(sq(7, 7)));
        assert_eq!(state.board()[sq(7, 7)], Some(Side::Black));
        // Two on the rank plus two on the file collapse at once.
        assert_eq!(state.flipped_last().len(), 4);
        for idx in [sq(7, 5), sq(7, 6), sq(5, 7), sq(6, 7)] {
            assert_eq!(state.board()[idx], Some(Side::Black), "flipped {idx}");
        }
        assert_eq!(state.score(), (7, 0));
        assert_eq!(state.last_move(), Some(sq(7, 7)));
    }

    #[test]
    fn auto_pass_hands_turn_back() {
        // White holds a5/c8-corner discs with no flipping line anywhere;
        // after black extends to a6 white is stranded while black can still
        // reach c8-region lines, so control must bounce straight back.
        let mut state = Reversi::set_board(
            &[
                ((0, 0), Side::Black),
                ((0, 1), Side::Black),
                ((0, 2), Side::Black),
                ((0, 3), Side::Black),
                ((0, 4), Side::White),
                ((0, 6), Side::White),
                ((7, 4), Side::Black),
                ((7, 7), Side::White),
            ],
            Side::Black,
        );
        assert_eq!(state.status(), Status::Ongoing);
        assert!(state.play(sq(0, 5)), "a6 flips a5");
        assert_eq!(
            state.turn(),
            Side::Black,
            "white had no reply so control returns to black"
        );
        assert!(state.passed_last());
        assert_eq!(state.status(), Status::Ongoing);
        assert!(!state.legal_moves().is_empty(), "black can continue");
    }

    #[test]
    fn double_pass_ends_game_with_majority_winner() {
        // After black takes h8 white owns no discs at all, so neither side
        // can ever move again: immediate win for black.
        let mut state = Reversi::set_board(
            &[
                ((7, 4), Side::Black),
                ((7, 5), Side::White),
                ((7, 6), Side::White),
            ],
            Side::Black,
        );
        assert!(state.play(sq(7, 7)));
        assert_eq!(state.status(), Status::Won(Side::Black));
        assert_eq!(state.score(), (4, 0));
        assert!(state.legal_moves().is_empty());
        assert!(!state.play(sq(0, 0)), "finished game rejects input");
    }

    #[test]
    fn full_board_draw_detection() {
        let mut pieces = Vec::with_capacity(64);
        for r in 0..8usize {
            for c in 0..8usize {
                let side = if (r + c) % 2 == 0 {
                    Side::Black
                } else {
                    Side::White
                };
                pieces.push(((r, c), side));
            }
        }
        let state = Reversi::set_board(&pieces, Side::Black);
        assert_eq!(state.status(), Status::Draw);
        assert_eq!(state.score(), (32, 32));

        // Same full board but one extra black disc tips the majority.
        pieces[0].1 = Side::Black;
        pieces[1].1 = Side::Black;
        let state = Reversi::set_board(&pieces, Side::White);
        assert_eq!(state.status(), Status::Won(Side::Black));
    }
}
