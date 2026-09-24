//! Pure search for the built-in reversi AI: negamax with alpha-beta pruning
//! over cloned states, scored by a positional weight table plus mobility and
//! disc difference. No TUI dependencies live here.

use game_core::Difficulty;
use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::engine::{Reversi, Side, Status, col, row, sq};

/// Score magnitude for a decided game; shaved by ply so faster wins rank higher.
const MATE: i32 = 100_000;
/// Window bound safely beyond any reachable score.
const INF: i32 = 2 * MATE;
/// Safety valve so pathological positions cannot stall a frame forever.
const NODE_BUDGET: u32 = 256;
const EXPERT_NODE_BUDGET: u32 = 512;
/// Easy picks uniformly among its best handful of root moves.
const EASY_POOL: usize = 6;

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

/// Search depth per strength preset.
fn depth_of(difficulty: Difficulty) -> i32 {
    match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Medium => 4,
        Difficulty::Hard => 6,
        Difficulty::Expert => 8,
    }
}

/// Positional weight of occupying a square: corners are gold, the C/X
/// squares that surrender them are poison, quiet edges mild positives.
fn square_weight(idx: usize) -> i32 {
    let (r, c) = (row(idx), col(idx));
    match (r, c) {
        (0, 0) | (0, 7) | (7, 0) | (7, 7) => 100,
        // C-squares: edge squares one step from a corner.
        (0, 1) | (0, 6) | (1, 0) | (1, 7) | (6, 0) | (6, 7) | (7, 1) | (7, 6) => -30,
        // X-squares: diagonals touching a corner.
        (1, 1) | (1, 6) | (6, 1) | (6, 6) => -60,
        _ if r == 0 || r == 7 || c == 0 || c == 7 => 10,
        _ => 0,
    }
}

/// Number of squares where `side` could legally place a disc right now.
fn side_mobility(board: &[Option<Side>; 64], side: Side) -> usize {
    let mut count = 0usize;
    'squares: for idx in 0..64usize {
        if board[idx].is_some() {
            continue;
        }
        let (r, c) = (row(idx) as isize, col(idx) as isize);
        let foe = side.opposite();
        for &(dr, dc) in &DIRS {
            let mut rr = r + dr;
            let mut cc = c + dc;
            let mut seen = 0usize;
            while (0..8).contains(&rr)
                && (0..8).contains(&cc)
                && board[sq(rr as usize, cc as usize)] == Some(foe)
            {
                seen += 1;
                rr += dr;
                cc += dc;
            }
            if seen > 0
                && (0..8).contains(&rr)
                && (0..8).contains(&cc)
                && board[sq(rr as usize, cc as usize)] == Some(side)
            {
                count += 1;
                continue 'squares;
            }
        }
    }
    count
}

/// Static evaluation from the side-to-move perspective: positional table,
/// four points per net mobility point, and a disc-difference term whose
/// weight grows late, when bulk starts to matter more than flexibility.
fn evaluate(game: &Reversi) -> i32 {
    let me = game.turn();
    let (mut discs_me, mut discs_foe, mut empties) = (0i32, 0i32, 0i32);
    let mut positional = 0i32;
    for (idx, cell) in game.board().iter().enumerate() {
        let Some(owner) = cell else {
            empties += 1;
            continue;
        };
        let mine = *owner == me;
        if mine {
            discs_me += 1;
        } else {
            discs_foe += 1;
        }
        positional += if mine {
            square_weight(idx)
        } else {
            -square_weight(idx)
        };
    }
    let mobility = (side_mobility(game.board(), me) as i32)
        - (side_mobility(game.board(), me.opposite()) as i32);
    let filled = 64i32 - empties;
    let disc_weight = match filled {
        0..=39 => 1,
        40..=49 => 3,
        _ => 8,
    };
    positional + 4 * mobility + disc_weight * (discs_me - discs_foe)
}

/// Value of playing `mv` from `game`, from the mover's perspective.
///
/// The engine resolves passes internally, so a child can be terminal,
/// hand the turn to the opponent, or bounce it straight back to the
/// mover; each case needs its own sign handling.
fn score_child(
    game: &Reversi,
    mv: usize,
    depth: i32,
    ply: i32,
    alpha: i32,
    beta: i32,
    nodes: &mut u32,
) -> i32 {
    let mut child = game.clone();
    let mover = child.turn();
    child.play(mv);
    match child.status() {
        Status::Won(w) => {
            if w == mover {
                MATE - ply
            } else {
                -(MATE - ply)
            }
        }
        Status::Draw => 0,
        Status::Ongoing if child.turn() == mover => {
            // Auto-pass: the mover goes again, so no negation.
            negamax(&child, depth, ply + 1, alpha, beta, nodes)
        }
        Status::Ongoing => -negamax(&child, depth, ply + 1, -beta, -alpha, nodes),
    }
}

