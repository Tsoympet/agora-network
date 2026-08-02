use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as StdHash, Hasher};
use std::time::Duration;

use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode};
use libp2p::identity::Keypair;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm, SwarmBuilder};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::messages::NetworkMessage;
use crate::topics::{blocks_topic, transactions_topic};
use crate::{NetworkConfig, P2pError};

#[derive(NetworkBehaviour)]
pub struct AgoraBehaviour {
    pub gossipsub: gossipsub::Behaviour,
}

/// Events surfaced to the node runtime.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Listening(Multiaddr),
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    Message {
        peer: PeerId,
        topic: String,
        message: NetworkMessage,
    },
}

enum Command {
    Dial(String),
    Publish(NetworkMessage),
    Shutdown,
}

/// Cloneable handle for dialing / publishing while the swarm runs on a background task.
#[derive(Clone)]
pub struct NetworkHandle {
    peer_id: PeerId,
    commands: mpsc::UnboundedSender<Command>,
}

impl NetworkHandle {
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn dial(&self, addr: &str) -> Result<(), P2pError> {
        self.commands
            .send(Command::Dial(addr.to_string()))
            .map_err(|_| P2pError::Network("swarm task stopped".into()))
    }

    pub fn publish_message(&self, message: NetworkMessage) -> Result<(), P2pError> {
        self.commands
            .send(Command::Publish(message))
            .map_err(|_| P2pError::Network("swarm task stopped".into()))
    }

    pub fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

/// libp2p gossip node for Agora.
pub struct NetworkNode {
    swarm: Swarm<AgoraBehaviour>,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    command_rx: mpsc::UnboundedReceiver<Command>,
}

impl NetworkNode {
    pub fn build(
        config: &NetworkConfig,
    ) -> Result<(NetworkHandle, mpsc::UnboundedReceiver<NetworkEvent>, Self), P2pError> {
        let id_keys = Keypair::generate_ed25519();
        let peer_id = id_keys.public().to_peer_id();

        let message_id_fn = |message: &gossipsub::Message| {
            let mut hasher = DefaultHasher::new();
            message.data.hash(&mut hasher);
            gossipsub::MessageId::from(hasher.finish().to_string())
        };

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_millis(500))
            .validation_mode(ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|e| P2pError::Gossip(e.to_string()))?;

        let mut gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub_config,
        )
        .map_err(|e| P2pError::Gossip(e.to_string()))?;

        gossipsub
            .subscribe(&transactions_topic())
            .map_err(|e| P2pError::Gossip(e.to_string()))?;
        gossipsub
            .subscribe(&blocks_topic())
            .map_err(|e| P2pError::Gossip(e.to_string()))?;

        let behaviour = AgoraBehaviour { gossipsub };

        let mut swarm = SwarmBuilder::with_existing_identity(id_keys)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )
            .map_err(|e| P2pError::Network(e.to_string()))?
            .with_behaviour(|_| behaviour)
            .map_err(|e| P2pError::Network(e.to_string()))?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let listen: Multiaddr = config
            .listen_addr
            .parse()
            .map_err(|e: libp2p::multiaddr::Error| P2pError::InvalidMultiaddr(e.to_string()))?;
        swarm
            .listen_on(listen)
            .map_err(|e| P2pError::Network(e.to_string()))?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        info!(%peer_id, listen = %config.listen_addr, "agora p2p node built");

        let handle = NetworkHandle {
            peer_id,
            commands: command_tx,
        };

        Ok((
            handle,
            event_rx,
            Self {
                swarm,
                event_tx,
                command_rx,
            },
        ))
    }

    fn dial_multiaddr(&mut self, addr: &str) -> Result<(), P2pError> {
        let multiaddr: Multiaddr = addr
            .parse()
            .map_err(|e: libp2p::multiaddr::Error| P2pError::InvalidMultiaddr(e.to_string()))?;
        self.swarm
            .dial(multiaddr)
            .map_err(|e| P2pError::Network(e.to_string()))?;
        Ok(())
    }

    fn publish(&mut self, topic: IdentTopic, data: Vec<u8>) -> Result<(), P2pError> {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)
            .map_err(|e| P2pError::Gossip(e.to_string()))?;
        Ok(())
    }

    fn publish_message(&mut self, message: &NetworkMessage) -> Result<(), P2pError> {
        match message {
            NetworkMessage::Transaction(_) => {
                self.publish(transactions_topic(), message.encode())
            }
            NetworkMessage::Block(_) | NetworkMessage::BlockAnnounce { .. } => {
                self.publish(blocks_topic(), message.encode())
            }
        }
    }

    /// Drive the swarm until shutdown.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(Command::Dial(addr)) => {
                            if let Err(err) = self.dial_multiaddr(&addr) {
                                warn!(error = %err, "dial failed");
                            }
                        }
                        Some(Command::Publish(message)) => {
                            if let Err(err) = self.publish_message(&message) {
                                warn!(error = %err, "publish failed");
                            }
                        }
                        Some(Command::Shutdown) | None => break,
                    }
                }
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!(%address, "listening");
                            let _ = self.event_tx.send(NetworkEvent::Listening(address));
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            info!(%peer_id, "peer connected");
                            let _ = self.event_tx.send(NetworkEvent::PeerConnected(peer_id));
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            info!(%peer_id, "peer disconnected");
                            let _ = self.event_tx.send(NetworkEvent::PeerDisconnected(peer_id));
                        }
                        SwarmEvent::Behaviour(AgoraBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message {
                                propagation_source,
                                message,
                                ..
                            },
                        )) => match NetworkMessage::decode(&message.data) {
                            Ok(decoded) => {
                                let topic = message.topic.to_string();
                                debug!(peer = %propagation_source, %topic, "gossip message");
                                let _ = self.event_tx.send(NetworkEvent::Message {
                                    peer: propagation_source,
                                    topic,
                                    message: decoded,
                                });
                            }
                            Err(err) => warn!(error = %err, "failed to decode gossip payload"),
                        },
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Format a dialable address that embeds the peer id (`/ip4/.../tcp/.../p2p/<peer>`).
pub fn dial_addr(listen: &Multiaddr, peer_id: PeerId) -> Multiaddr {
    let mut addr = listen.clone();
    addr.push(libp2p::multiaddr::Protocol::P2p(peer_id));
    addr
}
