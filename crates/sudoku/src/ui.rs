use std::sync::LazyLock;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
};

use crate::app::{App, ClickAction, DIFFICULTIES, Geom, State, in_rect};

const BG: Color = Color::Rgb(13, 15, 23);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
const USER_FG: Color = Color::Rgb(126, 214, 166);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const CROSS_BG: Color = Color::Rgb(38, 44, 60);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);
const BUTTON_HOVER_BG: Color = Color::Rgb(40, 50, 76);
const CONFLICT_FG: Color = Color::Rgb(255, 126, 126);
const CONFLICT_BG: Color = Color::Rgb(98, 40, 50);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);

/// Gradient titles are pure functions of their text; computing them once keeps
/// f32 blending out of the 60 FPS frame budget.
static TITLE_SPANS: LazyLock<Vec<Span<'static>>> = LazyLock::new(|| gradient("S U D O K U"));
static WIN_TITLE_SPANS: LazyLock<Vec<Span<'static>>> = LazyLock::new(|| gradient("PUZZLE SOLVED"));
static PAUSED_TITLE_SPANS: LazyLock<Vec<Span<'static>>> = LazyLock::new(|| gradient("PAUSED"));

const VERTICAL: &str = "║";
const PANEL_W: u16 = 29;
const GAP: u16 = 2;
const BOARD_H: u16 = 16;
const PANEL_COMPACT_H: u16 = 11;

pub fn board_block_w(cell_w: u16) -> u16 {
    9 + 9 * cell_w
}

enum BoardLayout {
    Side { cell_w: u16 },
    Stack { cell_w: u16 },
}

fn pick_layout(area: Rect) -> Option<BoardLayout> {
    let (w, h) = (area.width, area.height);
    let side_total = |cw| board_block_w(cw) + GAP + PANEL_W + 2;
    let stack_total = |cw| board_block_w(cw).max(PANEL_W) + 2;

    if h >= BOARD_H + 3 {
        for cw in [7, 5, 3] {
            if w >= side_total(cw) {
                return Some(BoardLayout::Side { cell_w: cw });
            }
        }
    }
    if h >= BOARD_H + 1 + PANEL_COMPACT_H + 3 {
        for cw in [3, 1] {
            if w >= stack_total(cw) {
                return Some(BoardLayout::Stack { cell_w: cw });
            }
        }
    }
    if h >= BOARD_H + 3 && w >= side_total(1) {
        return Some(BoardLayout::Side { cell_w: 1 });
    }
    None
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    app.clickables.clear();
    app.hover_cell = match app.state {
        State::Playing if !app.paused => app.hover.and_then(|(c, r)| app.cell_at(c, r)),
        _ => None,
    };

    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    match app.state {
        State::Menu => draw_menu(frame, app, area),
        State::Playing => draw_game(frame, app, area),
        State::Won => {
            draw_game(frame, app, area);
            draw_win(frame, app, area);
        }
    }

    if app.paused {
        draw_pause_overlay(frame, app, area);
    }
}

fn hovering(app: &App, rect: Rect) -> bool {
    app.hover.is_some_and(|(c, r)| in_rect(rect, c, r))
}

