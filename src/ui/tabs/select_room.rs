use crate::network::Client;
use crate::ui::cursor::Cursor;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::*,
    widgets::*,
};

use crate::logger;
use std::error::Error;

use crate::state::APP;

#[derive(Default, Clone)]
pub struct SelectRoom {
    rooms: Vec<String>,
    selected_room: usize,
}

impl SelectRoom {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(
                block::Title::from(Span::styled(
                    "SwapBytes",
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Yellow),
                ))
                .alignment(Alignment::Left)
                .position(block::Position::Top),
            )
            .borders(Borders::ALL)
            .style(Style::default());

        frame.render_widget(block, area)
    }

    pub(crate) async fn handle_events(
        &mut self,
        key: KeyEvent,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
