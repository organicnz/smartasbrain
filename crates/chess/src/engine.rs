//! Pure chess rules: movement, pins, castling, en passant, promotion, mate.
//!
//! Squares are indexed `r * 8 + c` with `(0, 0)` the top-left corner (a8,
//! black's back rank). White pawns advance toward decreasing `r`.

/// Piece color. White starts on rows 7-6 and pawns advance toward row 0.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

    /// Row delta a pawn of this side advances by.
    fn pawn_dir(self) -> isize {
        match self {
            Self::White => -1,
            Self::Black => 1,
        }
    }

    /// Home row of this side's king/rooks.
    fn back_rank(self) -> usize {
        match self {
            Self::White => 7,
            Self::Black => 0,
        }
    }

    /// Row a pawn of this side starts on, eligible for a double push.
    fn pawn_start(self) -> usize {
        match self {
            Self::White => 6,
            Self::Black => 1,
        }
    }

    /// Lowercase name for UI text ("white" / "black").
    pub fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
        }
    }
}

/// What shape a piece is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub side: Side,
    pub kind: Kind,
}

impl Piece {
    const fn new(side: Side, kind: Kind) -> Self {
        Self { side, kind }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    Won(Side),
    Stalemate,
}

/// Square index from row/column: `r * 8 + c`.
pub const fn sq(r: usize, c: usize) -> usize {
    r * 8 + c
}

/// Row (rank) of a square index, 0 = top of the board.
pub const fn row(i: usize) -> usize {
    i / 8
}

/// Column (file) of a square index, 0 = left of the board.
pub const fn col(i: usize) -> usize {
    i % 8
}

const KNIGHT_JUMPS: [(isize, isize); 8] = [
    (-2, -1),
    (-2, 1),
    (-1, -2),
    (-1, 2),
    (1, -2),
    (1, 2),
    (2, -1),
    (2, 1),
];

const ORTHO_DIRS: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
const DIAG_DIRS: [(isize, isize); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];
const ALL_DIRS: [(isize, isize); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
];

/// Castling-rights slots, indexed by these constants:
/// white kingside, white queenside, black kingside, black queenside.
mod rights {
    use super::{Side, sq};
    pub const WHITE_KINGSIDE: usize = 0;
    pub const WHITE_QUEENSIDE: usize = 1;
    pub const BLACK_KINGSIDE: usize = 2;
    pub const BLACK_QUEENSIDE: usize = 3;

    pub const fn side_kingside(side: Side) -> usize {
        match side {
            Side::White => WHITE_KINGSIDE,
            Side::Black => BLACK_KINGSIDE,
        }
    }

    pub const fn side_queenside(side: Side) -> usize {
        match side {
            Side::White => WHITE_QUEENSIDE,
            Side::Black => BLACK_QUEENSIDE,
        }
    }

