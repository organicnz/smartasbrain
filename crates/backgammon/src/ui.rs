//! Board rendering: 12 point columns in two mirrored halves, a center bar,
//! off trays, and an info panel with roll/restart buttons.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use game_core::geom::{centered_rect, in_rect};

use super::Button;
use super::engine::{BAR, Backgammon, Dest, OFF, Phase, Side};

const BG: Color = Color::Rgb(13, 15, 23);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const WHITE_FG: Color = Color::Rgb(226, 230, 244);
const BLACK_FG: Color = Color::Rgb(140, 150, 175);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const CROSS_BG: Color = Color::Rgb(44, 50, 68);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);

// Setup-menu palette, one shade of hell hotter than the board.
const FIRE: Color = Color::Rgb(178, 34, 34);
const BONE: Color = Color::Rgb(216, 208, 194);
const SETUP_SELECT_BG: Color = Color::Rgb(88, 16, 14);
const SETUP_HOVER_BG: Color = Color::Rgb(54, 22, 18);

/// One checker chip; both sides draw `●`, told apart by color.
const CHIP: char = '\u{25cf}';
/// Destination marker on a landing row or the off tray.
const DEST_MARK: char = '\u{25c8}';
/// Subtle marker on movable sources when nothing is selected.
const MOVE_DOT: char = '\u{00b7}';

pub const BOARD_INNER_W: u16 = 30;
pub const BOARD_BLOCK_W: u16 = BOARD_INNER_W + 2;
pub const BOARD_BLOCK_H: u16 = 13;
const HALF_ROWS: usize = 5;
const PANEL_W: u16 = 26;
/// turn / dice / moves / mode / pips / off / status.
const PANEL_LINES: u16 = 7;
const PANEL_BLOCK_H: u16 = PANEL_LINES + 2;

/// Rows of the setup menu, in confirm order.
pub(crate) const MENU_ITEMS: [&str; 7] = [
    "TWO PLAYERS",
    "VS AI - EASY",
    "VS AI - MEDIUM",
    "VS AI - HARD",
    "AI DUEL - EASY",
    "AI DUEL - MEDIUM",
    "AI DUEL - HARD",
];

/// A clickable board region.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Point(usize),
    Bar,
    Off,
}

impl Slot {
    pub(crate) fn index(self) -> usize {
        match self {
            Slot::Point(i) => i,
            Slot::Bar => BAR,
            Slot::Off => OFF,
        }
    }
}

/// Screen geometry captured during draw so clicks map to slots.
#[derive(Clone, Default)]
pub struct Geom {
    slots: Vec<(Rect, Slot)>,
}

impl Geom {
    pub(crate) fn slot_at(&self, col: u16, row: u16) -> Option<Slot> {
        self.slots
            .iter()
            .find(|(r, _)| in_rect(*r, col, row))
            .map(|(_, s)| *s)
    }

    /// All `(rect, slot)` hit regions in draw order.
    #[cfg(test)]
    pub(crate) fn rects(&self) -> &[(Rect, Slot)] {
        &self.slots
    }
}

/// Per-frame view state the shell owns between draws.
#[derive(Clone, Copy, Default)]
pub struct View {
    pub cursor: usize,
    pub selected: Option<usize>,
    pub hover: Option<(u16, u16)>,
}

fn col_x(c: usize) -> u16 {
    if c < 6 {
        (c * 2) as u16
    } else {
        16 + ((c - 6) * 2) as u16
    }
}

/// Column of a point inside its half; top half holds points 12..23.
fn point_col(idx: usize) -> (usize, bool) {
    if idx >= 12 {
        (idx - 12, true)
    } else {
        (11 - idx, false)
    }
}

/// Inner-row of stack position `k` counted from the owning edge.
fn stack_row(k: usize, top: bool) -> usize {
    if top { k } else { 10 - k }
}

/// Opponent context rendered in the info panel.
#[derive(Clone, Copy)]
pub(crate) struct ModeInfo<'a> {
    /// "two players" or e.g. "vs ai HARD (black)".
    pub(crate) label: &'a str,
    /// Seat shown by the thinking glyph: the solo AI's side in vs-AI, or the
    /// seat currently to play in duels (the glyph tracks whichever machine
    /// owns the turn).
    pub(crate) ai_side: Side,
    /// True while the AI owns the current turn.
    pub(crate) ai_thinking: bool,
}

