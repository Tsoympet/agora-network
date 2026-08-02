use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash as StdHash, Hasher};
use std::time::Duration;

use agora_types::{Block, Hash};
use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity};
use libp2p::identity::Keypair;
use libp2p::request_response::{self, ProtocolSupport, ResponseChannel};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm, SwarmBuilder};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::getblock::{getblock_protocol, GetBlockRequest, GetBlockResponse};
use crate::limits::connection_limits_behaviour;
use crate::messages::NetworkMessage;
use crate::scoring::{enable_peer_scoring, APP_SCORE_BAD_PEER, APP_SCORE_GOOD_PEER};
use crate::topics::{blocks_topic, transactions_topic};
use crate::{NetworkConfig, P2pError};

type GetBlockBehaviour = request_response::cbor::Behaviour<GetBlockRequest, GetBlockResponse>;

#[derive(NetworkBehaviour)]
pub struct AgoraBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub getblock: GetBlockBehaviour,
    pub connection_limits: libp2p::connection_limits::Behaviour,
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
    /// Inbound `/agora/getblock/1` request — respond via [`NetworkHandle::respond_get_block`].
    GetBlockRequest {
        peer: PeerId,
        hash: Hash,
        request_id: request_response::InboundRequestId,
    },
    /// Outbound getblock completed (block may be missing on the remote).
    GetBlockResponse {
        peer: PeerId,
        hash: Hash,
        block: Option<Block>,
    },
    /// Outbound getblock failed; caller may fall back to gossip `GetBlock`.
    GetBlockFailure {
        peer: PeerId,
        hash: Hash,
        error: String,
    },
}

enum Command {
    Dial(String),
    Publish(NetworkMessage),
    RequestBlock { peer: PeerId, hash: Hash },
    RespondGetBlock {
        request_id: request_response::InboundRequestId,
        block: Option<Block>,
    },
    SetAppScore { peer: PeerId, score: f64 },
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

    /// Request a full block body from a connected peer over request-response.
    pub fn request_block(&self, peer: PeerId, hash: Hash) -> Result<(), P2pError> {
        self.commands
            .send(Command::RequestBlock { peer, hash })
            .map_err(|_| P2pError::Network("swarm task stopped".into()))
    }

    /// Answer an inbound [`NetworkEvent::GetBlockRequest`].
    pub fn respond_get_block(
        &self,
        request_id: request_response::InboundRequestId,
        block: Option<Block>,
    ) -> Result<(), P2pError> {
        self.commands
            .send(Command::RespondGetBlock { request_id, block })
            .map_err(|_| P2pError::Network("swarm task stopped".into()))
    }

    /// Adjust gossipsub application-specific peer score (P5).
    pub fn set_application_score(&self, peer: PeerId, score: f64) -> Result<(), P2pError> {
        self.commands
            .send(Command::SetAppScore { peer, score })
            .map_err(|_| P2pError::Network("swarm task stopped".into()))
    }

    /// Convenience: bump a peer that delivered useful IBD / gossip work.
    pub fn reward_peer(&self, peer: PeerId) -> Result<(), P2pError> {
        self.set_application_score(peer, APP_SCORE_GOOD_PEER)
    }

    /// Convenience: penalize a peer that sent rejectable payloads.
    pub fn penalize_peer(&self, peer: PeerId) -> Result<(), P2pError> {
        self.set_application_score(peer, APP_SCORE_BAD_PEER)
    }

    pub fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

/// libp2p gossip + getblock request-response node for Agora.
pub struct NetworkNode {
    swarm: Swarm<AgoraBehaviour>,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    command_rx: mpsc::UnboundedReceiver<Command>,
    inbound_channels: HashMap<
        request_response::InboundRequestId,
        ResponseChannel<GetBlockResponse>,
    >,
    outbound_hashes: HashMap<request_response::OutboundRequestId, Hash>,
}

impl NetworkNode {
    pub fn build(
        config: &NetworkConfig,
    ) -> Result<(NetworkHandle, mpsc::UnboundedReceiver<NetworkEvent>, Self), P2pError> {
        let id_keys = config
            .identity
            .clone()
            .unwrap_or_else(Keypair::generate_ed25519);
        let peer_id = id_keys.public().to_peer_id();

        let message_id_fn = |message: &gossipsub::Message| {
            let mut hasher = DefaultHasher::new();
            message.data.hash(&mut hasher);
            gossipsub::MessageId::from(hasher.finish().to_string())
        };

        let gossipsub_config = config.gossip.build_config(message_id_fn)?;

        let mut gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub_config,
        )
        .map_err(|e| P2pError::Gossip(e.to_string()))?;

        if config.gossip.peer_scoring {
            enable_peer_scoring(&mut gossipsub)?;
            info!(
                heartbeat_ms = config.gossip.heartbeat_interval.as_millis() as u64,
                mesh_n = config.gossip.mesh_n,
                "gossipsub peer scoring enabled"
            );
        }