    /// Rook home squares paired with the right each one guards.
    pub const CORNERS: [(usize, usize); 4] = [
        (sq(7, 7), WHITE_KINGSIDE),
        (sq(7, 0), WHITE_QUEENSIDE),
        (sq(0, 7), BLACK_KINGSIDE),
        (sq(0, 0), BLACK_QUEENSIDE),
    ];
}

fn piece_at(board: &[Option<Piece>; 64], r: isize, c: isize) -> Option<Piece> {
    if !(0..8).contains(&r) || !(0..8).contains(&c) {
        return None;
    }
    board[sq(r as usize, c as usize)]
}

/// True when `target` is attacked by any piece of side `by`.
fn is_attacked(board: &[Option<Piece>; 64], target: usize, by: Side) -> bool {
    let (tr, tc) = (row(target) as isize, col(target) as isize);

    // Pawn attackers sit one step back along their push direction.
    let pr = tr - by.pawn_dir();
    for dc in [-1isize, 1] {
        if piece_at(board, pr, tc + dc) == Some(Piece::new(by, Kind::Pawn)) {
            return true;
        }
    }
    for (dr, dc) in KNIGHT_JUMPS {
        if piece_at(board, tr + dr, tc + dc) == Some(Piece::new(by, Kind::Knight)) {
            return true;
        }
    }
    for (dr, dc) in ALL_DIRS {
        if piece_at(board, tr + dr, tc + dc) == Some(Piece::new(by, Kind::King)) {
            return true;
        }
    }

    slide_attacks(board, tr, tc, by, &ORTHO_DIRS, [Kind::Rook])
        || slide_attacks(board, tr, tc, by, &DIAG_DIRS, [Kind::Bishop])
}

/// Walk rays from `(tr, tc)`; the first blocker on each ray stops it, and
/// counts as an attack when it is a `by`-side slider matching `kinds`
/// (queens always count).
fn slide_attacks(
    board: &[Option<Piece>; 64],
    tr: isize,
    tc: isize,
    by: Side,
    dirs: &[(isize, isize)],
    kinds: [Kind; 1],
) -> bool {
    for &(dr, dc) in dirs {
        let mut r = tr + dr;
        let mut c = tc + dc;
        while (0..8).contains(&r) && (0..8).contains(&c) {
            if let Some(p) = board[sq(r as usize, c as usize)] {
                if p.side == by && (p.kind == kinds[0] || p.kind == Kind::Queen) {
                    return true;
                }
                break; // any blocker stops the ray
            }
            r += dr;
            c += dc;
        }
    }
    false
}

#[derive(Clone)]
pub struct Chess {
    board: [Option<Piece>; 64],
    turn: Side,
    /// See [`rights`] slot constants; order WK, WQ, BK, BQ.
    castling: [bool; 4],
    en_passant: Option<usize>,
    last_move: Option<(usize, usize)>,
    status: Status,
}

impl Chess {
    pub fn new() -> Self {
        let mut board = [None; 64];
        const BACK: [Kind; 8] = [
            Kind::Rook,
            Kind::Knight,
            Kind::Bishop,
            Kind::Queen,
            Kind::King,
            Kind::Bishop,
            Kind::Knight,
            Kind::Rook,
        ];
        for c in 0..8usize {
            board[sq(0, c)] = Some(Piece::new(Side::Black, BACK[c]));
            board[sq(1, c)] = Some(Piece::new(Side::Black, Kind::Pawn));
            board[sq(6, c)] = Some(Piece::new(Side::White, Kind::Pawn));
            board[sq(7, c)] = Some(Piece::new(Side::White, BACK[c]));
        }
        Self {
            board,
            turn: Side::White,
            castling: [true; 4],
            en_passant: None,
            last_move: None,
            status: Status::Ongoing,
        }
    }

    /// Test constructor: place pieces directly on an otherwise empty board,
    /// then refresh status/legality as if the position were mid-game.
    #[cfg(test)]
    pub(crate) fn set_board(pieces: &[((usize, usize), Piece)], turn: Side) -> Self {
        let mut state = Self {
            board: [None; 64],
            turn,
            castling: [true; 4],
            en_passant: None,
            last_move: None,
            status: Status::Ongoing,
        };
        for &((r, c), piece) in pieces {
            state.board[sq(r, c)] = Some(piece);
        }
        state.refresh_status();
        state
    }