pub fn draw(
    frame: &mut Frame,
    state: &Backgammon,
    area: Rect,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Too small to draw anything safely (also covers 0x0 ptty startup).
    if area.width < 34 || area.height < 15 {
        *geom_out = None;
        return;
    }

    let side =
        area.width >= BOARD_BLOCK_W + 2 + PANEL_W && area.height >= BOARD_BLOCK_H + PANEL_BLOCK_H;
    let cluster_w = if side {
        BOARD_BLOCK_W + 2 + PANEL_W
    } else {
        BOARD_BLOCK_W.max(PANEL_W)
    };
    let cluster_h = if side {
        BOARD_BLOCK_H
    } else {
        BOARD_BLOCK_H + 1 + PANEL_BLOCK_H
    };

    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(BOARD_BLOCK_W),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    draw_board(frame, cols[0], state, view, geom_out);

    let panel = if side {
        cols[2]
    } else {
        Rect {
            x: cluster.x,
            y: cluster.y.saturating_add(BOARD_BLOCK_H + 1),
            width: PANEL_W.min(area.width),
            height: PANEL_BLOCK_H.min(area.height.saturating_sub(BOARD_BLOCK_H + 1)),
        }
    };
    draw_panel(frame, panel, state, view.hover, mode, buttons);
}

/// Modal opponent picker shown instead of the board until a mode confirms.
/// Item hit rects are rebuilt into `items_out` on every draw.
pub(crate) fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    items_out: &mut Vec<(Rect, usize)>,
) {
    items_out.clear();
    if area.width < 36 || area.height < 11 {
        return;
    }
    let popup = centered_rect(area, 36, 11);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " backgammon setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, label) in MENU_ITEMS.iter().enumerate() {
        let rect = Rect {
            x: inner.x,
            y: inner.y + i as u16,
            width: inner.width,
            height: 1,
        };
        items_out.push((rect, i));
        let hovered = hover.is_some_and(|(c, r)| in_rect(rect, c, r));
        let style = if i == selected {
            Style::default()
                .fg(BONE)
                .bg(SETUP_SELECT_BG)
                .add_modifier(Modifier::BOLD)
        } else if hovered {
            Style::default().fg(BONE).bg(SETUP_HOVER_BG)
        } else {
            Style::default().fg(BONE)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!(" {label} "), style)))
                .alignment(Alignment::Center),
            rect,
        );
    }

    let hint_y = inner.y + MENU_ITEMS.len() as u16 + 1;
    if hint_y < inner.bottom() {
        let hint = Rect {
            x: inner.x,
            y: hint_y,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "up/down · 1-7 · enter picks",
                Style::default().fg(DIM_FG),
            )))
            .alignment(Alignment::Center),
            hint,
        );
    }
}

#[derive(Clone)]
struct Cell {
    ch: char,
    style: Style,
}

