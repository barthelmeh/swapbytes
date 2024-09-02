use crate::network::Client;
use crate::network::RequestType;
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
    // (command, description)
    commands: Vec<(String, String)>,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            input: String::default(),
            vertical_scroll: usize::default(),
            vertical_scroll_state: ScrollbarState::default(),
            cursor: Cursor::default(),
            is_command: false,
            commands: vec![
                (
                    "/help".to_string(),
                    "View a list of all available commands".to_string(),
                ),
                (
                    "/create_room".to_string(),
                    "Create a new room and join it. (e.g. /create_room COSC401)".to_string(),
                ),
                (
                    "/list".to_string(),
                    "List all known users that have sent a message".to_string(),
                ),
                (
                    "/connect [nickname]".to_string(),
                    "Invite a peer to share files and chat privately.".to_string(),
                ),
                (
                    "/accept [nickname]".to_string(),
                    "Accept an invitation to connect from a peer".to_string(),
                ),
                (
                    "/reject [nickname]".to_string(),
                    "Reject an invitation to connect from a peer".to_string(),
                ),
                (
                    "/leave".to_string(),
                    "Leave a private messaging session".to_string(),
                ),
            ],
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
        let args: Vec<&str> = remove_first(self.input.as_str())
            .unwrap_or("")
            .split(" ")
            .collect();
        let cmd = if let Some(cmd) = args.get(0) {
            *cmd
        } else {
            self.handle_command_error(args.clone());
            self.input.clear();
            self.cursor.reset_cursor();
            return;
        };
        logger::info!("Handling command: {cmd}");

        // Handle all commands
        match cmd {
            "help" => {
                let mut app = APP.lock().unwrap();
                let topic_str = app.topic.clone().to_string();
                let topic = match app.connected {
                    true => None,
                    false => Some(&topic_str),
                };

                app.add_message(MessageType::Message, "".to_string(), topic);
                app.add_message(
                    MessageType::Help,
                    "All available commands:".to_string(),
                    topic,
                );
                app.add_message(MessageType::Message, "".to_string(), topic);
                for (cmd, description) in &self.commands {
                    app.add_message(MessageType::Help, format!("{cmd}: {description}"), topic);
                }
                drop(app);
            }
            "create_room" => {
                let mut app = APP.lock().unwrap();
                let topic_str = app.topic.clone().to_string();

                if args.len() < 2 || args[1].len() == 0 || app.connected {
                    drop(app);
                    self.handle_command_error(args.clone());
                } else {
                    let room = args[1];
                    let _ = match app.add_room(&room.to_string(), client).await {
                        Ok(_) => {}
                        Err(_) => {
                            app.add_message(
                                MessageType::Error,
                                format!("Unable to add room: {room}"),
                                Some(&topic_str),
                            );
                        }
                    };
                    drop(app);
                }
            }
            "list" => {
                let mut app = APP.lock().unwrap();
                if args.len() != 1 || app.connected {
                    drop(app);
                    self.handle_command_error(args.clone());
                } else {
                    let nicknames = app.nicknames.clone();
                    let topic_str = app.topic.clone().to_string();
                    let topic = match app.connected {
                        false => Some(&topic_str),
                        true => None,
                    };
                    app.add_message(MessageType::Info, "".to_string(), topic);
                    if nicknames.is_empty() {
                        app.add_message(
                            MessageType::Info,
                            "No users known. A user must first send a message to be known"
                                .to_string(),
                            topic,
                        );
                    } else {
                        app.add_message(MessageType::Info, "All known users:".to_string(), topic);
                        for nickname in nicknames.values() {
                            app.add_message(MessageType::Info, nickname.clone(), topic);
                        }
                    }
                    app.add_message(MessageType::Info, "".to_string(), topic);
                    drop(app);
                }
            }
            "connect" => {
                let mut app = APP.lock().unwrap();

                if args.len() < 2 || args[1].len() == 0 || app.connected {
                    drop(app);
                    self.handle_command_error(args.clone());
                } else {
                    let topic = app.topic.clone();
                    if app.connected_peer.is_some() {
                        let connected_peer_nickname = app.connected_peer.clone().unwrap();
                        app.add_message(MessageType::Error, format!("You already have a DM request from {}. Type \"/accept\" to accept or \"/reject\" to reject the request.", connected_peer_nickname), Some(&topic.to_string()));
                        drop(app);
                        return;
                    }

                    // Send a join message to the peer
                    let peer_nickname = args[1];
                    let _ = match app.connect(peer_nickname.to_string(), client).await {
                        Ok(_) => {}
                        Err(_) => {
                            app.add_message(
                                MessageType::Error,
                                format!("Unable to send connection message"),
                                Some(&topic.to_string()),
                            );
                        }
                    };
                    drop(app);
                }
            }
            "accept" => {
                let mut app = APP.lock().unwrap();

                if args.len() != 1 || app.connected {
                    drop(app);
                    self.handle_command_error(args.clone());
                } else {
                    let topic = app.topic.clone();
                    let _ = match app.accept_connection(client).await {
                        Ok(_) => {
                            logger::info!("Successfully sent accept request");
                        }
                        Err(_) => app.add_message(
                            MessageType::Error,
                            "Unable to accept incoming connection".to_string(),
                            Some(&topic.to_string()),
                        ),
                    };
                    drop(app);
                }
            }
            "reject" => {
                let mut app = APP.lock().unwrap();
                if args.len() != 1 || app.connected {
                    drop(app);
                    self.handle_command_error(args.clone());
                } else {
                    let topic = app.topic.clone();
                    let _ = match app.reject_connection(client).await {
                        Ok(_) => {
                            logger::info!("Successfully rejected connection request");
                        }
                        Err(_) => app.add_message(
                            MessageType::Error,
                            "Unable to reject incoming connection".to_string(),
                            Some(&topic.to_string()),
                        ),
                    };
                    drop(app);
                }
            }
            "leave" => {
                let mut app = APP.lock().unwrap();
                if args.len() != 1 || !app.connected {
                    drop(app);
                    self.handle_command_error(args.clone());
                } else {
                    // Leave
                    let _ = match app.leave_private_dm(client).await {
                        Ok(_) => {
                            logger::info!("Successfully left private message session");
                        }
                        Err(_) => app.add_message(
                            MessageType::Error,
                            "Unable to leave private message session".to_string(),
                            None,
                        ),
                    };
                    drop(app);
                }
            }
            _ => {
                // Send error message back
                self.handle_command_error(args.clone());
            }
        }
        self.input.clear();
        self.cursor.reset_cursor();
    }

    fn handle_command_error(&self, args: Vec<&str>) {
        // Send error message back
        let mut app = APP.lock().unwrap();
        let topic_str = app.topic.clone().to_string();

        if app.connected {
            app.add_message(
                MessageType::Error,
                format!("Unable to perform command: \"{}\"", args.join(" ")),
                None,
            );
        } else {
            app.add_message(
                MessageType::Error,
                format!("Unable to perform command: \"{}\"", args.join(" ")),
                Some(&topic_str),
            );
        }

        drop(app);
    }

    // TODO: When submitting a message, check if it goes off the screen and start to scroll.
    async fn submit_message(&mut self, client: &mut Client) -> Result<(), Box<dyn Error + Send>> {
        // When we push a message we want to include our nickname, so add it manually.
        let mut app = APP.lock().unwrap();
        let nickname = app.nickname.clone();
        let topic = app.topic.clone();
        let message = self.input.clone();
        let nickname_message = format!("{}: {}", nickname, self.input.clone());

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

fn remove_first(s: &str) -> Option<&str> {
    s.chars().next().map(|c| &s[c.len_utf8()..])
}
