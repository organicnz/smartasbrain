//! Pure search for the built-in morris AI: negamax with alpha-beta pruning.
//!
//! No TUI dependencies live here. Captures ride inside the same ply: a
//! place/move that closes a mill fans out over every legal removal before
//! handing the position to the opponent.

use game_core::Difficulty;
use rand::Rng;
use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::engine::{ADJACENCY, Action, MILLS, Morris, POINTS, Phase, Side, Status};

/// Score magnitude for a decided game; shaved by ply so faster wins rate higher.
const MATE: i32 = 100_000;
/// Window bound safely beyond any reachable score.
const INF: i32 = 2 * MATE;

fn depth_of(difficulty: Difficulty) -> i32 {
    match difficulty {
        Difficulty::Easy => 1,
        Difficulty::Medium => 3,
        Difficulty::Hard => 5,
    }
}

fn stones_of(game: &Morris, side: Side) -> Vec<usize> {
    (0..POINTS)
        .filter(|&i| game.point(i) == Some(side))
        .collect()
}

fn empties(game: &Morris) -> usize {
    (0..POINTS).filter(|&i| game.point(i).is_none()).count()
}

/// Mills fully owned by `side`.
fn mills_owned(game: &Morris, side: Side) -> i32 {
    MILLS
        .iter()
        .filter(|m| m.iter().all(|&p| game.point(p) == Some(side)))
        .count() as i32
}

/// Two own stones plus one empty on a mill line: one stone from closing it.
fn open_two(game: &Morris, side: Side) -> i32 {
    MILLS
        .iter()
        .filter(|m| {
            let mine = m.iter().filter(|&&p| game.point(p) == Some(side)).count();
            let void = m.iter().filter(|&&p| game.point(p).is_none()).count();
            mine == 2 && void == 1
        })
        .count() as i32
}

/// `side` stones with nowhere to slide; a flying side is never blocked.
fn blocked_stones(game: &Morris, side: Side) -> i32 {
    if game.stones_left(side) > 0 || game.flying(side) {
        return 0;
    }
    stones_of(game, side)
        .iter()
        .filter(|&&i| ADJACENCY[i].iter().all(|&j| game.point(j).is_some()))
        .count() as i32
}

/// Slide options summed over `side`'s stones; placement counts every empty
/// and flight counts every stone-empty pair. The cap keeps those three
/// counting regimes on one evaluation scale.
fn mobility(game: &Morris, side: Side) -> i32 {
    let free = empties(game) as i32;
    let stones = stones_of(game, side);
    let pairs = if game.stones_left(side) > 0 {
        free
    } else if game.flying(side) {
        free * stones.len().max(1) as i32
    } else {
        stones
            .iter()
            .map(|&i| {
                ADJACENCY[i]
                    .iter()
                    .filter(|&&j| game.point(j).is_none())
                    .count() as i32
            })
            .sum()
    };
    pairs.min(10)
}

/// Static evaluation from the side-to-move perspective.
fn evaluate(game: &Morris) -> i32 {
    let me = game.turn();
    let foe = me.opposite();
    let mine = stones_of(game, me).len() as i32 + i32::from(game.stones_left(me));
    let theirs = stones_of(game, foe).len() as i32 + i32::from(game.stones_left(foe));
    (mine - theirs) * 20
        + (mills_owned(game, me) - mills_owned(game, foe)) * 15
        + blocked_stones(game, foe) * 6
        + (open_two(game, me) - open_two(game, foe)) * 4
        + (mobility(game, me) - mobility(game, foe)) * 2
}

/// Every place/move action for the side to move; empty once the game is over.
fn actions(game: &Morris) -> Vec<Action> {
    let mut out = Vec::new();
    if game.status() != Status::Ongoing || game.removal_pending() {
        return out;
    }
    if game.phase() == Phase::Place {
        for i in 0..POINTS {
            if game.point(i).is_none() {
                out.push(Action::Place(i));
            }
        }
    } else {
        for i in stones_of(game, game.turn()) {
            for dest in game.legal_moves_from(i) {
                out.push(Action::Move(i, dest));
            }
        }
    }
    out
}

