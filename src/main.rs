use anyhow::Context;
use bigdecimal::{BigDecimal, Zero};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Flex, Layout},
    style::{Color, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Cell, Clear, Paragraph, Row, Table, Wrap},
    Frame,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Result, Write},
    str::FromStr,
};
use std::{env, path::PathBuf};
use tui_textarea::TextArea;

fn main() -> anyhow::Result<()> {
    let state = load_state().unwrap_or(State::default());
    let mut term = ratatui::init();
    let mut app = App::new(&state)?;
    let result = app.run(&mut term);
    ratatui::restore();
    result.context("UI failure")?;
    save_state(&app.state())?;
    Ok(())
}

struct App<'a> {
    exit: bool,
    valid: bool,
    textarea: TextArea<'a>,
    stack: VecDeque<BigDecimal>,
    help: bool,
}

const HELP_MSG: &str = r#"
Helix Calc is a simple Reverse Polish Notation calculator.

Type numbers followed by <Enter> to push them on the stack.

Use the following commands to operate on the stack:

- +, -, *, / : perform the operation on the top two values
- P : pop the top value off the stack.

The name is inspired by Helix Editor, and the functionality by the venerable GNU dc.
"#;

#[derive(Serialize, Deserialize, Default)]
struct State {
    stack: Vec<String>,
}

impl State {
    fn stack(&self) -> anyhow::Result<VecDeque<BigDecimal>> {
        let mut result = VecDeque::new();
        for v in &self.stack {
            result.push_back(BigDecimal::from_str(v)?);
        }
        Ok(result)
    }
}

impl<'a> App<'a> {
    pub fn new(state: &State) -> anyhow::Result<Self> {
        Ok(App {
            exit: false,
            valid: true,
            textarea: TextArea::default(),
            stack: state.stack()?,
            help: false,
        })
    }

    pub fn run(&mut self, term: &mut ratatui::DefaultTerminal) -> Result<()> {
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
        State {
            stack: self.stack.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn handle_events(&mut self) -> Result<()> {
        let empty = self.textarea.lines()[0].is_empty();
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
                    KeyCode::Char(c) if "+-/*".contains(c) => {
                        // We only accept operations with an empty buffer: this allows
                        // for negative signs mostly.
                        if self.stack.len() >= 2 && empty {
                            let b = self.stack.pop_front().unwrap();
                            let a = self.stack.pop_front().unwrap();
                            match c {
                                '+' => self.stack.push_front(a + b),
                                '-' => self.stack.push_front(a - b),
                                '*' => self.stack.push_front(a * b),
                                '/' => {
                                    if b != BigDecimal::zero() {
                                        self.stack.push_front(a / b);
                                    } else {
                                        self.stack.push_front(a);
                                        self.stack.push_front(b);
                                    }
                                }
                                _ => {}
                            }
                        } else if c == '-' && !empty {
                            if let Some(v) = self.value() {
                                self.textarea = TextArea::from([format!("{}", -v)]);
                            }
                        }
                    }
                    KeyCode::Char('P') => {
                        self.stack.pop_front();
                    }
                    KeyCode::Enter => {
                        self.consume();
                    }
                    KeyCode::Char('?') => {
                        self.help = !self.help;
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

    fn value(&mut self) -> Option<BigDecimal> {
        BigDecimal::from_str(&self.textarea.lines()[0]).ok()
    }

    fn consume(&mut self) {
        if let Some(v) = self.value() {
            self.stack.push_front(v);
            self.textarea = TextArea::default();
        }
    }

    fn render(&mut self, frame: &mut Frame) -> () {
        let page = Layout::horizontal([Constraint::Length(50)])
            .flex(Flex::Center)
            .split(frame.area());
        let layout = Layout::vertical([
            Constraint::Length(1),       // instructions
            Constraint::Percentage(100), // stack
            Constraint::Length(3),       // input
            Constraint::Length(1),       // status
        ])
        .split(page[0]);

        // instructions
        let instructions = Line::from(vec![
            " Helix Calc - ".into(),
            " Help ".into(),
            "<?> ".blue().bold(),
            " Quit ".into(),
            "<Q> ".blue().bold(),
        ])
        .centered();
        frame.render_widget(&instructions, layout[0]);

        // stack
        let stack: Vec<Row<'_>> = (1..=layout[1].height)
            .rev()
            .map(|index| {
                let idx = Cell::from(Text::from(
                    Span::from(format!("{}", index))
                        .italic()
                        .into_right_aligned_line(),
                ));
                let stack_index = (index as usize) - 1;
                let val = if stack_index < self.stack.len() {
                    format!("{}", self.stack[stack_index])
                } else {
                    "".into()
                };
                Row::new(vec![
                    Cell::from(Span::from(val).bold().into_right_aligned_line()),
                    idx,
                ])
            })
            .collect();
        let table = Table::new(stack, [Constraint::Percentage(100), Constraint::Length(5)])
            .column_spacing(1);
        frame.render_widget(&table, layout[1]);

        // input
        let block =
            Block::bordered().border_style(if self.valid { Color::White } else { Color::Red });
        self.textarea.set_block(block);
        frame.render_widget(&self.textarea, layout[2]);

        // status
        frame.render_widget(
            Text::from(format!("stack: {}", self.stack.len())),
            layout[3],
        );

        if self.help {
            let vertical = Layout::vertical([Constraint::Percentage(50)]).flex(Flex::Center);
            let horizontal = Layout::horizontal([Constraint::Percentage(50)]).flex(Flex::Center);
            let [area] = vertical.areas(frame.area());
            let [area] = horizontal.areas(area);
            frame.render_widget(Clear, area);

            let help_txt = Paragraph::new(Text::from(HELP_MSG))
                .block(Block::bordered().title(" Help"))
                .wrap(Wrap { trim: false });
            frame.render_widget(help_txt, area);
        }
    }
}

fn load_state() -> anyhow::Result<State> {
    let json = fs::read_to_string(config_file()?)?;
    let state: State = serde_json::from_str(&json)?;
    Ok(state)
}

fn save_state(state: &State) -> anyhow::Result<()> {
    let path = config_file()?;
    let prefix = path.parent().context("incorrect path")?;
    std::fs::create_dir_all(prefix)?;
    let mut output = File::create(path)?;
    output
        .write_all(serde_json::to_string(state)?.as_bytes())
        .context("failed to write")
}

#[cfg(windows)]
fn config_file() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(env::var("LOCALAPPDATA")?)
        .join("HelixCalc")
        .join("state.json"))
}

#[cfg(unix)]
fn config_file() -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(env::var("HOME")?)
        .join(".config")
        .join("helix-calc")
        .join("state.json"))
}
