use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use futures::StreamExt;
use libp2p::gossipsub::IdentTopic;
use tokio::io::AsyncReadExt;

use crate::logger;
use crate::state::{MessageType, APP};

use libp2p::{
    core::Multiaddr,
    gossipsub,
    kad::{self, store::MemoryStore, Mode},
    mdns, noise,
    request_response::{self, ProtocolSupport, ResponseChannel},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, PeerId,
};

use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::io::Error as StdError;
use std::io::ErrorKind;
use std::time::Duration;
use tokio::fs::File;

#[derive(NetworkBehaviour)]
struct Behaviour {
    mdns: mdns::tokio::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
    gossipsub: gossipsub::Behaviour,
    request_response: request_response::cbor::Behaviour<PrivateRequest, PrivateResponse>,
}

pub(crate) async fn new() -> Result<(Client, EventLoop), Box<dyn Error>> {
    let key = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = key.public().to_peer_id();

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(key)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            Ok(Behaviour {
                mdns: mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )?,
                kademlia: kad::Behaviour::new(
                    key.public().to_peer_id(),
                    MemoryStore::new(key.public().to_peer_id()),
                ),
                gossipsub: gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub::Config::default(),
                )?,
                request_response: request_response::cbor::Behaviour::new(
                    [(
                        StreamProtocol::new("/file-exchange/1"),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                ),
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60 * 60))) // 1 hour
        .build();

    let mut app = APP.lock().unwrap();
    app.peer_id = Some(peer_id);
    drop(app);

    // Setup kademlia
    swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

    let (command_sender, command_receiver) = mpsc::channel(0);

    Ok((
        Client {
            sender: command_sender,
        },
        EventLoop::new(swarm, command_receiver),
    ))
}

