use bigdecimal::BigDecimal;
use crossterm::event::Event;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Stylize},
    widgets::{Block, Paragraph, StatefulWidget, Widget},
};
use std::str::FromStr;
use thiserror::Error;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

#[derive(Error, Debug, PartialEq)]
pub enum InputError {
    #[error("Input is empty")]
    Empty,
    #[error("Input is invalid")]
    Invalid,
}

/// Number input widget. This is specialized for the handling of
/// helix calc numbers.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    input: Input,
    cursor: (u16, u16),
}

#[derive(Debug, Clone, Default)]
pub struct InputWidget {}

impl InputState {
    pub fn with_value(mut self, value: String) -> Self {
        self.input = self.input.with_value(value);
        self
    }

    pub fn reset(&mut self) {
        self.input.reset();
    }

    pub fn handle_event(&mut self, event: &Event) {
        self.input.handle_event(event);
    }

    pub fn value(&self) -> Result<BigDecimal, InputError> {
        let s = self.input.value();
        if s.is_empty() {
            return Err(InputError::Empty);
        }
        let mut s = s.to_owned();
        if s.starts_with("_") {
            s = format!("-{}", &s[1..]);
        }
        BigDecimal::from_str(&s).map_err(|_| InputError::Invalid)
    }

    pub fn is_empty(&self) -> bool {
        self.input.value().is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.is_empty() || self.value().is_ok()
    }

    pub fn cursor(&self) -> (u16, u16) {
        self.cursor
    }
}

impl StatefulWidget for InputWidget {
    type State = InputState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let width = area.width.max(3) - 3;
        let scroll = state.input.visual_scroll(width as usize);

        let input = Paragraph::new(state.input.value().to_owned())
            .block(
                Block::bordered()
                    .border_style(if state.is_valid() {
                        Color::White
                    } else {
                        Color::Red
                    })
                    .bg(Color::Black),
            )
            .scroll((0, scroll as u16));

        input.render(area, buf);

        let x = state.input.visual_cursor().max(scroll) - scroll + 1;
        state.cursor = (area.x + x as u16, area.y + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid() {
        let mut widget = InputState::default();
        // Since InputWidget::default() has empty input, is_valid() calls is_empty() || value().is_ok()
        // is_empty() is true, so is_valid() is true.
        assert!(widget.is_valid());
        assert_eq!(widget.value(), Err(InputError::Empty));

        widget = widget.with_value("123".to_string());
        assert!(widget.is_valid());
        assert_eq!(widget.value(), Ok(BigDecimal::from(123)));

        widget = widget.with_value("abc".to_string());
        assert!(!widget.is_valid());
        assert_eq!(widget.value(), Err(InputError::Invalid));
    }

    #[test]
    fn test_underscore_is_negative() {
        let widget = InputState::default().with_value("_123".to_string());
        assert!(widget.is_valid());
        assert_eq!(widget.value(), Ok(BigDecimal::from(-123)));
    }
}
