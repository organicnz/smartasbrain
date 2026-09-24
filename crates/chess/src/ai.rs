//! Pure search for the built-in chess AI: negamax with alpha-beta pruning.
//!
//! No TUI dependencies live here — the module reads the rules engine and
//! answers with `(from, to)` square pairs drawn from `legal_targets`.

use std::cmp::Reverse;

use game_core::Difficulty;
use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::engine::{Chess, Kind, Side, Status, col, row};

/// Centipawn values shared by evaluation and capture ordering.
const PAWN: i32 = 100;
const KNIGHT: i32 = 300;
const BISHOP: i32 = 320;
const ROOK: i32 = 500;
const QUEEN: i32 = 900;

/// Score magnitude for a forced mate; shaved by ply so faster mates win.
const MATE: i32 = 100_000;
/// Window bound safely beyond any reachable score.
const INF: i32 = 2 * MATE;

const NODE_BUDGET: u32 = 64;
const EXPERT_NODE_BUDGET: u32 = 128;
const EASY_BLUNDER_CP: i32 = 250;

/// Search depth per strength preset.
fn depth_of(difficulty: Difficulty) -> i32 {
    match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Medium => 3,
        Difficulty::Hard => 4,
        Difficulty::Expert => 5,
    }
}

fn value(kind: Kind) -> i32 {
    match kind {
        Kind::Pawn => PAWN,
        Kind::Knight => KNIGHT,
        Kind::Bishop => BISHOP,
        Kind::Rook => ROOK,
        Kind::Queen => QUEEN,
        Kind::King => 0,
    }
}

/// Small reward for occupying the middle rings, zero on the rim.
fn center_bonus(r: usize, c: usize) -> i32 {
    let ring = r.max(7 - r).max(c.max(7 - c)); // 4 near the centre .. 7 at edges
    (7 - ring as i32) * 6
}

/// Static evaluation in centipawns from the side-to-move perspective:
/// material plus light knight/bishop centrality and pawn advancement.
fn evaluate(game: &Chess) -> i32 {
    let mut score = 0;
    for (i, cell) in game.board().iter().enumerate() {
        let Some(piece) = cell else { continue };
        let mut worth = value(piece.kind);
        match piece.kind {
            Kind::Knight | Kind::Bishop => worth += center_bonus(row(i), col(i)),
            Kind::Pawn => {
                let steps = match piece.side {
                    Side::White => 6usize.saturating_sub(row(i)),
                    Side::Black => row(i).saturating_sub(1),
                };
                worth += (steps * 4) as i32;
            }
            _ => {}
        }
        score += match piece.side {
            Side::White => worth,
            Side::Black => -worth,
        };
    }
    match game.turn() {
        Side::White => score,
        Side::Black => -score,
    }
}

/// Every legal move for the side to move, most valuable victims first.
fn legal_moves(game: &Chess) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for from in 0..64usize {
        for to in game.legal_targets(from) {
            out.push((from, to));
        }
    }
    out.sort_by_key(|&(_, to)| Reverse(game.board()[to].map_or(0, |p| value(p.kind))));
    out
}

