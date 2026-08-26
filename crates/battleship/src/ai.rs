//! Battleship AI: random salvoes for Easy, hunt-and-target for Medium,
//! probability-density placement counting for Hard.

use crate::engine::{Battleship, CELLS, CellView, GRID};
use game_core::Difficulty;
use rand::Rng;

pub fn best_shot(game: &Battleship, difficulty: Difficulty) -> Option<usize> {
    let view = game.enemy_view(game.turn());
    let open: Vec<usize> = (0..CELLS)
        .filter(|&c| view[c] == CellView::Unknown)
        .collect();
    if open.is_empty() {
        return None;
    }
    let mut rng = rand::thread_rng();
    match difficulty {
        Difficulty::Easy => {
            let pick = rng.gen_range(0..open.len());
            open.get(pick).copied()
        }
        Difficulty::Medium => target_or_hunt(game, &view, &open, &mut rng),
        Difficulty::Hard => density(game, &view, &open),
    }
}

/// Orthogonal neighbours of `cell` that are still unknown.
fn fresh_neighbors(view: &[CellView; CELLS], cell: usize) -> Vec<usize> {
    let (r, c) = (cell / GRID, cell % GRID);
    let mut out = Vec::new();
    if r > 0 {
        out.push(cell - GRID);
    }
    if r < GRID - 1 {
        out.push(cell + GRID);
    }
    if c > 0 {
        out.push(cell - 1);
    }
    if c < GRID - 1 {
        out.push(cell + 1);
    }
    out.retain(|&n| view[n] == CellView::Unknown);
    out
}

fn target_or_hunt(
    _game: &Battleship,
    view: &[CellView; CELLS],
    open: &[usize],
    rng: &mut impl Rng,
) -> Option<usize> {
    // Finish wounded ships first.
    let mut adjacent: Vec<usize> = Vec::new();
    for c in 0..CELLS {
        if view[c] == CellView::Hit {
            adjacent.extend(fresh_neighbors(view, c));
        }
    }
    if !adjacent.is_empty() {
        let pick = rng.gen_range(0..adjacent.len());
        return adjacent.get(pick).copied();
    }
    // Checkerboard hunting finds the smallest ships fastest.
    let parity: Vec<usize> = open
        .iter()
        .copied()
        .filter(|&c| (c / GRID + c % GRID).is_multiple_of(2))
        .collect();
    let pool = if parity.is_empty() { open } else { &parity };
    let pick = rng.gen_range(0..pool.len());
    pool.get(pick).copied()
}

/// Count plausible placements of every surviving ship across unknown/hit
/// waters; shoot the hottest cell, with a nudge toward wounded-ship flanks.
fn density(game: &Battleship, view: &[CellView; CELLS], open: &[usize]) -> Option<usize> {
    let me = game.turn();
    let remaining = game.remaining(me);
    let mut weights = vec![0_u32; CELLS];
    for &(kind, _len) in &remaining {
        let len = kind.len();
        // Horizontal placements.
        for r in 0..GRID {
            for c in 0..=(GRID - len) {
                let base = r * GRID + c;
                if (base..base + len)
                    .all(|i| view[i] != CellView::Miss && view[i] != CellView::Sunk)
                {
                    for w in &mut weights[base..base + len] {
                        *w += 1;
                    }
                }
            }
        }
        // Vertical placements.
        for c in 0..GRID {
            for r in 0..=(GRID - len) {
                let base = r * GRID + c;
                if (0..len).all(|s| {
                    let i = base + s * GRID;
                    view[i] != CellView::Miss && view[i] != CellView::Sunk
                }) {
                    for s in 0..len {
                        weights[base + s * GRID] += 1;
                    }
                }
            }
        }
    }
    // Flank bonus around known hits.
    for c in 0..CELLS {
        if view[c] == CellView::Hit {
            for n in fresh_neighbors(view, c) {
                weights[n] += 6;
            }
        }
    }
    open.iter().copied().max_by_key(|&c| weights[c])
}
