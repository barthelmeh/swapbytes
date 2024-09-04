use crate::{network::Client, ui::chat::SelectedTab};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::*,
    widgets::*,
};
use std::error::Error;

use crate::state::APP;

pub struct SelectRoom {
    list_state: ListState,
}

impl Default for SelectRoom {
    fn default() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self { list_state }
    }
}

impl SelectRoom {
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
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
            .title(
                block::Title::from(Span::styled(
                    "Change tabs using ↑ and ↓",
                    Style::default().fg(Color::Yellow),
                ))
                .alignment(Alignment::Left)
                .position(block::Position::Bottom),
            )
            .title(
                block::Title::from(Span::styled(
                    "Confirm change using <Enter>",
                    Style::default().fg(Color::Yellow),
                ))
                .alignment(Alignment::Right)
                .position(block::Position::Bottom),
            )
            .borders(Borders::ALL)
            .style(Style::default());

        let app = APP.lock().unwrap();
        let rooms = app.rooms.clone();
        drop(app);

        let layout = Layout::default()
            .constraints([Constraint::Percentage(100)].as_ref())
            .split(area);

        let items: Vec<ListItem> = rooms
            .iter()
            .map(|room| ListItem::new(room.as_str()))
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().fg(Color::Yellow))
            .highlight_symbol("-> ");

        frame.render_stateful_widget(list, layout[0], &mut self.list_state);
    }

    pub(crate) async fn handle_events(
        &mut self,
        key: KeyEvent,
        client: &mut Client,
        selected_tab: &mut SelectedTab,
    ) -> Result<(), Box<dyn Error>> {
        // If we are on this tab and there are no connected peers, navigate back to the chat tab
        let app = APP.lock().unwrap();
        if app.num_connected_peers == 0 {
            *selected_tab = SelectedTab::Chat;
        }
        drop(app);

        match key.code {
            // Changing room
            KeyCode::Up => {
                self.list_state.select_previous();
            }

            KeyCode::Down => {
                self.list_state.select_next();
            }
            // Confirm room change
            KeyCode::Enter => {
                let selected_room_index = match self.list_state.selected() {
                    Some(room) => room,
                    None => 0,
                };

                let mut app = APP.lock().unwrap();
                let rooms = app.rooms.clone();
                app.join_room(&rooms[selected_room_index], client)
                    .await
                    .unwrap();
                drop(app);

                // Move to the chat screen
                *selected_tab = SelectedTab::Chat;
            }
            _ => {}
        }

        Ok(())
    }
}