#[derive(Debug)]
enum Command {
    StartListening {
        addr: Multiaddr,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    ChangeTopic {
        topic: String,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    SendTopicMessage {
        message: String,
        topic: IdentTopic,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    AddNickname {
        nickname: String,
        peer_id: PeerId,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    AddRoom {
        rooms: Vec<String>,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    FetchRooms {
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    SendRequest {
        peer_id: PeerId,
        request_type: RequestType,
        message: Option<String>,
        filename: Option<String>,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
}

#[derive(Clone)]
pub(crate) struct Client {
    sender: mpsc::Sender<Command>,
}

impl Client {
    pub(crate) async fn start_listening(
        &mut self,
        addr: Multiaddr,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::StartListening { addr, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub(crate) async fn change_topic(
        &mut self,
        topic: String,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::ChangeTopic { topic, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub(crate) async fn publish_message(
        &mut self,
        message: String,
        topic: IdentTopic,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::SendTopicMessage {
                message,
                topic,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub(crate) async fn add_nickname(
        &mut self,
        nickname: String,
        peer_id: PeerId,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::AddNickname {
                nickname,
                peer_id,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub(crate) async fn add_room(
        &mut self,
        rooms: Vec<String>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::AddRoom { rooms, sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub(crate) async fn fetch_rooms(&mut self) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::FetchRooms { sender })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    pub(crate) async fn send_request(
        &mut self,
        peer_id: PeerId,
        request_type: RequestType,
        message: Option<String>,
        filename: Option<String>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::SendRequest {
                peer_id,
                request_type,
                message,
                filename,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }
}

pub(crate) struct EventLoop {
    swarm: Swarm<Behaviour>,
    command_receiver: mpsc::Receiver<Command>,
    stored_messages: HashMap<String, gossipsub::Message>,
    stored_private_messages: HashMap<String, PrivateRequest>,
}

impl EventLoop {
    fn new(swarm: Swarm<Behaviour>, command_receiver: mpsc::Receiver<Command>) -> Self {
        Self {
            swarm,
            command_receiver,
            stored_messages: HashMap::new(),
            stored_private_messages: HashMap::new(),
        }
    }

    pub(crate) async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => self.handle_event(event).await,
                command = self.command_receiver.next() => match command {
                    Some(c) => self.handle_command(c).await,
                    // Command channel closed, thus shutting down the network event loop.
                    None =>  return,
                },
            }
        }
    }

    async fn handle_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            // Node connected
            SwarmEvent::NewListenAddr { address, .. } => {
                logger::info!("Node connected to {address}")
            }
            // Peer discovered
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                for (peer_id, multiaddr) in list {
                    logger::info!("Peer discovered: {peer_id}");

                    // Add to gossipsub
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);

                    // Add to kademlia
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, multiaddr);

                    // Add peer
                    let mut app = APP.lock().unwrap();
                    app.add_peer();
                    drop(app);
                }
            }
            // Peer expired
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                for (peer_id, multiaddr) in list {
                    logger::info!("Peer expired: {peer_id}");

                    // Remove from gossipsub
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);

                    // Remove from kademlia
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .remove_address(&peer_id, &multiaddr);

                    // Decrease count
                    let mut app = APP.lock().unwrap();
                    app.remove_peer(peer_id);
                    drop(app);
                }
            }
            // Connection Closed
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                logger::info!("Peer expired: {peer_id}");

                // Remove from gossipsub
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .remove_explicit_peer(&peer_id);

                // Remove from kademlia
                self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);

                // Decrease count
                let mut app = APP.lock().unwrap();
                app.remove_peer(peer_id);
                drop(app);
            }
            // Message received
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: peer_id,
                message_id: _id,
                message,
            })) => {
                let mut app = APP.lock().unwrap();
                let nicknames = app.nicknames.clone();
                let topic = message.topic.clone();
                let message_str = String::from_utf8_lossy(&message.data);

                // if the nickname is in app.nicknames, then add it
                // else try and get nickname

                match nicknames.get(&peer_id) {
                    Some(nickname) => {
                        app.add_message(
                            MessageType::Message,
                            format!("{nickname}: {}", message_str),
                            Some(&topic.into_string()),
                        );
                        logger::info!("{nickname}: {}", message_str);
                    }
                    None => {
                        // Nickname not stored so request it
                        // Need to store the message and wait until kademlia request is fufilled
                        let key = kad::RecordKey::new(&peer_id.to_bytes());
                        let query_id = self.swarm.behaviour_mut().kademlia.get_record(key);

                        self.stored_messages
                            .insert(query_id.to_string(), message.clone());
                        logger::info!("Getting nickname for {peer_id}");
                    }
                }

                drop(app);
            }

            // Kademlia
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(
                kad::Event::OutboundQueryProgressed { result, id, .. },
            )) => match result {
                kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
                    kad::PeerRecord {
                        record: kad::Record { key, value, .. },
                        ..
                    },
                ))) =>
                // Determine if the key is for a room or a nickname
                {
                    let key_str = String::from_utf8_lossy(key.as_ref());

                    if key_str == "rooms" {
                        // Deserialize the value as a vector of strings for rooms
                        match serde_cbor::from_slice::<Vec<String>>(&value) {
                            Ok(rooms) => {
                                logger::info!("Retrieved rooms: {:?}", rooms);

                                // Add the rooms to the application state, handle as needed
                                let mut app = APP.lock().unwrap();
                                app.rooms = rooms;
                                drop(app);
                            }
                            Err(e) => {
                                logger::error!("Error deserializing rooms: {:?}", e);
                            }
                        }
                    } else {
                        // Handle nickname retrieval
                        match String::from_utf8(value) {
                            Ok(nickname) => {
                                match PeerId::from_bytes(key.as_ref()) {
                                    Ok(peer_id) => {
                                        let mut app = APP.lock().unwrap();
                                        app.nicknames.insert(peer_id, nickname.clone());
                                        drop(app);

                                        logger::info!("Inserted nickname for peer: {:?}", key_str);

                                        // If its a message
                                        if self.stored_messages.contains_key(&id.to_string()) {
                                            if let Some(message) =
                                                self.stored_messages.remove(&id.to_string())
                                            {
                                                let message_str =
                                                    String::from_utf8_lossy(&message.data);
                                                let formatted_message =
                                                    format!("{}: {}", nickname, message_str);
                                                logger::info!("{}", formatted_message);

                                                let mut app = APP.lock().unwrap();
                                                app.add_message(
                                                    MessageType::Message,
                                                    formatted_message,
                                                    Some(&message.topic.into_string()),
                                                );
                                                drop(app);
                                            }
                                        } else if self
                                            .stored_private_messages
                                            .contains_key(&id.to_string())
                                        {
                                            // If its a private message
                                            let request = self
                                                .stored_private_messages
                                                .get(&id.to_string())
                                                .unwrap();
                                            let _ = self
                                                .handle_private_request(request.clone(), peer_id);
                                        }
                                    }
                                    Err(_) => {
                                        logger::error!(
                                            "Unable to get peerId from bytes: {:?}",
                                            key_str
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                logger::info!("Unable to get value from DHT: {:?}", e);
                            }
                        }
                    }
                }
                kad::QueryResult::GetRecord(Ok(_)) => {}
                kad::QueryResult::GetRecord(Err(err)) => {
                    logger::error!("Failed to get record: {:?}", err);
                }
                kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                    logger::info!("Successfully put record for {:?}", key);
                }
                kad::QueryResult::PutRecord(Err(err)) => {
                    logger::error!("Failed to put record: {:?}", err);
                }
                _ => (),
            },

            // Private Messaging
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                request_response::Event::Message { message, peer },
            )) => match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    // If we already have a requested_file and we recieve a file request, just send a file response
                    let app = APP.lock().unwrap();
                    let requested_file = app.requested_file.clone();
                    let requesting_file = app.requesting_file;
                    drop(app);