impl Cell {
    fn blank() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
        }
    }
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    state: &Backgammon,
    view: View,
    geom_out: &mut Option<Geom>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " backgammon ",
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match state.phase() {
                    Phase::GameOver => String::new(),
                    _ => format!("{} to play ", side_glyph_turn(state.turn())),
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut grid = vec![vec![Cell::blank(); BOARD_INNER_W as usize]; 11];
    paint_background(&mut grid, view);
    paint_points(&mut grid, state);
    paint_bar(&mut grid, state);
    paint_trays(&mut grid, state, view);
    paint_markers(&mut grid, state, view);

    let lines: Vec<Line> = grid
        .iter()
        .map(|row| {
            Line::from(
                row.iter()
                    .map(|c| Span::styled(c.ch.to_string(), c.style))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);

    // Hit rects for every slot, rebuilt each draw (visual order: top half
    // 12..23 left->right, bottom half 11..0, bar, trays).
    let mut slots = Vec::new();
    for half_start in [12usize, 0] {
        for i in half_start..half_start + 12 {
            let (c, top) = point_col(i);
            let x = inner.x.saturating_add(col_x(c));
            let rows = if top { 0..HALF_ROWS } else { 6..11 };
            slots.push((
                Rect {
                    x,
                    y: inner.y.saturating_add(rows.start as u16),
                    width: 2,
                    height: rows.len() as u16,
                },
                Slot::Point(i),
            ));
        }
    }
    slots.push((
        Rect {
            x: inner.x.saturating_add(12),
            y: inner.y,
            width: 3,
            height: 11,
        },
        Slot::Bar,
    ));
    slots.push((
        Rect {
            x: inner.x.saturating_add(28),
            y: inner.y,
            width: 2,
            height: 11,
        },
        Slot::Off,
    ));
    *geom_out = Some(Geom { slots });
}

/// Strip highlight for cursor / selected source.
fn strip(grid: &mut [Vec<Cell>], x: u16, top: bool, bg: Color) {
    let rows = if top { 0..HALF_ROWS } else { 6..11 };
    for (r, row) in grid.iter_mut().enumerate() {
        if !rows.contains(&r) {
            continue;
        }
        for dx in 0..2u16 {
            let cx = (x + dx) as usize;
            if cx < BOARD_INNER_W as usize {
                row[cx].style = row[cx].style.bg(bg);
            }
        }
    }
}

fn full_strip(grid: &mut [Vec<Cell>], x0: u16, w: u16, bg: Color) {
    for row in grid.iter_mut() {
        for dx in 0..w {
            let cx = (x0 + dx) as usize;
            if cx < BOARD_INNER_W as usize {
                row[cx].style = row[cx].style.bg(bg);
            }
        }
    }
}

fn paint_background(grid: &mut [Vec<Cell>], view: View) {
    let strip_for = |g: &mut [Vec<Cell>], slot_idx: usize, bg: Color| match slot_idx {
        BAR => full_strip(g, 12, 3, bg),
        OFF => full_strip(g, 28, 2, bg),
        i => {
            let (c, top) = point_col(i);
            strip(g, col_x(c), top, bg);
        }
    };
    if let Some(sel) = view.selected {
        strip_for(grid, sel, SELECT_BG);
    }
    strip_for(grid, view.cursor, CROSS_BG);
    // Selected wins over cursor when they collide on the same strip.
    if let Some(sel) = view.selected {
        strip_for(grid, sel, SELECT_BG);
    }
}

/// Chip symbol for stack position `k` given `n` checkers (>5 shows a digit).
fn stack_cell(n: usize, k: usize) -> Option<(char, bool)> {
    if n == 0 || k >= HALF_ROWS {
        return None;
    }
    if n > HALF_ROWS && k == HALF_ROWS - 1 {
        let rem = n - (HALF_ROWS - 1);
        let ch = if rem > 9 {
            '9'
        } else {
            std::char::from_digit(rem as u32, 10).unwrap_or('9')
        };
        return Some((ch, true));
    }
    if k < n { Some((CHIP, false)) } else { None }
}

fn put(grid: &mut [Vec<Cell>], r: usize, x: u16, ch: char, fg: Color, bold: bool) {
    if r < grid.len() && (x as usize) < BOARD_INNER_W as usize {
        let cell = &mut grid[r][x as usize];
        // Preserve any strip highlight already painted underneath.
        let mut style = Style::default().fg(fg);
        style.bg = cell.style.bg;
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        cell.ch = ch;
        cell.style = style;
    }
}

fn paint_points(grid: &mut [Vec<Cell>], state: &Backgammon) {
    for i in 0..24usize {
        let v = state.points()[i];
        let (n, white) = if v > 0 {
            (v as usize, true)
        } else {
            ((-v) as usize, false)
        };
        if n == 0 {
            continue;
        }
        let (c, top) = point_col(i);
        let x = col_x(c);
        let fg = if white { WHITE_FG } else { BLACK_FG };
        for k in 0..HALF_ROWS {
            if let Some((ch, bold)) = stack_cell(n, k) {
                put(grid, stack_row(k, top), x, ch, fg, bold);
            }
        }
    }
}

fn paint_bar(grid: &mut [Vec<Cell>], state: &Backgammon) {
    let (bw, bb) = state.bar();
    let x = 13u16; // center column of the 3-wide bar
    for k in 0..HALF_ROWS {
        if let Some((ch, bold)) = stack_cell(bb as usize, k) {
            put(grid, k, x, ch, BLACK_FG, bold);
        }
        if let Some((ch, bold)) = stack_cell(bw as usize, k) {
            put(grid, stack_row(k, false), x, ch, WHITE_FG, bold);
        }
    }
}

fn paint_trays(grid: &mut [Vec<Cell>], state: &Backgammon, view: View) {
    let (ow, ob) = state.off();
    let text = |n: u8| {
        if n >= 10 {
            format!("{n}")
        } else {
            format!(" {n}")
        }
    };
    let mut chars_b = text(ob);
    let mut chars_w = text(ow);
    let off_marked = view
        .selected
        .is_some_and(|src| state.destinations(src).iter().any(|(d, _)| *d == Dest::Off));
    if off_marked {
        match state.turn() {
            Side::Black => chars_b = format!("{} ", DEST_MARK),
            Side::White => chars_w = format!("{} ", DEST_MARK),
        }
    }
    for (dx, ch) in chars_b.chars().enumerate() {
        put(grid, 2, 28 + dx as u16, ch, BLACK_FG, true);
    }
    for (dx, ch) in chars_w.chars().enumerate() {
        put(grid, 8, 28 + dx as u16, ch, WHITE_FG, true);
    }
}

/// Landing-row destination markers and movable-source dots.
fn paint_markers(grid: &mut [Vec<Cell>], state: &Backgammon, view: View) {
    match view.selected {
        Some(src) => {
            for (dest, _) in state.destinations(src) {
                match dest {
                    Dest::Point(i) => {
                        let n = state.points()[i].unsigned_abs() as usize;
                        let k = n.min(HALF_ROWS - 1);
                        let (c, top) = point_col(i);
                        put(
                            grid,
                            stack_row(k, top),
                            col_x(c),
                            DEST_MARK,
                            ACCENT_FG,
                            true,
                        );
                    }
                    Dest::Off => {} // drawn by paint_trays
                }
            }
        }
        None => {
            if state.phase() != Phase::Move {
                return;
            }
            for src in state.movable() {
                match src {
                    BAR => {
                        let r = match state.turn() {
                            Side::White => 10,
                            Side::Black => 0,
                        };
                        put(grid, r, 13, MOVE_DOT, FAINT_FG, false);
                    }
                    i => {
                        let n = state.points()[i].unsigned_abs() as usize;
                        let k = n.min(HALF_ROWS - 1);
                        let (c, top) = point_col(i);
                        put(grid, stack_row(k, top), col_x(c), MOVE_DOT, FAINT_FG, false);
                    }
                }
            }
        }
    }
}

fn side_glyph(side: Side) -> &'static str {
    match side {
        Side::White => "\u{25cb}",
        Side::Black => "\u{25cf}",
    }
}

fn side_glyph_turn(side: Side) -> String {
    format!("{} {}", side_glyph(side), side_name(side))
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::White => "white",
        Side::Black => "black",
    }
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    state: &Backgammon,
    hover: Option<(u16, u16)>,
    mode: ModeInfo<'_>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(PANEL_BORDER_FG))
        .style(Style::default().bg(PANEL_BG))
        .title(Line::from(Span::styled(
            " info ",
            Style::default().fg(ACCENT_FG),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let kv = |key: &str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(format!("{key:<6}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };

    let dice_text = if state.phase() == Phase::Roll {
        "-".to_string()
    } else {
        format!("{}-{}", state.dice()[0], state.dice()[1])
    };
    let status = if mode.ai_thinking && state.winner().is_none() {
        Line::from(Span::styled(
            format!("{} thinking...", side_glyph(mode.ai_side)),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ))
    } else {
        match state.winner() {
            Some(w) => Line::from(Span::styled(
                format!("{} wins!", side_name(w)),
                Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
            )),
            None => Line::from(Span::styled(
                match state.phase() {
                    Phase::Roll => "press r to roll",
                    Phase::Move => "select a checker",
                    Phase::GameOver => "",
                },
                Style::default().fg(DIM_FG),
            )),
        }
    };

    let lines = vec![
        kv(
            "turn",
            Span::styled(
                side_glyph_turn(state.turn()),
                Style::default().fg(WHITE_FG).add_modifier(Modifier::BOLD),
            ),
        ),
        kv(
            "dice",
            Span::styled(dice_text, Style::default().fg(WHITE_FG)),
        ),
        kv(
            "mode",
            Span::styled(mode.label.to_string(), Style::default().fg(BONE)),
        ),
        kv(
            "moves",
            Span::styled(
                format!("{}", state.left().len()),
                Style::default().fg(WHITE_FG),
            ),
        ),
        kv(
            "pips",
            Span::styled(
                format!(
                    "\u{25cb}{} \u{25cf}{}",
                    state.pip(Side::White),
                    state.pip(Side::Black)
                ),
                Style::default().fg(WHITE_FG),
            ),
        ),
        kv(
            "off",
            Span::styled(
                format!("\u{25cb}{} \u{25cf}{}", state.off().0, state.off().1),
                Style::default().fg(WHITE_FG),
            ),
        ),
        status,
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    // Buttons along the bottom of the panel.
    if area.height < 2 {
        return;
    }
    let btn_y = area.y + area.height - 2;
    let half = inner.width / 2;
    let can_roll = state.phase() == Phase::Roll;
    let specs = [("roll", Button::Roll), ("restart", Button::Restart)];
    for (i, (label, button)) in specs.iter().enumerate() {
        let rect = Rect {
            x: inner.x + i as u16 * half,
            y: btn_y,
            width: half.max(4),
            height: 1,
        };
        buttons.push((rect, *button));
        let hovered = hover.is_some_and(|(c, r)| in_rect(rect, c, r));
        let base = if *button == Button::Roll && !can_roll {
            Style::default().fg(DIM_FG).bg(BUTTON_BG)
        } else {
            Style::default()
                .fg(ACCENT_FG)
                .bg(if hovered { HOVER_BG } else { BUTTON_BG })
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!(" {label} "), base)))
                .alignment(Alignment::Center),
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BackgammonGame;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use game_core::Game;
    use ratatui::backend::TestBackend;

    fn started_two_player() -> BackgammonGame {
        let mut game = BackgammonGame::new();
        game.handle_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Enter,
        ));
        game
    }

    fn render_at(w: u16, h: u16) {
        for setup_open in [true, false] {
            let mut game = BackgammonGame::new();
            if !setup_open {
                game = started_two_player();
            }
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| Game::draw(&mut game, f, f.area()))
                .unwrap();
        }
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [
            (0u16, 0u16),
            (30, 10),
            (40, 14),
            (60, 20),
            (80, 24),
            (120, 40),
        ] {
            render_at(w, h);
        }
    }

    #[test]
    fn slot_hit_test_round_trips() {
        let mut game = started_two_player();
        let backend = TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
        let geom = game.geom.clone().expect("geom captured at 100x30");
        assert_eq!(geom.rects().len(), 26, "24 points + bar + off");
        for (rect, slot) in geom.rects() {
            let cx = rect.x + rect.width / 2;
            let cy = rect.y + rect.height / 2;
            assert_eq!(geom.slot_at(cx, cy), Some(*slot), "{slot:?} center");
        }
        assert_eq!(geom.rects()[0].1, Slot::Point(12), "top-left is point 12");
        // Bottom-left is point 0 (bottom half runs 11..0 left->right).
        let p0 = geom
            .rects()
            .iter()
            .find(|(_, s)| *s == Slot::Point(0))
            .expect("point 0 present");
        assert!(
            p0.0.y > geom.rects()[0].0.y,
            "point 0 sits in the bottom half"
        );
    }

    #[test]
    fn integration_select_then_move_via_mouse_shrinks_left() {
        let mut game = started_two_player();
        game.state.roll_with(3, 1); // black to move; 23 -> 20 open with die 3
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();

        let geom = game.geom.clone().expect("geom captured at 80x24");
        let rect_of = |s: Slot| -> Rect {
            geom.rects()
                .iter()
                .find(|(_, slot)| *slot == s)
                .map(|(r, _)| *r)
                .expect("slot rect")
        };
        let click = |g: &mut BackgammonGame, col: u16, row: u16| {
            g.handle_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col,
                row,
                modifiers: KeyModifiers::empty(),
            });
        };

        let p23 = rect_of(Slot::Point(23));
        click(&mut game, p23.x, p23.y + p23.height / 2);
        assert_eq!(game.selected, Some(23), "source selected via mouse");

        let p20 = rect_of(Slot::Point(20));
        click(&mut game, p20.x, p20.y + p20.height / 2);
        assert_eq!(game.selected, None, "deselected after the move");
        assert_eq!(game.state.left().len(), 1, "one die consumed of two");
        assert_eq!(game.state.dice(), [3, 1]);
        assert_eq!(game.state.points()[20], -1);
    }
}
