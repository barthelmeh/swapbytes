use crate::cursor::Cursor;
use crate::logger;
use crate::network::Client;
use crate::state::APP;

use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    prelude::*,
    widgets::*,
};
use std::error::Error;

#[derive(Default)]
pub struct ChatScreen {
    input: String,
    vertical_scroll: usize,
    vertical_scroll_state: ScrollbarState,
    cursor: Cursor,
}

impl ChatScreen {
    pub fn render(&mut self, frame: &mut Frame) {
        let vertical = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]);
        let [messages_area, input_area] = vertical.areas(frame.area());

        // RENDER MESSAGES
        let app = APP.lock().unwrap();
        let chat_messages = app.messages.clone();
        let topic = app.topic.clone().to_string();
        let nickname = app.nickname.clone();
        drop(app);

        let joined_room_message = Span::styled(
            format!("Joined chat room: {}", topic),
            Style::default()
                .add_modifier(Modifier::ITALIC)
                .fg(Color::LightYellow),
        );
        let mut lines = vec![];
        lines.push(Line::from(joined_room_message));
        lines.push(Line::from(Span::raw("\n")));
        lines.push(Line::from(Span::raw(chat_messages.join("\n"))));

        let messages_content = Text::from(lines);

        let messages = Paragraph::new(messages_content)
            .block(
                Block::bordered()
                    .title(
                        block::Title::from(Span::styled(
                            ("SwapBytes"),
                            Style::default()
                                .add_modifier(Modifier::BOLD)
                                .fg(Color::Yellow),
                        ))
                        .alignment(Alignment::Left)
                        .position(block::Position::Top),
                    )
                    .title(
                        block::Title::from(Span::styled(
                            "Scroll using ↑ and ↓",
                            Style::default()
                                .add_modifier(Modifier::ITALIC)
                                .fg(Color::LightYellow),
                        ))
                        .alignment(Alignment::Right),
                    ),
            )
            .scroll((self.vertical_scroll as u16, 0));

        frame.render_widget(messages, messages_area);

        self.vertical_scroll_state = self
            .vertical_scroll_state
            .content_length(chat_messages.len());
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            messages_area,
            &mut self.vertical_scroll_state,
        );

        // Render input box
        let input = Paragraph::new(self.input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::bordered()
                    .title(Span::styled(
                        "Type your message - Press <Enter> to send",
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Color::Yellow),
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

    pub async fn handle_events(&mut self, client: &mut Client) -> Result<(), Box<dyn Error>> {
        // Handle events for the chat screen
        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        // User input
                        KeyCode::Char(c) => {
                            self.enter_char(c);
                        }
                        // Submit messages
                        KeyCode::Enter => {
                            let _ = match self.submit_message(client).await {
                                Ok(_) => {}
                                Err(e) => {
                                    logger::error!("Unhandled error: {:?}", e);
                                }
                            };
                        }
                        // Scrolling
                        KeyCode::Up => {
                            self.vertical_scroll = self.vertical_scroll.saturating_sub(1);
                            self.vertical_scroll_state =
                                self.vertical_scroll_state.position(self.vertical_scroll);
                        }
                        KeyCode::Down => {
                            self.vertical_scroll = self.vertical_scroll.saturating_add(1);
                            self.vertical_scroll_state =
                                self.vertical_scroll_state.position(self.vertical_scroll);
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

    // TODO: When submitting a message, check if it goes off the screen and start to scroll.
    async fn submit_message(&mut self, client: &mut Client) -> Result<(), Box<dyn Error + Send>> {
        // When we push a message we want to include our nickname, so add it manually.
        let mut app = APP.lock().unwrap();
        let nickname = app.nickname.clone();

        let message = self.input.clone();
        let nickname_message = format!("{}: {}", nickname, self.input.clone());

        app.messages.push(nickname_message.clone());
        drop(app);

        // Only want to publish the message, not including the nickname
        client.publish_message(message.clone()).await?;

        self.input.clear();
        self.cursor.reset_cursor();
        Ok(())
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.cursor.move_cursor_right(self.input.chars().count());
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

    // Get the byte index as each character in a string can contain multiple bytes
    fn byte_index(&mut self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor.pos)
            .unwrap_or(self.input.len())
    }
}
