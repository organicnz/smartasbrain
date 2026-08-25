use rand::{Rng, seq::SliceRandom, thread_rng};

use crate::board::{Board, SIZE};

pub struct Puzzle {
    pub board: Board,
    pub solution: [u8; SIZE],
}

const ALL_CANDIDATES: u16 = 0x1FF;

/// Row, column and box candidate masks. Bit `d - 1` set means digit `d` is
/// still available in that unit.
type Masks = ([u16; 9], [u16; 9], [u16; 9]);

pub fn generate(target_clues: usize) -> Puzzle {
    generate_with_rng(target_clues, &mut thread_rng())
}

/// Deterministic generation, useful for tests and reproducible puzzles.
#[cfg(test)]
pub fn generate_seeded(seed: u64, target_clues: usize) -> Puzzle {
    use rand::{SeedableRng, rngs::StdRng};
    generate_with_rng(target_clues, &mut StdRng::seed_from_u64(seed))
}

pub fn generate_with_rng<R: Rng>(target_clues: usize, rng: &mut R) -> Puzzle {
    let mut solution = [0u8; SIZE];
    fill_grid(&mut solution, rng);

    let mut puzzle = solution;
    dig_holes(&mut puzzle, target_clues, rng);

    Puzzle {
        board: Board::new(puzzle),
        solution,
    }
}

fn box_of(idx: usize) -> usize {
    (idx / 27) * 3 + (idx % 9) / 3
}

fn masks_of(grid: &[u8; SIZE]) -> Masks {
    let mut m = ([0u16; 9], [0u16; 9], [0u16; 9]);
    for (idx, &v) in grid.iter().enumerate() {
        if v != 0 {
            let bit = 1 << (v - 1);
            m.0[idx / 9] |= bit;
            m.1[idx % 9] |= bit;
            m.2[box_of(idx)] |= bit;
        }
    }
    m
}

fn candidates(m: &Masks, idx: usize) -> u16 {
    !(m.0[idx / 9] | m.1[idx % 9] | m.2[box_of(idx)]) & ALL_CANDIDATES
}

fn place(grid: &mut [u8; SIZE], m: &mut Masks, idx: usize, v: u8) {
    let bit = 1 << (v - 1);
    grid[idx] = v;
    m.0[idx / 9] |= bit;
    m.1[idx % 9] |= bit;
    m.2[box_of(idx)] |= bit;
}

fn unplace(grid: &mut [u8; SIZE], m: &mut Masks, idx: usize) {
    let v = grid[idx];
    if v == 0 {
        return;
    }
    let bit = !(1 << (v - 1));
    grid[idx] = 0;
    m.0[idx / 9] &= bit;
    m.1[idx % 9] &= bit;
    m.2[box_of(idx)] &= bit;
}

fn fill_grid<R: Rng>(grid: &mut [u8; SIZE], rng: &mut R) -> bool {
    let mut m = masks_of(grid);
    fill_from(grid, &mut m, rng)
}

fn fill_from<R: Rng>(grid: &mut [u8; SIZE], m: &mut Masks, rng: &mut R) -> bool {
    let Some(idx) = grid.iter().position(|&v| v == 0) else {
        return true;
    };
    let cands = candidates(m, idx);
    let mut values: Vec<u8> = (0..9)
        .filter(|b| cands & (1 << b) != 0)
        .map(|b| b + 1)
        .collect();
    values.shuffle(rng);
    for v in values {
        place(grid, m, idx, v);
        if fill_from(grid, m, rng) {
            return true;
        }
        unplace(grid, m, idx);
    }
    false
}

fn dig_holes<R: Rng>(puzzle: &mut [u8; SIZE], target: usize, rng: &mut R) {
    let mut order: Vec<usize> = (0..SIZE).collect();
    order.shuffle(rng);

    let mut removed = 0usize;
    for &idx in &order {
        if SIZE - removed <= target {
            break;
        }
        let backup = puzzle[idx];
        puzzle[idx] = 0;
        if count_solutions(puzzle, 2) == 1 {
            removed += 1;
        } else {
            puzzle[idx] = backup;
        }
    }
}

