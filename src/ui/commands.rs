use crate::logger;
use crate::network::Client;
use crate::state::MessageType;
use crate::APP;

pub struct Commands {
    pub commands: Vec<Command>,
}

impl Default for Commands {
    fn default() -> Self {
        let mut commands = Vec::new();

        // Add all commands here
        commands.push(Command {
            command: "/help".to_string(),
            description: "View a list of all available commands".to_string(),
        });
        commands.push(Command {
            command: "/list".to_string(),
            description: "List all known users that have sent a message".to_string(),
        });
        commands.push(Command {
            command: "/create_room [room]".to_string(),
            description: "Create a new room and join it. (e.g. /create_room COSC401)".to_string(),
        });
        commands.push(Command {
            command: "/connect [nickname]".to_string(),
            description: "Invite a peer to share files and chat privately.".to_string(),
        });
        commands.push(Command {
            command: "/request [file_path]".to_string(),
            description: "Request a file in a private messaging session".to_string(),
        });
        commands.push(Command {
            command: "/accept".to_string(),
            description: "Accept an incoming request (such as a file, or a connection)".to_string(),
        });
        commands.push(Command {
            command: "/reject".to_string(),
            description: "Accept an incoming request (such as a file, or a connection)".to_string(),
        });
        commands.push(Command {
            command: "/leave".to_string(),
            description: "Leave a private messaging session".to_string(),
        });
        Self { commands }
    }
}

impl Commands {
    pub async fn handle_command(&self, input: String, client: &mut Client) {
        let args: Vec<&str> = input.split(" ").collect();

        // Get the command from args
        let cmd = if let Some(cmd) = args.get(0) {
            *cmd
        } else {
            // No command found, handle error
            self.handle_command_error(args.clone());
            return;
        };

        // Handle commands based on the first argument
        match cmd {
            "/help" => self.handle_help().await,
            "/create_room" => self.handle_create_room(args, client).await,
            "/list" => self.handle_list(client).await,
            "/connect" => self.handle_connect(args, client).await,
            "/accept" => self.handle_accept(args, client).await,
            "/reject" => self.handle_reject(args, client).await,
            "/request" => self.handle_request(args, client).await,
            "/leave" => self.handle_leave(client).await,
            _ => {
                // Command not found, handle error
                self.handle_command_error(args);
            }
        }
    }

    async fn handle_help(&self) {
        let mut app = APP.lock().unwrap();
        let topic_str = app.topic.clone().to_string();
        let topic = match app.connected {
            true => None,
            false => Some(&topic_str),
        };

        let max_length = self
            .commands
            .iter()
            .map(|cmd| cmd.command.len())
            .max()
            .unwrap_or(0)
            + 1;

        for command in &self.commands {
            app.add_message(
                MessageType::Info,
                format!("{:<max_length$}{}", command.command, command.description),
                topic,
            );
        }
        drop(app);
    }

    async fn handle_create_room(&self, args: Vec<&str>, client: &mut Client) {
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

    async fn handle_list(&self, _client: &mut Client) {
        let mut app = APP.lock().unwrap();

        let nicknames = app.nicknames.clone();
        let topic_str = app.topic.clone().to_string();
        let topic = match app.connected {
            false => Some(&topic_str),
            true => None,
        };
        if nicknames.is_empty() {
            app.add_message(
                MessageType::Info,
                "No users known. A user must first send a message to be known".to_string(),
                topic,
            );
        } else {
            app.add_message(MessageType::Info, "All known users:".to_string(), topic);
            for nickname in nicknames.values() {
                app.add_message(MessageType::Info, nickname.clone(), topic);
            }

            drop(app);
        }
    }

    async fn handle_connect(&self, args: Vec<&str>, client: &mut Client) {
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

    async fn handle_accept(&self, args: Vec<&str>, client: &mut Client) {
        let mut app = APP.lock().unwrap();
        if args.len() != 1 {
            drop(app);
            self.handle_command_error(args.clone());
        } else {
            let topic_str = app.topic.clone().to_string();
            let topic = match app.connected {
                false => Some(&topic_str),
                true => None,
            };
            let _ = match app.accept_request(client).await {
                Ok(_) => {
                    logger::info!("Successfully sent accept request");
                }
                Err(_) => app.add_message(
                    MessageType::Error,
                    "Unable to accept incoming request".to_string(),
                    topic,
                ),
            };
            drop(app);
        }
    }

    async fn handle_reject(&self, args: Vec<&str>, client: &mut Client) {
        let mut app = APP.lock().unwrap();
        if args.len() != 1 {
            drop(app);
            self.handle_command_error(args.clone());
        } else {
            let topic = app.topic.clone();
            let _ = match app.reject_request(client).await {
                Ok(_) => {
                    logger::info!("Successfully rejected connection request");
                }
                Err(_) => app.add_message(
                    MessageType::Error,
                    "Unable to reject incoming request".to_string(),
                    Some(&topic.to_string()),
                ),
            };
            drop(app);
        }
    }

    async fn handle_request(&self, args: Vec<&str>, client: &mut Client) {
        // Set requesting file to true
        let mut app = APP.lock().unwrap();
        if args.len() < 2 || args[1].len() == 0 || !app.connected {
            drop(app);
            self.handle_command_error(args.clone());
        } else {
            let filename = args[1];
            let _ = match app.request_file(filename.to_string(), client).await {
                Ok(_) => {}
                Err(e) => {
                    app.add_message(
                        MessageType::Error,
                        "Unable to send file request.".to_string(),
                        None,
                    );
                    logger::error!("Error handling request: {:?}", e);
                }
            };
            drop(app);
        }
    }

    async fn handle_leave(&self, client: &mut Client) {
        let mut app = APP.lock().unwrap();
        if !app.connected {
            drop(app);
            self.handle_command_error(vec!["/leave"]);
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

    pub fn handle_command_error(&self, args: Vec<&str>) {
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
}

#[derive(Default)]
pub struct Command {
    pub command: String,
    pub description: String,
}