                    let path = match requested_file.is_some() && requesting_file == false {
                        true => {
                            let mut app = APP.lock().unwrap();
                            app.requested_file = None;
                            drop(app);
                            Some(requested_file.unwrap())
                        }
                        false => {
                            self.handle_private_request(request.clone(), peer);
                            None
                        }
                    };

                    self.send_response(path.is_none(), path, channel);
                }
                request_response::Message::Response { response, .. } => {
                    logger::info!("Recieved response");
                    self.handle_private_response(response);
                }
            },

            unhandled => logger::error!("Unhandled event: {:?}", unhandled),
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::StartListening { addr, sender } => {
                let _ = match self.swarm.listen_on(addr) {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }
            Command::ChangeTopic { topic, sender } => {
                let topic = IdentTopic::new(topic.clone());
                let _ = match self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }
            Command::SendTopicMessage {
                message,
                topic,
                sender,
            } => {
                let _ = match self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic, message.as_bytes())
                {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }
            Command::AddNickname {
                nickname,
                peer_id,
                sender,
            } => {
                let record = kad::Record {
                    key: kad::RecordKey::new(&peer_id.to_bytes()),
                    value: nickname.clone().into_bytes(),
                    publisher: None,
                    expires: None,
                };
                logger::info!(
                    "Creating new record with peer_id: {:?} and nickname: {:?}",
                    peer_id.to_base58(),
                    nickname.clone()
                );
                let _ = match self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .put_record(record, kad::Quorum::One)
                {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }

            Command::AddRoom { rooms, sender } => {
                logger::info!("Adding rooms {:?}", rooms);

                let rooms_bytes = match serde_cbor::to_vec(&rooms) {
                    Ok(room_bytes) => room_bytes,
                    Err(e) => {
                        sender.send(Err(Box::new(e))).unwrap();
                        return;
                    }
                };
                let record = kad::Record {
                    key: kad::RecordKey::new(&"rooms".to_string()),
                    value: rooms_bytes,
                    publisher: None,
                    expires: None,
                };
                let _ = match self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .put_record(record, kad::Quorum::One)
                {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }

            Command::FetchRooms { sender } => {
                let key = kad::RecordKey::new(&"rooms");
                self.swarm.behaviour_mut().kademlia.get_record(key);
                let _ = sender.send(Ok(()));
            }

            Command::SendRequest {
                peer_id,
                request_type,
                message,
                filename,
                sender,
            } => {
                logger::info!(
                    "Sending request with information: {}, {}, {:?} to peer: {}",
                    message.clone().unwrap_or("no message".to_string()),
                    filename.clone().unwrap_or("no file".to_string()),
                    request_type,
                    peer_id.clone()
                );
                let request = PrivateRequest {
                    request_type,
                    message,
                    filename,
                };

                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer_id, request);

                let _ = sender.send(Ok(()));
            }
        }
    }

    fn handle_private_request(&mut self, request: PrivateRequest, peer: PeerId) {
        let mut app = APP.lock().unwrap();
        logger::info!(
            "Recieved request with type {:?}",
            request.request_type.clone()
        );
        match request.request_type {
            RequestType::Join => {
                // Show the user that someone wants to connect
                // Get the nickname and display a message to the user
                // If the user already has someone trying to connect, return
                match app.connected_peer {
                    Some(_) => {
                        drop(app);
                        return;
                    }
                    _ => {}
                };

                let nicknames = app.nicknames.clone();
                logger::info!("{:?}", nicknames);
                let nickname = match nicknames.get(&peer) {
                    Some(nickname) => nickname,
                    None => {
                        // Handle getting the nickname from kademlia
                        let key = kad::RecordKey::new(&peer.to_bytes());
                        let query_id = self.swarm.behaviour_mut().kademlia.get_record(key);

                        self.stored_private_messages
                            .insert(query_id.to_string(), request);
                        drop(app);
                        return;
                    }
                };
                let topic = app.topic.clone();

                app.connected_peer = Some(peer.clone());

                app.add_message(
                    MessageType::Info,
                    format!(
                        "{} has invited you to chat! Type \"/accept\" to accept or \"/reject\" to reject",
                        nickname
                    ),
                    Some(&topic.to_string()),
                )
            }
            RequestType::Accept => {
                if app.requesting_file {
                    // send another file request as it has been accepted
                    let connected_peer = app.connected_peer.clone().unwrap();
                    let filename = app.requested_file.clone();
                    let request = PrivateRequest {
                        request_type: RequestType::FileRequest,
                        message: None,
                        filename,
                    };
                    self.swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&connected_peer, request);
                } else {
                    app.join_private_dm();
                }
            }
            RequestType::Reject => {
                // Peer has rejected
                let nicknames = app.nicknames.clone();
                let default_nickname = "Peer".to_string();
                let nickname = nicknames
                    .get(&app.connected_peer.unwrap())
                    .unwrap_or(&default_nickname);

                let topic_str = app.topic.clone().to_string();
                let topic = match app.connected {
                    true => None,
                    false => Some(&topic_str),
                };

                if app.requesting_file {
                    app.requesting_file = false;
                    app.requested_file = None;
                } else {
                    app.connected_peer = None;
                }
                app.add_message(
                    MessageType::Info,
                    format!("{} has rejected the request.", nickname),
                    topic,
                )
            }
            RequestType::Message => {
                // Display the message to the user
                let peer_id = app.connected_peer.clone().unwrap();
                let nicknames = app.nicknames.clone();
                let nickname = nicknames.get(&peer_id).unwrap();
                app.add_message(
                    MessageType::Message,
                    format!(
                        "{}: {}",
                        nickname,
                        request.message.unwrap_or("".to_string())
                    ),
                    None,
                );
            }
            RequestType::FileRequest => {
                let peer_id = app.connected_peer.clone().unwrap();
                let nicknames = app.nicknames.clone();
                let nickname = nicknames.get(&peer_id).unwrap();
                let requested_file = request.filename.unwrap();

                app.add_message(
                    MessageType::Info,
                    format!("{} has requested the file: {}", nickname, requested_file),
                    None,
                );
                app.add_message(
                    MessageType::Info,
                    format!("Type \"/accept\" or \"/reject\" to accept or reject the request"),
                    None,
                );
                app.requested_file = Some(requested_file);
            }
            RequestType::Leave => {
                // Show a message saying that the user has left
                logger::info!("Recieved a Leave message");
                let peer_id = app.connected_peer.clone().unwrap();
                let nicknames = app.nicknames.clone();
                let nickname = nicknames.get(&peer_id).unwrap();
                let topic = app.topic.clone();
                app.add_message(
                    MessageType::Info,
                    format!(
                        "{} has left the chat. You have been moved back to {}",
                        nickname,
                        topic.to_string()
                    ),
                    Some(&topic.to_string()),
                );
                app.connected_peer = None;
                app.connected = false
            }
        };

        drop(app);
    }

    fn get_file(&self, file_path: String) -> Vec<u8> {
        match std::fs::read(file_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                logger::error!("Unable to read file: {:?}", e);
                let mut app = APP.lock().unwrap();
                app.add_message(MessageType::Error, "Unable to read file".to_string(), None);
                drop(app);
                return Vec::new();
            }
        }
    }

    fn send_response(
        &mut self,
        ack: bool,
        file_path: Option<String>,
        channel: ResponseChannel<PrivateResponse>,
    ) {
        let file_bytes = if file_path.is_some() {
            Some(self.get_file(file_path.unwrap()))
        } else {
            None
        };

        let response = PrivateResponse { ack, file_bytes };
        let _ = self
            .swarm
            .behaviour_mut()
            .request_response
            .send_response(channel, response);
    }

    fn handle_private_response(&mut self, response: PrivateResponse) {
        if response.ack {
            // Ignore it
            return;
        }
        // Otherwise, want to download the file.
        // If we recieve empty file bytes then there is an error
        let mut app = APP.lock().unwrap();
        let bytes = response.file_bytes;

        // If we aren't requesting a file then we do nothing
        logger::info!("Handling private response that is not an ack!");
        if !app.requesting_file {
            return;
        }

        if bytes.is_none() || bytes.clone().unwrap().is_empty() {
            app.add_message(
                MessageType::Error,
                "Unable to download file".to_string(),
                None,
            );
        } else {
            // Download the file
            let path = app.requested_file.clone().unwrap();
            let _ = std::fs::write(path.clone(), bytes.unwrap());
            app.requesting_file = false;
            app.requested_file = None;

            app.add_message(MessageType::Info, format!("Downloaded file: {path}"), None);
        }

        drop(app);
    }
}

// Structs for sending DMs and potentially files

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestType {
    Join,
    Accept,
    Reject,
    Message,
    FileRequest,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PrivateRequest {
    request_type: RequestType,
    message: Option<String>,
    filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateResponse {
    ack: bool,
    file_bytes: Option<Vec<u8>>,
}