/// Count solutions up to `limit`. The grid is left unmodified.
pub fn count_solutions(grid: &mut [u8; SIZE], limit: usize) -> usize {
    let mut m = masks_of(grid);
    count_from(grid, &mut m, limit)
}

fn count_from(grid: &mut [u8; SIZE], m: &mut Masks, limit: usize) -> usize {
    // Minimum-remaining-values: branch on the most constrained cell first.
    let mut best: Option<(usize, u16)> = None;
    for (idx, &v) in grid.iter().enumerate() {
        if v != 0 {
            continue;
        }
        let cands = candidates(m, idx);
        match best {
            Some((_, bc)) if bc.count_ones() <= cands.count_ones() => {}
            _ => best = Some((idx, cands)),
        }
    }
    let Some((idx, cands)) = best else {
        // Complete grid: only counts if every unit holds all nine digits.
        return usize::from(m.0.iter().all(|&x| x == ALL_CANDIDATES));
    };
    if cands == 0 {
        return 0; // dead end
    }

    let mut found = 0;
    let mut bits = cands;
    while bits != 0 {
        let v = bits.trailing_zeros() as u8 + 1;
        bits &= bits - 1;

        let (r, c, b) = (idx / 9, idx % 9, box_of(idx));
        let bit = 1 << (v - 1);
        let saved = grid[idx];
        grid[idx] = v;
        m.0[r] |= bit;
        m.1[c] |= bit;
        m.2[b] |= bit;

        found += count_from(grid, m, limit.saturating_sub(found));

        grid[idx] = saved;
        if saved == 0 {
            m.0[r] &= !bit;
            m.1[c] &= !bit;
            m.2[b] &= !bit;
        }

        if found >= limit {
            break;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::conflict_map;

    fn is_valid_solution(cells: &[u8; SIZE]) -> bool {
        cells.iter().all(|&v| (1..=9).contains(&v)) && conflict_map(cells).iter().all(|&c| !c)
    }

    #[test]
    fn seeded_generation_is_deterministic() {
        let a = generate_seeded(42, 34);
        let b = generate_seeded(42, 34);
        assert_eq!(a.board.cells, b.board.cells);
        assert_eq!(a.solution, b.solution);

        let c = generate_seeded(43, 34);
        assert_ne!(a.board.cells, c.board.cells, "different seeds differ");
    }

    #[test]
    fn generated_puzzles_are_unique_and_consistent() {
        for seed in 0..12u64 {
            let clues = [45usize, 34, 28][seed as usize % 3];
            let puzzle = generate_seeded(seed * 7919, clues);

            assert_eq!(
                puzzle.solution.iter().map(|&v| v as u16).sum::<u16>(),
                405,
                "seed {}",
                seed
            );
            assert!(is_valid_solution(&puzzle.solution));

            let clue_count = puzzle.board.cells.iter().filter(|&&v| v != 0).count();
            assert!(clue_count >= clues, "seed {seed}: {clue_count} < {clues}");

            assert_eq!(
                count_solutions(&mut puzzle.board.cells.clone(), 2),
                1,
                "seed {seed} must be unique"
            );
            for i in 0..SIZE {
                if puzzle.board.cells[i] != 0 {
                    assert_eq!(puzzle.board.cells[i], puzzle.solution[i]);
                    assert!(puzzle.board.fixed[i]);
                }
            }
        }
    }

    #[test]
    fn count_solutions_detects_multiple_and_restores_grid() {
        let mut empty = [0u8; SIZE];
        assert!(count_solutions(&mut empty, 2) >= 2);
        assert!(empty.iter().all(|&v| v == 0), "grid must be restored");

        let mut solved = [5u8; SIZE]; // all conflicts, no solution
        assert_eq!(count_solutions(&mut solved, 5), 0);
    }

    #[test]
    fn hard_target_terminates_quickly_enough() {
        let start = std::time::Instant::now();
        for seed in 100..104u64 {
            let p = generate_seeded(seed, 24);
            assert!(p.board.cells.iter().filter(|&&v| v != 0).count() >= 24);
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "hard generation too slow: {elapsed:?}"
        );
    }
}
