//! Heuristic opponent for Go. Heuristic, not strong — it counts liberties,
//! captures and contact fights with fixed weights, and blunders on purpose
//! at lower difficulties. Plenty for a casual TUI match, nothing more.

use game_core::Difficulty;
use rand::seq::SliceRandom;
use rand::{Rng, thread_rng};

use super::engine::{GoState, MoveResult, Player};
use super::engine::{group_and_liberties, neighbors};

const CAPTURE_WEIGHT: isize = 30;
const SAVE_WEIGHT: isize = 25;
const ATARI_WEIGHT: isize = 12;
const ATARI_SIZE_WEIGHT: isize = 4;
const LIBERTY_WEIGHT: isize = 2;
const ENEMY_CONTACT_WEIGHT: isize = 3;
const FRIENDLY_CONTACT_WEIGHT: isize = 1;
const HOSHI_BONUS: isize = 2;
const SELF_ATARI_PENALTY: isize = 45;
const EYE_FILL_PENALTY: isize = 60;
const SURROUNDED_LATE_PENALTY: isize = 5;

/// How many top-scored moves Easy may wander among.
const EASY_TOP: usize = 8;
/// Chance Easy ignores scoring entirely and plays something legal.
const EASY_BLUNDER_RATE: f64 = 0.20;

/// Hoshi (star) points of the 9×9 board.
const HOSHI_9: [(usize, usize); 5] = [(2, 2), (2, 6), (6, 2), (6, 6), (4, 4)];

/// One legal move plus everything scoring needs about it.
struct Candidate {
    idx: usize,
    /// Side that played this move.
    mover: Player,
    /// Position after playing here; legality already proven by simulation.
    after: GoState,
    /// Stones this move captures.
    captured: u32,
    /// True when the move lifts one of our groups out of atari.
    saved_atari: bool,
}

/// Picks an intersection for the side to move, or `None` to pass.
///
/// Candidates are found by simulation on cloned states, so the returned
/// index is always empty and legal for [`GoState::play`].
pub fn best_move(state: &GoState, difficulty: Difficulty) -> Option<usize> {
    let cands = candidates(state);
    if cands.is_empty() {
        return None;
    }
    match difficulty {
        Difficulty::Easy => easy_pick(state, &cands),
        Difficulty::Medium => medium_pick(state, &cands),
        Difficulty::Hard => hard_pick(state, cands),
        Difficulty::Expert => hard_pick(state, cands),
    }
}

fn easy_pick(state: &GoState, cands: &[Candidate]) -> Option<usize> {
    let mut rng = thread_rng();
    if rng.gen_bool(EASY_BLUNDER_RATE) {
        return cands.choose(&mut rng).map(|c| c.idx);
    }
    let mut ranked: Vec<&Candidate> = cands.iter().collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(score_candidate(state, c)));
    let top = ranked.len().min(EASY_TOP);
    ranked[..top].choose(&mut rng).map(|c| c.idx)
}

fn medium_pick(state: &GoState, cands: &[Candidate]) -> Option<usize> {
    let mut best: Option<(&Candidate, isize)> = None;
    for cand in cands {
        let s = score_candidate(state, cand);
        if best.is_none_or(|(_, top)| s > top) {
            best = Some((cand, s));
        }
    }
    let (cand, score) = best?;
    if should_pass(state, score, cands) {
        return None;
    }
    Some(cand.idx)
}

fn hard_pick(state: &GoState, cands: Vec<Candidate>) -> Option<usize> {
    let mut scored: Vec<(isize, Candidate)> = cands
        .into_iter()
        .map(|c| {
            let s = score_candidate(state, &c);
            (s, c)
        })
        .collect();
    scored.sort_by_key(|(s, _)| std::cmp::Reverse(*s));

    // One-ply refinement: discount moves that hand the opponent a big
    // recapture, valued like a capture so ko-ish giveaways lose appeal.
    for (score, cand) in scored.iter_mut().take(3) {
        *score -= best_reply_capture(&cand.after) as isize * CAPTURE_WEIGHT;
    }

    let best = scored
        .iter()
        .enumerate()
        .max_by_key(|(_, (s, _))| *s)
        .map(|(i, _)| i)?;
    if should_pass(state, scored[best].0, &[]) {
        return None;
    }
    scored.into_iter().nth(best).map(|(_, c)| c.idx)
}

