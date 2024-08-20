use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use futures::StreamExt;

use crate::logger;
use crate::state::APP;

use libp2p::{
    core::Multiaddr,
    gossipsub::{self, Topic},
    identity,
    kad::{self, store::MemoryStore, Mode},
    mdns,
    multiaddr::Protocol,
    noise,
    request_response::{self, OutboundRequestId, ProtocolSupport, ResponseChannel},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, PeerId,
};

use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map, HashMap, HashSet};
use std::error::Error;
use std::time::Duration;

#[derive(NetworkBehaviour)]
struct Behaviour {
    mdns: mdns::tokio::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
    gossipsub: gossipsub::Behaviour,
}

pub(crate) async fn new() -> Result<(Client, impl Stream<Item = Event>, EventLoop), Box<dyn Error>>
{
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
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Subscribe gossipsub to a topic
    let topic = gossipsub::IdentTopic::new("global");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    // Set the topic globally
    let mut app = APP.lock().unwrap();
    app.topic = topic;
    app.peer_id = Some(peer_id);
    drop(app);

    // Setup kademlia
    swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

    let (command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, event_receiver) = mpsc::channel(0);

    Ok((
        Client {
            sender: command_sender,
        },
        event_receiver,
        EventLoop::new(swarm, command_receiver, event_sender),
    ))
}

#[derive(Debug)]
enum Command {
    StartListening {
        addr: Multiaddr,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    SendTopicMessage {
        message: String,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    AddNickname {
        nickname: String,
        peer_id: PeerId,
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

    pub(crate) async fn publish_message(
        &mut self,
        message: String,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::SendTopicMessage { message, sender })
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
}

#[derive(Debug)]
pub(crate) enum Event {
    InboundRequest {
        request: String,
        channel: ResponseChannel<FileResponse>,
    },
}

pub(crate) struct EventLoop {
    swarm: Swarm<Behaviour>,
    command_receiver: mpsc::Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    stored_messages: HashMap<String, String>,
}

impl EventLoop {
    fn new(
        swarm: Swarm<Behaviour>,
        command_receiver: mpsc::Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            stored_messages: HashMap::new(),
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
                    app.num_connected_peers += 1;
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

                    // Remove nickname
                    let mut app = APP.lock().unwrap();
                    app.nicknames.remove_entry(&peer_id);

                    // Decrease count
                    app.num_connected_peers -= 1;
                    drop(app);
                }
            }
            // Message received
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: peer_id,
                message_id: _id,
                message,
            })) => {
                let mut app = APP.lock().unwrap();
                let nicknames = app.nicknames.clone();
                let message = String::from_utf8_lossy(&message.data);

                // if the nickname is in app.nicknames, then add it
                // else try and get nickname

                match nicknames.get(&peer_id) {
                    Some(nickname) => {
                        app.messages.push(format!("{nickname}: {}", message));
                        logger::info!("{nickname}: {}", message);
                    }
                    None => {
                        // Nickname not stored so request it
                        // Need to store the message and wait until kademlia request is fufilled
                        self.stored_messages
                            .insert(peer_id.to_base58(), message.to_string());
                        let key = kad::RecordKey::new(&peer_id.to_base58());
                        self.swarm.behaviour_mut().kademlia.get_record(key);
                        logger::info!("Getting nickname for {peer_id}");
                    }
                }

                drop(app);
            }

            // Kademlia
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(
                kad::Event::OutboundQueryProgressed { result, .. },
            )) => match result {
                kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
                    kad::PeerRecord {
                        record: kad::Record { key, value, .. },
                        ..
                    },
                ))) => match String::from_utf8(value) {
                    Ok(nickname) => {
                        // Get the stored message and place on app.messages
                        let peer_id_base58 = match key.as_ref() {
                            key_bytes => std::str::from_utf8(key_bytes)
                                .unwrap_or_default()
                                .to_string(),
                        };

                        if let Some(message) = self.stored_messages.remove(&peer_id_base58) {
                            let formatted_message = format!("{}: {}", nickname, message);

                            let mut app = APP.lock().unwrap();

                            app.messages.push(formatted_message.clone());
                            logger::info!("{}", formatted_message);

                            drop(app);
                        } else {
                            logger::error!("No message found for key: {}", peer_id_base58);
                        }
                    }
                    Err(e) => {
                        logger::error!("Error deserialising nickname: {:?}", e);
                    }
                },
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
            Command::SendTopicMessage { message, sender } => {
                let app = APP.lock().unwrap();
                let topic = app.topic.clone();
                drop(app);

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
                    key: kad::RecordKey::new(&peer_id.to_base58()),
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
        }
    }
}

// Simple file exchange protocol
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileRequest(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileResponse(Vec<u8>);