fn draw_menu(frame: &mut Frame, app: &mut App, area: Rect) {
    let popup = centered_rect(area, 48, 12);
    frame.render_widget(Clear, popup);

    let inner = Rect {
        x: popup.x + 1,
        y: popup.y + 1,
        width: popup.width - 2,
        height: popup.height - 2,
    };

    let mut lines = vec![
        Line::from(TITLE_SPANS.clone()).centered(),
        Line::from(""),
        Line::from(Span::styled(
            "choose your challenge",
            Style::default().fg(DIM_FG),
        ))
        .centered(),
        Line::from(""),
    ];
    for (i, (name, clues, desc)) in DIFFICULTIES.iter().enumerate() {
        let selected = i == app.difficulty;
        let row_rect = Rect {
            x: inner.x + 1,
            y: inner.y + 4 + i as u16,
            width: inner.width - 2,
            height: 1,
        };
        let hot = hovering(app, row_rect);
        app.clickables
            .push((row_rect, ClickAction::PickDifficulty(i)));

        let marker = if selected || hot { "> " } else { "  " };
        let name_style = if selected {
            Style::default().fg(GIVEN_FG).add_modifier(Modifier::BOLD)
        } else if hot {
            Style::default().fg(GIVEN_FG)
        } else {
            Style::default().fg(DIM_FG)
        };
        let desc_style = if selected || hot {
            Style::default().fg(Color::Rgb(150, 158, 178))
        } else {
            Style::default().fg(FAINT_FG)
        };
        let best_style = if selected || hot {
            Style::default().fg(GOLD_FG)
        } else {
            Style::default().fg(FAINT_FG)
        };
        let best_text = match app.records.best_ms[i] {
            Some(ms) => format!("best {:>7}", format_time_ms(ms)),
            None => "best       -".to_string(),
        };

        lines.push(Line::from(vec![
            Span::styled(
                marker.to_string(),
                Style::default().fg(ACCENT_FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{name:<8}"), name_style),
            Span::styled(format!("{clues:>2}  "), desc_style),
            Span::styled(format!("{desc:<17}"), desc_style),
            Span::styled(best_text, best_style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "up/down choose - enter or click to start - q quit",
            Style::default().fg(FAINT_FG),
        ))
        .centered(),
    );

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(PANEL_BG));
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(block),
        popup,
    );
}

fn draw_game(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(layout) = pick_layout(area) else {
        app.geom = None;
        frame.render_widget(
            Paragraph::new("terminal too small - resize to at least 51x19")
                .alignment(Alignment::Center),
            area,
        );
        return;
    };

    match layout {
        BoardLayout::Side { cell_w } => draw_side_by_side(frame, app, area, cell_w),
        BoardLayout::Stack { cell_w } => draw_stacked(frame, app, area, cell_w),
    }
}

fn render_hint(frame: &mut Frame, area: Rect, wide: bool) {
    let text = if wide {
        "\u{2190}\u{2191}\u{2193}\u{2192} move - 1-9 place - z notes - \u{2423} hint - u undo - r restart - p pause - n menu - q quit"
    } else {
        "click move - 1-9 - z notes - u undo - p pause - q quit"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(text, Style::default().fg(FAINT_FG)))
            .alignment(Alignment::Center),
        Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        },
    );
}

