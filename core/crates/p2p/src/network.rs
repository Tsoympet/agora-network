use std::time::Duration;

use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode};
use libp2p::identity::Keypair;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{connection_limits, Multiaddr, PeerId, Swarm, SwarmBuilder};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use agora_types::NetworkFingerprint;

use crate::identity::load_or_create_identity;
use crate::messages::NetworkMessage;
use crate::topics::{blocks_topic, transactions_topic};
use crate::{NetworkConfig, P2pError};

/// Hard cap on gossip payloads before borsh decode (DoS guard).
pub const MAX_GOSSIP_MESSAGE_BYTES: usize = 1 << 20; // 1 MiB

#[derive(NetworkBehaviour)]
pub struct AgoraBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub connection_limits: connection_limits::Behaviour,
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
    commands: mpsc::Sender<Command>,
}

impl NetworkHandle {
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn dial(&self, addr: &str) -> Result<(), P2pError> {
        self.commands
            .try_send(Command::Dial(addr.to_string()))
            .map_err(|e| P2pError::Network(format!("command channel: {e}")))
    }

    pub fn publish_message(&self, message: NetworkMessage) -> Result<(), P2pError> {
        self.commands
            .try_send(Command::Publish(message))
            .map_err(|e| P2pError::Network(format!("command channel: {e}")))
    }

    pub fn shutdown(&self) {
        let _ = self.commands.try_send(Command::Shutdown);
    }
}

/// libp2p gossip node for Agora.
pub struct NetworkNode {
    swarm: Swarm<AgoraBehaviour>,
    event_tx: mpsc::Sender<NetworkEvent>,
    command_rx: mpsc::Receiver<Command>,
    fingerprint: NetworkFingerprint,
}

impl NetworkNode {
    pub fn build(
        config: &NetworkConfig,
    ) -> Result<(NetworkHandle, mpsc::Receiver<NetworkEvent>, Self), P2pError> {
        let id_keys = match &config.identity_path {
            Some(path) => load_or_create_identity(path)?,
            None => Keypair::generate_ed25519(),
        };
        let peer_id = id_keys.public().to_peer_id();
        let fingerprint = config.fingerprint.clone();

        // Cryptographic message ids (not std DefaultHasher).
        let message_id_fn = |message: &gossipsub::Message| {
            let digest = Sha256::digest(&message.data);
            gossipsub::MessageId::from(hex::encode(digest))
        };

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_millis(500))
            .validation_mode(ValidationMode::Strict)
            .max_transmit_size(MAX_GOSSIP_MESSAGE_BYTES)
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|e| P2pError::Gossip(e.to_string()))?;

        let mut gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub_config,
        )
        .map_err(|e| P2pError::Gossip(e.to_string()))?;

        gossipsub
            .subscribe(&transactions_topic(&fingerprint))
            .map_err(|e| P2pError::Gossip(e.to_string()))?;
        gossipsub
            .subscribe(&blocks_topic(&fingerprint))
            .map_err(|e| P2pError::Gossip(e.to_string()))?;

        let connection_limits = connection_limits::Behaviour::new(
            connection_limits::ConnectionLimits::default()
                .with_max_established(Some(config.max_peers))
                .with_max_established_incoming(Some(config.max_peers))
                .with_max_established_outgoing(Some(config.max_peers)),
        );

        let behaviour = AgoraBehaviour {
            gossipsub,
            connection_limits,
        };

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

        let (event_tx, event_rx) = mpsc::channel(config.event_channel_capacity.max(16));
        let (command_tx, command_rx) = mpsc::channel(config.command_channel_capacity.max(16));

        info!(
            %peer_id,
            listen = %config.listen_addr,
            fingerprint = %fingerprint.digest_hex(),
            max_peers = config.max_peers,
            "agora p2p node built"
        );

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
                fingerprint,
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
                self.publish(transactions_topic(&self.fingerprint), message.encode())
            }
            NetworkMessage::Block(_) | NetworkMessage::BlockAnnounce { .. } => {
                self.publish(blocks_topic(&self.fingerprint), message.encode())
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
                            let _ = self.event_tx.try_send(NetworkEvent::Listening(address));
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            info!(%peer_id, "peer connected");
                            let _ = self.event_tx.try_send(NetworkEvent::PeerConnected(peer_id));
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            info!(%peer_id, "peer disconnected");
                            let _ = self.event_tx.try_send(NetworkEvent::PeerDisconnected(peer_id));
                        }
                        SwarmEvent::Behaviour(AgoraBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message {
                                propagation_source,
                                message,
                                ..
                            },
                        )) => {
                            if message.data.len() > MAX_GOSSIP_MESSAGE_BYTES {
                                warn!(
                                    peer = %propagation_source,
                                    len = message.data.len(),
                                    "dropping oversized gossip payload"
                                );
                                continue;
                            }
                            match NetworkMessage::decode(&message.data) {
                                Ok(decoded) => {
                                    let topic = message.topic.to_string();
                                    let topic_ok = match &decoded {
                                        NetworkMessage::Transaction(_) => topic.contains("/txs/"),
                                        NetworkMessage::Block(_)
                                        | NetworkMessage::BlockAnnounce { .. } => {
                                            topic.contains("/blocks/")
                                        }
                                    };
                                    if !topic_ok {
                                        warn!(peer = %propagation_source, %topic, "topic/type mismatch");
                                        continue;
                                    }
                                    debug!(peer = %propagation_source, %topic, "gossip message");
                                    let _ = self.event_tx.try_send(NetworkEvent::Message {
                                        peer: propagation_source,
                                        topic,
                                        message: decoded,
                                    });
                                }
                                Err(err) => {
                                    warn!(error = %err, "failed to decode gossip payload")
                                }
                            }
                        }
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
