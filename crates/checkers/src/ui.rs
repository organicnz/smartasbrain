//! Rendering and hit testing for the checkers board.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use super::Button;
use super::engine::{Checkers, SIZE, Side, Status};
use game_core::geom::{centered_rect, in_rect};

const BG: Color = Color::Rgb(13, 15, 23);
const DARK_SQUARE: Color = Color::Rgb(40, 44, 58);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const WHITE_FG: Color = Color::Rgb(226, 230, 244);
const BLACK_FG: Color = Color::Rgb(140, 150, 175);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const CAPTURE_BG: Color = Color::Rgb(94, 52, 66);
const CROSS_BG: Color = Color::Rgb(44, 50, 68);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);
/// Subtle gold wash over dark squares of the AI's last move.
const GOLD_TINT_BG: Color = Color::Rgb(56, 50, 30);

// DOOM-ish setup menu palette.
const FIRE: Color = Color::Rgb(178, 34, 34);
const BONE: Color = Color::Rgb(216, 208, 194);
const EMBER_BG: Color = Color::Rgb(16, 5, 4);
const SETUP_SELECT_BG: Color = Color::Rgb(88, 16, 14);
const SETUP_HOVER_BG: Color = Color::Rgb(54, 22, 18);

/// Panel footprint (unchanged across canvas sizes).
const PANEL_W: u16 = 26;
const PANEL_H: u16 = 9;

/// Roomiest-first near-square candidates: cells up to 9 wide x 4 tall.
fn cell_options() -> Vec<game_core::geom::CellSize> {
    game_core::geom::proportional_ladder(SIZE as u16, SIZE as u16, 9, 4)
}

fn board_block_w(stride: u16) -> u16 {
    game_core::geom::board_width(SIZE as u16, stride) + 2
}

fn board_block_h(cell_h: u16) -> u16 {
    game_core::geom::board_height(SIZE as u16, cell_h) + 2
}

/// Screen-space snapshot taken during draw so clicks map back to squares.
pub struct Geom {
    /// Board-inner top-left corner; visible to tests for click mapping.
    pub(crate) x0: u16,
    pub(crate) y0: u16,
    /// Columns/rows per square (resizable canvas).
    pub(crate) stride: u16,
    pub(crate) cell_h: u16,
}

impl Geom {
    fn new(x0: u16, y0: u16, stride: u16, cell_h: u16) -> Self {
        Self {
            x0,
            y0,
            stride,
            cell_h: cell_h.max(1),
        }
    }

    /// Square under `(col, row)`. Every column/row inside a cell maps the
    /// same way; light squares are not playable and yield None.
    pub fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        let stride = self.stride.max(1);
        let cell_h = self.cell_h.max(1);
        if row < self.y0 || col < self.x0 {
            return None;
        }
        let (r, c) = (
            (row - self.y0) as usize / cell_h as usize,
            (col - self.x0) as usize / stride as usize,
        );
        if r >= SIZE || c >= SIZE || (r + c) % 2 != 1 {
            return None;
        }
        Some(r * SIZE + c)
    }
}

impl Clone for Geom {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Geom {}

/// Per-frame view state handed from the game shell to the renderer.
pub struct View {
    pub cursor: usize,
    pub selected: Option<usize>,
    pub hover: Option<(u16, u16)>,
    /// Squares of the AI's most recent move, tinted gold on the board.
    pub last_ai_move: Option<(usize, usize)>,
    /// True while an automated seat is on turn (duels keep it lit).
    pub thinking: bool,
    /// One-line mode description for the info panel.
    pub mode: String,
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::White => "white",
        Side::Black => "black",
    }
}

/// Dot glyph standing for a side across the panel (`●` white, `○` black).
fn side_glyph(side: Side) -> char {
    match side {
        Side::White => '\u{25cf}',
        Side::Black => '\u{25cb}',
    }
}

