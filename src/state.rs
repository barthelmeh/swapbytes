use crate::{logger, network::Client};
use lazy_static::lazy_static;
use libp2p::{gossipsub::IdentTopic, PeerId};
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};

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
}

#[derive(Clone)]
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
        }
    }

    pub fn add_message(&mut self, message_type: MessageType, message: String) {
        match self.messages.get_mut(&self.topic.to_string()) {
            Some(messages) => messages.push((message_type, message.clone())),
            None => {
                logger::error!(
                    "Unable to push message for topic: {:?}",
                    self.topic.to_string()
                );
            }
        };
    }

    pub fn get_messages(&self) -> Vec<(MessageType, String)> {
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
        self.messages.insert(room.clone(), Vec::new());
        self.topic = IdentTopic::new(room.clone());
        self.rooms.push(room.clone());

        // Get the client to subscribe to the new topic
        client.change_topic(room.clone()).await?;

        self.add_message(
            MessageType::Info,
            format!("Logged in as: {}", self.nickname),
        );
        self.add_message(MessageType::Info, format!("Joined chat room: {room}"));
        self.add_message(MessageType::Message, "".to_string());
        Ok(())
    }
}

lazy_static! {
    pub static ref APP: Arc<Mutex<App>> = Arc::new(Mutex::new(App::new()));
}
