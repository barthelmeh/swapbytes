use crate::network::Client;
use crate::network::RequestType;
use crate::ui::commands::Commands;
use crate::ui::cursor::Cursor;

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::*,
    widgets::*,
};

use crate::logger;
use std::error::Error;

use crate::state::{MessageType, APP};

pub struct Room {
    pub input: String,
    pub vertical_scroll: usize,
    pub vertical_scroll_state: ScrollbarState,
    pub cursor: Cursor,
    is_command: bool,
    command_handler: Commands,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            input: String::default(),
            vertical_scroll: usize::default(),
            vertical_scroll_state: ScrollbarState::default(),
            cursor: Cursor::default(),
            is_command: false,
            command_handler: Commands::default(),
        }
    }
}

impl Room {
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let vertical = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]);
        let [messages_area, input_area] = vertical.areas(area);

        // RENDER MESSAGES
        let app = APP.lock().unwrap();
        let chat_messages = app.get_messages();
        drop(app);

        let mut lines = vec![];

        // Add the chat messages
        for (message_type, message) in &chat_messages {
            lines.push(self.get_styled_line(message_type.clone(), message.clone()));
        }

        let messages_content = Text::from(lines);

        let messages = Paragraph::new(messages_content)
            .block(
                Block::bordered()
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
                            "Scroll using ↑ and ↓",
                            Style::default()
                                .add_modifier(Modifier::ITALIC)
                                .fg(Color::LightYellow),
                        ))
                        .alignment(Alignment::Right)
                        .position(block::Position::Bottom),
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
        let input_style = match self.is_command {
            true => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            false => Style::default().fg(Color::Yellow),
        };

        let input = Paragraph::new(self.input.as_str())
            .style(input_style)
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

    pub fn get_styled_line(&self, message_type: MessageType, message: String) -> Line {
        match message_type {
            MessageType::Message => Line::from(Span::raw(message)),
            MessageType::Info => Line::from(Span::styled(
                message,
                Style::default()
                    .add_modifier(Modifier::ITALIC)
                    .fg(Color::Yellow),
            )),
            MessageType::Error => Line::from(Span::styled(
                message,
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Red),
            )),
            MessageType::Help => Line::from(Span::styled(
                message,
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::ITALIC)
                    .fg(Color::Cyan),
            )),
        }
    }

    pub(crate) async fn handle_events(
        &mut self,
        key: KeyEvent,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error>> {
        match key.code {
            // User input
            KeyCode::Char(c) => {
                self.enter_char(c);
            }
            // Submit messages
            KeyCode::Enter => {
                // Check if it is a command:
                if self.input.starts_with("/") {
                    self.is_command = false;
                    self.handle_commands(client).await;
                } else {
                    let _ = match self.submit_message(client).await {
                        Ok(_) => {}
                        Err(e) => {
                            logger::error!("Unhandled error: {:?}", e);
                        }
                    };
                }
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
            _ => {}
        }
        Ok(())
    }

    pub(crate) async fn handle_commands(&mut self, client: &mut Client) {
        // Handle command and clear input
        self.command_handler
            .handle_command(self.input.clone(), client)
            .await;
        self.input.clear();
        self.cursor.reset_cursor();
    }

    // TODO: When submitting a message, check if it goes off the screen and start to scroll.
    async fn submit_message(&mut self, client: &mut Client) -> Result<(), Box<dyn Error + Send>> {
        // When we push a message we want to include our nickname, so add it manually.
        let mut app = APP.lock().unwrap();
        let nickname = app.nickname.clone();
        let topic = app.topic.clone();
        let message = self.input.clone();
        let nickname_message = format!("{}: {}", nickname, self.input.clone());

        if app.num_connected_peers == 0 {
            return Ok(());
        }

        if app.connected {
            client
                .send_request(
                    app.connected_peer.unwrap(),
                    RequestType::Message,
                    Some(message.clone()),
                    None,
                )
                .await?;
            app.add_message(MessageType::Message, nickname_message, None);
        } else {
            client
                .publish_message(message.clone(), topic.clone())
                .await?;
            app.add_message(
                MessageType::Message,
                nickname_message,
                Some(&topic.to_string()),
            );
        }

        drop(app);

        self.input.clear();
        self.cursor.reset_cursor();

        Ok(())
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();

        // If the first character is a / then it should be a command
        if index == 0 && new_char == '/' {
            self.is_command = true
        }

        self.input.insert(index, new_char);
        self.cursor.move_cursor_right(self.input.chars().count());
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.cursor.pos != 0;
        if is_not_cursor_leftmost {
            // Using remove on string is on bytes not chars

            let current_index = self.cursor.pos;
            let from_left_to_current_index = current_index - 1;

            if current_index == 1 {
                self.is_command = false;
            }

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
