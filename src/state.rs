use crate::{
    logger,
    network::{Client, RequestType},
};
use lazy_static::lazy_static;
use libp2p::{gossipsub::IdentTopic, PeerId};
use std::{
    collections::HashMap,
    error::Error,
    io,
    sync::{Arc, Mutex},
};
use tokio::fs;

pub struct App {
    // Stores a map of {topic, [(type, message)]}
    pub messages: HashMap<String, Vec<(MessageType, String)>>,
    pub nickname: String,
    pub screen: Screen,
    pub quitting: bool,
    pub num_connected_peers: usize,
    pub topic: IdentTopic,
    pub peer_id: Option<PeerId>,
    pub nicknames: HashMap<PeerId, String>,
    pub rooms: Vec<String>,
    pub private_messages: Vec<(MessageType, String)>,
    pub connected_peer: Option<PeerId>,
    pub connected: bool,
    pub requested_file: Option<String>,
    pub requesting_file: bool,
}

#[derive(Clone, PartialEq)]
pub enum Screen {
    Login,
    Chat,
}

#[derive(Default, Clone)]
pub enum MessageType {
    #[default]
    Message,
    Info,
    Error,
    Help,
}

impl App {
    fn new() -> Self {
        Self {
            messages: HashMap::new(),
            nickname: String::new(),
            nicknames: HashMap::new(),
            screen: Screen::Login,
            quitting: false,
            num_connected_peers: usize::MIN,
            topic: IdentTopic::new(""),
            rooms: vec![],
            peer_id: None,
            private_messages: vec![],
            connected_peer: None,
            connected: false,
            requested_file: None,
            requesting_file: false,
        }
    }

    pub fn add_peer(&mut self) {
        if self.num_connected_peers == 0 {
            // If adding a peer after having zero, show message
            let topic_str = self.topic.clone().to_string();

            self.add_message(
                MessageType::Info,
                "Peer has connected".to_string(),
                Some(&topic_str),
            );
            self.num_connected_peers = 0;
        }
        self.num_connected_peers += 1;
    }

    pub fn remove_peer(&mut self, peer_id: PeerId) {
        if self.num_connected_peers == 0 {
            return;
        };

        let topic_str = self.topic.clone().to_string();

        if (self.num_connected_peers) == 1 && self.screen == Screen::Chat {
            // If in a private context, return to global chat
            if self.connected {
                self.connected = false;
                self.connected_peer = None;
                self.requesting_file = false;
                self.requested_file = None;
            }

            self.add_message(
                MessageType::Error,
                "No connected peers.".to_string(),
                Some(&topic_str),
            );
        }
        self.nicknames.remove_entry(&peer_id);

        // If the peer that expired is the one that we are DMing, then leave the chat.
        if self.connected && self.connected_peer.unwrap() == peer_id {
            self.connected = false;
            self.connected_peer = None;
            self.requested_file = None;
            self.requesting_file = false;
            self.add_message(
                MessageType::Error,
                "Connected peer has left the application.".to_string(),
                Some(&topic_str),
            );
        }

        self.num_connected_peers -= 1;
    }

    pub fn add_message(
        &mut self,
        message_type: MessageType,
        message: String,
        topic: Option<&String>,
    ) {
        if topic.is_some() {
            match self.messages.get_mut(topic.unwrap()) {
                Some(messages) => messages.push((message_type, message.clone())),
                None => {
                    logger::error!(
                        "Unable to push message for topic: {:?}",
                        self.topic.to_string()
                    );
                }
            };
        } else {
            self.private_messages.push((message_type, message.clone()));
        }
    }

    pub fn get_messages(&self) -> Vec<(MessageType, String)> {
        if self.connected {
            return self.private_messages.clone();
        }

        return match self.messages.get(&self.topic.to_string()) {
            Some(messages) => messages.clone(),
            None => {
                logger::error!(
                    "Unable to get messages for topic: {:?}",
                    self.topic.to_string()
                );
                Vec::new()
            }
        };
    }

