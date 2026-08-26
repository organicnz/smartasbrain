//! Rendering and hit-testing for dominoes: the shared chain with open-end
//! badges, the player's rack of clickable tiles, and draw/pass actions.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::engine::Side;

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

pub(crate) const BOARD_W: u16 = 62;
pub(crate) const BOARD_H: u16 = 8; // ends badge + chain row + spare
pub(crate) const RACK_H: u16 = 4;
/// Each tile renders as `[a|b]` (5 cols) plus one space.
const PANEL_W: u16 = 26;

#[derive(Clone, Copy)]
pub(crate) struct View {
    pub(crate) cursor: Option<usize>,
    pub(crate) hover: Option<(u16, u16)>,
}

#[derive(Clone, Copy)]
pub(crate) struct ModeInfo<'a> {
    pub(crate) label: &'a str,
    pub(crate) ai_side: Side,
    pub(crate) thinking: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct Geom {
    rack_x: u16,
    rack_y: u16,
    board_x: u16,
    board_y: u16,
    /// Chosen tile-slot width (resizable canvas).
    tile_w: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Draw,
    Pass,
    Restart,
}

impl Geom {
    fn new(rack_x: u16, rack_y: u16, board_x: u16, board_y: u16, tile_w: u16) -> Self {
        Self {
            rack_x,
            rack_y,
            board_x,
            board_y,
            tile_w: tile_w.max(5),
        }
    }

    /// Hand index clicked on the rack row.
    pub(crate) fn tile_at(&self, col: u16, row: u16) -> Option<usize> {
        if row != self.rack_y + 1 {
            return None;
        }
        let rel = col.checked_sub(self.rack_x + 1)?;
        let idx = (rel / self.tile_w) as usize;
        Some(idx)
    }

    #[allow(dead_code)]
    pub(crate) fn action_buttons(&self) -> [Rect; 3] {
        let y = self.board_y + BOARD_H - 3;
        [
            Rect {
                x: self.board_x + 1,
                y,
                width: 9,
                height: 1,
            },
            Rect {
                x: self.board_x + 11,
                y,
                width: 8,
                height: 1,
            },
            Rect {
                x: self.board_x + 20,
                y,
                width: 10,
                height: 1,
            },
        ]
    }
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

/// Base tile glyph (5 columns); slots center it at their chosen width.
fn tile_str(a: u8, b: u8) -> String {
    format!("[{a}\u{2502}{b}]")
}

/// Slot widths tried from roomiest to tightest.
const TILE_WIDTHS: [u16; 3] = [9, 7, 5];

pub(crate) fn draw(
    frame: &mut Frame,
    game: &crate::engine::Dominoes,
    area: Rect,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    if area.width < BOARD_W || area.height < BOARD_H + RACK_H + 2 {
        *geom_out = None;
        return;
    }
    let side_by_side = area.width >= BOARD_W + 2 + PANEL_W && area.height >= BOARD_H + RACK_H + 2;

    let cluster_h = if side_by_side {
        BOARD_H + RACK_H + 2
    } else {
        BOARD_H + RACK_H + PANEL_H_PLACEHOLDER
    };
    let _ = cluster_h;

    let rows = Layout::vertical([Constraint::Length(BOARD_H), Constraint::Length(RACK_H)]).split(
        centered_rect(area, BOARD_W.min(area.width), BOARD_H + RACK_H),
    );

    let panel = if side_by_side {
        // Right of the combined block.
        let outer = centered_rect(area, area.width.min(BOARD_W + 2 + PANEL_W), area.height);
        Rect {
            x: outer.x + outer.width.saturating_sub(PANEL_W),
            y: outer.y + 1,
            width: PANEL_W.min(outer.width),
            height: (BOARD_H + RACK_H).min(outer.height.saturating_sub(2)),
        }
    } else {
        let y = rows[1].y.saturating_add(RACK_H);
        Rect {
            x: rows[0].x,
            y,
            width: PANEL_W.min(area.width),
            height: 7.min(area.y.saturating_add(area.height).saturating_sub(y)),
        }
    };

    draw_board(frame, rows[0], game, view, mode, geom_out, buttons);
    let tile_w = if area.width >= 95 {
        TILE_WIDTHS[0]
    } else if area.width >= 75 {
        TILE_WIDTHS[1]
    } else {
        TILE_WIDTHS[2]
    };
    draw_rack(frame, rows[1], game, view, tile_w, geom_out);
    draw_panel(frame, panel, game, mode, buttons);
}

const PANEL_H_PLACEHOLDER: u16 = 9;

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    game: &crate::engine::Dominoes,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            format!(
                " dominoes \u{2502} \u{25cb}{} \u{25cf}{} \u{2502} yard {} ",
                game.hand(Side::White).len(),
                game.hand(Side::Black).len(),
                game.boneyard_len()
            ),
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                mode.label.to_string(),
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let geom = Geom::new(0, 0, inner.x, inner.y, TILE_WIDTHS[2]);
    *geom_out = Some(geom);

