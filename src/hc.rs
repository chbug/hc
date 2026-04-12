use crate::format::format_number;
use crate::input::{InputError, InputState, InputWidget};
use crate::{
    help::{Help, HelpState},
    stack::{Op, Stack, StackError},
    state::State,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Text},
    widgets::{Cell, Row, StatefulWidget, Table, Widget},
};
use std::collections::HashMap;
use thiserror::Error;

/// Overall state of the app.
pub struct App {
    exit: bool,                      // If true, exit.
    input: InputState,               // The input widget.
    stack: Stack,                    // The stack of big numbers.
    help: HelpState,                 // The help widget and its display state.
    separator: bool,                 // If true, show decimal separator.
    ops: HashMap<char, Op>,          // The known operations on the stack.
    op: Option<char>,                // The latest operation.
    op_status: Result<(), AppError>, // The latest status.
}

#[derive(Error, Debug, PartialEq)]
enum AppError {
    #[error("{0}")]
    InputError(#[from] InputError),
    #[error("{0}")]
    StackError(#[from] StackError),
}

impl App {
    pub fn new(state: State) -> anyhow::Result<Self> {
        Ok(App {
            exit: false,
            input: InputState::default(),
            stack: state.try_into()?,
            help: HelpState::default(),
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
                ('o', Op::OutputBase),
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
            self.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))?;
        }
        Ok(())
    }

    fn handle_key(&mut self, k: KeyEvent) -> Result<(), AppError> {
        if self.help.is_visible() {
            self.help.handle_key(k);
            return Ok(());
        }
        let empty = self.input.is_empty();
        match (k.code, k.modifiers) {
            (KeyCode::Up, KeyModifiers::NONE) => {
                // Edit the top entry if there is one and the editor is empty.
                if self.input.is_empty() {
                    if let Some(n) = self.stack.edit_top() {
                        self.input = self.input.clone().with_value(n.to_plain_string());
                    }
                }
            }
            (KeyCode::Char('?'), KeyModifiers::NONE) => {
                self.help.set_visible(true);
            }
            (KeyCode::Char('q'), KeyModifiers::NONE) | (KeyCode::Esc, KeyModifiers::NONE) => {
                self.exit = true;
            }
            (KeyCode::Char('\''), KeyModifiers::NONE) => {
                self.separator = !self.separator;
            }
            (KeyCode::Enter, KeyModifiers::NONE)
            | (KeyCode::Char(' '), KeyModifiers::NONE)
            | (KeyCode::Char('m'), KeyModifiers::CONTROL) => {
                self.input_consume()?;
            }
            (KeyCode::Char('-'), KeyModifiers::NONE) if !empty => {
                if let Ok(v) = self.input.value() {
                    self.input = self.input.clone().with_value((-v).to_plain_string());
                } else {
                    let event = Event::Key(k);
                    self.input.handle_event(&event);
                }
            }
            (KeyCode::Char(c), KeyModifiers::NONE) if self.ops.contains_key(&c) => {
                if !empty {
                    self.input_consume()?;
                }
                self.op = Some(c);
                self.stack
                    .apply(self.ops[&c].clone())
                    .map_err(AppError::StackError)?;
            }
            _ => {
                let event = Event::Key(k);
                self.input.handle_event(&event);
            }
        }
        Ok(())
    }

    fn handle_events(&mut self) -> std::io::Result<()> {
        match crossterm::event::read()? {
            Event::Key(mut key_event) if key_event.kind == KeyEventKind::Press => {
                self.op = None;
                // crossterm is doing very inconsistent things with SHIFT between
                // letters and non-letters, for instance Shift-/ is '?' but is
                // reported Char('?') + SHIFT.
                //
                // As we don't really _care_ about SHIFT as a modifier, let's
                // filter it out altogether here.
                key_event.modifiers = key_event.modifiers.difference(KeyModifiers::SHIFT);
                self.op_status = self.handle_key(key_event);
            }
            _ => {}
        };
        Ok(())
    }

    fn input_consume(&mut self) -> Result<(), AppError> {
        if self.input.is_empty() {
            return Ok(());
        }
        let v = self.input.value()?;
        self.stack
            .apply(Op::Push(v))
            .map_err(AppError::StackError)?;
        self.input.reset();
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
        let base = self.stack.output_base();
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
                            base,
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
        let status = match (&self.op_status, self.op) {
            (Ok(_), Some(c)) => Line::from(format!("<{}>", c).blue().bold()),
            (Err(err), Some(c)) => Line::from(vec![
                format!("<{}>", c).blue().bold(),
                format!(": {}", err).into(),
            ]),
            (Err(err), None) => Line::from(err.to_string()),
            (Ok(_), None) => Line::raw(""),
        };
        Text::from(status).bg(Color::Black)
    }

    fn render_precision_base(&self) -> impl Widget {
        let base = self.stack.output_base();
        let sep = if self.separator { "on " } else { "off" };
        let label = format!(
            "Precision: {} | Base: {} | Separator: {}",
            self.stack.precision(),
            base,
            sep
        );
        Text::from(label.green().into_centered_line()).bg(Color::Black)
    }

    fn render_all(&mut self, area: Rect, buf: &mut Buffer) -> Option<(u16, u16)> {
        let [page] = Layout::horizontal([Constraint::Length(50)])
            .flex(Flex::Center)
            .areas(area);
        let [instructions_area, stack_area, input_area, status_op_area, status_info_area] =
            Layout::vertical([
                Constraint::Length(1),
                Constraint::Percentage(100),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(page);

        self.render_instructions().render(instructions_area, buf);
        self.render_stack(&stack_area).render(stack_area, buf);
        InputWidget::default().render(input_area, buf, &mut self.input);
        self.render_status().render(status_op_area, buf);
        self.render_precision_base().render(status_info_area, buf);
        Help::default().render(area, buf, &mut self.help);

        Some(self.input.cursor())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn validate_display_of_long_numbers() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("10000000 100000000 *")?;

        assert_eq!(render(app)?, "10000~16~00000     1");
        Ok(())
    }

    #[test]
    fn normalize_scientific_numbers() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("1e100 ")?;

        assert_eq!(render(app)?, "10000~101~0000     1");
        Ok(())
    }

    #[test]
    fn set_output_base() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("255 16 o")?;
        // stack shows "ff" right-aligned in the value column, then the index
        assert_eq!(render(app)?, "            ff     1");
        Ok(())
    }

    fn render(mut app: App) -> anyhow::Result<String> {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 7));
        app.render_all(buf.area, &mut buf);

        let mut line = String::with_capacity(buf.area.width as usize);
        for x in 0..buf.area.width {
            let c = buf[(x, 1)].symbol();
            line.push_str(c);
        }
        Ok(line)
    }
}