        gossipsub
            .subscribe(&transactions_topic())
            .map_err(|e| P2pError::Gossip(e.to_string()))?;
        gossipsub
            .subscribe(&blocks_topic())
            .map_err(|e| P2pError::Gossip(e.to_string()))?;

        let getblock = request_response::cbor::Behaviour::new(
            [(getblock_protocol(), ProtocolSupport::Full)],
            request_response::Config::default(),
        );

        let connection_limits = connection_limits_behaviour(config.max_peers);
        info!(max_peers = config.max_peers, "connection limits enabled");

        let behaviour = AgoraBehaviour {
            gossipsub,
            getblock,
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
                inbound_channels: HashMap::new(),
                outbound_hashes: HashMap::new(),
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
            NetworkMessage::Block(_)
            | NetworkMessage::BlockAnnounce { .. }
            | NetworkMessage::CompactBlock { .. }
            | NetworkMessage::GetBlock { .. } => {
                self.publish(blocks_topic(), message.encode())
            }
        }
    }

    fn request_block(&mut self, peer: PeerId, hash: Hash) {
        let id = self
            .swarm
            .behaviour_mut()
            .getblock
            .send_request(&peer, GetBlockRequest::new(hash));
        self.outbound_hashes.insert(id, hash);
        debug!(%peer, hash = %hash.to_hex(), %id, "getblock request sent");
    }

    fn respond_get_block(
        &mut self,
        request_id: request_response::InboundRequestId,
        block: Option<Block>,
    ) {
        let Some(channel) = self.inbound_channels.remove(&request_id) else {
            warn!(%request_id, "getblock response channel missing");
            return;
        };
        let response = match block {
            Some(b) => GetBlockResponse::found(b),
            None => GetBlockResponse::missing(),
        };
        if self
            .swarm
            .behaviour_mut()
            .getblock
            .send_response(channel, response)
            .is_err()
        {
            warn!(%request_id, "getblock send_response failed (channel closed)");
        }
    }

    fn handle_getblock_event(
        &mut self,
        event: request_response::Event<GetBlockRequest, GetBlockResponse>,
    ) {
        match event {
            request_response::Event::Message { peer, message, .. } => match message {
                request_response::Message::Request {
                    request_id,
                    request,
                    channel,
                } => {
                    self.inbound_channels.insert(request_id, channel);
                    let _ = self.event_tx.send(NetworkEvent::GetBlockRequest {
                        peer,
                        hash: request.hash,
                        request_id,
                    });
                }
                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    let hash = self
                        .outbound_hashes
                        .remove(&request_id)
                        .unwrap_or(Hash::ZERO);
                    let _ = self.event_tx.send(NetworkEvent::GetBlockResponse {
                        peer,
                        hash,
                        block: response.block,
                    });
                }
            },
            request_response::Event::OutboundFailure {
                peer,
                request_id,
                error,
                ..
            } => {
                let hash = self
                    .outbound_hashes
                    .remove(&request_id)
                    .unwrap_or(Hash::ZERO);
                let _ = self.event_tx.send(NetworkEvent::GetBlockFailure {
                    peer,
                    hash,
                    error: error.to_string(),
                });
            }
            request_response::Event::InboundFailure {
                peer,
                request_id,
                error,
                ..
            } => {
                self.inbound_channels.remove(&request_id);
                warn!(%peer, %request_id, error = %error, "getblock inbound failure");
            }
            request_response::Event::ResponseSent {
                peer,
                request_id,
                ..
            } => {
                debug!(%peer, %request_id, "getblock response sent");
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
                        Some(Command::RequestBlock { peer, hash }) => {
                            self.request_block(peer, hash);
                        }
                        Some(Command::RespondGetBlock { request_id, block }) => {
                            self.respond_get_block(request_id, block);
                        }
                        Some(Command::SetAppScore { peer, score }) => {
                            let ok = self
                                .swarm
                                .behaviour_mut()
                                .gossipsub
                                .set_application_score(&peer, score);
                            debug!(%peer, score, applied = ok, "application peer score");
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
                        SwarmEvent::Behaviour(AgoraBehaviourEvent::Getblock(ev)) => {
                            self.handle_getblock_event(ev);
                        }
                        SwarmEvent::IncomingConnectionError { error, .. } => {
                            warn!(error = %error, "incoming connection failed (limits or handshake)");
                        }
                        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                            warn!(?peer_id, error = %error, "outgoing connection failed (limits or dial)");
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Format a dialable address that embeds the peer id (`/ip4/.../tcp/.../p2p/<peer>`).
///
/// Rewrites `/ip4/0.0.0.0` → `/ip4/127.0.0.1` so seeder registrations are reachable locally.
pub fn dial_addr(listen: &Multiaddr, peer_id: PeerId) -> Multiaddr {
    use libp2p::multiaddr::Protocol;
    let mut addr = Multiaddr::empty();
    for proto in listen.iter() {
        match proto {
            Protocol::Ip4(ip) if ip.is_unspecified() => {
                addr.push(Protocol::Ip4(std::net::Ipv4Addr::LOCALHOST));
            }
            other => addr.push(other),
        }
    }
    addr.push(Protocol::P2p(peer_id));
    addr
}
