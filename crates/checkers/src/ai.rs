//! Search-based opponent — pure engine logic, no TUI dependencies.

use crate::engine::{Checkers, SIZE, Side, Status};
use game_core::Difficulty;
use rand::seq::SliceRandom;
use rand::thread_rng;

/// Score for a won position, scaled by remaining depth so faster wins are
/// preferred over slower ones.
const WIN: i32 = 100_000;
const INF: i32 = WIN * 2;

/// Easy picks uniformly among root moves scoring within this many
/// centipawns of the best root move.
const EASY_BAND: i32 = 200;
const NODE_BUDGET: u32 = 1_000;
const EXPERT_NODE_BUDGET: u32 = 2_000;

const MAN: i32 = 100;
const KING: i32 = 160;
/// Bonus per row already advanced toward the crowning row (men only).
const ADVANCE: i32 = 4;
/// Penalty for standing on an edge column.
const EDGE: i32 = 5;

/// Best legal move for the side to move, or `None` once the game is over.
///
/// Easy picks uniformly among root moves; Medium/Hard search deeper;
/// Expert does even more thorough search.
pub fn best_move(game: &Checkers, difficulty: Difficulty) -> Option<(usize, usize)> {
    if game.status() != Status::Ongoing {
        return None;
    }
    let mut roots = root_moves(game);
    if roots.is_empty() {
        return None;
    }
    let depth = match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Medium => 3,
        Difficulty::Hard => 6,
        Difficulty::Expert => 8,
    };
    let budget = match difficulty {
        Difficulty::Expert => EXPERT_NODE_BUDGET,
        _ => NODE_BUDGET,
    };
    let mut nodes = 0;

    match difficulty {
        Difficulty::Easy => {
            roots.shuffle(&mut thread_rng());
            let scores = root_scores(game, &roots, depth, &mut nodes, budget);
            let best = scores.iter().copied().max()?;
            let pool: Vec<(usize, usize)> = roots
                .into_iter()
                .zip(scores)
                .filter(|&(_, score)| score >= best - EASY_BAND)
                .map(|(mv, _)| mv)
                .collect();
            pool.choose(&mut thread_rng()).copied()
        }
        Difficulty::Medium | Difficulty::Hard | Difficulty::Expert => {
            roots.shuffle(&mut thread_rng());
            let scores = root_scores(game, &roots, depth, &mut nodes, budget);
            let mut best: Option<((usize, usize), i32)> = None;
            for (mv, score) in roots.into_iter().zip(scores) {
                if best.is_none_or(|(_, s)| score > s) {
                    best = Some((mv, score));
                }
            }
            best.map(|(mv, _)| mv)
        }
    }
}

/// Every `(from, to)` the side to move may legally play right now. During a
/// multi-jump chain this yields only continuations from the locked piece.
fn root_moves(game: &Checkers) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for from in 0..SIZE * SIZE {
        for to in game.legal_targets(from) {
            out.push((from, to));
        }
    }
    out
}

/// Negamax value of each root move, from the mover's perspective.
fn root_scores(
    game: &Checkers,
    roots: &[(usize, usize)],
    depth: i32,
    nodes: &mut u32,
    budget: u32,
) -> Vec<i32> {
    roots
        .iter()
        .copied()
        .map(|(from, to)| {
            let mut child = game.clone();
            child.play(from, to);
            if child.chain_from().is_some() {
                // Multi-jump chain: the same side keeps moving, so the child
                // is scored without flipping perspective.
                negamax(&child, depth, -INF, INF, nodes, budget)
            } else {
                -negamax(&child, depth - 1, -INF, INF, nodes, budget)
            }
        })
        .collect()
}

