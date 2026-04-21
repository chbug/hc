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
    widgets::{Block, Cell, Clear, Paragraph, Row, StatefulWidget, Table, Widget},
};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Copy)]
enum PendingReg {
    Load,
    Save,
}

const LOAD: char = 'l';
const SAVE: char = 's';

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
    pending_reg: Option<PendingReg>, // Waiting for register key after L/S.
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
                ('r', Op::Swap),
                ('u', Op::Undo),
                ('U', Op::Redo),
                ('c', Op::ClearStack),
                ('C', Op::ClearRegisters),
                ('y', Op::Permutation(true)),
                ('Y', Op::Permutation(false)),
            ]),
            op: None,
            op_status: Ok(()),
            pending_reg: None,
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
        if let Some(pending) = self.pending_reg {
            self.pending_reg = None;
            if let KeyCode::Char(c) = k.code {
                self.op = Some(match pending {
                    PendingReg::Load => LOAD,
                    PendingReg::Save => SAVE,
                });
                self.stack
                    .apply(match pending {
                        PendingReg::Load => Op::Load(c),
                        PendingReg::Save => Op::Save(c),
                    })
                    .map_err(AppError::StackError)?;
            }
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
            (KeyCode::Char(c), KeyModifiers::NONE) if self.ops.contains_key(&c) && empty => {
                self.op = Some(c);
                self.stack
                    .apply(self.ops[&c].clone())
                    .map_err(AppError::StackError)?;
            }
            (KeyCode::Char(LOAD), KeyModifiers::NONE) if empty => {
                self.pending_reg = Some(PendingReg::Load);
            }
            (KeyCode::Char(SAVE), KeyModifiers::NONE) if empty => {
                self.pending_reg = Some(PendingReg::Save);
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

    fn render_registers(&self, area: &Rect) -> impl Widget {
        let margin = 5; // same column layout as the stack
        let base = self.stack.output_base();
        // inner width after block borders (1 left + 1 right)
        let value_width = (area.width as u64).saturating_sub(margin as u64 + 1 + 2);
        let mut regs: Vec<(char, _)> = self
            .stack
            .registers()
            .iter()
            .map(|(&k, v)| (k, v.clone()))
            .collect();
        regs.sort_by_key(|(k, _)| *k);
        let rows: Vec<Row<'_>> = regs
            .into_iter()
            .map(|(key, val)| {
                Row::new(vec![
                    Cell::from(
                        format_number(&val, value_width, self.separator, base).right_aligned(),
                    ),
                    Cell::from(Line::raw(key.to_string()).right_aligned()),
                ])
            })
            .collect();
        Table::new(
            rows,
            [Constraint::Percentage(100), Constraint::Length(margin)],
        )
        .column_spacing(1)
        .block(Block::bordered().title_bottom(" Registers "))
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

    fn render_reg_prompt(&self, area: Rect, buf: &mut Buffer) {
        let msg = match self.pending_reg.unwrap() {
            PendingReg::Load => " Load from register: ",
            PendingReg::Save => " Save to register: ",
        };
        let popup_w = msg.len() as u16 + 2; // +2 for left/right borders
        let [v_center] = Layout::vertical([Constraint::Length(3)])
            .flex(Flex::Center)
            .areas(area);
        let [popup_area] = Layout::horizontal([Constraint::Length(popup_w)])
            .flex(Flex::Center)
            .areas(v_center);
        Clear.render(popup_area, buf);
        Paragraph::new(msg)
            .block(Block::bordered())
            .bg(Color::Black)
            .render(popup_area, buf);
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
        let num_regs = self.stack.registers().len();
        let reg_rows = num_regs.min(stack_area.height as usize / 2) as u16;
        if reg_rows > 0 {
            let [reg_area, remaining_stack] = Layout::vertical([
                Constraint::Length(reg_rows + 2), // +2 for block borders
                Constraint::Percentage(100),
            ])
            .areas(stack_area);
            self.render_registers(&reg_area).render(reg_area, buf);
            self.render_stack(&remaining_stack)
                .render(remaining_stack, buf);
        } else {
            self.render_stack(&stack_area).render(stack_area, buf);
        }
        InputWidget::default().render(input_area, buf, &mut self.input);
        self.render_status().render(status_op_area, buf);
        self.render_precision_base().render(status_info_area, buf);
        Help::default().render(area, buf, &mut self.help);

        if self.pending_reg.is_some() {
            self.render_reg_prompt(area, buf);
        }
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

    #[test]
    fn octal_prefix_not_consumed_as_op() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("0o17 ")?;
        // 0o17 == 15 decimal
        assert_eq!(render(app)?, "            15     1");
        Ok(())
    }

    #[test]
    fn op_requires_empty_input() -> anyhow::Result<()> {
        // "5+" should NOT push 5 and add; "5 +" should.
        let mut app = App::new(State::default())?;
        app.add_extra("3 5 +")?;
        assert_eq!(render(app)?, "             8     1");
        Ok(())
    }

    fn render(mut app: App) -> anyhow::Result<String> {
        render_row(&mut app, 7, 1)
    }

    // Render into a 20-wide buffer of the given height and return the text at the given row.
    fn render_row(app: &mut App, height: u16, row: u16) -> anyhow::Result<String> {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, height));
        app.render_all(buf.area, &mut buf);
        let mut line = String::with_capacity(20);
        for x in 0..20 {
            line.push_str(buf[(x, row)].symbol());
        }
        Ok(line)
    }

    #[test]
    fn register_box_borders_and_value() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("42 sx")?;
        // height=15 → stack_area=9 rows → reg_rows=1 → box at rows 1-3
        // value col=12, key col=5, spacing=1, borders=2
        assert_eq!(render_row(&mut app, 15, 1)?, "┌──────────────────┐");
        assert_eq!(render_row(&mut app, 15, 2)?, "│          42     x│");
        assert_eq!(render_row(&mut app, 15, 3)?, "└ Registers ───────┘");
        Ok(())
    }

    #[test]
    fn register_box_alphabetical_order() -> anyhow::Result<()> {
        let mut app = App::new(State::default())?;
        app.add_extra("2 sz 1 sa")?;
        // 'a' comes before 'z' regardless of insertion order
        assert_eq!(render_row(&mut app, 15, 2)?, "│           1     a│");
        assert_eq!(render_row(&mut app, 15, 3)?, "│           2     z│");
        Ok(())
    }

    #[test]
    fn register_box_height_capped_at_half_stack() -> anyhow::Result<()> {
        // height=9 → stack_area=3 rows → half=1 → at most 1 register row shown
        // even with 3 registers in 'a','b','c'
        let mut app = App::new(State::default())?;
        app.add_extra("1 sa 2 sb 3 sc")?;
        // Row 1: top border, row 2: only 'a' shown (cap=1), row 3: bottom border
        assert_eq!(render_row(&mut app, 9, 2)?, "│           1     a│");
        assert_eq!(render_row(&mut app, 9, 3)?, "└ Registers ───────┘");
        Ok(())
    }
}
