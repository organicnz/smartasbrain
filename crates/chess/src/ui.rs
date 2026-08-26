//! Rendering and hit-testing for the chess board and info panel.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use super::Button;
use super::engine::{Chess, Kind, Side, Status, sq};

const BG: Color = Color::Rgb(13, 15, 23);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
const BLACK_PIECE_FG: Color = Color::Rgb(140, 150, 175);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const LIGHT_SQ: Color = Color::Rgb(58, 62, 82);
const DARK_SQ: Color = Color::Rgb(36, 39, 54);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const CAPTURE_BG: Color = Color::Rgb(94, 52, 66);
const CROSS_BG: Color = Color::Rgb(44, 50, 68);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);

// Setup-menu palette, borrowed from the lobby's forge-fire look.
const FIRE: Color = Color::Rgb(178, 34, 34);
const BONE: Color = Color::Rgb(216, 208, 194);
const PICK_SELECT_BG: Color = Color::Rgb(88, 16, 14);
const PICK_HOVER_BG: Color = Color::Rgb(54, 22, 18);
/// Subtle gold wash marking the AI's previous from/to squares.
const AI_MOVE_BG: Color = Color::Rgb(72, 62, 30);

/// Board occupies 16 columns (8 two-char cells) and 8 rows inside its block.
const BOARD_INNER_W: u16 = 16;
const BOARD_INNER_H: u16 = 8;
const PANEL_W: u16 = 46;
const PANEL_H: u16 = 7;

/// Cursor/selection/hover snapshot handed to [`draw`] each frame.
pub struct Interaction {
    pub cursor: usize,
    pub selected: Option<usize>,
    pub hover: Option<(u16, u16)>,
    /// From/to squares of the AI's latest move, tinted gold.
    pub last_ai_move: Option<(usize, usize)>,
    /// Panel mode line ("two players" / "vs ai …" / "ai duel …").
    pub mode: String,
    /// Side currently deliberating, rendered as its glyph + "thinking...".
    /// During a duel this alternates with the turn every frame.
    pub thinking: Option<Side>,
}

#[derive(Clone, Copy)]
pub struct Geom {
    /// Screen position of the top-left cell.
    x0: u16,
    y0: u16,
    /// Columns per square (resizable canvas).
    stride: u16,
    /// Rows per square.
    cell_h: u16,
}

impl Geom {
    fn new(x0: u16, y0: u16, stride: u16, cell_h: u16) -> Self {
        Self {
            x0,
            y0,
            stride: stride.max(1),
            cell_h: cell_h.max(1),
        }
    }

    /// Map a click to its square; every column/row of the cell agrees.
    pub fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        let (dr, dc) = (
            (row.checked_sub(self.y0)?) as usize,
            (col.checked_sub(self.x0)?) as usize,
        );
        let (r, c) = (dr / self.cell_h as usize, dc / self.stride as usize);
        if r >= BOARD_INNER_H as usize || c >= 8 {
            return None;
        }
        Some(sq(r, c))
    }
}

fn board_block_w(stride: u16) -> u16 {
    game_core::geom::board_width(8, stride) + 2
}

fn board_block_h(cell_h: u16) -> u16 {
    game_core::geom::board_height(8, cell_h) + 2
}

/// Near-square candidates from roomiest to tightest.
fn cell_options() -> Vec<game_core::geom::CellSize> {
    game_core::geom::proportional_ladder(8, 8, 9, 4)
}

/// Filled glyphs for both sides; color tells them apart.
fn glyph(kind: Kind) -> &'static str {
    match kind {
        Kind::King => "\u{265a}",
        Kind::Queen => "\u{265b}",
        Kind::Rook => "\u{265c}",
        Kind::Bishop => "\u{265d}",
        Kind::Knight => "\u{265e}",
        Kind::Pawn => "\u{265f}",
    }
}