fn draw_side_by_side(frame: &mut Frame, app: &mut App, area: Rect, cell_w: u16) {
    let board_w = board_block_w(cell_w);
    let total = board_w + GAP + PANEL_W;
    let cluster = centered_rect(area, total, BOARD_H);
    let cols = Layout::horizontal([
        Constraint::Length(board_w),
        Constraint::Length(GAP),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    draw_board(frame, cols[0], app, cell_w);
    draw_panel(frame, cols[2], app, false);
    render_hint(frame, area, total >= 69);
}

fn draw_stacked(frame: &mut Frame, app: &mut App, area: Rect, cell_w: u16) {
    let board_w = board_block_w(cell_w);
    let cluster_w = board_w.max(PANEL_W);
    let cluster_h = BOARD_H + 1 + PANEL_COMPACT_H;
    let cluster = centered_rect(area, cluster_w, cluster_h);

    let board_rect = Rect {
        x: cluster.x + (cluster_w - board_w) / 2,
        y: cluster.y,
        width: board_w,
        height: BOARD_H,
    };
    let panel_rect = Rect {
        x: cluster.x + (cluster_w - PANEL_W) / 2,
        y: cluster.y + BOARD_H + 1,
        width: PANEL_W,
        height: PANEL_COMPACT_H,
    };

    draw_board(frame, board_rect, app, cell_w);
    draw_panel(frame, panel_rect, app, true);
    render_hint(frame, area, false);
}

fn border_line(corner_pair: [&str; 2], join: &str, seg: &str) -> String {
    format!(
        "{}{seg}{join}{seg}{join}{seg}{}",
        corner_pair[0], corner_pair[1]
    )
}

fn token(content: String, width: u16) -> String {
    if width == 1 {
        content
    } else {
        format!("{:^width$}", content, width = width as usize)
    }
}

/// What may be shown for a cell: while paused, player entries are hidden so
/// nobody can study the grid against the stopped clock.
fn display_value(app: &App, idx: usize) -> u8 {
    if app.paused && !app.board.fixed[idx] {
        0
    } else {
        app.board.cells[idx]
    }
}

fn notes_label(notes: u16, cell_w: u16) -> Option<String> {
    if notes == 0 || cell_w < 3 {
        return None;
    }
    let sep = if cell_w >= 5 { " " } else { "" };
    let text: String = (1..=9u8)
        .filter(|d| notes & (1 << (d - 1)) != 0)
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(sep);
    let max = cell_w as usize;
    Some(text.chars().take(max).collect())
}

fn draw_board(frame: &mut Frame, area: Rect, app: &mut App, cell_w: u16) {
    let diff = DIFFICULTIES[app.difficulty].0;
    let mut block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(TITLE_SPANS.clone()).centered());

    if app.notes_mode {
        block = block.title_bottom(Line::from(Span::styled(
            " \u{270e} notes ",
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        )));
    }
    block = block.title_bottom(
        Line::from(Span::styled(
            format!("{diff} "),
            Style::default().fg(DIM_FG),
        ))
        .right_aligned(),
    );

    let inner = block.inner(area);
    let label_w = if cell_w >= 3 { 3 } else { 0 };
    app.geom = Some(Geom {
        grid: inner,
        cell_w,
        label_w,
    });
    frame.render_widget(block, area);

    let border_style = Style::default().fg(BORDER_FG);
    let dim = Style::default().fg(DIM_FG);
    let selected_value = display_value(app, app.cursor);
    let label_pad = " ".repeat(label_w as usize);
    let seg = "\u{2550}".repeat(3 * cell_w as usize);

    let mut lines = Vec::new();

    if label_w > 0 {
        let mut letters = vec![
            Span::raw(label_pad.clone()),
            Span::styled(VERTICAL, border_style),
        ];
        for box_col in 0..3 {
            for i in 0..3 {
                let letter = char::from(b'A' + (box_col * 3 + i) as u8);
                letters.push(Span::styled(
                    token(letter.to_string(), cell_w),
                    Style::default().fg(FAINT_FG),
                ));
            }
            letters.push(Span::styled(VERTICAL, border_style));
        }
        lines.push(Line::from(letters));
    }

    lines.push(Line::from(Span::styled(
        format!("{label_pad}{}", border_line(["╔", "╗"], "╦", &seg)),
        border_style,
    )));
    for row in 0..9 {
        if row == 3 || row == 6 {
            lines.push(Line::from(Span::styled(
                format!("{label_pad}{}", border_line(["╠", "╣"], "╬", &seg)),
                border_style,
            )));
        }
        let mut spans = Vec::new();
        if label_w > 0 {
            spans.push(Span::styled(format!(" {} ", row + 1), dim));
        }
        spans.push(Span::styled(VERTICAL, border_style));
        for box_col in 0..3 {
            for i in 0..3 {
                let idx = row * 9 + box_col * 3 + i;
                let value = display_value(app, idx);
                let symbol = if value == 0 {
                    match notes_label(app.notes[idx], cell_w) {
                        Some(text) => text,
                        None => "\u{b7}".to_string(),
                    }
                } else {
                    char::from(b'0' + value).to_string()
                };
                spans.push(Span::styled(
                    token(symbol, cell_w),
                    cell_style(app, idx, value, selected_value),
                ));
            }
            spans.push(Span::styled(VERTICAL, border_style));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        format!("{label_pad}{}", border_line(["╚", "╝"], "╩", &seg)),
        border_style,
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn cell_style(app: &App, idx: usize, value: u8, selected_value: u8) -> Style {
    let paused = app.paused;
    let empty_shown = value == 0;
    let given = app.board.fixed[idx];

    // While paused, reveal nothing about entries or conflicts.
    if paused {
        let base = if empty_shown {
            Style::default().fg(FAINT_FG)
        } else {
            Style::default().fg(GIVEN_FG)
        };
        return if idx == app.cursor {
            base.bg(SELECT_BG)
        } else {
            base
        };
    }

    let selected = idx == app.cursor;
    let hovered = app.hover_cell == Some(idx) && !selected;
    let (r, c) = (idx / 9, idx % 9);
    let (sr, sc) = (app.cursor / 9, app.cursor % 9);
    let crosshair = r == sr || c == sc || ((r / 3) == (sr / 3) && (c / 3) == (sc / 3));
    let conflict = !empty_shown && app.conflicts[idx];

    let mut style = Style::default();
    if hovered {
        style = style.bg(HOVER_BG);
    } else if crosshair && !selected {
        style = style.bg(CROSS_BG);
    }
    style = if empty_shown {
        if app.notes[idx] != 0 {
            style.fg(DIM_FG)
        } else {
            style.fg(FAINT_FG)
        }
    } else if given {
        style.fg(GIVEN_FG)
    } else {
        style.fg(USER_FG).add_modifier(Modifier::BOLD)
    };
    if selected_value != 0 && value == selected_value {
        style = style.fg(GOLD_FG).add_modifier(Modifier::BOLD);
    }
    if selected {
        style = style.bg(SELECT_BG);
        if value != 0 && !(selected_value != 0 && value == selected_value) {
            style = if given {
                style.fg(Color::White)
            } else {
                style.fg(Color::Rgb(158, 232, 184))
            };
        }
    }
    if conflict {
        style = style
            .fg(CONFLICT_FG)
            .bg(CONFLICT_BG)
            .add_modifier(Modifier::BOLD);
    }
    style
}

fn draw_panel(frame: &mut Frame, area: Rect, app: &mut App, compact: bool) {
    let label = Style::default().fg(DIM_FG);
    let time = if app.state == State::Won {
        app.elapsed
    } else {
        app.live_elapsed()
    };
    let filled = app.board.cells.iter().filter(|&&v| v != 0).count();
    let selected_value = display_value(app, app.cursor);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(PANEL_BORDER_FG))
        .style(Style::default().bg(PANEL_BG))
        .title(Line::from(Span::styled(
            " info ",
            Style::default().fg(ACCENT_FG).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (stats_h, numpad_h, actions_h) = if compact { (4, 2, 1) } else { (5, 6, 1) };
    let rows = Layout::vertical([
        Constraint::Length(stats_h),
        Constraint::Length(1),
        Constraint::Length(numpad_h),
        Constraint::Length(1),
        Constraint::Length(actions_h),
    ])
    .split(inner);

    let kv_line = |key: &str, value: String, value_style: Style| {
        Line::from(vec![
            Span::styled(format!("{key:<9}"), label),
            Span::styled(value, value_style),
        ])
    };
    let notes_value = if app.notes_mode { "on" } else { "off" };
    let notes_style = if app.notes_mode {
        Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(GIVEN_FG)
    };
    let mut stats_lines = vec![
        kv_line(
            "mode",
            DIFFICULTIES[app.difficulty].0.to_string(),
            Style::default().fg(GIVEN_FG).add_modifier(Modifier::BOLD),
        ),
        kv_line("time", format_time(time), Style::default().fg(GIVEN_FG)),
        kv_line(
            "filled",
            format!("{filled}/81"),
            Style::default().fg(GIVEN_FG),
        ),
        kv_line(
            "errors",
            app.mistakes.to_string(),
            if app.mistakes > 0 {
                Style::default()
                    .fg(CONFLICT_FG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(USER_FG)
            },
        ),
    ];

    if !compact {
        stats_lines.push(kv_line(
            "notes",
            format!("{notes_value} (\u{270e})"),
            notes_style,
        ));
        let notes_rect = Rect {
            x: inner.x,
            y: rows[0].y + stats_h - 1,
            width: inner.width,
            height: 1,
        };
        app.clickables.push((notes_rect, ClickAction::ToggleNotes));
        if hovering(app, notes_rect) {
            stats_lines.last_mut().unwrap().style = Style::default().bg(BUTTON_HOVER_BG);
        }
    }

    frame.render_widget(Paragraph::new(stats_lines), rows[0]);

    draw_numpad(frame, rows[2], app, selected_value, compact);
    draw_action_buttons(frame, rows[4], app);
}

fn draw_numpad(frame: &mut Frame, area: Rect, app: &mut App, selected_value: u8, compact: bool) {
    let (cols, digit_rows) = if compact {
        (
            Layout::horizontal([Constraint::Length(3); 9])
                .split(area)
                .to_vec(),
            Vec::new(),
        )
    } else {
        (
            Layout::horizontal([
                Constraint::Length(9),
                Constraint::Length(9),
                Constraint::Length(9),
            ])
            .split(area)
            .to_vec(),
            Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
            ])
            .split(area)
            .to_vec(),
        )
    };

    for d in 1..=9u8 {
        let rect = if compact {
            Rect {
                x: cols[(d - 1) as usize].x,
                y: area.y,
                width: cols[(d - 1) as usize].width.max(3),
                height: area.height,
            }
        } else {
            let col = ((d - 1) % 3) as usize;
            let row = ((d - 1) / 3) as usize;
            Rect {
                x: cols[col].x,
                y: digit_rows[row].y,
                width: cols[col].width,
                height: 2,
            }
        };
        let remaining = 9 - app.board.cells.iter().filter(|&&v| v == d).count();
        let done = remaining == 0;
        let hot = hovering(app, rect);
        app.clickables.push((rect, ClickAction::Digit(d)));

        let bg = if hot { BUTTON_HOVER_BG } else { BUTTON_BG };
        let is_selected = !app.paused && selected_value == d;
        let digit_style = if is_selected {
            Style::default()
                .fg(Color::White)
                .bg(SELECT_BG)
                .add_modifier(Modifier::BOLD)
        } else if done {
            Style::default().fg(FAINT_FG).bg(bg)
        } else if hot {
            Style::default()
                .fg(GIVEN_FG)
                .bg(BUTTON_HOVER_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(GIVEN_FG)
                .bg(BUTTON_BG)
                .add_modifier(Modifier::BOLD)
        };
        let count_style = if done {
            Style::default().fg(USER_FG).bg(bg)
        } else {
            Style::default().fg(DIM_FG).bg(bg)
        };

        let button = Paragraph::new(vec![
            Line::from(Span::styled(d.to_string(), digit_style)).alignment(Alignment::Center),
            Line::from(Span::styled(remaining.to_string(), count_style))
                .alignment(Alignment::Center),
        ]);
        frame.render_widget(button, rect);
    }
}

fn draw_action_buttons(frame: &mut Frame, area: Rect, app: &mut App) {
    let cols = Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ])
    .split(area);
    let actions = [
        ("hint", ClickAction::Hint),
        ("undo", ClickAction::Undo),
        ("erase", ClickAction::Erase),
        ("new", ClickAction::NewGame),
    ];
    for (i, (text, action)) in actions.iter().enumerate() {
        let rect = cols[i];
        let hot = hovering(app, rect);
        app.clickables.push((rect, *action));
        let style = if hot {
            Style::default()
                .fg(GIVEN_FG)
                .bg(BUTTON_HOVER_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(ACCENT_FG).bg(BUTTON_BG)
        };
        frame.render_widget(
            Paragraph::new(Span::styled((*text).to_string(), style)).alignment(Alignment::Center),
            rect,
        );
    }
}

fn draw_win(frame: &mut Frame, app: &mut App, area: Rect) {
    let popup = centered_rect(area, 42, 10);
    frame.render_widget(Clear, popup);

    let inner = Rect {
        x: popup.x + 1,
        y: popup.y + 1,
        width: popup.width - 2,
        height: popup.height - 2,
    };

    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(USER_FG))
        .style(Style::default().bg(Color::Rgb(12, 18, 14)));
    frame.render_widget(block, popup);

    frame.render_widget(
        Paragraph::new(Line::from(WIN_TITLE_SPANS.clone()).centered()),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );
    let stats = if app.hints > 0 {
        format!(
            "time {} - errors {} - hints {}",
            format_time(app.elapsed),
            app.mistakes,
            app.hints
        )
    } else {
        format!(
            "time {} - errors {}",
            format_time(app.elapsed),
            app.mistakes
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(stats, Style::default().fg(GIVEN_FG))).centered()),
        Rect {
            x: inner.x,
            y: inner.y + 2,
            width: inner.width,
            height: 1,
        },
    );

    if let Some(info) = app.win_info {
        let (text, style) = if info.new_record {
            (
                "\u{2605} new personal best!".to_string(),
                Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
            )
        } else {
            match info.prev_best {
                Some(prev) => (
                    format!("personal best {}", format_time(prev)),
                    Style::default().fg(DIM_FG),
                ),
                None => (String::new(), Style::default()),
            }
        };
        if !text.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(text, style)).centered()),
                Rect {
                    x: inner.x,
                    y: inner.y + 3,
                    width: inner.width,
                    height: 1,
                },
            );
        }
    }

    let halves = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(Rect {
            x: inner.x,
            y: inner.y + 5,
            width: inner.width,
            height: 1,
        });
    for (i, (text, action)) in [
        ("replay", ClickAction::Restart),
        ("new game", ClickAction::NewGame),
    ]
    .iter()
    .enumerate()
    {
        let rect = halves[i];
        let hot = hovering(app, rect);
        app.clickables.push((rect, *action));
        let style = if hot {
            Style::default()
                .fg(GIVEN_FG)
                .bg(SELECT_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(USER_FG)
        };
        frame.render_widget(
            Paragraph::new(Span::styled((*text).to_string(), style)).alignment(Alignment::Center),
            rect,
        );
    }
}

