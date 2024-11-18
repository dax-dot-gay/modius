use std::{
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_channel::{Receiver, Sender};
use libp2p::{
    futures::StreamExt,
    identity::Keypair,
    noise,
    rendezvous::Namespace,
    swarm::{DialError, NetworkBehaviour, SwarmEvent},
    tcp, yamux, PeerId, Stream, StreamProtocol, Swarm, SwarmBuilder,
};

use super::{
    command::{CommandKind, CommandWrapper},
    event::Event,
};

#[derive(NetworkBehaviour)]
struct Behaviour {
    pub stream: libp2p_stream::Behaviour,
    pub ping: libp2p::ping::Behaviour,
    pub mdns: libp2p::mdns::tokio::Behaviour,
    pub upnp: libp2p::upnp::tokio::Behaviour,
    pub identify: libp2p::identify::Behaviour,
    pub rendezvous: libp2p::rendezvous::client::Behaviour,
    pub relay: libp2p::relay::client::Behaviour,
}

enum LoopEvent {
    Command(CommandWrapper),
    Swarm(SwarmEvent<BehaviourEvent>),
    Stream(PeerId, Stream),
}

pub struct Client {
    commands: Receiver<CommandWrapper>,
    events: Sender<Event>,
    key: Keypair,
    name: String,
    group: String,
    port: usize,
    swarm: Arc<Mutex<Swarm<Behaviour>>>,
}

const MODIUS_PROTOCOL: StreamProtocol = StreamProtocol::new("/modius/1.0.0");

impl Client {
    pub fn create(
        key: Keypair,
        name: String,
        group: String,
        port: usize,
    ) -> Result<(Self, Sender<CommandWrapper>, Receiver<Event>), Box<dyn Error>> {
        let (tx_cmd, rx_cmd) = async_channel::unbounded::<CommandWrapper>();
        let (tx_evt, rx_evt) = async_channel::unbounded::<Event>();
        let swarm = SwarmBuilder::with_existing_identity(key.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|key, relay| Behaviour {
                stream: libp2p_stream::Behaviour::new(),
                ping: libp2p::ping::Behaviour::default(),
                mdns: libp2p::mdns::tokio::Behaviour::new(
                    libp2p::mdns::Config::default(),
                    key.public().to_peer_id(),
                )
                .expect("To be able to configure MDNS"),
                upnp: libp2p::upnp::tokio::Behaviour::default(),
                identify: libp2p::identify::Behaviour::new(libp2p::identify::Config::new(
                    String::from("/modius/1.0.0"),
                    key.public(),
                )),
                rendezvous: libp2p::rendezvous::client::Behaviour::new(key.clone()),
                relay,
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();
        Ok((
            Client {
                commands: rx_cmd,
                events: tx_evt,
                key: key.clone(),
                name: name.clone(),
                group: group.clone(),
                port,
                swarm: Arc::new(Mutex::new(swarm)),
            },
            tx_cmd,
            rx_evt,
        ))
    }

    async fn handle_command(&mut self, command: CommandWrapper) -> Result<(), Box<dyn Error>> {
        if let Ok(mut swarm) = self.swarm.clone().lock() {
            let result: Result<(), Box<dyn Error>> = match command.clone().kind() {
                CommandKind::AddRelay(peer) => {
                    command.respond(swarm.dial(peer.address)).await?;
                    Ok(())
                }
                CommandKind::AddRendezvous(peer) => {
                    match swarm.dial(peer.address) {
                        Ok(_) => {
                            command
                                .respond(swarm.behaviour_mut().rendezvous.register(
                                    Namespace::from_static("modius"),
                                    peer.id,
                                    None,
                                ))
                                .await?
                        }
                        Err(e) => command.respond::<(), DialError>(Err(e)).await?,
                    }
                    Ok(())
                }
            };
        }

        Ok(())
    }

    async fn handle_event(
        &mut self,
        event: SwarmEvent<BehaviourEvent>,
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    async fn handle_stream(&mut self, peer: PeerId, stream: Stream) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    async fn event_loop(&mut self) -> Result<(), Box<dyn Error>> {
        let mut inbox = self
            .swarm
            .clone()
            .lock()
            .expect("To be able to lock swarm")
            .behaviour()
            .stream
            .new_control()
            .accept(MODIUS_PROTOCOL)?;
        loop {
            let event = match self.swarm.clone().lock() {
                Ok(mut swarm) => {
                    tokio::select! {
                        command = self.commands.recv() => command.and_then(|s| Ok(LoopEvent::Command(s))).or(Err(())),
                        event = swarm.next() => event.and_then(|s| Some(Ok(LoopEvent::Swarm(s)))).or(Some(Err(()))).unwrap(),
                        recv = inbox.next() => recv.and_then(|(peer, stream)| Some(Ok(LoopEvent::Stream(peer, stream)))).or(Some(Err(()))).unwrap()
                    }
                }
                _ => Err(()),
            };

            if let Ok(evt) = event {
                if let Err(e) = match evt {
                    LoopEvent::Command(command) => self.handle_command(command).await,
                    LoopEvent::Swarm(event) => self.handle_event(event).await,
                    LoopEvent::Stream(peer, stream) => self.handle_stream(peer, stream).await,
                } {
                    return Err(e);
                }
            }
        }
    }

    pub async fn main(&mut self) -> Result<(), Box<dyn Error>> {
        let listener = self.swarm.lock().expect("Failed to lock").listen_on(
            String::from("/ip4/0.0.0.0/tcp/".to_owned() + &self.port.to_string()).parse()?,
        )?;
        let loop_result = self.event_loop().await;
        self.swarm
            .lock()
            .expect("Failed to lock")
            .remove_listener(listener);
        self.commands.close();
        self.events.close();
        loop_result
    }
}
