use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
};

use super::Button;
use super::engine::{GoState, Player};

const BG: Color = Color::Rgb(13, 15, 23);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const CROSS_BG: Color = Color::Rgb(38, 44, 60);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);

// Setup menu palette (doom-adjacent).
const FIRE_FG: Color = Color::Rgb(178, 34, 34);
const BONE_FG: Color = Color::Rgb(216, 208, 194);
const MENU_SELECT_BG: Color = Color::Rgb(88, 16, 14);
const MENU_HOVER_BG: Color = Color::Rgb(54, 22, 18);

/// Entries of the setup overlay; digits 1..=N jump to an entry.
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

/// Panel facts the game layer computes each frame.
pub struct Hud<'a> {
    /// e.g. `"two players"`, `"vs ai MEDIUM (white)"` or `"ai duel HARD"`.
    pub mode: &'a str,
    /// Side whose automated move is owed, shown as `<glyph> thinking...`.
    pub thinking: Option<Player>,
}

pub struct Geom {
    /// Screen position of the top-left intersection.
    x0: u16,
    y0: u16,
    /// Columns per cell: 2 in roomy layouts, 1 when the terminal is tight.
    stride: u16,
    /// Rows per cell: 2 on tall/large screens keeps the board square.
    cell_h: u16,
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

    pub fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        if row < self.y0 || col < self.x0 {
            return None;
        }
        let (dr, dc) = ((row - self.y0), (col - self.x0));
        if self.stride == 0 || dc % self.stride != 0 {
            return None;
        }
        let (r, c) = ((dr / self.cell_h) as usize, (dc / self.stride) as usize);
        if r >= super::SIZE || c >= super::SIZE {
            return None;
        }
        Some(r * super::SIZE + c)
    }
}

impl Clone for Geom {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Geom {}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

pub fn in_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Board block width for a given cell stride (borders included).
fn board_block_w(stride: u16) -> u16 {
    game_core::geom::board_width(super::SIZE as u16, stride) + 2
}
fn board_block_h(cell_h: u16) -> u16 {
    game_core::geom::board_height(super::SIZE as u16, cell_h) + 2
}
const PANEL_W: u16 = 26;
/// Cell widths tried from roomiest to tightest when placing the canvas.
/// Roomiest-first ladder: cells up to 13x6 (a ~4.5x linear scale-up).
fn cell_options() -> Vec<game_core::geom::CellSize> {
    game_core::geom::proportional_ladder(super::SIZE as u16, super::SIZE as u16, 13, 6)
}

pub fn draw(
    frame: &mut Frame,
    state: &mut GoState,
    area: Rect,
    cursor: usize,
    hud: &Hud<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Resizable canvas: pick the widest cell that fits, else stand down
    // (also covers 0x0 ptty startup).
    let options = cell_options();
    let Some(canvas) =
        game_core::geom::fit_canvas(area, super::SIZE as u16, super::SIZE as u16, 1, &options)
    else {
        *geom_out = None;
        return;
    };
    if area.height < board_block_h(canvas.cell_h) + 1 {
        *geom_out = None;
        return;
    }
    let block_w = board_block_w(canvas.cell_w);

    let side =
        area.width >= block_w + 2 + PANEL_W && area.height > board_block_h(canvas.cell_h) + 7;
    let cluster_w = if side {
        block_w + 2 + PANEL_W
    } else {
        block_w.max(PANEL_W)
    };
    let cluster_h = if side {
        board_block_h(canvas.cell_h)
    } else {
        board_block_h(canvas.cell_h).min(area.height.saturating_sub(1))
            + 1
            + 7.min(area.height.saturating_sub(board_block_h(canvas.cell_h) + 1))
    };
    let _ = cluster_h;

    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
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
        cursor,
        geom_out,
        canvas.cell_w,
        canvas.cell_h,
    );

    let panel = if side {
        cols[2]
    } else {
        let panel_h = 7.min(area.height - board_block_h(canvas.cell_h));
        Rect {
            x: cluster.x,
            y: cluster.y + board_block_h(canvas.cell_h),
            width: PANEL_W.min(area.width),
            height: panel_h,
        }
    };
    draw_panel(frame, panel, state, hud, buttons);
}