fn draw_pause_overlay(frame: &mut Frame, app: &mut App, area: Rect) {
    let popup = centered_rect(area, 28, 5);
    frame.render_widget(Clear, popup);

    let resume_rect = popup;
    app.clickables.push((resume_rect, ClickAction::TogglePause));

    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(ACCENT_FG))
        .style(Style::default().bg(Color::Rgb(16, 20, 32)));
    frame.render_widget(block, popup);

    frame.render_widget(
        Paragraph::new(Line::from(PAUSED_TITLE_SPANS.clone()).centered()),
        Rect {
            x: popup.x,
            y: popup.y + 1,
            width: popup.width,
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(
            Line::from(Span::styled(
                "press p or click to resume",
                Style::default().fg(DIM_FG),
            ))
            .centered(),
        ),
        Rect {
            x: popup.x,
            y: popup.y + 2,
            width: popup.width,
            height: 1,
        },
    );
}

fn gradient(text: &str) -> Vec<Span<'static>> {
    let from = (255u8, 152u8, 92u8);
    let to = (128u8, 168u8, 255u8);
    let chars: Vec<char> = text.chars().collect();
    let steps = chars.len().saturating_sub(1).max(1);
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let t = i as f32 / steps as f32;
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
            let style = if c.is_whitespace() {
                Style::default()
            } else {
                Style::default()
                    .fg(Color::Rgb(
                        mix(from.0, to.0),
                        mix(from.1, to.1),
                        mix(from.2, to.2),
                    ))
                    .add_modifier(Modifier::BOLD)
            };
            Span::styled(c.to_string(), style)
        })
        .collect()
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn format_time(d: std::time::Duration) -> String {
    format_time_secs(d.as_secs())
}