    pub fn board(&self) -> &[Option<Piece>; 64] {
        &self.board
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn last_move(&self) -> Option<(usize, usize)> {
        self.last_move
    }

    /// True when the side to move is in check.
    pub fn in_check(&self) -> bool {
        self.in_check_for(self.turn)
    }

    fn in_check_for(&self, side: Side) -> bool {
        self.find_king(side)
            .is_some_and(|k| is_attacked(&self.board, k, side.opposite()))
    }

    fn find_king(&self, side: Side) -> Option<usize> {
        let want = Piece::new(side, Kind::King);
        self.board.iter().position(|&p| p == Some(want))
    }

    /// Every legal destination from `from`. Empty unless `from` holds a piece
    /// of the side to move and the game is still live.
    pub fn legal_targets(&self, from: usize) -> Vec<usize> {
        if self.status != Status::Ongoing || from >= 64 {
            return Vec::new();
        }
        if self.board[from].is_none_or(|p| p.side != self.turn) {
            return Vec::new();
        }
        self.pseudo_targets(from)
            .into_iter()
            .filter(|&to| self.leaves_king_safe(from, to))
            .collect()
    }

    /// Apply a move iff it is legal; toggles the turn and recomputes status.
    /// Returns false when the move is rejected.
    pub fn play(&mut self, from: usize, to: usize) -> bool {
        if !self.legal_targets(from).contains(&to) {
            return false;
        }
        self.apply_move(from, to);
        self.refresh_status();
        true
    }

    /// True when making `from`->`to` would not leave the mover's king attacked.
    fn leaves_king_safe(&self, from: usize, to: usize) -> bool {
        let mut probe = self.clone();
        probe.apply_move(from, to);
        !probe.in_check_for(self.turn)
    }

    /// Move generation before the clone-filter legality check.
    fn pseudo_targets(&self, from: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let Some(piece) = self.board[from] else {
            return out;
        };
        let (r, c) = (row(from) as isize, col(from) as isize);

        // Land on empty or enemy squares inside the board.
        let try_add =
            |out: &mut Vec<usize>, rr: isize, cc: isize| match piece_at(&self.board, rr, cc) {
                Some(p) if p.side != piece.side => out.push(sq(rr as usize, cc as usize)),
                Some(_) => {}
                None if (0..8).contains(&rr) && (0..8).contains(&cc) => {
                    out.push(sq(rr as usize, cc as usize));
                }
                None => {}
            };

        match piece.kind {
            Kind::Pawn => {
                let dir = piece.side.pawn_dir();
                let start = piece.side.pawn_start() as isize;
                let fwd = r + dir;
                if (0..8).contains(&fwd) && self.board[sq(fwd as usize, c as usize)].is_none() {
                    out.push(sq(fwd as usize, c as usize));
                    let dbl = fwd + dir;
                    if r == start
                        && (0..8).contains(&dbl)
                        && self.board[sq(dbl as usize, c as usize)].is_none()
                    {
                        out.push(sq(dbl as usize, c as usize));
                    }
                }
                for dc in [-1isize, 1] {
                    let cc = c + dc;
                    if !(0..8).contains(&cc) || !(0..8).contains(&fwd) {
                        continue;
                    }
                    let t = sq(fwd as usize, cc as usize);
                    let enemy_there =
                        self.board[t].is_some_and(|p| p.side == piece.side.opposite());
                    if enemy_there || self.en_passant == Some(t) {
                        out.push(t);
                    }
                }
            }
            Kind::Knight | Kind::King => {
                let jumps: &[(isize, isize)] = match piece.kind {
                    Kind::King => &ALL_DIRS,
                    _ => &KNIGHT_JUMPS,
                };
                for &(dr, dc) in jumps {
                    try_add(&mut out, r + dr, c + dc);
                }
                if piece.kind == Kind::King {
                    self.try_add_castles(piece.side, r, c, &mut out);
                }
            }
            Kind::Bishop | Kind::Rook | Kind::Queen => {
                let dirs: &[(isize, isize)] = match piece.kind {
                    Kind::Bishop => &DIAG_DIRS,
                    Kind::Rook => &ORTHO_DIRS,
                    _ => &ALL_DIRS,
                };
                for &(dr, dc) in dirs {
                    let mut rr = r + dr;
                    let mut rc = c + dc;
                    while (0..8).contains(&rr) && (0..8).contains(&rc) {
                        let t = sq(rr as usize, rc as usize);
                        match self.board[t] {
                            None => out.push(t),
                            Some(p) => {
                                if p.side != piece.side {
                                    out.push(t);
                                }
                                break;
                            }
                        }
                        rr += dr;
                        rc += dc;
                    }
                }
            }
        }
        out
    }

    /// Castling destinations for a king standing on its home e-file square.
    /// Requires rights intact, path empty, no current check, and unattacked
    /// transit squares (the destination is also a transit square).
    fn try_add_castles(&self, side: Side, r: isize, c: isize, out: &mut Vec<usize>) {
        let enemy = side.opposite();
        let home = side.back_rank() as isize;
        if r != home || c != 4 || is_attacked(&self.board, sq(home as usize, 4), enemy) {
            return;
        }
        let ks = rights::side_kingside(side);
        let qs = rights::side_queenside(side);
        if self.castling[ks]
            && self.board[sq(home as usize, 7)] == Some(Piece::new(side, Kind::Rook))
            && self.board[sq(home as usize, 5)].is_none()
            && self.board[sq(home as usize, 6)].is_none()
            && !is_attacked(&self.board, sq(home as usize, 5), enemy)
            && !is_attacked(&self.board, sq(home as usize, 6), enemy)
        {
            out.push(sq(home as usize, 6));
        }
        if self.castling[qs]
            && self.board[sq(home as usize, 0)] == Some(Piece::new(side, Kind::Rook))
            && self.board[sq(home as usize, 3)].is_none()
            && self.board[sq(home as usize, 2)].is_none()
            && self.board[sq(home as usize, 1)].is_none()
            && !is_attacked(&self.board, sq(home as usize, 3), enemy)
            && !is_attacked(&self.board, sq(home as usize, 2), enemy)
        {
            out.push(sq(home as usize, 2));
        }
    }

    /// Apply an already-validated move: captures, en passant removal, castle
    /// rook shuffle, promotion, rights bookkeeping, ep target, turn toggle.
    fn apply_move(&mut self, from: usize, to: usize) {
        let Some(piece) = self.board[from] else {
            return;
        };
        self.board[to] = Some(piece);
        self.board[from] = None;

        // En passant removes the bypassed pawn rather than anything on `to`.
        if piece.kind == Kind::Pawn && col(from) != col(to) && self.en_passant == Some(to) {
            self.board[sq(row(from), col(to))] = None;
        }

        // Castling shuffles the rook over as well.
        if piece.kind == Kind::King && col(to).abs_diff(col(from)) == 2 {
            let r = row(from);
            let (rook_from, rook_to) = if col(to) == 6 {
                (sq(r, 7), sq(r, 5))
            } else {
                (sq(r, 0), sq(r, 3))
            };
            self.board[rook_to] = self.board[rook_from];
            self.board[rook_from] = None;
        }

        // Rights vanish once the king or a relevant rook moves or is captured.
        if piece.kind == Kind::King {
            self.castling[rights::side_kingside(piece.side)] = false;
            self.castling[rights::side_queenside(piece.side)] = false;
        }
        for &(corner, right) in &rights::CORNERS {
            if from == corner || to == corner {
                self.castling[right] = false;
            }
        }

        // Double pushes hand the opponent an en passant target.
        self.en_passant = if piece.kind == Kind::Pawn && row(from).abs_diff(row(to)) == 2 {
            Some((from + to) / 2)
        } else {
            None
        };

        self.last_move = Some((from, to));

        // Pawns reaching their final rank auto-promote to queens.
        let promo_row = match piece.side {
            Side::White => 0,
            Side::Black => 7,
        };
        if piece.kind == Kind::Pawn && row(to) == promo_row {
            self.board[to] = Some(Piece::new(piece.side, Kind::Queen));
        }

        self.turn = self.turn.opposite();
    }

    /// Recompute game-over state for the side to move.
    fn refresh_status(&mut self) {
        let any_legal = (0..64).any(|i| !self.legal_targets(i).is_empty());
        self.status = if any_legal {
            Status::Ongoing
        } else if self.in_check() {
            Status::Won(self.turn.opposite())
        } else {
            Status::Stalemate
        };
    }
}

impl Default for Chess {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc(side: Side, kind: Kind) -> Piece {
        Piece { side, kind }
    }

