//! Rendering and hit-testing for Connect Four: a 7×6 gravity grid with a
//! column cursor, hover tinting and a side info panel.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::engine::{COLS, Connect4, ROWS, Side, Status};

const BG: Color = Color::Rgb(13, 15, 23);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
const BLACK_FG: Color = Color::Rgb(140, 150, 175);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const ASH: Color = Color::Rgb(92, 68, 58);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const CROSS_BG: Color = Color::Rgb(44, 50, 68);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);
const FIRE: Color = Color::Rgb(178, 34, 34);
const BONE: Color = Color::Rgb(216, 208, 194);
const PICK_SELECT_BG: Color = Color::Rgb(88, 16, 14);
const PICK_HOVER_BG: Color = Color::Rgb(54, 22, 18);

pub(crate) const SETUP_ITEMS: [&str; 9] = [
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

/// Cell widths tried from roomiest to tightest (resizable canvas).
/// Roomiest-first ladder: columns up to 9 wide, 5 tall.
fn cell_options() -> Vec<game_core::geom::CellSize> {
    game_core::geom::proportional_ladder(COLS as u16, ROWS as u16, 9, 5)
}
pub(crate) const BOARD_H: u16 = ROWS as u16 + 2;
/// Board block width for a given cell stride (borders included).
#[must_use]
pub(crate) fn board_block_w(stride: u16) -> u16 {
    game_core::geom::board_width(COLS as u16, stride) + 2
}

/// Board block height for a given cell height (borders included).
#[must_use]
pub(crate) fn board_block_h(cell_h: u16) -> u16 {
    game_core::geom::board_height(ROWS as u16, cell_h) + 2
}
const PANEL_W: u16 = 26;

/// Per-frame interaction context handed to [`draw`].
#[derive(Clone, Copy)]
pub(crate) struct View {
    pub(crate) cursor_col: usize,
    pub(crate) hover: Option<(u16, u16)>,
}

/// Opponent context for the info panel.
#[derive(Clone, Copy)]
pub(crate) struct ModeInfo<'a> {
    pub(crate) label: &'a str,
    pub(crate) ai_side: Side,
    pub(crate) thinking: bool,
}

/// Screen position of the board's inner top-left; rebuilt every draw.
#[derive(Clone, Copy)]
pub(crate) struct Geom {
    x0: u16,
    y0: u16,
    stride: u16,
    /// Kept for symmetry with the go canvas and future row-precise hits.
    #[allow(dead_code)]
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

    #[cfg(test)]
    pub(crate) fn origin(&self) -> u16 {
        self.x0
    }

    #[cfg(test)]
    pub(crate) fn row0(&self) -> u16 {
        self.y0
    }

    #[cfg(test)]
    pub(crate) fn stride(&self) -> u16 {
        self.stride
    }

    /// Column under `(col, row)` — any row of the grid maps to its column;
    /// separator half-cells round down to the column on their left.
    pub(crate) fn col_at(&self, col: u16, row: u16) -> Option<usize> {
        // Strictly the playable interior — borders never map.
        if !(self.y0..self.y0 + ROWS as u16).contains(&row) {
            return None;
        }
        if !((self.x0)..(self.x0 + COLS as u16 * self.stride)).contains(&col) {
            return None;
        }
        Some(((col - self.x0) / self.stride) as usize)
    }
}

pub(crate) fn in_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// Button identifiers whose hit rects live in `buttons`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Restart,
}