/// Children of `game` after `action`; mill-forming actions fan out over every
/// legal capture so removal consequences stay inside one ply.
fn expand(game: &Morris, action: Action) -> Vec<Morris> {
    let mut base = game.clone();
    match action {
        Action::Place(i) => {
            base.place(i);
        }
        Action::Move(from, to) => {
            base.move_stone(from, to);
        }
        Action::Remove(_) => {}
    }
    if base.removal_pending() {
        base.removable_points()
            .into_iter()
            .map(|r| {
                let mut child = base.clone();
                child.remove(r);
                child
            })
            .collect()
    } else {
        vec![base]
    }
}

fn negamax(game: &Morris, depth: i32, ply: i32, mut alpha: i32, beta: i32) -> i32 {
    if game.status() != Status::Ongoing {
        // The side to move here is always the defeated one.
        return -(MATE - ply);
    }
    if depth <= 0 {
        return evaluate(game);
    }
    let moves = actions(game);
    if moves.is_empty() {
        return evaluate(game);
    }
    let mut best = -INF;
    for action in moves {
        for child in expand(game, action) {
            let score = -negamax(&child, depth - 1, ply + 1, -beta, -alpha);
            if score > best {
                best = score;
            }
            if best > alpha {
                alpha = best;
            }
            if alpha >= beta {
                return best;
            }
        }
    }
    best
}

/// Score every root action exactly and pick uniformly among the top four
/// (Easy) or walk the root with a rising window and take the optimum.
fn pick_root(game: &Morris, root: Vec<Action>, depth: i32, difficulty: Difficulty) -> Action {
    let mut rng = thread_rng();
    if difficulty == Difficulty::Easy {
        let mut scored: Vec<(Action, i32)> = root
            .into_iter()
            .map(|action| {
                let score = expand(game, action)
                    .into_iter()
                    .map(|child| -negamax(&child, depth - 1, 1, -INF, INF))
                    .max()
                    .unwrap_or(-INF);
                (action, score)
            })
            .collect();
        scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
        let top = scored.len().min(4);
        scored[rng.gen_range(0..top)].0
    } else {
        let mut best = root[0];
        let mut alpha = -INF;
        for action in root {
            for child in expand(game, action) {
                let score = -negamax(&child, depth - 1, 1, -INF, -alpha);
                if score > alpha {
                    alpha = score;
                    best = action;
                }
            }
        }
        best
    }
}

/// Next place/move for the side to move, or `None` while a capture is owed
/// or the game has ended.
pub fn best_action(game: &Morris, difficulty: Difficulty) -> Option<Action> {
    if game.status() != Status::Ongoing || game.removal_pending() {
        return None;
    }
    let mut root = actions(game);
    if root.is_empty() {
        return None;
    }
    // Shuffle first so ties and Easy picks vary between runs.
    root.shuffle(&mut thread_rng());
    Some(pick_root(game, root, depth_of(difficulty), difficulty))
}

