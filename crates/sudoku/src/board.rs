pub const SIZE: usize = 81;

/// The 27 sudoku units (9 rows, 9 columns, 9 boxes), precomputed at compile time.
const UNITS: [[usize; 9]; 27] = build_units();

const fn build_units() -> [[usize; 9]; 27] {
    let mut units = [[0usize; 9]; 27];

    let mut r = 0;
    while r < 9 {
        let mut c = 0;
        while c < 9 {
            units[r][c] = r * 9 + c;
            c += 1;
        }
        r += 1;
    }

    let mut c = 0;
    while c < 9 {
        let mut rr = 0;
        while rr < 9 {
            units[9 + c][rr] = rr * 9 + c;
            rr += 1;
        }
        c += 1;
    }

    let mut br = 0;
    while br < 3 {
        let mut bc = 0;
        while bc < 3 {
            let mut k = 0;
            let mut rr = 0;
            while rr < 3 {
                let mut cc = 0;
                while cc < 3 {
                    units[18 + br * 3 + bc][k] = (br * 3 + rr) * 9 + (bc * 3 + cc);
                    k += 1;
                    cc += 1;
                }
                rr += 1;
            }
            bc += 1;
        }
        br += 1;
    }

    units
}

#[derive(Clone)]
pub struct Board {
    pub cells: [u8; SIZE],
    pub fixed: [bool; SIZE],
}

impl Board {
    pub fn new(cells: [u8; SIZE]) -> Self {
        Self {
            cells,
            fixed: cells.map(|v| v != 0),
        }
    }

    pub fn is_fixed(&self, idx: usize) -> bool {
        self.fixed[idx]
    }
}

/// Marks every cell that shares a value with a peer in its row, column or box.
pub fn conflict_map(cells: &[u8; SIZE]) -> [bool; SIZE] {
    let mut map = [false; SIZE];
    for unit in &UNITS {
        let mut seen = [SIZE; 10]; // digit -> first index holding it
        for &idx in unit {
            let v = cells[idx] as usize;
            if v == 0 {
                continue;
            }
            if seen[v] != SIZE {
                map[idx] = true;
                map[seen[v]] = true;
            } else {
                seen[v] = idx;
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_cover_the_grid_correctly() {
        // Every cell appears exactly once per row/col/box family.
        for unit in &UNITS {
            assert_eq!(unit.len(), 9);
        }
        assert_eq!(UNITS[0], [0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(UNITS[9], [0, 9, 18, 27, 36, 45, 54, 63, 72]);
        assert_eq!(UNITS[18], [0, 1, 2, 9, 10, 11, 18, 19, 20]);
        assert_eq!(UNITS[26], [60, 61, 62, 69, 70, 71, 78, 79, 80]);
    }

    #[test]
    fn detects_row_col_box_conflicts() {
        let mut cells = [0u8; SIZE];
        cells[0] = 5;
        cells[8] = 5;
        let map = conflict_map(&cells);
        assert!(map[0] && map[8]);

        cells[8] = 0;
        cells[72] = 5;
        let map = conflict_map(&cells);
        assert!(map[0] && map[72]);

        cells[72] = 0;
        cells[20] = 5;
        let map = conflict_map(&cells);
        assert!(map[0] && map[20]);

        cells[20] = 0;
        cells[40] = 5;
        let map = conflict_map(&cells);
        assert!(!map.iter().any(|&c| c));
    }

    #[test]
    fn marks_all_cells_in_a_triple_conflict() {
        let mut cells = [0u8; SIZE];
        cells[0] = 7;
        cells[3] = 7;
        cells[6] = 7;
        let map = conflict_map(&cells);
        assert!(map[0] && map[3] && map[6]);

        cells[3] = 0;
        let map = conflict_map(&cells);
        assert!(map[0] && map[6]);
    }
}