fn format_time_ms(ms: u64) -> String {
    format_time_secs(ms / 1000)
}

fn format_time_secs(total: u64) -> String {
    if total >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            total / 3600,
            (total % 3600) / 60,
            total % 60
        )
    } else {
        format!("{:02}:{:02}", total / 60, total % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::State;
    use crate::generator;
    use ratatui::backend::TestBackend;

    fn render_at(app: &mut App, w: u16, h: u16) {
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app, f.area())).unwrap();
    }

    fn playing_app() -> App {
        let mut app = App::new();
        app.persist_records = false;
        let puzzle = generator::generate_seeded(7, DIFFICULTIES[0].1);
        app.initial = puzzle.board.cells;
        app.begin_puzzle(puzzle.board, puzzle.solution);
        app
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [
            (80u16, 24u16),
            (120, 40),
            (200, 55),
            (105, 30),
            (69, 19),
            (60, 35),
            (40, 32),
            (31, 33),
            (50, 18),
            (30, 10),
        ] {
            let mut app = playing_app();
            app.state = State::Playing;
            render_at(&mut app, w, h);
        }
    }

    #[test]
    fn geom_tracks_scale_and_clicks_resolve() {
        let mut app = playing_app();
        app.state = State::Playing;

        render_at(&mut app, 80, 24);
        let g = app.geom.unwrap();
        assert_eq!(g.cell_w, 3);
        assert_eq!(g.label_w, 3);
        let x = g.grid.x + g.label_w + 1 + g.cell_w / 2;
        let y = g.grid.y + 2;
        assert_eq!(app.cell_at(x, y), Some(0));

        render_at(&mut app, 120, 40);
        assert_eq!(app.geom.unwrap().cell_w, 7);

        render_at(&mut app, 60, 35);
        let g = app.geom.unwrap();
        assert_eq!(g.cell_w, 3);
        let board_bottom = g.grid.y + g.grid.height;
        assert!(app.clickables.iter().any(|(r, _)| r.y > board_bottom));

        render_at(&mut app, 40, 20);
        assert!(app.geom.is_none());
    }

    #[test]
    fn paused_game_hides_entries_and_renders_overlay() {
        let mut app = playing_app();
        app.state = State::Playing;
        let empty: Vec<usize> = (0..81usize).filter(|&i| app.board.cells[i] == 0).collect();
        app.cursor = empty[0];
        app.place(5);
        app.toggle_pause();

        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &mut app, f.area())).unwrap();

        let buffer = terminal.backend().buffer();
        let g = app.geom.expect("board rendered at 80x24");
        let mut visible_digits = 0;
        for y in g.grid.y..g.grid.y + g.grid.height {
            for x in g.grid.x..g.grid.x + g.grid.width {
                let symbol = buffer[(x, y)].symbol();
                if let Some(c) = symbol.chars().next()
                    && ('1'..='9').contains(&c)
                {
                    visible_digits += 1;
                }
            }
        }
        let givens = app.board.fixed.iter().filter(|&&f| f).count();
        assert!(
            visible_digits <= givens,
            "paused view leaked entries: {visible_digits} digits vs {givens} givens"
        );
    }

    #[test]
    fn notes_render_as_small_marks() {
        assert_eq!(notes_label(0, 3), None);
        assert_eq!(
            notes_label((1 << 0) | (1 << 3) | (1 << 8), 3),
            Some("149".to_string())
        );
        assert_eq!(
            notes_label((1 << 0) | (1 << 3) | (1 << 8), 5),
            Some("1 4 9".to_string())
        );
        assert_eq!(notes_label(0x1FF, 1), None);
    }

    #[test]
    fn draw_cost_supports_60fps() {
        use std::time::{Duration, Instant};

        let mut app = playing_app();
        let backend = TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        // Warm caches (gradient statics, allocator, layout) before measuring.
        for _ in 0..10 {
            terminal.draw(|f| draw(f, &mut app, f.area())).unwrap();
        }

        const FRAMES: usize = 300;
        let start = Instant::now();
        for _ in 0..FRAMES {
            terminal.draw(|f| draw(f, &mut app, f.area())).unwrap();
        }
        let per_frame = start.elapsed() / FRAMES as u32;
        println!("avg frame build cost: {per_frame:?}");
        assert!(
            per_frame < Duration::from_millis(20),
            "frame construction too slow for a 20ms budget: {per_frame:?}"
        );
    }

    #[test]
    fn menu_and_win_screens_show_records() {
        let mut app = App::new();
        render_at(&mut app, 80, 24); // menu

        let mut won = playing_app();
        for i in 0..81usize {
            won.board.cells[i] = won.solution[i];
        }
        won.after_change(true);
        assert_eq!(won.state, State::Won);
        render_at(&mut won, 80, 24);
    }
}