#[allow(clippy::too_many_lines)]
pub fn draw(
    frame: &mut Frame,
    state: &Chess,
    area: Rect,
    inter: &Interaction,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Resizable canvas: roomiest near-square cells that fit.
    let Some(canvas) = game_core::geom::fit_canvas(area, 8, 8, 1, &cell_options()) else {
        *geom_out = None;
        return;
    };
    let block_w = board_block_w(canvas.cell_w);
    let block_h = board_block_h(canvas.cell_h);

    let side = area.width >= block_w + 2 + PANEL_W && area.height > block_h + PANEL_H;
    let cluster_w = if side {
        block_w + 2 + PANEL_W
    } else {
        block_w.max(PANEL_W)
    };
    let cluster_h = if side { block_h } else { block_h + 1 + PANEL_H };

    let cluster =
        game_core::geom::centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(block_w),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    draw_board(
        frame,
        cols[0],
        state,
        inter,
        geom_out,
        canvas.cell_w,
        canvas.cell_h,
    );

    let panel = if side {
        cols[2]
    } else {
        let panel_h = PANEL_H.min(area.height - board_block_h(canvas.cell_h));
        Rect {
            x: cluster.x,
            y: cluster.y + board_block_h(canvas.cell_h),
            width: PANEL_W.min(area.width),
            height: panel_h,
        }
    };
    draw_panel(frame, panel, state, inter, buttons);
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    state: &Chess,
    inter: &Interaction,
    geom_out: &mut Option<Geom>,
    stride: u16,
    cell_h: u16,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " chess ",
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match state.status() {
                    Status::Won(winner) => format!("\u{25cf} {} wins ", winner.name()),
                    _ => format!("\u{25cf} {} to play ", state.turn().name()),
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    *geom_out = Some(Geom::new(inner.x, inner.y, stride.max(1), cell_h.max(1)));

    let targets: Vec<usize> = inter
        .selected
        .map(|from| state.legal_targets(from))
        .unwrap_or_default();

    let mut lines =
        Vec::with_capacity(game_core::geom::board_height(BOARD_INNER_H, cell_h) as usize);
    for r in 0..BOARD_INNER_H as usize {
        // Spacer lines between tall square bands.
        for _ in 0..(cell_h - 1) {
            if r > 0 {
                lines.push(Line::from(vec![Span::raw(
                    " ".repeat(inner.width as usize),
                )]));
            }
        }
        let mut spans = Vec::with_capacity((BOARD_INNER_W as usize) * stride as usize);
        for c in 0..8usize {
            let idx = sq(r, c);
            let is_selected = inter.selected == Some(idx);
            let is_target = targets.contains(&idx);
            let is_cursor = idx == inter.cursor;
            let is_ai_move = inter
                .last_ai_move
                .is_some_and(|(from, to)| from == idx || to == idx);

            let checker = if (r + c) % 2 == 0 { LIGHT_SQ } else { DARK_SQ };
            let bg = if is_selected {
                SELECT_BG
            } else if is_target && state.board()[idx].is_some() {
                CAPTURE_BG
            } else if is_ai_move {
                AI_MOVE_BG
            } else if is_cursor {
                CROSS_BG
            } else {
                checker
            };

            let (glyph_text, mut style) = match state.board()[idx] {
                Some(piece) => (
                    glyph(piece.kind),
                    Style::default().fg(match piece.side {
                        Side::White => GIVEN_FG,
                        Side::Black => BLACK_PIECE_FG,
                    }),
                ),
                None if is_target => ("\u{b7}", Style::default().fg(ACCENT_FG)),
                None => (" ", Style::default().fg(DIM_FG)),
            };
            if state.board()[idx].is_some_and(|p| p.side == Side::White) {
                style = style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(
                format!("{glyph_text:^width$}", width = stride as usize),
                style.bg(bg),
            ));
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    state: &Chess,
    inter: &Interaction,
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

    let mut to_play_line = vec![
        Span::styled("to play  ", Style::default().fg(DIM_FG)),
        Span::styled(
            format!("\u{25cf} {}", state.turn().name()),
            Style::default().fg(GIVEN_FG).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(side) = inter.thinking {
        to_play_line.push(Span::styled(
            format!(" \u{25cf} {} thinking...", side.name()),
            Style::default().fg(GOLD_FG),
        ));
    }

    let status_text = match state.status() {
        Status::Won(winner) => format!("checkmate \u{2014} {} wins", winner.name()),
        Status::Stalemate => "stalemate".to_string(),
        Status::Ongoing if state.in_check() => "check!".to_string(),
        Status::Ongoing => "-".to_string(),
    };
    let status_style = if matches!(state.status(), Status::Ongoing) && !state.in_check() {
        Style::default().fg(DIM_FG)
    } else {
        Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD)
    };

    let lines = vec![
        Line::from(to_play_line),
        kv("status", Span::styled(status_text, status_style)),
        kv(
            "mode",
            Span::styled(inter.mode.clone(), Style::default().fg(DIM_FG)),
        ),
        kv(
            "promotion",
            Span::styled("auto \u{265b}", Style::default().fg(DIM_FG)),
        ),
    ];

    frame.render_widget(Paragraph::new(lines), inner);

    // Buttons along the bottom of the panel.
    if area.height < 2 {
        return;
    }
    let btn_y = area.y + area.height - 2;
    let rect = Rect {
        x: inner.x,
        y: btn_y,
        width: inner.width.max(1),
        height: 1,
    };
    let hovered = inter
        .hover
        .is_some_and(|(c, r)| game_core::geom::in_rect(rect, c, r));
    buttons.push((rect, Button::Restart));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " [ restart ] ",
            Style::default()
                .fg(ACCENT_FG)
                .bg(if hovered { HOVER_BG } else { BUTTON_BG }),
        )))
        .alignment(Alignment::Center),
        rect,
    );
}