    // Open-end badge line.
    let (l, r) = game.ends();
    let ends_text = match (l, r) {
        (Some(l), Some(r)) => format!("open ends:  \u{25c4} {l} \u{2500}\u{2500}\u{25b6} {r} "),
        _ => "empty table \u{2014} lead anything".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            ends_text,
            Style::default().fg(GOLD_FG),
        )))
        .alignment(Alignment::Center),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );

    // Chain window (fits the width; ellipsises both sides when long).
    let halves = game.line();
    let capacity = ((inner.width as usize) / 6).max(2) * 2;
    let skip_head = halves
        .len()
        .saturating_sub(capacity / 2 * 2)
        .min(halves.len());
    let keep_tail = halves.len().saturating_sub(skip_head).min(capacity);
    let window: Vec<u8> = halves[skip_head..skip_head + keep_tail].to_vec();

    let mut spans: Vec<Span<'static>> = Vec::new();
    if skip_head > 0 {
        spans.push(Span::styled("\u{2026} ", Style::default().fg(FAINT_FG)));
    }
    for pair in window.chunks(2) {
        let a = pair.first().copied().unwrap_or(0);
        let b = pair.get(1).copied().unwrap_or(a);
        spans.push(Span::styled(tile_str(a, b), Style::default().fg(GIVEN_FG)));
        spans.push(Span::raw(" "));
    }
    if skip_head + keep_tail < halves.len() {
        spans.push(Span::styled("\u{2026}", Style::default().fg(FAINT_FG)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        Rect {
            x: inner.x,
            y: inner.y + 2,
            width: inner.width,
            height: 1,
        },
    );

    // Action buttons along the bottom of the board block.
    if inner.height < 5 || inner.width < 30 {
        return;
    }
    let btn_y = inner.y + inner.height - 2;
    let specs = [
        (
            Rect {
                x: inner.x + 1,
                y: btn_y,
                width: 9,
                height: 1,
            },
            Button::Draw,
            " draw ",
        ),
        (
            Rect {
                x: inner.x + 12,
                y: btn_y,
                width: 8,
                height: 1,
            },
            Button::Pass,
            " pass ",
        ),
        (
            Rect {
                x: inner.x + 22,
                y: btn_y,
                width: 10,
                height: 1,
            },
            Button::Restart,
            " restart ",
        ),
    ];
    for (rect, button, label) in specs {
        buttons.push((rect, button));
        let enabled = match button {
            Button::Draw => game.must_draw(),
            Button::Pass => game.can_pass(),
            Button::Restart => true,
        };
        let hot = view.hover.is_some_and(|(c, r)| in_rect(rect, c, r));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label.to_string(),
                Style::default()
                    .fg(if enabled { ACCENT_FG } else { FAINT_FG })
                    .bg(if hot && enabled { HOVER_BG } else { BUTTON_BG }),
            ))),
            rect,
        );
    }

    if mode.thinking && inner.height > 5 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(
                    "{} thinking...",
                    if mode.ai_side == Side::White {
                        "\u{25cb}"
                    } else {
                        "\u{25cf}"
                    }
                ),
                Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            Rect {
                x: inner.x,
                y: btn_y.saturating_sub(1),
                width: inner.width,
                height: 1,
            },
        );
    }
}

fn draw_rack(
    frame: &mut Frame,
    area: Rect,
    game: &crate::engine::Dominoes,
    view: View,
    tile_w: u16,
    geom_out: &mut Option<Geom>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " your rack ",
            Style::default().fg(BORDER_FG),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    *geom_out = Some(Geom::new(inner.x, inner.y, 0, 0, tile_w));

    let legal = game.legal_moves();
    let is_playable = |i: usize| legal.iter().any(|&(idx, _, _)| idx == i);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, t) in game.hand(game.turn()).iter().enumerate() {
        let cursored = view.cursor == Some(i);
        let hot = view.hover.is_some_and(|(c, r)| {
            c >= inner.x + 1 + i as u16 * tile_w
                && c < inner.x + 1 + i as u16 * tile_w + 5
                && r == inner.y + 1
        });
        let (fg, bg, modifier) = if cursored && is_playable(i) {
            (BONE, CROSS_BG, Modifier::BOLD)
        } else if is_playable(i) {
            (ACCENT_FG, if hot { HOVER_BG } else { BG }, Modifier::BOLD)
        } else {
            (ASH, if hot { HOVER_BG } else { BG }, Modifier::empty())
        };
        spans.push(Span::styled(
            format!("{:^width$}", tile_str(t.a, t.b), width = tile_w as usize),
            Style::default().fg(fg).bg(bg).add_modifier(modifier),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: 1,
        },
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2190}\u{2192} pick \u{00b7} enter play \u{00b7} d draw \u{00b7} p pass",
            Style::default().fg(FAINT_FG),
        )))
        .alignment(Alignment::Center),
        Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: inner.width,
            height: 1,
        },
    );
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    game: &crate::engine::Dominoes,
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
            Span::styled(format!("{key:<8}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };
    let status_line = match game.status() {
        crate::engine::Status::Ongoing => Span::styled("-", Style::default().fg(GIVEN_FG)),
        crate::engine::Status::Won(s, pts) => Span::styled(
            format!("{} wins +{}", s.symbol(), pts),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ),
        crate::engine::Status::Draw => Span::styled("draw", Style::default().fg(GIVEN_FG)),
    };
    let thinking_line = if mode.thinking {
        Line::from(Span::styled(
            format!("{} thinking...", mode.ai_side.symbol()),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::raw(""))
    };

    frame.render_widget(
        Paragraph::new(vec![
            kv("status", status_line),
            kv(
                "yard",
                Span::styled(
                    format!("{} left", game.boneyard_len()),
                    Style::default().fg(GIVEN_FG),
                ),
            ),
            kv(
                "mode",
                Span::styled(mode.label.to_string(), Style::default().fg(BONE)),
            ),
            thinking_line,
        ]),
        inner,
    );
    let _ = buttons;
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
            " dominoes setup ",
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
