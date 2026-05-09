use iroh_tickets::{Ticket, ParseError};
use iroh_tickets::endpoint::EndpointTicket;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InviteTicket {
    pub token: [u8; 32],
    pub endpoint: EndpointTicket,
}

impl Ticket for InviteTicket {
    const KIND: &'static str = "boop";

    fn encode_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("postcard serialization should not fail")
    }

    fn decode_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        postcard::from_bytes(bytes).map_err(Into::into)
    }
}

impl std::fmt::Display for InviteTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", <Self as Ticket>::encode_string(self))
    }
}

impl std::str::FromStr for InviteTicket {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <Self as Ticket>::decode_string(s)
    }
}
