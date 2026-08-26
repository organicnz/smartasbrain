//! Rendering and hit-testing for battleship: twin 10x10 waters (targeting
//! left, own fleet right when wide), fleet bars, and the standard panel.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::engine::{Battleship, CELLS, CellView, GRID, Side, Status};

const BG: Color = Color::Rgb(13, 15, 23);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
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

pub(crate) const SETUP_ITEMS: [&str; 7] = [
    "TWO PLAYERS",
    "VS AI - EASY",
    "VS AI - MEDIUM",
    "VS AI - HARD",
    "AI DUEL - EASY",
    "AI DUEL - MEDIUM",
    "AI DUEL - HARD",
];

/// One-char cells plus borders.
pub(crate) const SEA_W: u16 = GRID as u16 + 2;
pub(crate) const SEA_H: u16 = GRID as u16 + 2;

/// Cell strides tried from roomiest to tightest.
const STRIDES: [u16; 2] = [2, 1];

/// Sea block width for a given cell stride (borders included).
#[must_use]
pub(crate) fn sea_w(stride: u16) -> u16 {
    game_core::geom::board_width(GRID as u16, stride) + 2
}
const PANEL_W: u16 = 26;

#[derive(Clone, Copy)]
pub(crate) struct View {
    pub(crate) cursor: usize,
    pub(crate) hover: Option<(u16, u16)>,
}

#[derive(Clone, Copy)]
pub(crate) struct ModeInfo<'a> {
    pub(crate) label: &'a str,
    pub(crate) ai_side: Side,
    pub(crate) thinking: bool,
}

/// Enemy-sea hit rect for click mapping.
#[derive(Clone, Copy)]
pub(crate) struct Geom {
    sea_x: u16,
    sea_y: u16,
    /// Columns per sea cell (resizable canvas).
    pub(crate) stride: u16,
}

impl Geom {
    fn new(sea_x: u16, sea_y: u16, stride: u16) -> Self {
        Self {
            sea_x,
            sea_y,
            stride: stride.max(1),
        }
    }

    #[cfg(test)]
    pub(crate) fn sea_x(&self) -> u16 {
        self.sea_x
    }

    #[cfg(test)]
    pub(crate) fn sea_y(&self) -> u16 {
        self.sea_y
    }

    /// Cell under a click on the enemy sea.
    #[allow(dead_code)]
    pub(crate) fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        if col < self.sea_x || col >= self.sea_x + GRID as u16 * self.stride {
            return None;
        }
        if row < self.sea_y || row >= self.sea_y + GRID as u16 {
            return None;
        }
        let (r, c) = (
            (row - self.sea_y) as usize,
            ((col - self.sea_x) / self.stride) as usize,
        );
        Some(r * GRID + c)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Restart,
}

