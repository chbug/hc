use bigdecimal::num_bigint::BigInt;
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
        let s = s.to_owned();
        let (negative, s) = if s.starts_with('_') {
            (true, &s[1..])
        } else {
            (false, s.as_str())
        };
        let result = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            parse_radix_int(hex, 16)
        } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
            parse_radix_int(bin, 2)
        } else if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
            parse_radix_int(oct, 8)
        } else {
            BigDecimal::from_str(s).map_err(|_| InputError::Invalid)
        }?;
        Ok(if negative { -result } else { result })
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

fn parse_radix_int(digits: &str, radix: u32) -> Result<BigDecimal, InputError> {
    if digits.is_empty() {
        return Err(InputError::Invalid);
    }
    let n = BigInt::parse_bytes(digits.as_bytes(), radix).ok_or(InputError::Invalid)?;
    Ok(BigDecimal::from(n))
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

    #[test]
    fn test_hex_prefix() {
        let widget = InputState::default().with_value("0xff".to_string());
        assert!(widget.is_valid());
        assert_eq!(widget.value(), Ok(BigDecimal::from(255)));
    }

    #[test]
    fn test_binary_prefix() {
        let widget = InputState::default().with_value("0b1010".to_string());
        assert!(widget.is_valid());
        assert_eq!(widget.value(), Ok(BigDecimal::from(10)));
    }

    #[test]
    fn test_octal_prefix() {
        let widget = InputState::default().with_value("0o17".to_string());
        assert!(widget.is_valid());
        assert_eq!(widget.value(), Ok(BigDecimal::from(15)));
    }

    #[test]
    fn test_negative_hex() {
        let widget = InputState::default().with_value("_0xff".to_string());
        assert!(widget.is_valid());
        assert_eq!(widget.value(), Ok(BigDecimal::from(-255)));
    }

    #[test]
    fn test_incomplete_prefix_is_invalid() {
        let widget = InputState::default().with_value("0x".to_string());
        assert!(!widget.is_valid());
    }
}
