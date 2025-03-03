use bigdecimal::BigDecimal;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Cell, Clear, Paragraph, Row, Table, Widget, Wrap},
    Frame,
};
use std::{collections::HashMap, str::FromStr};
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
    op_status: Result<(), StackError>,
}

const HELP_MSG: &str = r#"
Helix Calc is a simple Reverse Polish Notation calculator.

Type numbers followed by <Enter> to push them on the stack.

Use the following commands to operate on the stack:

- +, -, *, / : perform the operation on the top two values.
- P : pop the top value off the stack.
- d : duplicate the top value.

The name is inspired by Helix Editor, and the functionality by the venerable GNU dc.
"#;

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
                ('d', Op::Duplicate),
                ('P', Op::Pop),
            ]),
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

    fn empty_input(&self) -> bool {
        self.textarea.lines()[0].is_empty()
    }

    fn handle_events(&mut self) -> std::io::Result<()> {
        let empty = self.empty_input();
        match crossterm::event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                // Keep TextArea as a single-line entry.
                if key_event.code == KeyCode::Char('m')
                    && key_event.modifiers == KeyModifiers::CONTROL
                {
                    return Ok(());
                }
                match key_event.code {
                    KeyCode::Char('q') => {
                        self.exit = true;
                    }
                    KeyCode::Enter => {
                        self.consume();
                    }
                    KeyCode::Char('?') => {
                        self.help = !self.help;
                    }
                    KeyCode::Char('-') if !empty => {
                        if let Some(v) = self.value() {
                            self.textarea = TextArea::from([format!("{}", -v)]);
                        }
                    }
                    KeyCode::Char(c) if empty && self.ops.contains_key(&c) => {
                        self.op_status = self.stack.apply(self.ops[&c].clone());
                    }
                    _ => {
                        self.textarea.input(key_event);
                    }
                }
            }
            _ => {}
        };
        let txt = &self.textarea.lines()[0];
        self.valid = txt.is_empty() || BigDecimal::from_str(txt).is_ok();
        Ok(())
    }

    fn value(&self) -> Option<BigDecimal> {
        BigDecimal::from_str(&self.textarea.lines()[0]).ok()
    }

    fn consume(&mut self) {
        if let Some(v) = self.value() {
            self.op_status = self.stack.apply(Op::Push(v));
            self.textarea = TextArea::default();
        }
    }

    fn instructions(&self) -> impl Widget {
        Line::from(vec![
            " Helix Calc - ".into(),
            " Help ".into(),
            "<?> ".blue().bold(),
            " Quit ".into(),
            "<Q> ".blue().bold(),
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
                if self.empty_input() {
                    "Ok".into()
                } else if self.value().is_some() {
                    "<Enter> to add to the stack".into()
                } else {
                    "Input is not a valid number".into()
                }
            }
            Err(err) => err.to_string(),
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