/// Negamax with alpha-beta pruning; score is relative to the side to move.
fn negamax(
    game: &Checkers,
    depth: i32,
    mut alpha: i32,
    beta: i32,
    nodes: &mut u32,
    budget: u32,
) -> i32 {
    if let Status::Won(winner) = game.status() {
        return if winner == game.turn() {
            WIN
        } else {
            // The side to move has no moves and loses; more remaining depth
            // means the result was seen earlier.
            -(WIN + depth.max(0))
        };
    }
    if depth <= 0 || *nodes >= budget {
        return evaluate(game);
    }
    *nodes += 1;
    let mut best = -INF;
    for &(from, to) in &root_moves(game) {
        let mut child = game.clone();
        child.play(from, to);
        let score = if child.chain_from().is_some() {
            negamax(&child, depth, alpha, beta, nodes, budget)
        } else {
            -negamax(&child, depth - 1, -beta, -alpha, nodes, budget)
        };
        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break;
        }
    }
    best
}

/// Heuristic board value from the perspective of the side to move:
/// material plus small positional terms.
fn evaluate(game: &Checkers) -> i32 {
    let mut balance = 0i32;
    for sq in 0..SIZE * SIZE {
        let Some(pc) = game.piece_at(sq) else {
            continue;
        };
        let (r, c) = ((sq / SIZE) as i32, (sq % SIZE) as i32);
        let mut value = if pc.king { KING } else { MAN };
        if !pc.king {
            let advanced = match pc.side {
                Side::White => SIZE as i32 - 1 - r,
                Side::Black => r,
            };
            value += ADVANCE * advanced;
        }
        if c == 0 || c == SIZE as i32 - 1 {
            value -= EDGE;
        }
        balance += match pc.side {
            Side::White => value,
            Side::Black => -value,
        };
    }
    match game.turn() {
        Side::White => balance,
        Side::Black => -balance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(r: usize, c: usize) -> usize {
        r * SIZE + c
    }

    #[test]
    fn easy_self_play_moves_stay_legal_and_games_terminate() {
        for run in 0..4u64 {
            let mut game = Checkers::new();
            let mut plies = 0;
            while game.status() == Status::Ongoing && plies < 300 {
                let Some((from, to)) = best_move(&game, Difficulty::Easy) else {
                    break;
                };
                assert!(
                    game.legal_targets(from).contains(&to),
                    "run {run} ply {plies}: {from}->{to} was not a legal target"
                );
                assert!(game.play(from, to));
                plies += 1;
            }
            assert_ne!(
                game.status(),
                Status::Ongoing,
                "run {run}: no result after {plies} plies"
            );
        }
    }

    #[test]
    fn medium_finds_capture_when_available() {
        let game = Checkers::test_position(
            Side::White,
            &[
                (sq(5, 2), Side::White, false),
                (sq(4, 3), Side::Black, false),
            ],
        );
        let (from, to) = best_move(&game, Difficulty::Medium).expect("a move exists");
        assert_eq!(from, sq(5, 2));
        assert_eq!((from / SIZE).abs_diff(to / SIZE), 2, "two rows past origin");
        assert_eq!(
            (from % SIZE).abs_diff(to % SIZE),
            2,
            "two columns past origin"
        );
        let mid = (from + to) / 2;
        assert_eq!(
            game.piece_at(mid).map(|pc| pc.side),
            Some(Side::Black),
            "destination sits beyond an enemy man"
        );
    }

    #[test]
    fn hard_completes_multi_jump_chains_from_locked_square() {
        let mut game = Checkers::test_position(
            Side::White,
            &[
                (sq(5, 2), Side::White, false),
                (sq(4, 3), Side::Black, false),
                (sq(2, 3), Side::Black, false),
            ],
        );
        let first = best_move(&game, Difficulty::Hard).expect("forced jump exists");
        assert!(game.play(first.0, first.1));
        assert_eq!(game.chain_from(), Some(sq(3, 4)), "chain still open");

        let second = best_move(&game, Difficulty::Hard).expect("chain move exists");
        assert_eq!(second.0, sq(3, 4), "only the locked chain piece moves");
        assert!(game.play(second.0, second.1));
        assert_eq!(game.status(), Status::Won(Side::White));
        assert_eq!(best_move(&game, Difficulty::Hard), None);
    }
}
