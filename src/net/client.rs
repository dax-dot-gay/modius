use std::{
    error::Error,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_channel::{Receiver, Sender};
use libp2p::{
    futures::StreamExt,
    identity::Keypair,
    noise,
    swarm::{self, NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, Swarm, SwarmBuilder,
};

use super::{command::CommandWrapper, event::Event};

#[derive(NetworkBehaviour)]
struct Behaviour {
    pub stream: libp2p_stream::Behaviour,
    pub ping: libp2p::ping::Behaviour,
    pub mdns: libp2p::mdns::tokio::Behaviour,
    pub upnp: libp2p::upnp::tokio::Behaviour,
    pub identify: libp2p::identify::Behaviour,
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
            .with_behaviour(|key| Behaviour {
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
        Ok(())
    }

    async fn handle_event(&mut self, event: SwarmEvent<BehaviourEvent>) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    async fn event_loop(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            if let Ok(mut swarm) = self.swarm.lock() {
                let result = tokio::select! {
                    command = self.commands.recv() => {
                        if let Ok(com) = command {
                            self.handle_command(com).await
                        } else {
                            Ok(())
                        }
                    },
                    event = swarm.next() => {
                        if let Some(evt) = event {
                            self.handle_event(evt).await
                        } else {
                            Ok(())
                        }
                    }
                };
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