/// Pass when nothing is worth doing. Tactical shots always override.
fn should_pass(state: &GoState, best_score: isize, cands: &[Candidate]) -> bool {
    let tactical = cands.iter().any(|c| c.captured > 0 || c.saved_atari);
    if tactical {
        return false;
    }
    if state.passes >= 1 && best_score <= 0 {
        // Opponent passed last and we have no plan either — settle the game.
        return true;
    }
    best_score <= 0
}

/// Every empty point where [`GoState::play`] would succeed, via clones.
fn candidates(state: &GoState) -> Vec<Candidate> {
    let size = state.size();
    let me = state.turn;
    let mut out = Vec::new();
    for idx in 0..size * size {
        if state.board()[idx] != Player::Empty {
            continue;
        }
        let mut trial = state.clone();
        if trial.play(idx) != MoveResult::Played {
            continue; // occupied / suicide / superko
        }
        let captured = match me {
            Player::Black => trial.captures_black - state.captures_black,
            _ => trial.captures_white - state.captures_white,
        };
        let saved_atari = rescues_atari_group(state, &trial, idx);
        out.push(Candidate {
            idx,
            mover: me,
            after: trial,
            captured,
            saved_atari,
        });
    }
    out
}

/// True when `idx` connects to a friendly group that had exactly one
/// liberty before the move and clearly more afterwards.
fn rescues_atari_group(before: &GoState, after: &GoState, idx: usize) -> bool {
    let size = before.size();
    let me = before.turn;
    for n in neighbors(size, idx) {
        if before.board()[n] != me {
            continue;
        }
        let (_, libs_before) = group_and_liberties(before.board(), size, n);
        if libs_before != 1 {
            continue;
        }
        let (_, libs_after) = group_and_liberties(after.board(), size, idx);
        if libs_after >= 2 {
            return true;
        }
    }
    false
}

/// Heuristic value of a candidate from the mover's perspective.
fn score_candidate(before: &GoState, cand: &Candidate) -> isize {
    let size = before.size();
    let cells_before = before.board();
    let cells_after = cand.after.board();
    let me = cand.mover;
    let opp = me.opponent();

    let mut s = cand.captured as isize * CAPTURE_WEIGHT;
    if cand.saved_atari {
        s += SAVE_WEIGHT;
    }

    // Enemy groups pushed into atari (each group counted once).
    let mut ataried_roots: Vec<usize> = Vec::new();
    for n in neighbors(size, cand.idx) {
        if cells_after[n] != opp {
            continue;
        }
        let (group, libs) = group_and_liberties(cells_after, size, n);
        if libs == 1 && !ataried_roots.contains(&group[0]) {
            s += ATARI_WEIGHT + group.len() as isize * ATARI_SIZE_WEIGHT;
            ataried_roots.push(group[0]);
        }
    }

    // Breathing room of our resulting group.
    let (own_group, own_libs) = group_and_liberties(cells_after, size, cand.idx);
    s += own_libs as isize * LIBERTY_WEIGHT;

    // Contact play looks natural.
    let mut enemy_neighbours = 0usize;
    for n in neighbors(size, cand.idx) {
        match cells_before[n] {
            p if p == opp => {
                enemy_neighbours += 1;
                s += ENEMY_CONTACT_WEIGHT;
            }
            p if p == me => s += FRIENDLY_CONTACT_WEIGHT,
            _ => {}
        }
    }

    // Opening shape: star points and a gentle pull toward the centre.
    let mid = size.saturating_sub(1) / 2;
    let (r, c) = (cand.idx / size, cand.idx % size);
    let on_hoshi =
        (size == 9 && HOSHI_9.contains(&(r, c))) || (size % 2 == 1 && r == mid && c == mid);
    if on_hoshi {
        s += HOSHI_BONUS;
    }
    let centre_dist = r.abs_diff(mid).max(c.abs_diff(mid));
    s -= centre_dist as isize;

    // Penalties.
    if own_libs == 1 {
        s -= SELF_ATARI_PENALTY;
    }
    if fills_own_eye(cells_before, size, cand.idx, me) {
        s -= EYE_FILL_PENALTY;
    }
    if is_late(before) && own_group.len() == 1 && enemy_neighbours >= 3 {
        s -= SURROUNDED_LATE_PENALTY;
    }

    s
}