/// Modal setup overlay; records each entry's screen rect for mouse input.
pub fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<usize>,
    rects_out: &mut Vec<(Rect, usize)>,
) {
    rects_out.clear();
    let width = 26.min(area.width);
    let height = (SETUP_ITEMS.len() as u16 + 3).min(area.height);
    if width < 4 || height < 3 {
        return;
    }
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " go setup ",
            Style::default().fg(FIRE_FG).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, label) in SETUP_ITEMS.iter().enumerate() {
        let row = i as u16;
        if row >= inner.height {
            break;
        }
        let rect = Rect {
            x: inner.x,
            y: inner.y + row,
            width: inner.width,
            height: 1,
        };
        rects_out.push((rect, i));

        let style = if i == selected {
            Style::default()
                .fg(BONE_FG)
                .bg(MENU_SELECT_BG)
                .add_modifier(Modifier::BOLD)
        } else if hover == Some(i) {
            Style::default().fg(BONE_FG).bg(MENU_HOVER_BG)
        } else {
            Style::default().fg(DIM_FG)
        };
        let line = Line::from(vec![
            Span::styled(format!(" {} ", i + 1), Style::default().fg(FIRE_FG)),
            Span::styled(*label, style),
        ]);
        frame.render_widget(Paragraph::new(line), rect);
    }
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    state: &GoState,
    cursor: usize,
    geom_out: &mut Option<Geom>,
    stride: u16,
    cell_h: u16,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            format!(" go {}x{} ", state.size(), state.size()),
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match state.turn {
                    Player::Black => "\u{25cf} to play ",
                    Player::White => "\u{25cb} to play ",
                    Player::Empty => "",
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    *geom_out = Some(Geom::new(inner.x, inner.y, stride, cell_h));

    let size = state.size();
    let (cr, cc) = (cursor / size, cursor % size);
    let stone_lines = game_core::geom::board_height(size as u16, cell_h) as usize;
    let mut lines = Vec::with_capacity(stone_lines);
    for r in 0..size {
        for _ in 0..(cell_h - 1) {
            if r > 0 {
                // Spacer rows between tall cell bands.
                lines.push(Line::from(vec![Span::raw(" ".repeat(
                    game_core::geom::board_width(size as u16, stride) as usize,
                ))]));
            }
        }
        let mut spans = Vec::with_capacity(size * 2 - 1);
        for c in 0..size {
            if c > 0 && stride > 1 {
                spans.push(Span::raw(" ".repeat((stride - 1) as usize)));
            }
            let idx = r * size + c;
            let stone = state.board()[idx];
            let is_cursor = idx == cursor;
            let on_cross = r == cr || c == cc;

            let fg = match stone {
                Player::Black => GIVEN_FG,
                Player::White => ACCENT_FG,
                Player::Empty => FAINT_FG,
            };
            let last = state.last_move == Some(idx);
            let mut style = Style::default().fg(fg);
            if stone != Player::Empty && last {
                style = style.fg(GOLD_FG).add_modifier(Modifier::BOLD);
            } else if is_cursor || (stone == Player::Empty && on_cross) {
                style = style.bg(if is_cursor { SELECT_BG } else { CROSS_BG });
                if is_cursor {
                    style = style.add_modifier(Modifier::BOLD);
                }
            }
            spans.push(Span::styled(stone.symbol(), style));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    state: &GoState,
    hud: &Hud<'_>,
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

    let turn_span = Span::styled(
        match state.turn {
            Player::Black => "\u{25cf} black".to_string(),
            Player::White => "\u{25cb} white".to_string(),
            Player::Empty => "-".to_string(),
        },
        Style::default().fg(GIVEN_FG).add_modifier(Modifier::BOLD),
    );

    let result_line = if state.over {
        vec![kv(
            "result",
            Span::styled(
                state.score().result_text(),
                Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
            ),
        )]
    } else {
        vec![]
    };

    let mut lines = vec![
        kv("to play", turn_span),
        kv(
            "captures",
            Span::styled(
                format!(
                    "\u{25cf} {} \u{25cb} {}",
                    state.captures_black, state.captures_white
                ),
                Style::default().fg(GIVEN_FG),
            ),
        ),
        kv(
            "passes",
            Span::styled(format!("{} / 2", state.passes), Style::default().fg(DIM_FG)),
        ),
    ];
    lines.push(kv(
        "mode",
        Span::styled(hud.mode.to_string(), Style::default().fg(DIM_FG)),
    ));
    if let Some(side) = hud.thinking {
        lines.push(Line::from(Span::styled(
            format!("{} thinking...", side.symbol()),
            Style::default().fg(GOLD_FG),
        )));
    }
    lines.extend(result_line);

    frame.render_widget(Paragraph::new(lines), inner);

    // Buttons along the bottom of the panel.
    if area.height < 2 {
        return;
    }
    let btn_y = area.y + area.height - 2;
    let half = inner.width / 2;
    let specs = [("pass", Button::Pass), ("restart", Button::Restart)];
    for (i, (label, button)) in specs.iter().enumerate() {
        let rect = Rect {
            x: inner.x + i as u16 * half,
            y: btn_y,
            width: half.max(4),
            height: 1,
        };
        buttons.push((rect, *button));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {label} "),
                Style::default().fg(ACCENT_FG).bg(BUTTON_BG),
            )))
            .alignment(Alignment::Center),
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GoGame, SIZE};
    use game_core::Game;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16) {
        let mut game = GoGame::new();
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
            (10, 3),
            (12, 12),
            (21, 11),
            (40, 14),
            (60, 20),
            (120, 40),
        ] {
            render_at(w, h);
        }
    }

    #[test]
    fn compact_canvas_renders_at_narrow_sizes() {
        // 9x9 with single-char cells needs only 11 columns of board.
        let mut game = GoGame::new();
        game.handle_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::Enter,
        )); // leave the setup overlay
        let backend = TestBackend::new(13, 13);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
        assert!(game.geom.is_some(), "compact canvas should fit");
    }

    #[test]
    fn click_mapping_round_trips() {
        let geom = Geom::new(2, 1, 2, 1);
        assert_eq!(geom.cell_at(2, 1), Some(0));
        assert_eq!(geom.cell_at(4, 1), Some(1));
        assert_eq!(geom.cell_at(2, 2), Some(SIZE));
        assert_eq!(geom.cell_at(3, 1), None, "gap column");
        assert_eq!(geom.cell_at(100, 1), None);
    }
}
