use crate::{
    stack::{Op, Stack, StackError},
    state::State,
};
use bigdecimal::{BigDecimal, Zero};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        StatefulWidget, Table, Widget, Wrap,
    },
};
use std::{cmp::min, collections::HashMap, str::FromStr};
use thiserror::Error;
use tui_input::{backend::crossterm::EventHandler, Input};

/// Overall state of the app.
pub struct App {
    exit: bool,                      // If true, exit.
    input: Input,                    // The input widget.
    stack: Stack,                    // The stack of big numbers.
    help: Help,                      // The help widget and its display state.
    separator: bool,                 // If true, show decimal separator.
    ops: HashMap<char, Op>,          // The known operations on the stack.
    op: Option<char>,                // The latest operation.
    op_status: Result<(), AppError>, // The latest status.
}

#[derive(Error, Debug, PartialEq)]
enum AppError {
    #[error("Input is invalid")]
    InputError,
    #[error("{0}")]
    StackError(StackError),
}

impl App {
    pub fn new(state: State) -> anyhow::Result<Self> {
        Ok(App {
            exit: false,
            input: Input::default(),
            stack: state.try_into()?,
            help: Help::default(),
            separator: false,
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
                ('u', Op::Undo),
                ('U', Op::Redo),
            ]),
            op: None,
            op_status: Ok(()),
        })
    }

    /// The app's main loop.
    pub fn run(&mut self, term: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
        while !self.exit {
            term.draw(|frame| {
                if let Some(cursor) = self.render_all(frame.area(), frame.buffer_mut()) {
                    frame.set_cursor_position(cursor);
                }
            })?;
            self.handle_events()?;
        }
        Ok(())
    }

    pub fn state(&self) -> State {
        (&self.stack).into()
    }

    pub fn add_extra<S: AsRef<str>>(&mut self, extra: S) -> anyhow::Result<()> {
        for c in extra.as_ref().chars() {
            self.handle_key(KeyCode::Char(c))?;
        }
        Ok(())
    }

    fn handle_key(&mut self, k: KeyCode) -> Result<(), AppError> {
        if self.help.visible {
            self.help.handle_key(k);
            return Ok(());
        }
        let empty = self.input_is_empty();
        match k {
            KeyCode::Up => {
                // Edit the top entry if there is one and the editor is empty.
                if self.input_is_empty() {
                    if let Some(n) = self.stack.edit_top() {
                        self.input = self.input.clone().with_value(n.to_plain_string());
                    }
                }
            }
            KeyCode::Char('?') => {
                self.help.visible = true;
            }
            KeyCode::Char('q') => {
                self.exit = true;
            }
            KeyCode::Char('\'') => {
                self.separator = !self.separator;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.input_consume()?;
            }
            KeyCode::Char('-') if !empty => {
                if let Ok(v) = self.input_value() {
                    self.input = self.input.clone().with_value((-v).to_plain_string());
                } else {
                    let event = Event::Key(KeyEvent::new(k, KeyModifiers::empty()));
                    self.input.handle_event(&event);
                }
            }
            KeyCode::Char(c) if self.ops.contains_key(&c) => {
                if !empty {
                    self.input_consume()?;
                }
                self.op = Some(c);
                self.stack
                    .apply(self.ops[&c].clone())
                    .map_err(AppError::StackError)?;
            }
            k => {
                let event = Event::Key(KeyEvent::new(k, KeyModifiers::empty()));
                self.input.handle_event(&event);
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
                self.op_status = self.handle_key(keycode);
            }
            _ => {}
        };
        Ok(())
    }

    fn input_is_valid(&self) -> bool {
        self.input_is_empty() || self.input_value().is_ok()
    }

    fn input_is_empty(&self) -> bool {
        self.input.value().is_empty()
    }

    fn input_value(&self) -> Result<BigDecimal, AppError> {
        let mut s = self.input.value().to_owned();
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
            .map_err(AppError::StackError)?;
        self.input = Input::default();
        Ok(())
    }

    fn render_input(&self, area: &Rect) -> (Paragraph<'static>, (u16, u16)) {
        let width = area.width.max(3) - 3;
        let scroll = self.input.visual_scroll(width as usize);

        let input = Paragraph::new(self.input.value().to_owned())
            .block(
                Block::bordered()
                    .border_style(if self.input_is_valid() {
                        Color::White
                    } else {
                        Color::Red
                    })
                    .bg(Color::Black),
            )
            .scroll((0, scroll as u16));
        let x = self.input.visual_cursor().max(scroll) - scroll + 1;

        (input, (area.x + x as u16, area.y + 1))
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
                        format_number(
                            &snapshot[stack_index],
                            (area.width - (margin + 1)) as u64,
                            self.separator,
                        ),
                        Line::raw(format!("{}", index)).style(Color::White),
                    ]
                } else {
                    [Line::raw(""), Line::raw("")]
                };
                Row::new(vec![
                    Cell::from(val.right_aligned()),
                    Cell::from(idx.right_aligned()),
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
                        format!("Precision: {}", self.stack.precision())
                            .blue()
                            .into_right_aligned_line()
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
    fn render_all(&mut self, area: Rect, buf: &mut Buffer) -> Option<(u16, u16)> {
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
        let (w, cursor) = self.render_input(&input_area);
        w.render(input_area, buf);
        self.render_status().render(status_area, buf);
        self.help.render(area, buf);

        Some(cursor)
    }
}

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

struct Help {
    content: Text<'static>,
    visible: bool,
    vs_state: ScrollbarState,
}

impl Help {
    fn handle_key(&mut self, k: KeyCode) {
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
}

impl Default for Help {
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

impl Widget for &mut Help {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.visible {
            return;
        }
        let vertical = Layout::vertical([Constraint::Percentage(50)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(50)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        Clear.render(area, buf);

        Paragraph::new(self.content.clone())
            .block(
                Block::bordered()
                    .title("<Press Esc to close>")
                    .bg(Color::Black),
            )
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left)
            .scroll((self.vs_state.get_position() as u16, 0))
            .render(area, buf);
        Scrollbar::new(ScrollbarOrientation::VerticalRight).render(area, buf, &mut self.vs_state);
    }
}

fn add_separators(repr: &str) -> String {
    let (sign, rest) = if repr.starts_with('-') {
        ("-", &repr[1..])
    } else {
        ("", &repr[..])
    };
    let (digits, rest) = if let Some(idx) = rest.find('.') {
        (&rest[..idx], &rest[idx..])
    } else {
        (rest, "")
    };
    let mut result = String::new();
    let len = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(' ');
        }
        result.push(ch);
    }
    format!("{}{}{}", sign, result, rest)
}

fn format_number<'b>(n: &BigDecimal, width: u64, separator: bool) -> Line<'b> {
    let repr = n.normalized().to_plain_string();
    let total = repr.len() as u64;
    // Trivial case: the representation already fits the display.
    if total <= width {
        if !separator {
            return Line::raw(repr);
        }
        let separated_repr = add_separators(&repr);
        // It's probably still better to remove the separators than to switch to
        // extended representation if the size is a bit tight.
        if separated_repr.len() as u64 <= width {
            return Line::raw(separated_repr);
        }
        return Line::raw(repr);
    }
    // Simple case: we can truncate after the decimal place as we retain
    // the most important information. We still want to indicate truncation
    // though, as this is all with fixed precision.
    let digits_after_dot = if let Some(idx) = repr.find('.') {
        (total - idx as u64 - 1) as i64
    } else {
        0
    };
    let digits_to_dot = total as i64 - digits_after_dot;
    let digits_before_dot = digits_to_dot - if digits_after_dot > 0 { 1 } else { 0 };
    // Check that we can display the final ~ if we need to truncate.
    let extra_precision = width as i64 - digits_to_dot - 1;
    if digits_after_dot > 0 && extra_precision >= 0 {
        let result = vec![
            Span::from(String::from(
                &repr[..(digits_to_dot + extra_precision) as usize],
            )),
            Span::from(String::from("~")).yellow(),
        ];
        return Line::from(result);
    }

    // More complex case: we want to keep both magnitude information and
    // details about the number.
    //
    // We want to spend our "width budget" on a mix of areas
    // of the string, as we don't know what the user cares about.
    // An alternative would be scientific notation, but I'd rather
    // we introduce "display modes" for those.
    //
    // [SGN][MSB][~<POW>~][LSB].[RES]
    let abs_start = if n < &BigDecimal::zero() { 1 } else { 0 };

    let mut budget = width as i64; // remaining space to allocate.
    let mut parts = 2; // number of parts to insert from the original representation.

    budget -= abs_start; // We need to insert the sign in the end.
    if digits_after_dot > 0 {
        parts += 1; // we need to insert the decimal information.
        budget -= 1; // we need to insert the dot in the end.
    }
    let pow = format!("~{}~", digits_before_dot - abs_start);
    budget -= pow.len() as i64; // we need to insert the magnitude back.
    if budget < parts {
        // Don't have enough space to represent this :(
        return Line::from(Span::from(String::from("~")).red());
    }
    // We can now split the budget in "parts" and allocate the remainder to
    // [MSB] as it carries most of the information.
    let msb = (budget / parts + (budget % parts)) as usize;
    let lsb = (budget / parts) as usize;
    let result = vec![
        Span::from(String::from(&repr[..msb + abs_start as usize])),
        Span::from(String::from(&pow)).yellow(),
        Span::from(String::from(if digits_after_dot > 0 {
            &repr[digits_to_dot as usize - lsb - 1
                ..min(digits_to_dot as usize + lsb, total as usize)]
        } else {
            &repr[total as usize - lsb..]
        })),
    ];
    Line::from(result)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn format_regular_number() {
        let n: BigDecimal = "12345".parse().unwrap();
        assert_eq!(format_number(&n, 10, false).to_string(), "12345");
    }

    #[test]
    fn format_regular_number_with_separators() {
        let n: BigDecimal = "12345".parse().unwrap();
        assert_eq!(format_number(&n, 10, true).to_string(), "12 345");
    }

    #[test]
    fn negative_number_with_separators() {
        let n: BigDecimal = "-12345".parse().unwrap();
        assert_eq!(format_number(&n, 10, true).to_string(), "-12 345");
    }

    #[test]
    fn negative_number_with_separators_and_decimals() {
        let n: BigDecimal = "-12345.6789".parse().unwrap();
        assert_eq!(format_number(&n, 15, true).to_string(), "-12 345.6789");
    }

    #[test]
    fn drop_separators_under_pressure() {
        let n: BigDecimal = "123456789".parse().unwrap();
        assert_eq!(format_number(&n, 10, true).to_string(), "123456789");
    }

    #[test]
    fn format_long_number() {
        let n: BigDecimal = "123456789098".parse().unwrap();
        assert_eq!(format_number(&n, 10, false).to_string(), "123~12~098");
        assert_eq!(format_number(&n, 11, false).to_string(), "1234~12~098");
    }

    #[test]
    fn format_long_negative_number() {
        let n: BigDecimal = "-123456789098".parse().unwrap();
        assert_eq!(format_number(&n, 8, false).to_string(), "-12~12~8");
        assert_eq!(format_number(&n, 7, false).to_string(), "-1~12~8");
        // We need at least 7 characters for this...
        assert_eq!(format_number(&n, 6, false).to_string(), "~");
    }

    #[test]
    fn format_long_decimal_number() {
        let n: BigDecimal = "12345678.34567".parse().unwrap();
        assert_eq!(format_number(&n, 7, false).to_string(), "1~8~8.3");
    }

    #[test]
    fn format_dont_overflow_decimal() {
        let n: BigDecimal = "12345678909876543.21".parse().unwrap();
        assert_eq!(format_number(&n, 18, false).to_string(), "12345~17~6543.21");
    }

    #[test]
    fn format_long_negative_decimal_number() {
        let n: BigDecimal = "-12345678.34567".parse().unwrap();
        assert_eq!(format_number(&n, 8, false).to_string(), "-1~8~8.3");
    }

    #[test]
    fn truncate_decimal_part() {
        let n: BigDecimal = "0.123456789".parse().unwrap();
        assert_eq!(format_number(&n, 4, false).to_string(), "0.1~");
        let n: BigDecimal = "10.12345678".parse().unwrap();
        assert_eq!(format_number(&n, 4, false).to_string(), "10.~");
    }

    #[test]
    fn handle_negative_scale() {
        let n: BigDecimal = "100000000000".parse().unwrap();
        let n = n.normalized();
        assert_eq!(format_number(&n, 10, false).to_string(), "100~12~000");
    }

    #[test]
    fn validate_display_of_long_numbers() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("10000000 100000000 *")?;

        assert_eq!(render(app)?, "10000~16~00000     1");
        Ok(())
    }

    #[test]
    fn trim_unneeded_zeros() {
        let n: BigDecimal = "0.000100000".parse().unwrap();
        assert_eq!(format_number(&n, 10, false).to_string(), "0.0001");
        let n: BigDecimal = "1e100".parse().unwrap();
        assert_eq!(format_number(&n, 10, false).to_string(), "100~101~00");
    }

    #[test]
    fn normalize_scientific_numbers() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("1e100 ")?;

        assert_eq!(render(app)?, "10000~101~0000     1");
        Ok(())
    }

    fn render(mut app: App) -> anyhow::Result<String> {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 6));
        app.render_all(buf.area, &mut buf);

        let mut line = String::with_capacity(buf.area.width as usize);
        for x in 0..buf.area.width {
            let c = buf[(x, 1)].symbol();
            line.push_str(c);
        }
        Ok(line)
    }
}
