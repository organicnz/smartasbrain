//! The lobby: a roadmap screen shown as the first tab. It exists to prove the
//! shell hosts more than one game and gives new arrivals a landing page.

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::Game;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const DIM_FG: ratatui::style::Color = ratatui::style::Color::Rgb(112, 120, 140);
const FAINT_FG: ratatui::style::Color = ratatui::style::Color::Rgb(84, 92, 112);
const GIVEN_FG: ratatui::style::Color = ratatui::style::Color::Rgb(226, 230, 244);
const USER_FG: ratatui::style::Color = ratatui::style::Color::Rgb(126, 214, 166);
const ACCENT_FG: ratatui::style::Color = ratatui::style::Color::Rgb(122, 162, 247);

pub struct Lobby;

impl Lobby {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Lobby {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for Lobby {
    fn id(&self) -> &'static str {
        "lobby"
    }

    fn title(&self) -> &'static str {
        "lobby"
    }

    fn handle_key(&mut self, _key: KeyEvent) {}

    fn handle_mouse(&mut self, _mouse: MouseEvent) {}

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let rows = Layout::vertical([
            Constraint::Percentage(22),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Percentage(18),
        ])
        .split(area);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "S M A R T A S B R A I N",
                Style::default().fg(GIVEN_FG).add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            rows[1],
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "an arcade for games - and for the ais that play them",
                Style::default().fg(DIM_FG),
            )))
            .alignment(Alignment::Center),
            rows[2],
        );

        let bullet = Style::default().fg(ACCENT_FG);
        let item = |text: &'static str| {
            Line::from(vec![Span::styled("  \u{25b8} ", bullet), Span::raw(text)])
        };
        let roadmap = vec![
            item("sudoku - shipped"),
            Line::from(Span::styled(
                "  \u{25b8} go      - coming soon",
                Style::default().fg(FAINT_FG),
            )),
            Line::from(Span::styled(
                "  \u{25b8} ai battleground - ais competing live",
                Style::default().fg(FAINT_FG),
            )),
        ];
        frame.render_widget(
            Paragraph::new(roadmap)
                .style(Style::default().fg(USER_FG))
                .alignment(Alignment::Center),
            rows[4],
        );

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "[ ] or alt+1..9 switch tabs - ctrl+q quit",
                Style::default().fg(FAINT_FG),
            )))
            .alignment(Alignment::Center),
            rows[6],
        );
    }
}