/// Setup-menu geometry: box width/height and per-item row stride. Seven
/// rows at stride 2 plus the key-hint row need 17 outer rows.
const SETUP_W: u16 = 40;
const SETUP_H: u16 = 17;
const SETUP_STRIDE: u16 = 2;

/// Full-screen opponent picker shown instead of the board while active.
/// Registers each item's hit rect into `buttons` for click activation.
pub fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let box_area =
        game_core::geom::centered_rect(area, SETUP_W.min(area.width), SETUP_H.min(area.height));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " chess setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(box_area);
    frame.render_widget(block, box_area);
    if inner.width < 6 || inner.height < 2 {
        return;
    }

    let item_rect = |i: usize| Rect {
        x: inner.x + 2,
        y: inner.y + 1 + i as u16 * SETUP_STRIDE,
        width: inner.width - 4,
        height: 1,
    };

    for (i, label) in super::SETUP_ITEMS.iter().enumerate() {
        let y = item_rect(i).y;
        // Keep one row clear at the bottom for the key hints.
        if y + 1 >= inner.y + inner.height {
            break;
        }
        let rect = item_rect(i);
        let is_sel = i == selected;
        let hovered = hover.is_some_and(|(c, r)| game_core::geom::in_rect(rect, c, r));

        let (fg, bg, modifier) = if is_sel {
            (BONE, PICK_SELECT_BG, Modifier::BOLD)
        } else if hovered {
            (GOLD_FG, PICK_HOVER_BG, Modifier::BOLD)
        } else {
            (DIM_FG, BG, Modifier::empty())
        };
        let padded = format!("{:<width$}", label, width = (rect.width - 3) as usize);

        buttons.push((rect, super::Button::MenuItem(i)));
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{:>2} ", i + 1), Style::default().fg(FIRE)),
                Span::styled(
                    padded,
                    Style::default().fg(fg).bg(bg).add_modifier(modifier),
                ),
            ])),
            rect,
        );
    }

    let hint_row = Rect {
        x: inner.x,
        y: inner.y + inner.height - 1,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2191}\u{2193} choose \u{00b7} enter confirm \u{00b7} esc quit",
            Style::default().fg(DIM_FG),
        )))
        .alignment(Alignment::Center),
        hint_row,
    );
}