    /// Coordinate-flavored play helper: rows count from the top (row 6 = rank 2).
    fn mv(state: &mut Chess, from: (usize, usize), to: (usize, usize)) -> bool {
        state.play(sq(from.0, from.1), sq(to.0, to.1))
    }

    #[test]
    fn initial_position_has_20_legal_moves() {
        let state = Chess::new();
        let total: usize = (0..64).map(|i| state.legal_targets(i).len()).sum();
        assert_eq!(total, 20);
    }

    #[test]
    fn rejects_moves_that_are_not_generated() {
        let mut state = Chess::new();
        // King cannot leap two ranks.
        assert!(!mv(&mut state, (7, 4), (5, 4)));
        // Not white's turn to move the e7 pawn.
        assert!(!mv(&mut state, (1, 4), (2, 4)));
    }

    #[test]
    fn fools_mate_ends_won_black() {
        let mut state = Chess::new();
        assert!(mv(&mut state, (6, 5), (5, 5))); // f2f3
        assert!(mv(&mut state, (1, 4), (3, 4))); // e7e5
        assert!(mv(&mut state, (6, 6), (4, 6))); // g2g4
        assert!(mv(&mut state, (0, 3), (4, 7))); // d8h4#
        assert_eq!(state.status(), Status::Won(Side::Black));
        assert!(state.in_check());
    }

    #[test]
    fn castles_kingside_when_clear() {
        let mut state = Chess::set_board(
            &[
                ((7, 4), pc(Side::White, Kind::King)),
                ((7, 7), pc(Side::White, Kind::Rook)),
                ((0, 4), pc(Side::Black, Kind::King)),
            ],
            Side::White,
        );
        assert!(state.legal_targets(sq(7, 4)).contains(&sq(7, 6)));
        assert!(mv(&mut state, (7, 4), (7, 6)));
        assert_eq!(state.board[sq(7, 6)], Some(pc(Side::White, Kind::King)));
        assert_eq!(state.board[sq(7, 5)], Some(pc(Side::White, Kind::Rook)));
        assert_eq!(state.board[sq(7, 7)], None);
        assert!(!state.castling[rights::WHITE_KINGSIDE]);
        assert_eq!(state.last_move(), Some((sq(7, 4), sq(7, 6))));
    }

