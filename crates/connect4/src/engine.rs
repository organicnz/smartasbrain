//! Pure Connect Four rules: gravity drops on a 7×6 board, four-in-a-row
//! wins in every direction, draw when the grid fills. Row 0 is the bottom
//! row internally so drop math stays trivial; rendering flips it.

pub const COLS: usize = 7;
pub const ROWS: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Black,
    White,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
        }
    }

    /// Filled-disc glyph used across UIs.
    pub fn symbol(self) -> char {
        '\u{25cf}'
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    Won(Side),
    Draw,
}

#[derive(Clone)]
pub struct Connect4 {
    /// Indexed `[col][row]`, row 0 at the bottom.
    cols: [[Option<Side>; ROWS]; COLS],
    heights: [usize; COLS],
    turn: Side,
    status: Status,
    last_drop: Option<(usize, usize)>,
    moves: u8,
}

impl Connect4 {
    pub fn new() -> Self {
        Self {
            cols: [[None; ROWS]; COLS],
            heights: [0; COLS],
            turn: Side::Black,
            status: Status::Ongoing,
            last_drop: None,
            moves: 0,
        }
    }

    /// Disc at `(row, col)` counting from the bottom, or None.
    pub fn cell(&self, row: usize, col: usize) -> Option<Side> {
        self.cols
            .get(col)
            .and_then(|c| c.get(row))
            .copied()
            .flatten()
    }

    #[cfg(test)]
    pub fn col_height(&self, col: usize) -> usize {
        self.heights.get(col).copied().unwrap_or(0)
    }

    /// Columns with free space, in natural order.
    pub fn open_columns(&self) -> Vec<usize> {
        (0..COLS).filter(|&c| self.heights[c] < ROWS).collect()
    }

    /// Drops a disc into `col`; returns false when the game is over or the
    /// column is full. Updates win/draw status and flips the turn.
    pub fn drop(&mut self, col: usize) -> bool {
        if self.status != Status::Ongoing || col >= COLS || self.heights[col] >= ROWS {
            return false;
        }
        let row = self.heights[col];
        let mover = self.turn;
        self.cols[col][row] = Some(mover);
        self.heights[col] += 1;
        self.last_drop = Some((row, col));
        self.moves += 1;

        if lands_four(&self.cols, row, col, mover) {
            self.status = Status::Won(mover);
        } else if self.moves == (ROWS * COLS) as u8 {
            self.status = Status::Draw;
        }
        // The turn always advances; finished games simply refuse further
        // drops, which keeps negamax's alternating-side convention intact.
        self.turn = mover.opposite();
        true
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn last_drop(&self) -> Option<(usize, usize)> {
        self.last_drop
    }
}

impl Status {
    /// Convenience for gating input and automation.
    pub fn is_ongoing(self) -> bool {
        matches!(self, Self::Ongoing)
    }
}

impl Default for Connect4 {
    fn default() -> Self {
        Self::new()
    }
}

/// True when the disc just landed at `(row, col)` completes four in a row.
fn lands_four(cols: &[[Option<Side>; ROWS]; COLS], row: usize, col: usize, side: Side) -> bool {
    const DIRS: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
    DIRS.iter().any(|&(dr, dc)| {
        let mut count = 1_u8;
        for sign in [-1_i8, 1] {
            let (mut r, mut c) = (row as i8 + dr * sign, col as i8 + dc * sign);
            while r >= 0
                && r < ROWS as i8
                && c >= 0
                && c < COLS as i8
                && cols[c as usize][r as usize] == Some(side)
            {
                count += 1;
                r += dr * sign;
                c += dc * sign;
            }
        }
        count >= 4
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drops through a fixed sequence of columns (panics in tests only).
    fn seq(g: &mut Connect4, cols: &[usize]) {
        for &c in cols {
            assert!(g.drop(c), "drop into {c} should land");
        }
    }

    #[test]
    fn drop_fills_upward_and_refuses_full_columns() {
        let mut g = Connect4::new();
        seq(&mut g, &[0, 0, 0]);
        assert_eq!(g.cell(0, 0), Some(Side::Black));
        assert_eq!(g.cell(1, 0), Some(Side::White));
        assert_eq!(g.cell(2, 0), Some(Side::Black));
        assert_eq!(g.cell(5, 0), None, "column not full yet");
        let mut filler = Connect4::new();
        for _ in 0..ROWS {
            filler.drop(3);
        }
        assert_eq!(filler.col_height(3), ROWS);
        assert!(!filler.drop(3), "full column refuses");
    }

    #[test]
    fn horizontal_vertical_and_diagonal_wins_are_detected() {
        // Black takes the bottom row while white leaks upward elsewhere.
        let mut h = Connect4::new();
        for c in [0_usize, 4, 1, 5, 2, 6, 3] {
            h.drop(c);
        }
        assert!(matches!(h.status(), Status::Won(Side::Black)));

        let mut v = Connect4::new();
        for c in [0_usize, 1, 0, 1, 0, 1, 0] {
            v.drop(c);
        }
        assert!(matches!(v.status(), Status::Won(Side::Black)));

        // Rising diagonal (0,0)-(1,1)-(2,2)-(3,3).
        let mut d = Connect4::new();
        for c in [0_usize, 1, 1, 2, 2, 3, 2, 3, 3, 5, 3] {
            assert!(d.drop(c));
        }
        assert!(matches!(d.status(), Status::Won(Side::Black)));
    }

    #[test]
    fn draw_is_reachable_when_the_grid_fills() {
        use rand::{Rng, SeedableRng, rngs::StdRng};
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..600 {
            let mut g = Connect4::new();
            while matches!(g.status(), Status::Ongoing) {
                let open = g.open_columns();
                let col = open[rng.gen_range(0..open.len())];
                g.drop(col);
            }
            if g.status() == Status::Draw {
                return;
            }
        }
        panic!("no draw occurred across 600 seeded random games");
    }

    #[test]
    fn turns_strictly_alternate_and_stops_on_win() {
        let mut g = Connect4::new();
        assert_eq!(g.turn(), Side::Black);
        g.drop(0);
        assert_eq!(g.turn(), Side::White);
        let mut w = Connect4::new();
        for c in [0_usize, 1, 0, 1, 0, 1, 0] {
            w.drop(c);
        }
        assert!(matches!(w.status(), Status::Won(Side::Black)));
        // The seat advances even on the final drop; finished games refuse
        // further moves via status alone.
        assert_eq!(w.turn(), Side::White);
        assert!(!w.drop(2), "finished game refuses further drops");
    }
}