/// Negamax with alpha-beta pruning; `nodes` drains toward zero and falls
/// back to the static evaluation once exhausted. Every non-terminal node
/// reached here has at least one legal move for its side to move.
fn negamax(
    game: &Reversi,
    depth: i32,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    nodes: &mut u32,
) -> i32 {
    if depth == 0 {
        return evaluate(game);
    }
    if *nodes == 0 {
        return evaluate(game);
    }
    *nodes -= 1;
    // Corners first: strong squares early produce deep cutoffs.
    let mut moves = game.legal_moves();
    moves.sort_by_key(|&mv| std::cmp::Reverse(square_weight(mv)));
    let mut best = -INF;
    for mv in moves {
        let score = score_child(game, mv, depth - 1, ply + 1, alpha, beta, nodes);
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
/// Easy scores one ply deep and shuffles among its six best options;
/// Medium/Hard shuffle only to break ties, then play strictly optimal moves.
pub fn best_move(game: &Reversi, difficulty: Difficulty) -> Option<usize> {
    if game.status() != Status::Ongoing {
        return None;
    }
    let mut root = game.legal_moves();
    if root.is_empty() {
        return None;
    }
    root.shuffle(&mut thread_rng());
    let depth = depth_of(difficulty);
    let budget = if difficulty == Difficulty::Expert {
        EXPERT_NODE_BUDGET
    } else {
        NODE_BUDGET
    };

    if difficulty == Difficulty::Easy {
        let mut scored: Vec<(usize, i32)> = root
            .into_iter()
            .map(|mv| {
                let mut nodes = budget;
                (
                    mv,
                    score_child(game, mv, depth - 1, 1, -INF, INF, &mut nodes),
                )
            })
            .collect();
        scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        let pool_len = EASY_POOL.min(scored.len());
        return scored[..pool_len]
            .choose(&mut thread_rng())
            .map(|&(mv, _)| mv);
    }

    let mut nodes = budget;
    let mut best = root[0];
    let mut alpha = -INF;
    for &mv in &root {
        let score = score_child(game, mv, depth - 1, 1, alpha, INF, &mut nodes);
        if score > alpha {
            alpha = score;
            best = mv;
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easy_easy_playouts_stay_legal_to_completion() {
        for run in 0..5 {
            let mut game = Reversi::new();
            let mut plies = 0usize;
            while game.status() == Status::Ongoing {
                let Some(mv) = best_move(&game, Difficulty::Easy) else {
                    panic!("ongoing game must offer a move (run {run}, ply {plies})");
                };
                assert!(
                    game.legal_moves().contains(&mv),
                    "run {run} ply {plies}: illegal suggestion {mv}"
                );
                assert!(game.play(mv));
                plies += 1;
                assert!(plies <= 64, "run {run} exceeded square count");
            }
            assert!(matches!(game.status(), Status::Won(_) | Status::Draw));
        }
    }

    #[test]
    fn medium_prefers_corner_when_available() {
        // a1 sits open behind three white discs on the top rank; the only
        // alternative is the poisoned b2 X-square, which hands white the
        // corner instead. Seizing a1 strands white's last disc and wins
        // outright, so every strength must take it.
        let state = Reversi::set_board(
            &[
                ((0, 1), Side::White),
                ((0, 2), Side::White),
                ((0, 3), Side::White),
                ((0, 4), Side::Black),
                ((2, 2), Side::White),
                ((3, 3), Side::Black),
            ],
            Side::Black,
        );
        assert_eq!(
            state.legal_moves(),
            vec![sq(0, 0), sq(1, 1)],
            "exactly the corner and the X-square are available"
        );
        // Easy samples a random pool on purpose; strict play must convert.
        for difficulty in [Difficulty::Medium, Difficulty::Hard] {
            assert_eq!(
                best_move(&state, difficulty),
                Some(sq(0, 0)),
                "{difficulty:?} must seize a1"
            );
        }
    }

    #[test]
    fn finished_games_yield_no_move() {
        let state = Reversi::set_board(&[((0, 0), Side::Black)], Side::Black);
        assert_eq!(state.status(), Status::Won(Side::Black));
        for difficulty in Difficulty::ALL {
            assert_eq!(best_move(&state, difficulty), None);
        }
    }
}
