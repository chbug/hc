use std::io::Result;

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    prelude::{Buffer, Rect},
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph},
    Frame,
};

fn main() -> Result<()> {
    let mut term = ratatui::init();
    let mut app = App::default();
    let result = app.run(&mut term);
    ratatui::restore();
    result
}

#[derive(Default)]
struct App {
    exit: bool,
}

impl App {
    pub fn run(&mut self, term: &mut ratatui::DefaultTerminal) -> Result<()> {
        while !self.exit {
            term.draw(|frame| {
                self.render(frame);
            })?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> Result<()> {
        match crossterm::event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                if key_event.code == KeyCode::Char('q') {
                    self.exit = true;
                }
            }
            _ => {}
        };
        Ok(())
    }

    fn render(&self, frame: &mut Frame) -> () {
        let instructions = Line::from(vec![
            " Helix Calc - ".into(),
            " Help ".into(),
            "<?> ".blue().bold(),
            " Quit ".into(),
            "<Q> ".blue().bold(),
        ]);
        let block = Block::bordered().title_bottom(instructions.left_aligned());

        let layout =
            Layout::vertical([Constraint::Percentage(100), Constraint::Min(3)]).split(frame.area());
        let counter_text =
            Text::from(vec![Line::from(vec!["Value: ".into()])]).alignment(Alignment::Right);

        let widget = Paragraph::new(counter_text).right_aligned().block(block);
        frame.render_widget(widget, layout[1]);
    }
}
