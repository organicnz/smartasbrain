//! Board rendering: three nested squares with rail connectors, an info
//! panel with a restart button, and the modal setup menu.
//!
//! [`POINT_XY`] pins every point index to `(col, row)` offsets inside the
//! board block's inner area (17 wide, 11 tall):
//!
//! ```text
//! 0───────1───────2          outer ring rows 0 / 10, cols 0 / 16
//! │ ┌─3─┐ │ ┌─5─┐ │          mid ring   rows 2 / 8,  cols 2 / 14
//! │ │ 6─7─8 │ │ │           inner ring rows 4 / 6,  cols 4 / 12
//! 9─10─11  12─13─14          rails join the ring midpoints
//! ```

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use game_core::geom::{centered_rect, in_rect};

use super::Button;
use super::engine::{Action, Morris, POINTS, Phase, Side, Status};

const BG: Color = Color::Rgb(13, 15, 23);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const WHITE_FG: Color = Color::Rgb(226, 230, 244);
/// Doom palette: bone-white stones, fire trim, dried-blood selections.
const BONE: Color = Color::Rgb(216, 208, 194);
const FIRE: Color = Color::Rgb(178, 34, 34);
const SELECT_BG: Color = Color::Rgb(88, 16, 14);
const HOVER_BG: Color = Color::Rgb(54, 22, 18);
const CURSOR_BG: Color = Color::Rgb(66, 26, 20);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(232, 122, 68);
/// Second-seat stones: cold steel against bone.
const STEEL_FG: Color = Color::Rgb(140, 150, 175);

/// One stone; both sides draw `●`, told apart by colour.
const STONE: char = '\u{25cf}';
/// Empty point slot.
const DOT: char = '\u{00b7}';
const HORZ: char = '\u{2500}';
const VERT: char = '\u{2502}';
const CROSS: char = '\u{253c}';

pub const BOARD_INNER_W: u16 = 17;
pub const BOARD_INNER_H: u16 = 11;
pub const BOARD_BLOCK_W: u16 = BOARD_INNER_W + 2;
pub const BOARD_BLOCK_H: u16 = BOARD_INNER_H + 2;
pub const PANEL_W: u16 = 26;
/// turn / phase / stones / mode / status, plus the button row below.
const PANEL_LINES: u16 = 5;
const PANEL_BLOCK_H: u16 = PANEL_LINES + 3;

/// Point index -> (col, row) inside the board block's inner area. Rings sit
/// at rows 0/2/4-6/8/10 and cols 0/2/4-12/14/16 so all three squares stay
/// visible; rails link the ring midpoints down the centre cross.
pub const POINT_XY: [(u16, u16); POINTS] = [
    (0, 0),
    (8, 0),
    (16, 0),
    (2, 2),
    (8, 2),
    (14, 2),
    (4, 4),
    (8, 4),
    (12, 4),
    (0, 5),
    (2, 5),
    (4, 5),
    (12, 5),
    (14, 5),
    (16, 5),
    (4, 6),
    (8, 6),
    (12, 6),
    (2, 8),
    (8, 8),
    (14, 8),
    (0, 10),
    (8, 10),
    (16, 10),
];

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

/// Screen geometry captured during draw so clicks map to points.
#[derive(Clone, Copy, Default)]
pub struct Geom {
    inner: Rect,
    /// Ring-coordinate multiplier chosen by the resizable canvas.
    scale: u16,
}

impl Geom {
    /// Point under `(col, row)` when a playable spot sits within one scaled
    /// cell of it.
    pub(crate) fn slot_at(&self, col: u16, row: u16) -> Option<usize> {
        let s = self.scale.max(1);
        if !in_rect(self.inner, col, row) {
            return None;
        }
        let base_col = (col - self.inner.x) / s;
        let base_row = (row - self.inner.y) / s;
        nearest_point(i32::from(base_col), i32::from(base_row), i32::from(s))
    }

    #[cfg(test)]
    pub(crate) fn inner(&self) -> Rect {
        self.inner
    }
}