pub(crate) fn draw(
    frame: &mut Frame,
    game: &Connect4,
    area: Rect,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Resizable canvas: widest cells that fit, else stand down (also
    // covers 0x0 ptty startup).
    let Some(canvas) =
        game_core::geom::fit_canvas(area, COLS as u16, ROWS as u16, 1, &cell_options())
    else {
        *geom_out = None;
        return;
    };
    if area.height < board_block_h(canvas.cell_h) {
        *geom_out = None;
        return;
    }
    let board_w = board_block_w(canvas.cell_w);
    let side =
        area.width >= board_w + 2 + PANEL_W && area.height >= board_block_h(canvas.cell_h).max(11);
    let cluster_w = if side {
        board_w + 2 + PANEL_W
    } else {
        board_w.max(PANEL_W)
    };
    let cluster_h = if side {
        board_block_h(canvas.cell_h).max(11)
    } else {
        board_block_h(canvas.cell_h) + 10
    };

    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(board_w),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    let panel = if side {
        cols[2]
    } else {
        Rect {
            x: cluster.x,
            y: cluster.y.saturating_add(BOARD_H + 1),
            width: PANEL_W.min(area.width),
            height: 9_u16.min(area.height.saturating_sub(BOARD_H + 1)),
        }
    };

    draw_board(
        frame,
        cols[0],
        game,
        view,
        geom_out,
        canvas.cell_w,
        canvas.cell_h,
    );
    draw_panel(frame, panel, game, view.hover, mode, buttons);
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    game: &Connect4,
    view: View,
    geom_out: &mut Option<Geom>,
    stride: u16,
    cell_h: u16,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " connect four ",
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match game.turn() {
                    Side::Black => "\u{25cf} to play ",
                    Side::White => "\u{25cb} to play ",
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let geom = Geom::new(inner.x, inner.y, stride, cell_h);
    *geom_out = Some(geom);

    let hover_col = view.hover.and_then(|(c, r)| geom.col_at(c, r));
    let last = game.last_drop();
    let total_lines = game_core::geom::board_height(ROWS as u16, cell_h) as usize;
    let mut lines = Vec::with_capacity(total_lines);
    // Row 0 is the bottom internally; paint top-down.
    for (band, vis_row) in (0..ROWS).rev().enumerate() {
        if cell_h == 2 && band > 0 {
            lines.push(Line::from(vec![Span::raw(
                " ".repeat(inner.width as usize),
            )]));
        }
        let mut spans = Vec::with_capacity(COLS * 2 - 1);
        for col in 0..COLS {
            if col > 0 && stride == 2 {
                spans.push(Span::styled("\u{2502}", Style::default().fg(FAINT_FG)));
            }
            let disc = game.cell(vis_row, col);
            let bg = if col == view.cursor_col {
                CROSS_BG
            } else if hover_col == Some(col) {
                HOVER_BG
            } else {
                BG
            };
            let (glyph, fg) = match disc {
                None => ('\u{00b7}', ASH),
                Some(s) => (
                    s.symbol(),
                    if s == Side::White { GIVEN_FG } else { BLACK_FG },
                ),
            };
            let mut style = Style::default().fg(fg).bg(bg);
            if last == Some((vis_row, col)) && disc.is_some() {
                style = style.fg(GOLD_FG).add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(format!("{glyph} "), style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    game: &Connect4,
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
            Span::styled(format!("{key:<8}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };

    let status_text = match game.status() {
        Status::Ongoing => "-".to_string(),
        Status::Won(s) => format!("{} wins!", if s == Side::White { "white" } else { "black" }),
        Status::Draw => "draw".to_string(),
    };
    let status_span = if matches!(game.status(), Status::Won(_)) {
        Span::styled(
            status_text,
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(status_text, Style::default().fg(GIVEN_FG))
    };

    let turn_glyph = match game.turn() {
        Side::Black => "\u{25cf} black",
        Side::White => "\u{25cb} white",
    };
    let thinking_line = if mode.thinking {
        Line::from(Span::styled(
            format!("{} thinking...", glyph_of(mode.ai_side)),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::raw(""))
    };

    frame.render_widget(
        Paragraph::new(vec![
            kv(
                "to play",
                Span::styled(turn_glyph, Style::default().fg(GIVEN_FG)),
            ),
            kv("status", status_span),
            kv(
                "mode",
                Span::styled(mode.label.to_string(), Style::default().fg(BONE)),
            ),
            thinking_line,
        ]),
        inner,
    );

    if area.height < 2 || inner.width < 4 {
        return;
    }
    let rect = Rect {
        x: inner.x,
        y: area.y + area.height - 2,
        width: inner.width,
        height: 1,
    };
    buttons.push((rect, Button::Restart));
    let hot = hover.is_some_and(|(c, r)| in_rect(rect, c, r));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " restart ",
            Style::default()
                .fg(ACCENT_FG)
                .bg(if hot { HOVER_BG } else { BUTTON_BG }),
        )))
        .alignment(Alignment::Center),
        rect,
    );
}

fn glyph_of(side: Side) -> &'static str {
    if side == Side::White {
        "\u{25cb}"
    } else {
        "\u{25cf}"
    }
}

/// Modal opponent picker shown instead of the board until confirmed.
/// Item hit rects are rebuilt into `items_out` on every draw.
pub(crate) fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    items_out: &mut Vec<(Rect, usize)>,
) {
    items_out.clear();
    if area.width < 24 || area.height < 12 {
        return;
    }
    let popup = centered_rect(area, 30, 12);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " connect four setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, label) in SETUP_ITEMS.iter().enumerate() {
        let y = inner.y + i as u16;
        if i > 0 && y >= inner.y + inner.height.saturating_sub(1) {
            break;
        }
        let rect = Rect {
            x: inner.x + 1,
            y,
            width: inner.width.saturating_sub(2),
            height: 1,
        };
        items_out.push((rect, i));
        let hot = selected == i;
        let hovered = hover.is_some_and(|(c, r)| in_rect(rect, c, r));
        let (base, fg, modifier) = if hot {
            (PICK_SELECT_BG, BONE, Modifier::BOLD)
        } else if hovered {
            (PICK_HOVER_BG, GOLD_FG, Modifier::BOLD)
        } else {
            (BG, FAINT_FG, Modifier::empty())
        };
        let number = format!("{:>02}", i + 1);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "\u{2020} ",
                    if hot {
                        Style::default().fg(FIRE).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(number, Style::default().fg(FAINT_FG)),
                Span::raw(" "),
                Span::styled(
                    (*label).to_string(),
                    Style::default().fg(fg).add_modifier(modifier),
                ),
            ]))
            .style(Style::default().bg(base)),
            rect,
        );
    }

    let hints_y = inner.y + inner.height - 1;
    if hints_y > inner.y {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2191}\u{2193} choose \u{00b7} enter start \u{00b7} esc quit",
                Style::default().fg(ASH),
            )))
            .alignment(Alignment::Center),
            Rect {
                x: inner.x,
                y: hints_y,
                width: inner.width,
                height: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// Draws one frame at the given size and returns the board geometry.
    fn rendered(w: u16, h: u16) -> Option<Geom> {
        let game = Connect4::new();
        let mut geom = None;
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let result = terminal.draw(|f| {
            draw(
                f,
                &game,
                f.area(),
                View {
                    cursor_col: 3,
                    hover: None,
                },
                ModeInfo {
                    label: "two players",
                    ai_side: Side::Black,
                    thinking: false,
                },
                &mut geom,
                &mut Vec::new(),
            );
        });
        result.expect("draw must not fail");
        geom
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [
            (0u16, 0u16),
            (20, 6),
            (24, 10),
            (40, 14),
            (80, 24),
            (120, 40),
        ] {
            let _ = rendered(w, h);
        }
    }

    #[test]
    fn click_mapping_round_trips_columns() {
        let Some(g) = rendered(80, 24) else {
            panic!("board drawn at 80x24");
        };
        // The ladder picks roomy cells here; whatever the stride, the
        // mapping must stay exact across all seven columns.
        let stride = g.stride();
        assert!(stride >= 2);
        for k in 0..COLS as u16 {
            assert_eq!(g.col_at(g.x0 + k * stride, g.y0), Some(k as usize));
            assert_eq!(
                g.col_at(g.x0 + k * stride + stride - 1, g.y0),
                Some(k as usize),
                "trailing half-cell maps left"
            );
        }
        assert!(g.col_at(g.x0.saturating_sub(1), g.y0).is_none());
        assert!(g.col_at(g.x0, g.y0.saturating_sub(3)).is_none());
    }
}