fn in_rect(r: Rect, col: u16, row: u16) -> bool {
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

pub(crate) fn draw(
    frame: &mut Frame,
    game: &Battleship,
    area: Rect,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Resizable canvas: double-width sea cells when the terminal allows
    // twin seas plus panel; otherwise single-width; tiny stands down.
    let mut stride = 1_u16;
    for &s in &STRIDES {
        let w = sea_w(s);
        if area.height >= SEA_H.max(14)
            && (area.width >= w * 2 + 4 || area.width >= w + 2 + PANEL_W)
        {
            stride = s;
            break;
        }
    }
    let sea_width = sea_w(stride);
    if area.width < sea_width || area.height < SEA_H.max(14) {
        *geom_out = None;
        return;
    }
    let twin = area.width >= sea_width * 2 + 2 && area.height >= SEA_H.max(14);
    let with_panel = area.width >= (if twin { sea_width * 2 + 4 } else { sea_width }) + 2 + PANEL_W;

    let total_w =
        if twin { sea_width * 2 + 2 } else { sea_width } + if with_panel { 2 + PANEL_W } else { 0 };
    let cluster = centered_rect(
        area,
        total_w.min(area.width),
        (SEA_H.max(if twin { SEA_H } else { SEA_H + 9 })).min(area.height),
    );

    let enemy_rect = Rect {
        x: cluster.x,
        y: cluster.y,
        width: sea_width.min(cluster.width),
        height: SEA_H.min(cluster.height.saturating_sub(if twin { 0 } else { 10 })),
    };
    // The targeting sea owns the click geometry.
    *geom_out = Some(Geom::new(enemy_rect.x + 1, enemy_rect.y + 1, stride));

    draw_sea(
        frame,
        enemy_rect,
        game.enemy_view(game.turn()),
        " target waters ",
        Some(view.cursor),
        stride,
    );

    let own_rect = if twin {
        Rect {
            x: enemy_rect.x + sea_width + 2,
            y: enemy_rect.y,
            width: sea_width,
            height: SEA_H,
        }
    } else {
        Rect {
            x: cluster.x,
            y: cluster.y.saturating_add(SEA_H + 1),
            width: sea_width.min(area.width),
            height: SEA_H.min(area.height.saturating_sub(SEA_H + 1)),
        }
    };
    if twin || area.height > SEA_H * 2 {
        draw_own(frame, own_rect, game, stride);
    }

    let panel = if with_panel {
        Rect {
            x: (own_rect.x + SEA_W + 2).min(area.x + area.width.saturating_sub(PANEL_W)),
            y: cluster.y + 1,
            width: PANEL_W,
            height: SEA_H.max(11).min(area.height.saturating_sub(2)),
        }
    } else {
        Rect {
            x: cluster.x,
            y: own_rect.y.saturating_add(SEA_H + 1),
            width: PANEL_W.min(area.width),
            height: 8.min(area.height.saturating_sub(own_rect.y + SEA_H + 1)),
        }
    };
    draw_panel(frame, panel, game, view.hover, mode, buttons);
}

fn sea_cell_glyphs(view_cell: CellView, cursor_here: bool) -> (&'static str, Style) {
    let base = match view_cell {
        CellView::Unknown => ("\u{00b7}", Style::default().fg(FAINT_FG)),
        CellView::Miss => ("~", Style::default().fg(DIM_FG)),
        CellView::Hit => (
            "\u{2715}",
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ),
        CellView::Sunk => (
            "\u{25a0}",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        ),
    };
    if cursor_here {
        (base.0, base.1.bg(CROSS_BG))
    } else {
        base
    }
}

fn draw_sea(
    frame: &mut Frame,
    area: Rect,
    view_cells: [CellView; CELLS],
    title: &str,
    cursor: Option<usize>,
    stride: u16,
) {
    if area.width < SEA_W || area.height < SEA_H {
        return;
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            title.to_string(),
            Style::default().fg(BORDER_FG),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::with_capacity(GRID);
    for r in 0..GRID {
        let mut spans = Vec::with_capacity(GRID);
        for c in 0..GRID {
            let idx = r * GRID + c;
            let (glyph, style) = sea_cell_glyphs(view_cells[idx], cursor == Some(idx));
            spans.push(Span::styled(
                format!("{glyph:^width$}", width = stride as usize),
                style,
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_own(frame: &mut Frame, area: Rect, game: &Battleship, stride: u16) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " your fleet ",
            Style::default().fg(BORDER_FG),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::with_capacity(GRID);
    for r in 0..GRID {
        let mut spans = Vec::with_capacity(GRID);
        for c in 0..GRID {
            let idx = r * GRID + c;
            let (ship, shot) = game.own_cell(game.turn(), idx);
            let (glyph_text, style) = match (ship, shot) {
                (Some(_), true) => (
                    "\u{2715}".to_string(),
                    Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
                ),
                (Some(k), false) => (
                    kind_letter(k).to_string(),
                    Style::default().fg(BONE).add_modifier(Modifier::BOLD),
                ),
                (None, true) => ("~".to_string(), Style::default().fg(DIM_FG)),
                (None, false) => ("\u{00b7}".to_string(), Style::default().fg(FAINT_FG)),
            };
            let span = Span::styled(
                format!("{glyph_text:^width$}", width = stride as usize),
                style,
            );
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn kind_letter(kind: crate::engine::ShipKind) -> char {
    match kind {
        crate::engine::ShipKind::Carrier => 'C',
        crate::engine::ShipKind::Battleship => 'B',
        crate::engine::ShipKind::Cruiser => 'c',
        crate::engine::ShipKind::Submarine => 's',
        crate::engine::ShipKind::Destroyer => 'd',
    }
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    game: &Battleship,
    hover: Option<(u16, u16)>,
    mode: ModeInfo<'_>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(PANEL_BG))
        .title(Line::from(Span::styled(
            " info ",
            Style::default().fg(ACCENT_FG),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let kv = |key: &str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(format!("{key:<7}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };
    let status_line = match game.status() {
        Status::Ongoing => Span::styled("-", Style::default().fg(GIVEN_FG)),
        Status::Won(s) => Span::styled(
            format!("{} wins!", s.symbol()),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ),
    };
    let thinking_line = if mode.thinking {
        Line::from(Span::styled(
            format!("{} thinking...", mode.ai_side.symbol()),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::raw(""))
    };
    let _fleet_line = |side: Side| -> Line<'static> {
        let left = game.remaining(side).len();
        Line::from(vec![
            Span::styled(side.symbol().to_string(), Style::default().fg(GIVEN_FG)),
            Span::styled(
                format!(" \u{25a0}{}", 5 - left),
                Style::default().fg(FAINT_FG),
            ),
            Span::styled("\u{25a0}".repeat(left), Style::default().fg(ACCENT_FG)),
        ])
    };

    let mut lines: Vec<Line> = vec![
        kv(
            "to play",
            Span::styled(
                game.turn().symbol().to_string(),
                Style::default().fg(GIVEN_FG),
            ),
        ),
        kv("status", status_line),
        kv(
            "mode",
            Span::styled(mode.label.to_string(), Style::default().fg(BONE)),
        ),
        thinking_line,
        kv(
            "fleet",
            Span::styled(
                format!(
                    "\u{25cb}{} \u{25cf}{}",
                    5 - game.remaining(Side::White).len(),
                    5 - game.remaining(Side::Black).len()
                ),
                Style::default().fg(GIVEN_FG),
            ),
        ),
    ];
    let _ = &mut lines;
    frame.render_widget(Paragraph::new(lines), inner);

    if area.height < 3 || inner.width < 6 {
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

/// Modal opponent picker shown instead of play until confirmed.
pub(crate) fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    items_out: &mut Vec<(Rect, usize)>,
) {
    items_out.clear();
    if area.width < 24 || area.height < 13 {
        return;
    }
    let popup = centered_rect(area, 30, 13);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " battleship setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, label) in SETUP_ITEMS.iter().enumerate() {
        let y = inner.y + (i as u16) * 2;
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
                Span::styled(format!("{:>02}", i + 1), Style::default().fg(FAINT_FG)),
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
