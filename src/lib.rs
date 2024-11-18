use std::{error::Error, sync::Arc};

use async_channel::{Receiver, Sender};
use chrono::Utc;
use derive_builder::Builder;
use libp2p::{identity::{Keypair, PublicKey}, PeerId};
use net::{command::CommandWrapper, event::Event};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use util::Peer;

mod util;
mod net;

#[derive(Clone, Debug, Builder)]
pub struct Node {
    #[builder(default = "Keypair::generate_ed25519()")]
    pub key: Keypair,

    #[builder(default = "Vec::new()")]
    pub peers: Vec<util::Peer>,

    #[builder(default = "String::from(Utc::now().timestamp().to_string() + \".modius\")")]
    pub name: String,

    #[builder(default = "String::from(\"modius.generic\")")]
    pub group: String,

    #[builder(default = "8000")]
    pub port: usize,

    #[builder(setter(skip))]
    pub commands: Option<Sender<CommandWrapper>>,

    #[builder(setter(skip))]
    pub events: Option<Receiver<Event>>,

    #[builder(setter(skip))]
    pub thread: Option<Arc<JoinHandle<Result<(), Box<dyn Error>>>>>
}

impl NodeBuilder {
    pub fn try_bootstrap<I: AsRef<str>, A: AsRef<str>>(&mut self, id: I, addr: A) -> Result<(), Box<dyn Error>> {
        self.with_peer(Peer::try_new(util::PeerType::Bootstrap, id, addr)?);

        Ok(())
    }

    pub fn with_peer(&mut self, peer: Peer) -> () {
        if let Some(ref mut peers) = self.peers {
            peers.push(peer);
        } else {
            let _ = self.peers.insert(vec![peer]);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedNode {
    pub key: Vec<u8>,
    pub peers: Vec<util::Peer>,
    pub name: String,
    pub group: String,
    pub port: usize
}

impl SavedNode {
    pub fn hydrate(&self) -> Result<Node, Box<dyn Error>> {
        let key = Keypair::from_protobuf_encoding(&self.key.as_slice())?;
        Ok(Node {
            key: key.clone(),
            peers: self.peers.clone(),
            name: self.name.clone(),
            group: self.group.clone(),
            port: self.port,
            commands: None,
            events: None,
            thread: None
        })
    }

    pub fn save(node: &Node) -> Self {
        SavedNode {
            key: node.key.to_protobuf_encoding().expect("Failed to parse Keypair"),
            peers: node.peers.clone(),
            name: node.name.clone(),
            group: node.group.clone(),
            port: node.port
        }
    }
}

impl Node {
    pub fn public_key(&self) -> PublicKey {
        self.key.public()
    }

    pub fn peer_id(&self) -> PeerId {
        self.key.public().to_peer_id()
    }

    pub fn id(&self) -> String {
        self.peer_id().to_string()
    }

    pub fn active(&self) -> bool {
        if let Some(commands) = &self.commands {
            if commands.is_closed() {
                return false;
            }
        } else {
            return false;
        }

        if let Some(events) = &self.events {
            if events.is_closed() {
                return false;
            }
        } else {
            return false;
        }

        if let Some(handle) = &self.thread {
            return !handle.is_finished();
        }
        
        false
    }

    pub fn save(&self) -> SavedNode {
        SavedNode::save(self)
    }

    pub fn load(state: SavedNode) -> Result<Node, Box<dyn Error>> {
        state.hydrate()
    }
}