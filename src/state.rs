use lazy_static::lazy_static;
use libp2p::{gossipsub::IdentTopic, PeerId};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct App {
    pub messages: Vec<String>,
    pub nickname: String,
    pub screen: Screen,
    pub quitting: bool,
    pub num_connected_peers: usize,
    pub topic: IdentTopic,
    pub peer_id: Option<PeerId>,
    pub nicknames: HashMap<PeerId, String>,
}

#[derive(Clone)]
pub enum Screen {
    Login,
    Chat,
}

impl App {
    fn new() -> Self {
        Self {
            messages: Vec::new(),
            nickname: String::new(),
            nicknames: HashMap::new(),
            screen: Screen::Login,
            quitting: false,
            num_connected_peers: usize::MIN,
            topic: IdentTopic::new(""),
            peer_id: None,
        }
    }
}

lazy_static! {
    pub static ref APP: Arc<Mutex<App>> = Arc::new(Mutex::new(App::new()));
}
