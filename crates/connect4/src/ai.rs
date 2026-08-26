//! Negamax with alpha-beta over cloned boards. Windows scoring plus a
//! center-column bias; Easy adds a blunder band, Hard deepens under a node
//! budget with iterative deepening.

use crate::engine::{COLS, Connect4, Side, Status};
use game_core::Difficulty;

/// Mate-scale score; ply adjustments prefer faster wins.
const WIN: i32 = 100_000;
/// Node ceiling shared across one entire `best_move` call. Past the cap the
/// search falls back to static evals, keeping Hard interactive in debug.
const NODE_BUDGET: u32 = 80_000;
/// Root column order steering alpha-beta toward tactical lines first.
const CENTER_ORDER: [usize; COLS] = [3, 2, 4, 1, 5, 0, 6];

pub fn best_move(game: &Connect4, difficulty: Difficulty) -> Option<usize> {
    let mut rng = rand::thread_rng();
    let legal = game.open_columns();
    if legal.is_empty() {
        return None;
    }
    let mut nodes = 0_u32;
    match difficulty {
        Difficulty::Easy => easy_band(game, &legal, &mut rng, &mut nodes),
        Difficulty::Medium => argmax_root(game, &legal, 6, &mut nodes),
        Difficulty::Hard => {
            let mut best = legal.first().copied();
            for depth in 1..=9_u8 {
                match argmax_root(game, &legal, depth, &mut nodes) {
                    Some(b) => best = Some(b),
                    None => break,
                }
                if nodes > NODE_BUDGET {
                    break;
                }
            }
            best
        }
    }
}

/// Depth-2 search, then a uniform pick among columns within `BAND` points
/// of the best — weak on purpose, never illegal.
fn easy_band(
    game: &Connect4,
    legal: &[usize],
    rng: &mut impl rand::Rng,
    nodes: &mut u32,
) -> Option<usize> {
    const BAND: i32 = 15;
    let scored: Vec<(usize, i32)> = legal
        .iter()
        .filter_map(|&c| root_score(game, c, 2, nodes).map(|s| (c, s)))
        .collect();
    let best = scored.iter().map(|(_, s)| *s).max()?;
    let pool: Vec<usize> = scored
        .into_iter()
        .filter(|(_, s)| *s >= best - BAND)
        .map(|(c, _)| c)
        .collect();
    let pick = if pool.is_empty() {
        0
    } else {
        rng.gen_range(0..pool.len())
    };
    pool.get(pick).copied()
}

/// Score of dropping into `col` at `depth`, from the mover's perspective.
fn root_score(game: &Connect4, col: usize, depth: u8, nodes: &mut u32) -> Option<i32> {
    let mut child = game.clone();
    if !child.drop(col) {
        return None;
    }
    Some(-negamax(
        &child,
        depth.saturating_sub(1),
        -i32::MAX,
        i32::MAX,
        1,
        nodes,
    ))
}

fn argmax_root(game: &Connect4, legal: &[usize], depth: u8, nodes: &mut u32) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    let mut alpha = -i32::MAX;
    for &col in CENTER_ORDER.iter().filter(|c| legal.contains(c)) {
        let Some(score) = root_score(game, col, depth, nodes) else {
            continue;
        };
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((col, score));
        }
        alpha = alpha.max(score);
    }
    best.map(|(c, _)| c)
}

fn negamax(
    game: &Connect4,
    depth: u8,
    mut alpha: i32,
    beta: i32,
    ply: u32,
    nodes: &mut u32,
) -> i32 {
    // Terminal from the previous drop lands here: winner made the last move,
    // so the side to move is either losing or the game drew.
    match game.status() {
        Status::Won(w) => {
            return if w == game.turn() {
                WIN - ply as i32
            } else {
                -(WIN - ply as i32)
            };
        }
        Status::Draw => return 0,
        Status::Ongoing => {}
    }
    if depth == 0 {
        return heuristic(game);
    }
    let mut best = -i32::MAX;
    for &col in CENTER_ORDER.iter() {
        // Budget guard inside the tree: past the cap we fall back to the
        // heuristic so Hard stays interactive even in debug builds.
        if *nodes >= NODE_BUDGET {
            return heuristic(game);
        }
        let mut child = game.clone();
        if !child.drop(col) {
            continue;
        }
        *nodes += 1;
        let score = -negamax(&child, depth - 1, -beta, -alpha, ply + 1, nodes);
        best = best.max(score);
        alpha = alpha.max(score);
        if alpha >= beta {
            break;
        }
    }
    if best == -i32::MAX {
        // No legal column: board full without a win already reported.
        return 0;
    }
    best
}

