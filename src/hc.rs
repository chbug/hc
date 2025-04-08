use bigdecimal::{BigDecimal, Zero};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Cell, Clear, Paragraph, Row, Table, Widget, Wrap},
};
use std::{cmp::min, collections::HashMap, str::FromStr};
use thiserror::Error;
use tui_textarea::TextArea;

use crate::{
    stack::{Op, Stack, StackError},
    state::State,
};

pub struct App<'a> {
    exit: bool,
    valid: bool,
    textarea: TextArea<'a>,
    stack: Stack,
    help: bool,
    ops: HashMap<char, Op>,
    op: Option<char>,
    op_status: Result<(), AppError>,
}

const HELP_MSG: &str = r#"
Helix Calc is a simple Reverse Polish Notation calculator.

List of all available operations:

   https://github.com/epthos/epthos

The name is inspired by Helix Editor, and the functionality by the venerable GNU dc.
"#;

#[derive(Error, Debug, PartialEq)]
enum AppError {
    #[error("Input is invalid")]
    InputError,
    #[error("{0}")]
    StackError(StackError),
}

impl App<'_> {
    pub fn new(state: State) -> anyhow::Result<Self> {
        Ok(App {
            exit: false,
            valid: true,
            textarea: TextArea::default(),
            stack: state.try_into()?,
            help: false,
            ops: HashMap::from([
                ('+', Op::Add),
                ('-', Op::Subtract),
                ('/', Op::Divide),
                ('*', Op::Multiply),
                ('%', Op::Modulo),
                ('^', Op::Pow),
                ('v', Op::Sqrt),
                ('d', Op::Duplicate),
                ('P', Op::Pop),
                ('k', Op::Precision),
                ('r', Op::Rotate),
            ]),
            op: None,
            op_status: Ok(()),
        })
    }

    pub fn run(&mut self, term: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        self.update_valid();
        while !self.exit {
            term.draw(|frame| {
                frame.render_widget(&*self, frame.area());
            })?;
            self.handle_events()?;
        }
        Ok(())
    }

    pub fn state(&self) -> State {
        (&self.stack).into()
    }

    pub fn add_extra(&mut self, extra: String) -> anyhow::Result<()> {
        for c in extra.chars() {
            self.handle_key(KeyCode::Char(c))?;
        }
        Ok(())
    }

    fn handle_key(&mut self, k: KeyCode) -> Result<(), AppError> {
        let empty = self.input_is_empty();
        match k {
            KeyCode::Char('q') => {
                self.exit = true;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.input_consume()?;
            }
            KeyCode::Char('-') if !empty => {
                let v = self.input_value()?;
                self.textarea = TextArea::from([format!("{}", -v)]);
            }
            KeyCode::Char(c) if self.ops.contains_key(&c) => {
                if !empty {
                    self.input_consume()?;
                }
                self.op = Some(c);
                self.stack
                    .apply(self.ops[&c].clone())
                    .map_err(|e| AppError::StackError(e))?;
            }
            _ => {
                let event = KeyEvent::new(k, KeyModifiers::empty());
                self.textarea.input(event);
            }
        }
        Ok(())
    }

    fn handle_events(&mut self) -> std::io::Result<()> {
        match crossterm::event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.op = None;
                self.op_status = Ok(());
                // Perform a few translations between what's allowed interactively and
                // what's only done from the command-line.
                let keycode = match key_event.code {
                    KeyCode::Char('m') if key_event.modifiers == KeyModifiers::CONTROL => {
                        KeyCode::Enter
                    }
                    KeyCode::Esc => KeyCode::Char('q'),
                    c => c,
                };
                match keycode {
                    KeyCode::Char('?') => {
                        self.help = !self.help;
                    }
                    c => {
                        self.op_status = self.handle_key(c);
                    }
                }
            }
            _ => {}
        };
        self.update_valid();
        Ok(())
    }

    fn update_valid(&mut self) {
        self.valid = self.input_is_empty() || self.input_value().is_ok();
        self.textarea.set_block(
            Block::bordered()
                .border_style(if self.valid { Color::White } else { Color::Red })
                .bg(Color::Black),
        );
    }

    fn input_is_empty(&self) -> bool {
        self.textarea.lines()[0].is_empty()
    }

    fn input_value(&self) -> Result<BigDecimal, AppError> {
        let mut s = self.textarea.lines()[0].clone();
        if s.starts_with("_") {
            s = format!("-{}", &s[1..]);
        }
        BigDecimal::from_str(&s).map_err(|_| AppError::InputError)
    }

    fn input_consume(&mut self) -> Result<(), AppError> {
        if self.input_is_empty() {
            return Ok(());
        }
        let v = self.input_value()?;
        self.stack
            .apply(Op::Push(v))
            .map_err(|e| AppError::StackError(e))?;
        self.textarea = TextArea::default();
        Ok(())
    }

    fn render_instructions(&self) -> impl Widget {
        Line::from(vec![
            format!(" Helix Calc {} - ", env!("CARGO_PKG_VERSION")).into(),
            " Help ".into(),
            "<?> ".blue().bold(),
            " Quit ".into(),
            "<Q> ".blue().bold(),
        ])
        .centered()
        .bg(Color::Black)
    }

    fn render_stack(&self, area: &Rect) -> impl Widget {
        let margin = 5; // Size of the margin holding the stack index.
        let snapshot = self.stack.snapshot();
        let stack: Vec<Row<'_>> = (1..=area.height)
            .rev()
            .map(|index| {
                let stack_index = (index as usize) - 1;
                let [val, idx] = if stack_index < snapshot.len() {
                    [
                        Span::from(format_number(
                            &snapshot[stack_index],
                            area.width - (margin + 1),
                        )),
                        Span::from(format!("{}", index)).style(Color::White),
                    ]
                } else {
                    [Span::from(""), Span::from("")]
                };
                Row::new(vec![
                    Cell::from(val.bold().into_right_aligned_line()),
                    Cell::from(idx.into_right_aligned_line()),
                ])
            })
            .collect();
        Table::new(
            stack,
            [Constraint::Percentage(100), Constraint::Length(margin)],
        )
        .column_spacing(1)
        .bg(Color::Black)
    }

    fn render_status(&self) -> impl Widget {
        let status = match &self.op_status {
            Ok(_) => {
                if self.input_is_empty() {
                    if let Some(c) = self.op {
                        Line::from(format!("<{}>", c).blue().bold())
                    } else {
                        Line::from("")
                    }
                } else if self.input_value().is_ok() {
                    Line::from(vec!["<Enter>".bold().blue(), " to add to the stack".into()])
                } else {
                    Line::from("Input is not a valid number")
                }
            }
            Err(err) => {
                if let Some(c) = self.op {
                    Line::from(vec![
                        format!("<{}>", c).blue().bold(),
                        format!(": {}", err).into(),
                    ])
                } else {
                    Line::from(err.to_string())
                }
            }
        };
        Text::from(status).bg(Color::Black)
    }
}