/// Whose stone to take while [`Morris::removal_pending`] holds, honouring the
/// mill-exemption rule by construction of `removable_points`.
pub fn best_removal(game: &Morris, difficulty: Difficulty) -> Option<usize> {
    if !game.removal_pending() {
        return None;
    }
    let mut options = game.removable_points();
    if options.is_empty() {
        return None;
    }
    options.shuffle(&mut thread_rng());
    let depth = depth_of(difficulty).saturating_sub(1);
    let mut rng = thread_rng();
    if difficulty == Difficulty::Easy {
        let mut scored: Vec<(usize, i32)> = options
            .iter()
            .map(|&r| {
                let mut child = game.clone();
                child.remove(r);
                (r, -negamax(&child, depth, 1, -INF, INF))
            })
            .collect();
        scored.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
        let top = scored.len().min(4);
        Some(scored[rng.gen_range(0..top)].0)
    } else {
        let mut best = options[0];
        let mut alpha = -INF;
        for &r in &options {
            let mut child = game.clone();
            child.remove(r);
            let score = -negamax(&child, depth, 1, -INF, -alpha);
            if score > alpha {
                alpha = score;
                best = r;
            }
        }
        Some(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Side::White;

    #[test]
    fn ai_completes_mill_then_removes_protected_pool_correctly() {
        // Every white stone is frozen except two sliders that both land on 8
        // and close 6-7-8, so whatever the evaluation tastes like, the search
        // must hand back a mill completion. Black's 3-4-5 and 5-13-20 mills
        // shield most of its force, pinning the capture to the free stones.
        let g = Morris::set_position(
            &[
                (6, Side::White),
                (7, Side::White),
                (12, Side::White),
                (16, Side::White),
                (3, Side::Black),
                (4, Side::Black),
                (5, Side::Black),
                (11, Side::Black),
                (13, Side::Black),
                (15, Side::Black),
                (17, Side::Black),
                (19, Side::Black),
                (20, Side::Black),
            ],
            [0, 0],
            White,
            false,
        );
        assert_eq!(g.removable_points(), Vec::<usize>::new());
        assert!(
            best_removal(&g, Difficulty::Medium).is_none(),
            "nothing pending"
        );
        let action = best_action(&g, Difficulty::Medium).expect("moves available");
        assert!(
            matches!(action, Action::Move(_, 8)),
            "only the mill-completing slides exist, got {action:?}"
        );

        let mut g = g;
        match action {
            Action::Move(from, to) => assert!(g.move_stone(from, to)),
            _ => unreachable!("checked above"),
        }
        assert!(g.removal_pending(), "mill closed, capture owed");
        let legal = g.removable_points();
        assert_eq!(legal, vec![11, 15, 17, 19], "mill stones shielded");
        let target = best_removal(&g, Difficulty::Medium).expect("capture due");
        assert!(legal.contains(&target), "exemption respected");
        assert!(g.remove(target));
        assert!(!g.removal_pending());
        // Play passes to black, who still gets ordinary place/move answers.
        let reply = best_action(&g, Difficulty::Medium);
        assert!(
            matches!(reply, Some(Action::Place(_)) | Some(Action::Move(_, _))),
            "black must receive an action, got {reply:?}"
        );
    }

    #[test]
    fn ai_removal_picks_only_legal_targets_even_when_all_are_shielded() {
        // Black's whole force lives inside 9-10-11: the shield drops and any
        // of its stones becomes fair game.
        let g = Morris::set_position(
            &[
                (0, Side::White),
                (1, Side::White),
                (2, Side::White),
                (16, Side::White),
                (9, Side::Black),
                (10, Side::Black),
                (11, Side::Black),
            ],
            [0, 0],
            White,
            true,
        );
        assert_eq!(g.removable_points(), vec![9, 10, 11]);
        for _ in 0..20 {
            let target = best_removal(&g, Difficulty::Easy).expect("capture due");
            assert!(
                [9usize, 10, 11].contains(&target),
                "picked protected stone {target}"
            );
            assert!(g.point(target) == Some(Side::Black));
        }
    }

    #[test]
    fn easy_vs_easy_playouts_stay_legal_and_finish() {
        // Easy shuffles safely once ahead, so every tenth step takes a random
        // legal action to shake oscillations; the win must still arrive.
        let mut rng = thread_rng();
        for _ in 0..10 {
            let mut g = Morris::new();
            let mut finished = false;
            for _step in 0..5000 {
                match g.status() {
                    Status::Won(_) => {
                        finished = true;
                        break;
                    }
                    Status::Ongoing => {}
                }
                if g.removal_pending() {
                    let legal = g.removable_points();
                    let target = if rng.gen_bool(0.1) {
                        legal[usize::from(rng.gen_bool(0.5)) % legal.len()]
                    } else {
                        best_removal(&g, Difficulty::Easy)
                            .expect("pending capture must offer a target")
                    };
                    assert!(legal.contains(&target), "illegal capture {target}");
                    assert!(g.remove(target), "engine refused its own suggestion");
                } else {
                    let action = if rng.gen_bool(0.1) {
                        random_action(&g)
                    } else {
                        best_action(&g, Difficulty::Easy)
                            .expect("ongoing position without any action")
                    };
                    match action {
                        Action::Place(i) => assert!(g.place(i), "illegal place {i}"),
                        Action::Move(from, to) => {
                            assert!(g.move_stone(from, to), "illegal move {from}->{to}");
                        }
                        Action::Remove(_) => panic!("action slot returned a capture"),
                    }
                }
            }
            assert!(finished, "easy duel with shakes never reached a win");
        }
    }

    /// Uniformly random legal place/move, for stall-breaking only.
    fn random_action(game: &Morris) -> Action {
        let pool = actions(game);
        pool[thread_rng().gen_range(0..pool.len())]
    }

    #[test]
    fn hard_search_terminates_quickly_from_the_start() {
        let start = std::time::Instant::now();
        let chosen = best_action(&Morris::new(), Difficulty::Hard);
        let elapsed = start.elapsed();
        assert!(matches!(chosen, Some(Action::Place(_))));
        assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");
    }
}
