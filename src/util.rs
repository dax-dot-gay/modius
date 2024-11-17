use std::{error::Error, str::FromStr};

use chrono::{DateTime, Utc};
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PeerType {
    Bootstrap,
    Discovered,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Peer {
    pub id: PeerId,
    pub address: Multiaddr,
    pub last_seen: Option<DateTime<Utc>>,
    pub name: Option<String>,
    pub kind: PeerType,
}

impl Peer {
    pub fn new(kind: PeerType, id: PeerId, address: Multiaddr) -> Self {
        Peer {
            id,
            address,
            kind,
            last_seen: None,
            name: None,
        }
    }

    pub fn try_new<I: AsRef<str>, A: AsRef<str>>(
        kind: PeerType,
        id: I,
        address: A,
    ) -> Result<Self, Box<dyn Error>> {
        let peer_id = PeerId::from_str(id.as_ref())?;
        let multiaddr = Multiaddr::from_str(address.as_ref())?;
        Ok(Peer::new(kind, peer_id, multiaddr))
    }
}