impl Widget for &App<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [page] = Layout::horizontal([Constraint::Length(50)])
            .flex(Flex::Center)
            .areas(area);
        let [instructions_area, stack_area, input_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(100),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(page);

        self.render_instructions().render(instructions_area, buf);
        self.render_stack(&stack_area).render(stack_area, buf);
        self.textarea.render(input_area, buf);
        self.render_status().render(status_area, buf);

        if self.help {
            Help::default().render(area, buf);
        }
    }
}

#[derive(Default)]
struct Help {}

impl Widget for Help {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let vertical = Layout::vertical([Constraint::Percentage(50)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(50)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        Clear::default().render(area, buf);

        Paragraph::new(Text::from(HELP_MSG))
            .block(Block::bordered().title(" Help").bg(Color::Black))
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

fn format_number(n: &BigDecimal, width: u16) -> String {
    let repr = n.to_string();
    if repr.len() <= width as usize {
        return repr;
    }
    // We want to spend our "width budget" on a mix of areas
    // of the string, as we don't know what the user cares about.
    // An alternative would be scientific notation, but I'd rather
    // we introduce "display modes" for those.
    //
    // [SGN][MSB]/.<POW>./[LSB].[RES]
    let neg = if n < &BigDecimal::zero() { 1 } else { 0 };
    let dot = repr.find('.');
    let mut budget = width;
    budget -= neg as u16; // We need to insert the sign in the end.
    let mut parts = 2;
    let pow = if let Some(idx) = dot {
        parts += 1;
        budget -= 1; // we need to insert the dot in the end.
        repr[neg..idx].len()
    } else {
        repr[neg..].len()
    };
    let pow = format!("[~{}~]", pow);
    budget -= pow.len() as u16; // we need to insert the magnitude back.
    if budget < parts {
        // Don't have enough space to represent this :(
        return "?".into();
    }
    // We can now split the budget in "parts" and allocate the remainder to
    // [MSB] as it carries most of the information.
    let msb = (budget / parts + (budget % parts)) as usize;
    let lsb = (budget / parts) as usize;
    let mut result = vec![];
    result.push(&repr[..msb + neg]);
    result.push(&pow);
    if let Some(idx) = dot {
        result.push(&repr[idx - lsb..min(idx + lsb + 1, repr.len())]);
    } else {
        result.push(&repr[repr.len() - lsb..]);
    }

    result.join("")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn format_regular_number() {
        let n: BigDecimal = "12345".parse().unwrap();
        assert_eq!(format_number(&n, 10), "12345");
    }

    #[test]
    fn format_long_number() {
        let n: BigDecimal = "123456789098".parse().unwrap();
        assert_eq!(format_number(&n, 10), "12[~12~]98");
        assert_eq!(format_number(&n, 11), "123[~12~]98");
    }

    #[test]
    fn format_long_negative_number() {
        let n: BigDecimal = "-123456789098".parse().unwrap();
        assert_eq!(format_number(&n, 10), "-12[~12~]8");
        assert_eq!(format_number(&n, 9), "-1[~12~]8");
        // We need at least 9 characters for this...
        assert_eq!(format_number(&n, 8), "?");
    }

    #[test]
    fn format_long_decimal_number() {
        let n: BigDecimal = "1234567.89098".parse().unwrap();
        assert_eq!(format_number(&n, 10), "12[~7~]7.8");
        assert_eq!(format_number(&n, 9), "1[~7~]7.8");
    }

    #[test]
    fn format_dont_overflow_decimal() {
        let n: BigDecimal = "12345678909876543.21".parse().unwrap();
        assert_eq!(format_number(&n, 18), "12345[~17~]543.21");
    }

    #[test]
    fn format_long_negative_decimal_number() {
        let n: BigDecimal = "-1234567.89098".parse().unwrap();
        assert_eq!(format_number(&n, 10), "-1[~7~]7.8");
    }

    #[test]
    fn validate_display_of_long_numbers() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("10000000 100000000 *".into())?;

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 6));
        app.render(buf.area, &mut buf);

        let mut line = String::with_capacity(buf.area.width as usize);
        for x in 0..buf.area.width {
            let c = buf[(x, 1)].symbol();
            line.push_str(c);
        }
        assert_eq!(line, "1000[~16~]0000     1");
        Ok(())
    }
}