#[cfg(test)]
mod tests {
    use super::super::engine::{col, row};
    use super::*;
    use crate::ChessGame;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use game_core::Game;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16) {
        let mut game = ChessGame::new();
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
            render_at(w, h);
        }
    }

    #[test]
    fn click_mapping_round_trips() {
        let geom = Geom::new(2, 1, 2, 1);
        assert_eq!(geom.cell_at(2, 1), Some(sq(0, 0)));
        assert_eq!(geom.cell_at(3, 1), Some(sq(0, 0)), "second half of cell");
        assert_eq!(geom.cell_at(4, 1), Some(sq(0, 1)));
        assert_eq!(geom.cell_at(2, 2), Some(sq(1, 0)));
        assert_eq!(geom.cell_at(17, 8), Some(sq(7, 7)), "bottom-right cell");
        assert_eq!(geom.cell_at(18, 1), None, "past right edge");
        assert_eq!(geom.cell_at(2, 9), None, "past bottom edge");
        assert_eq!(geom.cell_at(1, 1), None, "left of origin");
        assert_eq!(geom.cell_at(2, 0), None, "above origin");
    }

    #[test]
    fn select_then_play_via_keys_flips_turn() {
        use super::super::engine::Side;
        let mut game = ChessGame::new();
        game.setup_open = false; // exercise board input, not the setup menu
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        // e4 sits two rows above e2 (rows increase downward).
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());

        game.cursor = sq(6, 4); // e2
        game.handle_key(enter);
        assert_eq!(game.selected, Some(sq(6, 4)));

        game.handle_key(up);
        game.handle_key(up);
        assert_eq!(game.cursor, sq(4, 4)); // e4

        game.handle_key(enter);
        assert_eq!(game.selected, None);
        assert_eq!(game.engine.turn(), Side::Black);
        assert_eq!(
            game.engine.board()[sq(4, 4)],
            Some(super::super::engine::Piece {
                side: Side::White,
                kind: Kind::Pawn,
            })
        );
        assert_eq!(game.engine.board()[sq(6, 4)], None);

        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
    }

    #[test]
    fn mouse_click_selects_and_moves() {
        use super::super::engine::Side;

        let mut game = ChessGame::new();
        game.setup_open = false; // exercise board input, not the setup menu
        let backend = TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();

        let geom = game.geom.expect("board drawn at 120x40");
        let press = |col, row| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        };

        // Click e2 then e4 through screen geometry.
        let (e2_col, e2_row) = click_of(&geom, sq(6, 4));
        game.handle_mouse(press(e2_col, e2_row));
        assert_eq!(game.selected, Some(sq(6, 4)));

        let (e4_col, e4_row) = click_of(&geom, sq(4, 4));
        game.handle_mouse(press(e4_col, e4_row));
        assert_eq!(game.selected, None);
        assert_eq!(game.engine.turn(), Side::Black);
    }

    /// Screen coordinates of the left half of a square under `geom`.
    fn click_of(geom: &Geom, index: usize) -> (u16, u16) {
        (
            geom.x0 + geom.stride * col(index) as u16,
            geom.y0 + geom.cell_h * row(index) as u16,
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn left_click(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    #[test]
    fn setup_menu_blocks_board_input() {
        let mut game = ChessGame::new();
        assert!(game.setup_open);
        let backend = TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();

        // Pretend a board was drawn earlier: plant stale geometry, then click
        // where e2 would sit. The open menu must reject it.
        let geom = Geom::new(2, 1, 2, 1);
        let (c, r) = click_of(&geom, sq(6, 4));
        game.geom = Some(geom);
        game.handle_mouse(left_click(c, r));

        assert_eq!(game.selected, None);
        assert!(game.setup_open, "menu must stay open");
        assert_eq!(game.engine.turn(), Side::White);
        assert!(game.engine.board()[sq(6, 4)].is_some(), "board untouched");

        // Arrow keys drive the menu, not the board cursor.
        game.handle_key(key(KeyCode::Down));
        game.handle_key(key(KeyCode::Up));
        assert_eq!(game.cursor, crate::START_CURSOR);
    }

    #[test]
    fn selecting_vs_ai_easy_then_ticks_makes_ai_reply() {
        let mut game = ChessGame::new();
        game.handle_key(key(KeyCode::Down)); // highlight "VS AI - EASY"
        game.handle_key(key(KeyCode::Enter));
        assert!(!game.setup_open);
        assert_eq!(
            game.opponent,
            super::super::Opponent::Ai(game_core::Difficulty::Easy)
        );
        assert_eq!(game.ai_side, Side::Black);
        assert_eq!(game.engine.turn(), Side::White);

        // Human (white) plays e2-e4 by keyboard.
        game.handle_key(key(KeyCode::Enter));
        game.handle_key(key(KeyCode::Up));
        game.handle_key(key(KeyCode::Up));
        game.handle_key(key(KeyCode::Enter));
        assert_eq!(game.engine.turn(), Side::Black);

        for _ in 0..200 {
            Game::tick(&mut game);
            if game.engine.turn() == Side::White {
                break;
            }
        }
        assert_eq!(game.engine.turn(), Side::White, "AI replied");
        assert!(game.last_ai_move.is_some());
        let (from, to) = game.last_ai_move.unwrap();
        assert!(game.engine.board()[to].is_some());
        assert_eq!(game.engine.last_move(), Some((from, to)));
    }

    #[test]
    fn human_cannot_move_for_ai() {
        let mut game = ChessGame::new();
        game.handle_key(key(KeyCode::Char('2'))); // jump-select VS AI - EASY
        game.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            game.opponent,
            super::super::Opponent::Ai(game_core::Difficulty::Easy)
        ));

        // White steps aside so it becomes the AI's (black's) turn.
        game.handle_key(key(KeyCode::Enter));
        game.handle_key(key(KeyCode::Up));
        game.handle_key(key(KeyCode::Up));
        game.handle_key(key(KeyCode::Enter));
        assert_eq!(game.engine.turn(), Side::Black);

        // Keyboard attempt to select the black e7 pawn is rejected.
        game.cursor = sq(1, 4);
        game.handle_key(key(KeyCode::Enter));
        assert_eq!(game.selected, None);
        assert_eq!(game.engine.turn(), Side::Black);

        // Mouse attempt likewise.
        let backend = TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
        let geom = game.geom.expect("board drawn after setup closed");
        let (c, r) = click_of(&geom, sq(1, 3));
        game.handle_mouse(left_click(c, r));
        assert_eq!(game.selected, None);
        assert_eq!(game.engine.turn(), Side::Black);
        assert!(game.engine.board()[sq(1, 3)].is_some(), "d7 pawn intact");
    }

    #[test]
    fn duel_menu_rows_selectable() {
        assert_eq!(
            crate::SETUP_ITEMS,
            [
                "TWO PLAYERS",
                "VS AI - EASY",
                "VS AI - MEDIUM",
                "VS AI - HARD",
                "AI DUEL - EASY",
                "AI DUEL - MEDIUM",
                "AI DUEL - HARD",
            ]
        );

        let mut game = ChessGame::new();
        for digit in '5'..'8' {
            game.handle_key(key(KeyCode::Char(digit)));
            assert_eq!(
                game.setup_sel,
                digit.to_digit(10).unwrap() as usize - 1,
                "digit {digit} jumps to its row"
            );
        }
        assert_eq!(game.setup_sel, 6);
        game.handle_key(key(KeyCode::Enter)); // confirm AI DUEL - HARD
        assert!(!game.setup_open);
        assert_eq!(
            game.opponent,
            crate::Opponent::Battle(game_core::Difficulty::Hard)
        );
    }

    #[test]
    fn duel_self_play_progresses_or_ends() {
        let mut game = ChessGame::new();
        game.handle_key(key(KeyCode::Char('7'))); // AI DUEL - HARD
        game.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            game.opponent,
            crate::Opponent::Battle(game_core::Difficulty::Hard)
        ));
        assert_eq!(game.engine.turn(), Side::White);

        let mut changes = 0usize;
        let mut prev = *game.engine.board();
        // Wall-clock budget so a pathological shuffle of games cannot stall
        // CI; the frame-20 assertion below still always runs first.
        let started = std::time::Instant::now();
        for frame in 0..400usize {
            Game::tick(&mut game);
            if game.engine.status() != Status::Ongoing {
                // Ended by mate or stalemate — a result must be recorded.
                assert!(matches!(
                    game.engine.status(),
                    Status::Won(_) | Status::Stalemate
                ));
                break;
            }
            if game.engine.board() != &prev {
                changes += 1;
                prev = *game.engine.board();
            }
            if frame + 1 == 20 {
                assert!(changes > 0, "duel produced no moves in 20 ticks");
            }
            if started.elapsed() > std::time::Duration::from_secs(45) {
                break;
            }
        }
        assert!(changes > 0, "hard duel must keep playing or finish");
    }

    #[test]
    fn duel_input_inert() {
        let mut game = ChessGame::new();
        game.handle_key(key(KeyCode::Char('5'))); // AI DUEL - EASY
        game.handle_key(key(KeyCode::Enter));
        assert!(!game.setup_open);
        Game::tick(&mut game);
        assert!(game.last_ai_move.is_some(), "white engine opened the duel");
        let snapshot = *game.engine.board();

        game.cursor = sq(1, 4); // e7, a black pawn
        game.handle_key(key(KeyCode::Enter)); // select attempt
        game.handle_key(key(KeyCode::Up)); // cursor drift attempt
        game.handle_key(key(KeyCode::Char(' '))); // play attempt
        game.handle_key(key(KeyCode::Char('r'))); // must NOT reset mid-duel
        assert_eq!(game.cursor, sq(1, 4), "cursor frozen while spectating");
        assert_eq!(game.engine.board(), &snapshot, "'r' must not reset a duel");

        // Clicking a board square selects nothing and moves nothing.
        let backend = TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
        let geom = game.geom.expect("board drawn during duel");
        let (c, r) = click_of(&geom, sq(1, 3));
        game.handle_mouse(left_click(c, r));
        assert_eq!(game.selected, None);
        assert_eq!(game.engine.board(), &snapshot, "click left board alone");

        // The allowed spectate keys still respond.
        game.handle_key(key(KeyCode::Char('R')));
        assert_eq!(game.engine.last_move(), None, "'R' restarts the duel");
        assert_eq!(game.engine.status(), Status::Ongoing);
        game.handle_key(key(KeyCode::Char('m')));
        assert!(game.setup_open, "'m' reopens setup");
    }
}