/// Negamax with alpha-beta pruning; scores always favour faster mates.
fn negamax(
    game: &Chess,
    depth: i32,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    nodes: &mut u32,
    budget: u32,
) -> i32 {
    match game.status() {
        Status::Won(_) => return -(MATE - ply),
        Status::Stalemate => return 0,
        Status::Ongoing => {}
    }
    if depth == 0 || *nodes >= budget {
        return evaluate(game);
    }
    *nodes += 1;
    let mut best = -INF;
    for &(from, to) in &legal_moves(game) {
        let mut child = game.clone();
        child.play(from, to);
        let score = -negamax(&child, depth - 1, ply + 1, -beta, -alpha, nodes, budget);
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

/// Pick a move for the side to move, or `None` when the game is over.
///
/// Easy shuffles the root moves and plays the first one landing within a
/// 250cp blunder window of the best, keeping games winnable for humans.
/// Medium/Hard shuffle only to break ties, then play strictly optimal moves.
pub fn best_move(game: &Chess, difficulty: Difficulty) -> Option<(usize, usize)> {
    let mut rng = thread_rng();
    let mut root = legal_moves(game);
    if root.is_empty() {
        return None;
    }
    root.shuffle(&mut rng);
    let depth = depth_of(difficulty);
    let budget = if difficulty == Difficulty::Expert {
        EXPERT_NODE_BUDGET
    } else {
        NODE_BUDGET
    };
    let mut nodes = 0;

    if difficulty == Difficulty::Easy {
        let mut scores = Vec::with_capacity(root.len());
        for &(from, to) in &root {
            let mut child = game.clone();
            child.play(from, to);
            scores.push(-negamax(
                &child,
                depth - 1,
                1,
                -INF,
                INF,
                &mut nodes,
                budget,
            ));
        }
        let best_score = scores.iter().copied().max().unwrap_or(0);
        return root
            .into_iter()
            .zip(scores)
            .find(|&(_, score)| score >= best_score - EASY_BLUNDER_CP)
            .map(|(mv, _)| mv);
    }

    let mut best = root[0];
    let mut alpha = -INF;
    for &(from, to) in &root {
        if nodes >= budget {
            break;
        }
        let mut child = game.clone();
        child.play(from, to);
        let score = -negamax(&child, depth - 1, 1, -INF, -alpha, &mut nodes, budget);
        if score > alpha {
            alpha = score;
            best = (from, to);
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::engine::Piece;

    fn pc(side: Side, kind: Kind) -> Piece {
        Piece { side, kind }
    }

    #[test]
    fn best_move_returns_only_legal_moves() {
        let start = Chess::new();
        let endgame = Chess::set_board(
            &[
                ((7, 4), pc(Side::White, Kind::King)),
                ((4, 3), pc(Side::White, Kind::Queen)),
                ((0, 0), pc(Side::Black, Kind::King)),
                ((2, 5), pc(Side::White, Kind::Pawn)),
                ((1, 2), pc(Side::Black, Kind::Pawn)),
            ],
            Side::White,
        );
        let black_to_move = Chess::set_board(
            &[
                ((7, 4), pc(Side::White, Kind::King)),
                ((4, 4), pc(Side::White, Kind::Rook)),
                ((0, 4), pc(Side::Black, Kind::King)),
            ],
            Side::Black,
        );
        let positions = [&start, &endgame, &black_to_move];
        for state in positions {
            for difficulty in Difficulty::ALL {
                let Some((from, to)) = best_move(state, difficulty) else {
                    panic!("position has legal moves but best_move returned none");
                };
                assert!(
                    state.legal_targets(from).contains(&to),
                    "{difficulty:?} suggested illegal {from}->{to}"
                );
            }
        }
    }

    #[test]
    fn mate_in_one_found_at_medium() {
        // Back-rank setup: Rd1-d8 delivers mate against the boxed-in h8 king.
        let state = Chess::set_board(
            &[
                ((0, 7), pc(Side::Black, Kind::King)),
                ((1, 6), pc(Side::Black, Kind::Pawn)),
                ((1, 7), pc(Side::Black, Kind::Pawn)),
                ((7, 3), pc(Side::White, Kind::Rook)),
                ((7, 4), pc(Side::White, Kind::King)),
            ],
            Side::White,
        );
        let (from, to) = best_move(&state, Difficulty::Medium)
            .expect("white must have moves in the mate-in-one position");
        let mut played = state;
        assert!(played.play(from, to));
        assert_eq!(played.status(), Status::Won(Side::White));
    }

    #[test]
    fn easy_differs_across_calls() {
        let state = Chess::new();
        let mut seen = HashSet::new();
        for _ in 0..50 {
            if let Some(mv) = best_move(&state, Difficulty::Easy) {
                seen.insert(mv);
            }
            if seen.len() >= 2 {
                break;
            }
        }
        assert!(seen.len() >= 2, "easy should vary its opening choices");
    }

    #[test]
    fn hard_start_position_search_within_budget() {
        let state = Chess::new();
        let started = Instant::now();
        let chosen = best_move(&state, Difficulty::Hard);
        let elapsed = started.elapsed();
        assert!(chosen.is_some());
        assert!(
            elapsed < Duration::from_secs(5),
            "hard search took {elapsed:?}"
        );
    }
}