/// Static eval from the side-to-move's perspective.
fn heuristic(game: &Connect4) -> i32 {
    let me = game.turn();
    window_score(game, me) + center_bonus(game, me)
        - (window_score(game, me.opposite()) + center_bonus(game, me.opposite()))
}

/// Sum over all 4-windows: lone +1, pair +10, triple +50 for `side`;
/// windows holding both colors count nothing.
fn window_score(game: &Connect4, side: Side) -> i32 {
    const TABLE: [i32; 5] = [0, 1, 10, 50, 0];
    let mut total = 0;
    for row in 0..crate::engine::ROWS {
        for col in 0..COLS {
            for (dr, dc) in [(0_i8, 1_i8), (1, 0), (1, 1), (-1, 1)] {
                let mut mine = 0_usize;
                let mut theirs = 0_usize;
                let mut valid = true;
                for step in 0..4_i8 {
                    let (r, c) = (row as i8 + dr * step, col as i8 + dc * step);
                    let Some(cell) = in_bounds(r, c).then(|| game.cell(r as usize, c as usize))
                    else {
                        valid = false;
                        break;
                    };
                    match cell {
                        Some(s) if s == side => mine += 1,
                        Some(_) => theirs += 1,
                        None => {}
                    }
                }
                if valid && (mine == 0 || theirs == 0) {
                    total += TABLE[mine.min(3)] - TABLE[theirs.min(3)];
                }
            }
        }
    }
    total
}

fn center_bonus(game: &Connect4, side: Side) -> i32 {
    (0..crate::engine::ROWS)
        .filter(|&r| game.cell(r, 3) == Some(side))
        .count() as i32
        * 6
}

fn in_bounds(r: i8, c: i8) -> bool {
    r >= 0 && r < crate::engine::ROWS as i8 && c >= 0 && c < COLS as i8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Side;

    #[test]
    fn immediate_win_taken_at_medium() {
        // Black owns bottom cells (0..=2); col 3 bottom is free: winning drop.
        let mut g = Connect4::new();
        for c in [0_usize, 4, 1, 5, 2, 6] {
            g.drop(c);
        }
        assert_eq!(g.turn(), Side::Black);
        assert_eq!(best_move(&g, Difficulty::Medium), Some(3));
    }

    #[test]
    fn blocks_obvious_threat_at_medium_and_hard() {
        // Black owns bottom cols 0..=2 after five drops; white to move must
        // take col 3 immediately or lose.
        let mut g = Connect4::new();
        for c in [0_usize, 4, 1, 5, 2] {
            g.drop(c);
        }
        assert_eq!(g.turn(), Side::White);
        assert_eq!(best_move(&g, Difficulty::Medium), Some(3));
        assert_eq!(best_move(&g, Difficulty::Hard), Some(3));
    }

    #[test]
    fn easy_self_play_stays_legal_until_finished() {
        use rand::{SeedableRng, rngs::StdRng};
        for seed in 0..8_u64 {
            let _rng = StdRng::seed_from_u64(seed);
            let mut g = Connect4::new();
            let mut plies = 0;
            while matches!(g.status(), Status::Ongoing) {
                let Some(col) = best_move(&g, Difficulty::Easy) else {
                    break;
                };
                assert!(g.open_columns().contains(&col), "illegal ai move");
                assert!(g.drop(col));
                plies += 1;
                assert!(plies <= 42);
            }
            assert!(plies > 0);
        }
    }

    #[test]
    fn hard_search_completes_within_budget_on_start_position() {
        let g = Connect4::new();
        let start = std::time::Instant::now();
        let mv = best_move(&g, Difficulty::Hard);
        assert!(mv.is_some());
        assert!(
            start.elapsed().as_secs_f64() < 5.0,
            "hard search must stay CI-safe"
        );
    }
}