    #[test]
    fn castling_refused_when_path_blocked() {
        let state = Chess::set_board(
            &[
                ((7, 4), pc(Side::White, Kind::King)),
                ((7, 7), pc(Side::White, Kind::Rook)),
                ((7, 5), pc(Side::White, Kind::Bishop)), // f1 blocks
                ((0, 4), pc(Side::Black, Kind::King)),
            ],
            Side::White,
        );
        assert!(!state.legal_targets(sq(7, 4)).contains(&sq(7, 6)));
    }

    #[test]
    fn castling_refused_when_in_check() {
        let state = Chess::set_board(
            &[
                ((7, 4), pc(Side::White, Kind::King)),
                ((7, 7), pc(Side::White, Kind::Rook)),
                ((0, 4), pc(Side::Black, Kind::Rook)), // checks e1 down the file
                ((0, 0), pc(Side::Black, Kind::King)),
            ],
            Side::White,
        );
        assert!(state.in_check());
        let targets = state.legal_targets(sq(7, 4));
        assert!(!targets.contains(&sq(7, 6)));
        assert!(!targets.contains(&sq(7, 2)));
    }

    #[test]
    fn castling_refused_when_transit_attacked() {
        let state = Chess::set_board(
            &[
                ((7, 4), pc(Side::White, Kind::King)),
                ((7, 7), pc(Side::White, Kind::Rook)),
                ((0, 5), pc(Side::Black, Kind::Rook)), // rakes the f-file
                ((0, 0), pc(Side::Black, Kind::King)),
            ],
            Side::White,
        );
        assert!(!state.in_check());
        assert!(!state.legal_targets(sq(7, 4)).contains(&sq(7, 6)));
    }

    #[test]
    fn en_passant_captures_and_removes_pawn() {
        let mut state = Chess::new();
        assert!(mv(&mut state, (6, 4), (4, 4))); // e2e4
        assert!(mv(&mut state, (1, 0), (2, 0))); // a7a6
        assert!(mv(&mut state, (4, 4), (3, 4))); // e4e5
        assert!(mv(&mut state, (1, 3), (3, 3))); // d7d5, ep target d6
        assert_eq!(state.en_passant, Some(sq(2, 3)));
        assert!(mv(&mut state, (3, 4), (2, 3))); // exd6 e.p.
        assert_eq!(state.board[sq(3, 3)], None, "captured pawn vanishes");
        assert_eq!(state.board[sq(2, 3)], Some(pc(Side::White, Kind::Pawn)));
    }

    #[test]
    fn promotion_auto_places_queen() {
        let mut state = Chess::set_board(
            &[
                ((1, 4), pc(Side::White, Kind::Pawn)),
                ((7, 4), pc(Side::White, Kind::King)),
                ((0, 0), pc(Side::Black, Kind::King)),
            ],
            Side::White,
        );
        assert!(mv(&mut state, (1, 4), (0, 4))); // e7-e8
        assert_eq!(state.board[sq(0, 4)], Some(pc(Side::White, Kind::Queen)));
    }

    #[test]
    fn pinned_piece_may_not_move_off_line() {
        let state = Chess::set_board(
            &[
                ((7, 4), pc(Side::White, Kind::King)),
                ((4, 4), pc(Side::White, Kind::Rook)), // pinned by Re8
                ((0, 4), pc(Side::Black, Kind::Rook)),
                ((0, 0), pc(Side::Black, Kind::King)),
            ],
            Side::White,
        );
        let targets = state.legal_targets(sq(4, 4));
        // Still free to slide along the pin line, including taking the pinner.
        assert!(targets.contains(&sq(3, 4)));
        assert!(targets.contains(&sq(0, 4)));
        // Stepping off the file would expose the king.
        assert!(!targets.contains(&sq(4, 3)));
        assert!(!targets.contains(&sq(4, 5)));
    }

    #[test]
    fn known_stalemate_position_yields_stalemate() {
        let state = Chess::set_board(
            &[
                ((0, 0), pc(Side::Black, Kind::King)),  // a8
                ((2, 1), pc(Side::White, Kind::Queen)), // b6 covers a7/b7/b8
                ((7, 7), pc(Side::White, Kind::King)),
            ],
            Side::Black,
        );
        assert!(!state.in_check());
        assert_eq!(state.status(), Status::Stalemate);
    }
}
