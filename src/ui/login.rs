use crate::logger;
use crate::network::Client;
use crate::state::{Screen, APP};
use crate::ui::cursor::Cursor;

use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    prelude::*,
    widgets::*,
};
use std::cmp::Ordering;
use std::error::Error;

#[derive(Default)]
pub struct LoginScreen {
    input: String,
    cursor: Cursor,
}

impl LoginScreen {
    pub fn render(&mut self, frame: &mut Frame) {
        let vertical = Layout::vertical([
            Constraint::Length(8),
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(3),
        ]);
        let [logo_area, nickname_area, error_area, input_area] = vertical.areas(frame.area());

        // Determine the font based on the available width and height
        let font = if logo_area.width < 85 {
            "small"
        } else {
            "standard"
        };

        // Generate the title text as ASCII art
        let ascii_art = match text_to_ascii_art::to_art("SwapBytes".to_string(), font, 0, 0, 0) {
            Ok(art) => art,
            Err(_) => {
                return;
            }
        };

        let logo_paragraph = Paragraph::new(ascii_art)
            .block(Block::default())
            .alignment(Alignment::Center) // Center horizontally
            .style(Style::default().fg(Color::Yellow));

        frame.render_widget(logo_paragraph, logo_area);

        let nickname_paragraph = Paragraph::new("A p2p platform for file sharing")
            .block(Block::default())
            .style(Style::default().add_modifier(Modifier::ITALIC))
            .alignment(Alignment::Center); // Center the text below the logo

        // Render the nickname text below the logo
        frame.render_widget(nickname_paragraph, nickname_area);

        // Error message
        let app = APP.lock().unwrap();
        let error_message = match app.num_connected_peers.cmp(&1) {
            Ordering::Less => "No connected peers",
            _ => "",
        };
        drop(app);

        let error_paragraph = Paragraph::new(error_message)
            .block(Block::default())
            .style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Red))
            .alignment(Alignment::Center); // Center the error message

        frame.render_widget(error_paragraph, error_area);

        // Input box
        let input = Paragraph::new(self.input.as_str())
            .style(Style::default())
            .block(
                Block::bordered()
                    .title(Span::styled(
                        "Choose a nickname",
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Color::LightYellow),
                    ))
                    .title_alignment(Alignment::Left),
            );
        frame.render_widget(input, input_area);

        // Render the cursor
        #[allow(clippy::cast_possible_truncation)]
        frame.set_cursor_position(Position {
            // Draw the cursor at the current position in the input field.
            // This position is can be controlled via the left and right arrow key
            x: input_area.x + self.cursor.pos as u16 + 1,
            // Move one line down, from the border to the input line
            y: input_area.y + 1,
        });
    }

    pub(crate) async fn handle_events(
        &mut self,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error>> {
        if let Ok(true) = ratatui::crossterm::event::poll(std::time::Duration::from_millis(16)) {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        // User input
                        KeyCode::Char(c) => {
                            self.enter_char(c);
                        }
                        // Submit nickname
                        KeyCode::Enter => {
                            let app = APP.lock().unwrap();
                            if app.num_connected_peers <= 0 {
                                return Ok(());
                            }
                            drop(app);

                            let _ = match self.submit(client).await {
                                Ok(_) => {}
                                Err(e) => {
                                    logger::error!("Unhandled error: {:?}", e);
                                }
                            };
                        }
                        // Moving the cursor
                        KeyCode::Left => {
                            self.cursor.move_cursor_left(self.input.chars().count());
                        }
                        KeyCode::Right => {
                            self.cursor.move_cursor_right(self.input.chars().count());
                        }
                        // Deleting Characters
                        KeyCode::Backspace => {
                            self.delete_char();
                        }
                        // Closing the application
                        KeyCode::Esc => {
                            let mut app = APP.lock().unwrap();
                            app.quitting = true;
                            drop(app);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        if new_char != ' ' {
            self.input.insert(index, new_char);
            self.cursor.move_cursor_right(self.input.chars().count());
        }
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.cursor.pos != 0;
        if is_not_cursor_leftmost {
            // Using remove on string is on bytes not chars

            let current_index = self.cursor.pos;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.cursor.move_cursor_left(self.input.chars().count());
        }
    }

    // Submit the nickname
    async fn submit(&mut self, client: &mut Client) -> Result<(), Box<dyn Error + Send>> {
        let mut app = APP.lock().unwrap();

        let nickname = self.input.clone();
        let peer_id = app.peer_id.clone();

        app.nickname = nickname.clone();
        app.screen = Screen::Chat;

        drop(app);

        // Add the nickname to the network
        if let Some(peer_id) = peer_id {
            client.add_nickname(nickname, peer_id).await?;

            // Connect app to global
            let mut app = APP.lock().unwrap();
            app.rooms.push("Global".to_string());
            app.join_room(&"Global".to_string(), client).await?;
            drop(app);

            // Fetch all rooms
            client.fetch_rooms().await?;
        }

        self.input.clear();
        Ok(())
    }

    // Get the byte index as each character in a string can contain multiple bytes
    fn byte_index(&mut self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor.pos)
            .unwrap_or(self.input.len())
    }
}