    pub(crate) async fn add_room(
        &mut self,
        room: &String,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        // Adds a new room, sets the current topic to the new room, and adds join message

        // Add the room to the DHT
        if self.rooms.contains(&room.clone()) {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Room already exists",
            )));
        }
        self.rooms.push(room.clone());
        client.add_room(self.rooms.clone()).await?;

        // Get the client to subscribe to the new topic
        client.change_topic(room.clone()).await?;

        // Join the room
        self.join_room(room, client).await?;
        Ok(())
    }

    pub(crate) async fn join_room(
        &mut self,
        room: &String,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        // Subscribe to the new topic
        if self.topic.to_string() == *room {
            return Ok(());
        }
        self.topic = IdentTopic::new(room.clone());

        match self.messages.get(&room.clone()) {
            Some(_) => {}
            None => {
                // Insert into messages if doesn't exist
                self.messages.insert(room.clone(), Vec::new());
                self.add_message(
                    MessageType::Info,
                    format!("Logged in as: {}", self.nickname),
                    Some(&self.topic.clone().to_string()),
                );
                self.add_message(
                    MessageType::Info,
                    format!("Joined chat room: {room}"),
                    Some(&self.topic.clone().to_string()),
                );
                self.add_message(
                    MessageType::Help,
                    "Type \"/help\" to view all available commands".to_string(),
                    Some(&self.topic.clone().to_string()),
                );
            }
        };

        client.change_topic(room.clone()).await?;

        Ok(())
    }

    pub(crate) async fn fetch_rooms(
        &self,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        client.fetch_rooms().await?;
        Ok(())
    }

    pub(crate) async fn connect(
        &mut self,
        nickname: String,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        // Get peerId from nickname
        let peer_id = self.nicknames.iter().find_map(|(peer_id, nick)| {
            if nick == &nickname {
                Some(peer_id)
            } else {
                None
            }
        });

        // If the peer_id is not found, return an error
        if peer_id.is_none() {
            logger::error!(
                "Peer ID not found for connection with nickname {}",
                nickname
            );
            return Err(Box::new(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Peer ID not found for nickname: {}", nickname),
            )));
        }

        let peer_id = peer_id.unwrap();
        self.connected_peer = Some(*peer_id);
        client
            .send_request(*peer_id, RequestType::Join, None, None)
            .await?;
        let topic = self.topic.clone();
        self.add_message(
            MessageType::Info,
            format! {"Connection sent to {}", nickname},
            Some(&topic.to_string()),
        );
        Ok(())
    }

    pub(crate) async fn accept_request(
        &mut self,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        // Accepts any incoming request

        let topic = self.topic.clone();

        if self.connected_peer.is_some() {
            // Check that we have the file

            if self.requested_file.is_some() {
                if !self
                    .check_file_exists(self.requested_file.clone().unwrap())
                    .await
                {
                    // Unable to accept as file doesn't exist
                    self.add_message(
                        MessageType::Error,
                        "Unable to send file as file doesn't exist".to_string(),
                        None,
                    );
                    self.add_message(
                        MessageType::Error,
                        "Sending automatic reject message".to_string(),
                        None,
                    );
                    // Send reject message
                    client
                        .send_request(
                            self.connected_peer.unwrap(),
                            RequestType::Reject,
                            None,
                            None,
                        )
                        .await?;

                    self.requesting_file = false;
                    self.requested_file = None;

                    return Ok(());
                }
            }

            client
                .send_request(
                    self.connected_peer.unwrap(),
                    RequestType::Accept,
                    None,
                    None,
                )
                .await?;

            if self.requested_file.is_none() {
                self.join_private_dm();
            }
        } else {
            self.add_message(
                MessageType::Error,
                "Unable to accept request as there is no incoming request.".to_string(),
                Some(&topic.to_string()),
            );
        }

        Ok(())
    }

    pub(crate) async fn reject_request(
        &mut self,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        // Rejects any incoming request

        let topic = self.topic.clone();

        if self.connected_peer.is_some() {
            client
                .send_request(
                    self.connected_peer.unwrap(),
                    RequestType::Reject,
                    None,
                    None,
                )
                .await?;
            if self.requested_file.is_some() {
                self.requested_file = None;
            } else {
                self.connected_peer = None;
                self.connected = false;
            }
        } else {
            self.add_message(
                MessageType::Error,
                "Unable to reject request as there is no incoming request.".to_string(),
                Some(&topic.to_string()),
            )
        }
        Ok(())
    }

    pub(crate) fn join_private_dm(&mut self) {
        if self.connected_peer.is_none() {
            return;
        }
        self.connected = true;
        self.private_messages = Vec::new();

        // Set the private messages
        let peer_nickname = self.nicknames.get(&self.connected_peer.unwrap()).unwrap();

        logger::info!("Joining DM with {}", peer_nickname);

        self.add_message(
            MessageType::Info,
            format!("Joined private message with {}", peer_nickname),
            None,
        );
        self.add_message(
            MessageType::Info,
            format!("To leave the private message, type \"/leave\""),
            None,
        );
        self.add_message(
            MessageType::Help,
            format!("Type \"/help\" to view all available commands"),
            None,
        );
    }

    pub(crate) async fn leave_private_dm(
        &mut self,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        if self.connected_peer.is_none() {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No connected peer"),
            )));
        }

        // Send a leave request
        let peer_id = self.connected_peer.clone().unwrap();
        logger::info!("Leaving private DM with {}", peer_id);
        let _ = client
            .send_request(peer_id, RequestType::Leave, None, None)
            .await;
        self.connected = false;
        self.connected_peer = None;
        self.requested_file = None;
        self.requesting_file = false;
        Ok(())
    }

    pub(crate) async fn request_file(
        &mut self,
        filename: String,
        client: &mut Client,
    ) -> Result<(), Box<dyn Error + Send>> {
        if self.connected_peer.is_none() {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No connected peer"),
            )));
        }
        logger::info!("Sending file request for file: {}", filename.clone());
        let peer_id = self.connected_peer.clone().unwrap();
        let _ = client
            .send_request(
                peer_id,
                RequestType::FileRequest,
                None,
                Some(filename.clone()),
            )
            .await;

        self.add_message(
            MessageType::Info,
            format!("Requested file: {}", filename.clone()),
            None,
        );

        self.requesting_file = true;
        self.requested_file = Some(filename);

        Ok(())
    }

    async fn check_file_exists(&self, file_path: String) -> bool {
        match fs::metadata(file_path).await {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

lazy_static! {
    pub static ref APP: Arc<Mutex<App>> = Arc::new(Mutex::new(App::new()));
}
