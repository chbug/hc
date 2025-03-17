use bigdecimal::BigDecimal;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Cell, Clear, Paragraph, Row, Table, Widget, Wrap},
    Frame,
};
use std::{collections::HashMap, str::FromStr};
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

Type numbers followed by <Enter> to push them on the stack.

Use the following commands to operate on the stack:

- +, -, *, / : perform the arithmetic operation on the top two values.
- % : compute the modulo of the second value divided by the first.
- ^ : raise the second value to the power of the first.
- P : pop the top value off the stack.
- d : duplicate the top value.
- v : compute the square root of the top value.
- k : pop the top value and use it to set the precision.
- r : swap the first two values.

The name is inspired by Helix Editor, and the functionality by the venerable GNU dc.
"#;

#[derive(Error, Debug, PartialEq)]
enum AppError {
    #[error("input is invalid")]
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
        self.valid = true;
        while !self.exit {
            term.draw(|frame| {
                self.render(frame);
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
            KeyCode::Char(c) if empty && self.ops.contains_key(&c) => {
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
        let txt = &self.textarea.lines()[0];
        self.valid = txt.is_empty() || BigDecimal::from_str(txt).is_ok();
        Ok(())
    }

    fn input_is_empty(&self) -> bool {
        self.textarea.lines()[0].is_empty()
    }

    fn input_value(&self) -> Result<BigDecimal, AppError> {
        BigDecimal::from_str(&self.textarea.lines()[0]).map_err(|_| AppError::InputError)
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

    fn instructions(&self) -> impl Widget {
        Line::from(vec![
            format!(" Helix Calc {} - ", env!("CARGO_PKG_VERSION")).into(),
            " Help ".into(),
            "< ? > ".blue().bold(),
            " Quit ".into(),
            "< Q > ".blue().bold(),
        ])
        .centered()
        .bg(Color::Black)
    }

    fn stack(&self, area: &Rect) -> impl Widget {
        let snapshot = self.stack.snapshot();
        let stack: Vec<Row<'_>> = (1..=area.height)
            .rev()
            .map(|index| {
                let stack_index = (index as usize) - 1;
                let [val, idx] = if stack_index < snapshot.len() {
                    [
                        Span::from(format!("{}", snapshot[stack_index])),
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
        Table::new(stack, [Constraint::Percentage(100), Constraint::Length(5)])
            .column_spacing(1)
            .bg(Color::Black)
    }

    fn render_input(&mut self, frame: &mut Frame, area: Rect) {
        self.textarea.set_block(
            Block::bordered()
                .border_style(if self.valid { Color::White } else { Color::Red })
                .bg(Color::Black),
        );
        frame.render_widget(&self.textarea, area);
    }

    fn status(&self) -> impl Widget {
        let status = match &self.op_status {
            Ok(_) => {
                if self.input_is_empty() {
                    if let Some(c) = self.op {
                        Line::from(format!("< {} >", c).blue().bold())
                    } else {
                        Line::from("")
                    }
                } else if self.input_value().is_ok() {
                    Line::from(vec![
                        "< Enter >".bold().blue(),
                        " or ".into(),
                        "< Space >".bold().blue(),
                        " to add to the stack".into(),
                    ])
                } else {
                    Line::from("Input is not a valid number")
                }
            }
            Err(err) => {
                if let Some(c) = self.op {
                    Line::from(vec![
                        format!("< {} >", c).blue().bold(),
                        format!(": {}", err).into(),
                    ])
                } else {
                    Line::from(err.to_string())
                }
            }
        };
        Text::from(status).bg(Color::Black)
    }

    fn render_help(&self, frame: &mut Frame) {
        let vertical = Layout::vertical([Constraint::Percentage(50)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(50)]).flex(Flex::Center);
        let [area] = vertical.areas(frame.area());
        let [area] = horizontal.areas(area);
        frame.render_widget(Clear, area);

        let help_txt = Paragraph::new(Text::from(HELP_MSG))
            .block(Block::bordered().title(" Help").bg(Color::Black))
            .wrap(Wrap { trim: false });
        frame.render_widget(help_txt, area);
    }

    fn render(&mut self, frame: &mut Frame) {
        let [page] = Layout::horizontal([Constraint::Length(50)])
            .flex(Flex::Center)
            .areas(frame.area());
        let [instructions_area, stack_area, input_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(100),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(page);

        frame.render_widget(self.instructions(), instructions_area);
        frame.render_widget(self.stack(&stack_area), stack_area);
        self.render_input(frame, input_area);
        frame.render_widget(self.status(), status_area);

        if self.help {
            self.render_help(frame);
        }
    }
}