/// All four orthogonal neighbours exist, belong to `me`, and every existing
/// diagonal belongs to `me` too — filling that would destroy a real eye.
fn fills_own_eye(cells: &[Player], size: usize, idx: usize, me: Player) -> bool {
    let orth: Vec<usize> = neighbors(size, idx).collect();
    if orth.len() < 4 || orth.iter().any(|&n| cells[n] != me) {
        return false;
    }
    let (r, c) = (idx / size, idx % size);
    for dr in [-1isize, 1] {
        for dc in [-1isize, 1] {
            let (rr, cc) = (r as isize + dr, c as isize + dc);
            if rr < 0 || cc < 0 || rr >= size as isize || cc >= size as isize {
                continue;
            }
            if cells[rr as usize * size + cc as usize] != me {
                return false;
            }
        }
    }
    true
}

/// Rough "endgame" test: fewer than a third of the points still empty.
fn is_late(state: &GoState) -> bool {
    let empties = state
        .board()
        .iter()
        .filter(|&&p| p == Player::Empty)
        .count();
    empties * 3 < state.size() * state.size()
}

/// Largest number of stones the side to move can capture right now.
fn best_reply_capture(state: &GoState) -> u32 {
    candidates(state)
        .iter()
        .map(|c| c.captured)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 9;

    const fn pt(r: usize, c: usize) -> usize {
        r * N + c
    }

    fn play_ok(s: &mut GoState, idx: usize) {
        assert_eq!(s.play(idx), MoveResult::Played, "setup move at {idx}");
    }

    #[test]
    fn ai_never_plays_illegal_or_occupied() {
        let mut state = GoState::new(N);
        let mut plies = 0usize;
        while plies < 120 && !state.over {
            let Some(idx) = best_move(&state, Difficulty::Easy) else {
                state.pass();
                continue;
            };
            assert_eq!(
                state.board()[idx],
                Player::Empty,
                "ply {plies} targeted an occupied point"
            );
            assert_eq!(state.play(idx), MoveResult::Played, "ply {plies}");
            plies += 1;
        }
        assert!(plies > 0, "the bot should have moved at least once");
    }

    #[test]
    fn ai_takes_free_capture() {
        let mut s = GoState::new(N);
        // White (0,0) ends up with its last liberty at (1,0); black to move.
        // Both black stones stay safe, and white's second move is far away,
        // so the capture is genuinely free (nothing to rescue, no recapture).
        play_ok(&mut s, pt(8, 8)); // B filler
        play_ok(&mut s, pt(0, 0)); // W target
        play_ok(&mut s, pt(0, 1)); // B north
        play_ok(&mut s, pt(0, 5)); // W far away
        assert_eq!(s.turn, Player::Black);

        for difficulty in [Difficulty::Medium, Difficulty::Hard] {
            let mut trial = s.clone();
            let mv = best_move(&trial, difficulty)
                .unwrap_or_else(|| panic!("{difficulty:?} should find a move"));
            assert_eq!(mv, pt(1, 0), "{difficulty:?} must capture at (1,0)");
            assert_eq!(trial.play(mv), MoveResult::Played);
            assert_eq!(trial.board()[pt(0, 0)], Player::Empty, "stone captured");
        }
    }

    #[test]
    fn ai_passes_when_board_full() {
        let cells: Vec<Player> = (0..N * N)
            .map(|i| {
                if i % 2 == 0 {
                    Player::Black
                } else {
                    Player::White
                }
            })
            .collect();
        let state = GoState::from_cells(N, cells, Player::Black);
        for difficulty in Difficulty::ALL {
            assert_eq!(best_move(&state, difficulty), None);
        }
    }
}