pub fn draw(
    frame: &mut Frame,
    state: &Checkers,
    area: Rect,
    view: View,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Resizable canvas: roomiest near-square cells that fit (0x0 safe).
    let Some(canvas) =
        game_core::geom::fit_canvas(area, SIZE as u16, SIZE as u16, 1, &cell_options())
    else {
        *geom_out = None;
        return;
    };
    let board_w = board_block_w(canvas.cell_w);
    let board_h = board_block_h(canvas.cell_h);

    let side = area.width >= board_w + 2 + PANEL_W && area.height > board_h + PANEL_H;
    let cluster_w = if side {
        board_w + 2 + PANEL_W
    } else {
        board_w.max(PANEL_W)
    };
    let cluster_h = if side { board_h } else { board_h + 1 + PANEL_H };
    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(board_w),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    draw_board(
        frame,
        cols[0],
        state,
        &view,
        geom_out,
        canvas.cell_w,
        canvas.cell_h,
    );

    let panel = if side {
        cols[2]
    } else {
        Rect {
            x: cluster.x,
            y: cluster.y + board_h,
            width: PANEL_W.min(area.width),
            height: PANEL_H.min(area.height.saturating_sub(board_h)),
        }
    };
    draw_panel(frame, panel, state, &view, buttons);
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    state: &Checkers,
    view: &View,
    geom_out: &mut Option<Geom>,
    stride: u16,
    cell_h: u16,
) {
    let cursor = view.cursor;
    let selected = view.selected;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " checkers ",
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                format!("\u{25cf} {} to play ", side_name(state.turn())),
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    *geom_out = Some(Geom::new(inner.x, inner.y, stride, cell_h));

    let targets = selected
        .map(|from| state.legal_targets(from))
        .unwrap_or_default();
    let is_jump = |from: usize, to: usize| {
        (from / SIZE).abs_diff(to / SIZE) == 2 && (from % SIZE).abs_diff(to % SIZE) == 2
    };

    let mut lines = Vec::with_capacity(game_core::geom::board_height(SIZE as u16, cell_h) as usize);
    for r in 0..SIZE {
        for _ in 0..(cell_h - 1) {
            if r > 0 {
                lines.push(Line::from(vec![Span::raw(
                    " ".repeat(inner.width as usize),
                )]));
            }
        }
        let mut spans = Vec::with_capacity(SIZE * stride as usize);
        for c in 0..SIZE {
            let idx = r * SIZE + c;
            let target = targets.contains(&idx);
            let capture_target = target && selected.is_some_and(|from| is_jump(from, idx));

            let mut bg = if (r + c) % 2 == 1 { DARK_SQUARE } else { BG };
            if view
                .last_ai_move
                .is_some_and(|(from, to)| from == idx || to == idx)
                && bg == DARK_SQUARE
            {
                bg = GOLD_TINT_BG;
            }
            if capture_target {
                bg = CAPTURE_BG;
            }
            if idx == cursor {
                bg = CROSS_BG;
            }
            if selected == Some(idx) {
                bg = SELECT_BG;
            }

            let (glyph, fg, modifier) = if target {
                ('\u{00b7}', ACCENT_FG, Modifier::BOLD)
            } else {
                match state.piece_at(idx) {
                    Some(pc) => {
                        let fg = match pc.side {
                            Side::White => WHITE_FG,
                            Side::Black => BLACK_FG,
                        };
                        let modifier = match pc.king {
                            true => Modifier::UNDERLINED,
                            false => match pc.side {
                                Side::White => Modifier::BOLD,
                                Side::Black => Modifier::empty(),
                            },
                        };
                        (if pc.king { '\u{25c9}' } else { '\u{25cf}' }, fg, modifier)
                    }
                    None => (' ', DIM_FG, Modifier::empty()),
                }
            };
            spans.push(Span::styled(
                format!("{glyph:^width$}", width = stride as usize),
                Style::default().fg(fg).bg(bg).add_modifier(modifier),
            ));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    state: &Checkers,
    view: &View,
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
            Span::styled(format!("{key:<9}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };

    let (white, black) = state.counts();
    let mut lines = vec![
        kv(
            "to play",
            Span::styled(
                format!("\u{25cf} {}", side_name(state.turn())),
                Style::default().fg(WHITE_FG).add_modifier(Modifier::BOLD),
            ),
        ),
        kv(
            "men",
            Span::styled(
                format!("\u{25cf} {white} \u{25cb} {black}"),
                Style::default().fg(WHITE_FG),
            ),
        ),
        kv(
            "mode",
            Span::styled(view.mode.clone(), Style::default().fg(DIM_FG)),
        ),
        kv(
            "status",
            match state.status() {
                Status::Ongoing => Span::styled("-", Style::default().fg(DIM_FG)),
                Status::Won(winner) => Span::styled(
                    format!("{} wins", side_name(winner)),
                    Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
                ),
            },
        ),
    ];
    if state.chain_from().is_some() {
        lines.push(Line::from(Span::styled(
            "\u{21bb} keep jumping!",
            Style::default().fg(ACCENT_FG),
        )));
    }
    if view.thinking {
        lines.push(Line::from(Span::styled(
            format!("{} thinking...", side_glyph(state.turn())),
            Style::default().fg(GOLD_FG),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    // Restart button along the bottom of the panel.
    if area.height < 2 {
        return;
    }
    let rect = Rect {
        x: inner.x,
        y: area.y + area.height - 2,
        width: inner.width.max(4),
        height: 1,
    };
    buttons.push((rect, Button::Restart));
    let hovered = view.hover.is_some_and(|(col, row)| in_rect(rect, col, row));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " restart ",
            Style::default()
                .fg(ACCENT_FG)
                .bg(if hovered { HOVER_BG } else { BUTTON_BG }),
        )))
        .alignment(Alignment::Center),
        rect,
    );
}

/// Setup overlay entries in confirm order; digit `n` picks entry `n - 1`.
pub const SETUP_ITEMS: [&str; 9] = [
    "TWO PLAYERS",
    "VS AI - EASY",
    "VS AI - MEDIUM",
    "VS AI - HARD",
    "VS AI - EXPERT",
    "AI DUEL - EASY",
    "AI DUEL - MEDIUM",
    "AI DUEL - HARD",
    "AI DUEL - EXPERT",
];

/// Full-screen DOOM-flavoured setup menu. Rebuilds one hitbox per entry into
/// `rects_out` so clicks map back through [`in_rect`].
pub fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    rects_out: &mut Vec<(Rect, usize)>,
) {
    frame.render_widget(Block::default().style(Style::default().bg(EMBER_BG)), area);
    let outer = centered_rect(area, 42.min(area.width), 12.min(area.height));
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(EMBER_BG))
        .title(Line::from(Span::styled(
            " checkers setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )))
        .title_bottom(
            Line::from(Span::styled(
                " \u{2620} choose wisely ",
                Style::default().fg(FIRE),
            ))
            .right_aligned(),
        );
    let inner = block.inner(outer);
    frame.render_widget(block, outer);

    let items = SETUP_ITEMS.len() as u16;
    let top = inner.y + inner.height.saturating_sub(items) / 2;
    for (i, label) in SETUP_ITEMS.iter().enumerate() {
        let rect = Rect {
            x: inner.x,
            y: top + i as u16,
            width: inner.width,
            height: 1,
        };
        if rect.width == 0 || rect.y >= inner.y + inner.height {
            continue;
        }
        rects_out.push((rect, i));
        let hovered = hover.is_some_and(|(col, row)| in_rect(rect, col, row));
        let style = match i == selected {
            true => Style::default()
                .fg(BONE)
                .bg(SETUP_SELECT_BG)
                .add_modifier(Modifier::BOLD),
            false if hovered => Style::default().fg(BONE).bg(SETUP_HOVER_BG),
            false => Style::default().fg(FIRE).bg(EMBER_BG),
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {} ", i + 1), style),
                Span::styled(format!(" {label}  "), style),
            ]))
            .alignment(Alignment::Center),
            rect,
        );
    }

    if inner.height > items {
        let hints = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "j/k move \u{b7} enter confirm \u{b7} esc quit",
                Style::default().fg(FIRE).add_modifier(Modifier::DIM),
            )))
            .alignment(Alignment::Center),
            hints,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CheckersGame;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use game_core::Game;
    use ratatui::backend::TestBackend;

    /// Leave the startup setup menu via the "TWO PLAYERS" hotkey.
    fn dismiss_menu(game: &mut CheckersGame) {
        game.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::empty()));
        assert!(!game.setup_open);
    }

    fn render_at(w: u16, h: u16, setup: bool) {
        let mut game = CheckersGame::new();
        if !setup {
            dismiss_menu(&mut game);
        }
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [
            (0u16, 0u16),
            (20, 6),
            (40, 14),
            (60, 20),
            (80, 24),
            (120, 40),
        ] {
            render_at(w, h, true);
            render_at(w, h, false);
        }
    }

    #[test]
    fn click_mapping_round_trips_dark_squares_only() {
        let geom = Geom::new(2, 1, 2, 1);
        assert_eq!(geom.cell_at(4, 1), Some(1), "(0,1): first column of cell");
        assert_eq!(geom.cell_at(5, 1), Some(1), "(0,1): second column of cell");
        assert_eq!(geom.cell_at(2, 2), Some(8), "(1,0) dark");
        assert_eq!(geom.cell_at(2, 1), None, "(0,0) light");
        assert_eq!(geom.cell_at(3, 1), None, "still inside the light cell");
        assert_eq!(geom.cell_at(100, 1), None);
        assert_eq!(geom.cell_at(4, 100), None);
    }

    #[test]
    fn mouse_play_flips_turn_and_updates_board() {
        let mut game = CheckersGame::new();
        dismiss_menu(&mut game);
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();

        let geom = game.geom.unwrap();
        let click = |game: &mut CheckersGame, square: usize| {
            game.handle_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: geom.x0 + geom.stride * (square % 8) as u16,
                row: geom.y0 + geom.cell_h * (square / 8) as u16,
                modifiers: KeyModifiers::empty(),
            });
        };

        // White: (5,0) -> (4,1), a plain opening move.
        click(&mut game, sq(5, 0));
        assert_eq!(game.selected, Some(sq(5, 0)), "own piece got selected");
        click(&mut game, sq(4, 1));
        assert_eq!(game.state.piece_at(sq(5, 0)), None);
        assert_eq!(
            game.state.piece_at(sq(4, 1)).map(|pc| pc.side),
            Some(Side::White)
        );
        assert_eq!(game.state.turn(), Side::Black);

        // Black replies (2,7) -> (3,6).
        click(&mut game, sq(2, 7));
        click(&mut game, sq(3, 6));
        assert_eq!(game.state.piece_at(sq(2, 7)), None);
        assert_eq!(
            game.state.piece_at(sq(3, 6)).map(|pc| pc.side),
            Some(Side::Black)
        );
        assert_eq!(game.state.turn(), Side::White);
    }

    fn sq(r: usize, c: usize) -> usize {
        r * 8 + c
    }
}