/// Closest point to base-space coordinates within Manhattan distance
/// `cutoff`, preferring exact hits. Mid-cell ties resolve to the lower
/// index. `cutoff` scales with the canvas so big boards stay forgiving.
fn nearest_point(local_col: i32, local_row: i32, cutoff: i32) -> Option<usize> {
    let mut best: Option<(i32, usize)> = None;
    for (i, &(x, y)) in POINT_XY.iter().enumerate() {
        let d = (i32::from(x) - local_col).abs() + (i32::from(y) - local_row).abs();
        if d <= cutoff && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}

/// Per-frame view state the shell owns between draws.
#[derive(Clone, Copy, Default)]
pub struct View {
    pub cursor: usize,
    pub selected: Option<usize>,
    pub hover: Option<(u16, u16)>,
    /// False while an automated seat owns the board; hides aim assistance.
    pub interactive: bool,
}

/// Opponent context rendered in the info panel.
#[derive(Clone, Copy)]
pub(crate) struct ModeInfo<'a> {
    /// "two players", "vs ai EASY (black)", ...
    pub(crate) label: &'a str,
    /// Seat shown by the thinking glyph: the solo AI's side in vs-AI, or
    /// whoever plays next in duels.
    pub(crate) ai_side: Side,
    pub(crate) ai_thinking: bool,
}

