//! Help popup implementation.
use crossterm::event::KeyCode;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
        Widget, Wrap,
    },
};

/// The stateful Help widget.
#[derive(Default)]
pub struct Help {}

/// State for the Help widget (scrolling, visibility)
pub struct HelpState {
    content: Text<'static>,
    visible: bool,
    vs_state: ScrollbarState,
}

impl HelpState {
    pub fn handle_key(&mut self, k: KeyCode) {
        match k {
            KeyCode::Char('q') | KeyCode::Char('?') => {
                self.visible = false;
            }
            KeyCode::Up => {
                self.vs_state.prev();
            }
            KeyCode::Down => {
                self.vs_state.next();
            }

            _ => {}
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}

/// Generate the full help text.
fn help() -> Text<'static> {
    let lines: Vec<Line> = vec![
        Line::from("Helix Calc is a Reverse Polish Notation calculator."),
        Line::from(""),
        Line::from("Operators manipulate the stack of values [S1, S2, ...]:"),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            "+ - * /".blue(),
            Span::raw(" : perform the arithmetic operation on S2 and S1"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "%".blue(),
            Span::raw(" : compute the modulo of S2 divided by S1"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "^".blue(),
            Span::raw(" : raise S2 to the power of S1"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "P".blue(),
            Span::raw(" : pop S1 off the stack"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "d".blue(),
            Span::raw(" : duplicate S1"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "v".blue(),
            Span::raw(" : compute the square root of S1"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "k".blue(),
            Span::raw(" : pop S1 and use it to set the precision"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "r".blue(),
            Span::raw(" : swap S1 and S2"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "u".blue(),
            Span::raw(" : undo the last operation"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "U".blue(),
            Span::raw(" : redo the last undone operation"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "'".blue(),
            Span::raw(" : toggle the decimal separator"),
        ]),
        Line::from(vec![
            Span::raw("  "),
            "[Up]".blue(),
            Span::raw(" : edit S1"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Negative numbers can be entered as "),
            "_123".blue(),
            Span::raw(" or as "),
            "123-".blue(),
            Span::raw(" (no space between the digits and the sign)."),
        ]),
        Line::from(""),
        Line::from("Helix Calc supports numbers of arbitrary length, and uses ~ to indicate when a number is truncated."),
        Line::from("For instance, 1e100 will be represented as:"),
        Line::from(""),
        Line::from(vec![
            Span::raw("10000000000000000000"),
            "~101~".yellow(),
            Span::raw("0000000000000000000"),
        ]),
        Line::from(""),
        Line::from("Check out the code and report bugs at:"),
        Line::from("   https://github.com/chbug/hc"),
        Line::from(""),
        Line::from("The name is inspired by Helix Editor, and the functionality by the venerable GNU dc."),
    ];
    Text::from(lines)
}

impl Default for HelpState {
    fn default() -> Self {
        let help = help();
        let h = help.height();
        Self {
            content: help,
            visible: false,
            vs_state: ScrollbarState::default().content_length(h),
        }
    }
}

impl StatefulWidget for Help {
    type State = HelpState;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut HelpState) {
        if !state.visible {
            return;
        }
        let vertical = Layout::vertical([Constraint::Percentage(50)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(50)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        Clear.render(area, buf);

        Paragraph::new(state.content.clone())
            .block(
                Block::bordered()
                    .title("<Press Esc to close>")
                    .bg(Color::Black),
            )
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left)
            .scroll((state.vs_state.get_position() as u16, 0))
            .render(area, buf);
        Scrollbar::new(ScrollbarOrientation::VerticalRight).render(area, buf, &mut state.vs_state);
    }
}