pub fn draw(
    frame: &mut Frame,
    state: &Morris,
    area: Rect,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Resizable canvas: scale the whole ring board up to triple size when
    // the terminal allows it; tiny terminals keep the base rendition.
    let mut scale = 1_u16;
    for &s in &[3_u16, 2] {
        if area.width >= BOARD_BLOCK_W * s && area.height >= BOARD_BLOCK_H * s {
            scale = s;
            break;
        }
    }
    let block_w = BOARD_BLOCK_W * scale;
    let block_h = BOARD_BLOCK_H * scale;

    let side = area.width >= block_w + 2 + PANEL_W && area.height >= block_h + PANEL_BLOCK_H;
    let cluster_w = if side {
        block_w + 2 + PANEL_W
    } else {
        block_w.max(PANEL_W)
    };
    let cluster_h = if side {
        block_h
    } else {
        block_h + 1 + PANEL_BLOCK_H
    };

    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(block_w),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    draw_board(frame, cols[0], state, view, geom_out, scale);

    let panel = if side {
        cols[2]
    } else {
        Rect {
            x: cluster.x,
            y: cluster.y.saturating_add(block_h + 1),
            width: PANEL_W.min(area.width),
            height: PANEL_BLOCK_H.min(area.height.saturating_sub(block_h + 1)),
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
            " morris setup ",
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
                .bg(SELECT_BG)
                .add_modifier(Modifier::BOLD)
        } else if hovered {
            Style::default().fg(BONE).bg(HOVER_BG)
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

fn put(
    grid: &mut [Vec<Cell>],
    r: usize,
    x: u16,
    ch: char,
    fg: Color,
    bold: bool,
    bg: Option<Color>,
) {
    if r < grid.len() && (x as usize) < BOARD_INNER_W as usize {
        let cell = &mut grid[r][x as usize];
        cell.ch = ch;
        let mut style = Style::default().fg(fg);
        style.bg = bg.or(cell.style.bg);
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        cell.style = style;
    }
}

fn hline(grid: &mut [Vec<Cell>], y: u16, x0: u16, x1: u16) {
    for x in x0..=x1 {
        let cell = &mut grid[y as usize][x as usize];
        cell.ch = if cell.ch == VERT { CROSS } else { HORZ };
        cell.style = Style::default().fg(FAINT_FG);
    }
}

fn vline(grid: &mut [Vec<Cell>], x: u16, y0: u16, y1: u16) {
    for y in y0..=y1 {
        let cell = &mut grid[y as usize][x as usize];
        cell.ch = if cell.ch == HORZ { CROSS } else { VERT };
        cell.style = Style::default().fg(FAINT_FG);
    }
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    state: &Morris,
    view: View,
    geom_out: &mut Option<Geom>,
    scale: u16,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " morris ",
            Style::default().fg(FIRE),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match state.status() {
                    Status::Won(_) => String::new(),
                    _ => format!("{} to play ", side_glyph_turn(state.turn())),
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let inner_w = (BOARD_INNER_W * scale) as usize;
    let inner_h = (BOARD_INNER_H * scale) as usize;
    let mut grid = vec![vec![Cell::blank(); inner_w]; inner_h];
    // All rail coordinates live in base space and get multiplied here.
    let sx = |v: u16| v * scale;
    let sy = |v: u16| v * scale;
    let hl = |grid: &mut Vec<Vec<Cell>>, y: u16, x0: u16, x1: u16| {
        hline(grid, sy(y), sx(x0), sx(x1));
    };
    let vl = |grid: &mut Vec<Vec<Cell>>, x: u16, y0: u16, y1: u16| {
        vline(grid, sx(x), sy(y0), sy(y1));
    };

    // Three nested squares, then the four midpoint rails. Every genuine
    // crossing hosts a point, so the `┼` branch stays a safety net.
    hl(&mut grid, 0, 0, 16);
    hl(&mut grid, 10, 0, 16);
    vl(&mut grid, 0, 0, 10);
    vl(&mut grid, 16, 0, 10);
    hl(&mut grid, 2, 2, 14);
    hl(&mut grid, 8, 2, 14);
    vl(&mut grid, 2, 2, 8);
    vl(&mut grid, 14, 2, 8);
    hl(&mut grid, 4, 4, 12);
    hl(&mut grid, 6, 4, 12);
    vl(&mut grid, 4, 4, 6);
    vl(&mut grid, 12, 4, 6);
    vl(&mut grid, 8, 0, 4);
    vl(&mut grid, 8, 6, 10);
    hl(&mut grid, 5, 0, 4);
    hl(&mut grid, 5, 12, 16);

    let hover_point = view.hover.and_then(|(c, r)| {
        if in_rect(inner, c, r) {
            nearest_point(
                (i32::from(c) - i32::from(inner.x)) / i32::from(scale.max(1)),
                (i32::from(r) - i32::from(inner.y)) / i32::from(scale.max(1)),
                i32::from(scale),
            )
        } else {
            None
        }
    });
    let removable: Vec<usize> = if view.interactive && state.removal_pending() {
        state.removable_points()
    } else {
        Vec::new()
    };
    let targets: Vec<usize> =
        if view.interactive && !state.removal_pending() && state.status() == Status::Ongoing {
            match view.selected {
                Some(src) => state.legal_moves_from(src),
                None if state.phase() == Phase::Place => {
                    (0..POINTS).filter(|&i| state.point(i).is_none()).collect()
                }
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
    let last_points: Vec<usize> = match state.last_action() {
        Some(Action::Place(i)) | Some(Action::Remove(i)) => vec![i],
        Some(Action::Move(from, to)) => vec![from, to],
        None => Vec::new(),
    };

    for (i, &(bx, by)) in POINT_XY.iter().enumerate() {
        let (x, y) = (bx * scale, by * scale);
        // Background layers, lowest priority first.
        let mut bg = None;
        if hover_point == Some(i) {
            bg = Some(HOVER_BG);
        }
        if view.cursor == i {
            bg = Some(CURSOR_BG);
        }
        if view.selected == Some(i) || removable.contains(&i) {
            bg = Some(SELECT_BG);
        }
        let occupant = state.point(i);
        let (ch, mut fg, bold) = match occupant {
            Some(Side::White) => (STONE, BONE, true),
            Some(Side::Black) => (STONE, STEEL_FG, true),
            None if targets.contains(&i) => (DOT, ACCENT_FG, true),
            None => (DOT, FAINT_FG, false),
        };
        if last_points.contains(&i) {
            fg = GOLD_FG;
        }
        put(&mut grid, y as usize, x, ch, fg, bold, bg);
    }

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

    *geom_out = Some(Geom { inner, scale });
}

fn side_glyph(side: Side) -> &'static str {
    match side {
        Side::White => "\u{25cb}",
        Side::Black => "\u{25cf}",
    }
}

fn side_glyph_turn(side: Side) -> String {
    format!("{} {}", side_glyph(side), side.name())
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    state: &Morris,
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
            Style::default().fg(FIRE),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let kv = |key: &str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(format!("{key:<6}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };

    let phase_text = if state.removal_pending() {
        "remove a stone".to_string()
    } else {
        match state.phase() {
            Phase::Place => format!("place {} left", state.stones_left(state.turn())),
            Phase::Move if state.flying(state.turn()) => "FLY!".to_string(),
            Phase::Move => "move".to_string(),
            Phase::Over => "-".to_string(),
        }
    };
    let status = if mode.ai_thinking && state.status() == Status::Ongoing {
        Line::from(Span::styled(
            format!("{} thinking...", side_glyph(mode.ai_side)),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ))
    } else {
        match state.status() {
            Status::Won(w) => Line::from(Span::styled(
                format!("{} wins!", w.name()),
                Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
            )),
            Status::Ongoing if state.removal_pending() => Line::from(Span::styled(
                "take an enemy stone",
                Style::default().fg(DIM_FG),
            )),
            Status::Ongoing if state.phase() == Phase::Place => Line::from(Span::styled(
                "pick a dot to place",
                Style::default().fg(DIM_FG),
            )),
            Status::Ongoing => Line::from(Span::styled(
                "pick a stone to slide",
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
            "phase",
            Span::styled(phase_text, Style::default().fg(WHITE_FG)),
        ),
        kv(
            "stones",
            Span::styled(
                format!(
                    "\u{25cb}{} \u{25cf}{}",
                    state.on_board(Side::White),
                    state.on_board(Side::Black)
                ),
                Style::default().fg(WHITE_FG),
            ),
        ),
        kv(
            "mode",
            Span::styled(mode.label.to_string(), Style::default().fg(BONE)),
        ),
        status,
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    // Button along the bottom of the panel.
    if area.height < 2 || inner.width == 0 {
        return;
    }
    let btn_y = area.y + area.height - 2;
    let rect = Rect {
        x: inner.x,
        y: btn_y,
        width: inner.width,
        height: 1,
    };
    buttons.push((rect, Button::Restart));
    let hovered = hover.is_some_and(|(c, r)| in_rect(rect, c, r));
    let base = Style::default()
        .fg(ACCENT_FG)
        .bg(if hovered { HOVER_BG } else { BUTTON_BG });
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(" [ restart ] ", base)))
            .alignment(Alignment::Center),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MorrisGame;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use game_core::Game;
    use ratatui::backend::TestBackend;

    fn started_two_player() -> MorrisGame {
        let mut game = MorrisGame::new();
        game.handle_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Enter,
        ));
        game
    }

    fn render_at(w: u16, h: u16) {
        for setup_open in [true, false] {
            let mut game = MorrisGame::new();
            if !setup_open {
                game = started_two_player();
                // Mid-game flavour: two stones per side on the board.
                for pt in [0usize, 9, 1, 10] {
                    let _ = game.state.place(pt);
                }
            }
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| Game::draw(&mut game, f, f.area()))
                .unwrap();
        }
    }

    #[test]
    fn big_terminal_scales_the_rings() {
        // 3x scale needs (17*3+2)+panel width; 160x80 is plenty.
        let mut game = crate::MorrisGame::new();
        game.dismiss_setup_for_test();
        let backend = TestBackend::new(160, 80);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| game_core::Game::draw(&mut game, f, f.area()))
            .unwrap();
        let geom = game.geom.expect("board drawn at 160x80");
        let inner = geom.inner();
        // Inner includes the scaled grid plus nothing else; the ladder
        // multiplies the whole block (borders included).
        assert_eq!(inner.width, (BOARD_INNER_W + 2) * 3 - 2);
        assert_eq!(inner.height, (BOARD_INNER_H + 2) * 3 - 2);
        // A click at a scaled point lands exactly on that point.
        let (bx, by) = POINT_XY[12];
        game.handle_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: inner.x + bx * 3,
            row: inner.y + by * 3,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(0u16, 0u16), (24, 10), (40, 14), (80, 24), (120, 40)] {
            render_at(w, h);
        }
    }

    #[test]
    fn geom_hit_test_round_trips_every_point() {
        let mut game = started_two_player();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
        let geom = game.geom.expect("geom captured at 80x24");
        let inner = geom.inner();
        assert_eq!((inner.width, inner.height), (BOARD_INNER_W, BOARD_INNER_H));
        for (i, &(x, y)) in POINT_XY.iter().enumerate() {
            assert_eq!(
                geom.slot_at(inner.x + x, inner.y + y),
                Some(i),
                "point {i} at ({x},{y})"
            );
        }
        // Between the inner ring and nothing else: a clean miss.
        assert_eq!(geom.slot_at(inner.x + 6, inner.y + 5), None);
        assert_eq!(geom.slot_at(0, 0), None, "outside the board");
    }

    #[test]
    fn mouse_places_for_both_humans() {
        let mut game = started_two_player();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
        let geom = game.geom.expect("geom captured");
        let inner = geom.inner();
        let click = |g: &mut MorrisGame, pt: usize| {
            let (x, y) = POINT_XY[pt];
            g.handle_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: inner.x + x,
                row: inner.y + y,
                modifiers: KeyModifiers::empty(),
            });
        };
        click(&mut game, 4);
        assert_eq!(game.state.point(4), Some(Side::White));
        assert_eq!(game.cursor, 4, "click steers the cursor");
        click(&mut game, 19);
        assert_eq!(game.state.point(19), Some(Side::Black));
        click(&mut game, 4);
        assert_eq!(game.state.point(4), Some(Side::White), "occupied stays");
    }
}
