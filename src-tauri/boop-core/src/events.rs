use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::address_book::Friend;
use crate::iroh_boops::PendingBoopDto;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoreEvent {
    StateSnapshot {
        friends: Vec<Friend>,
        pending_boops: HashMap<uuid::Uuid, Vec<PendingBoopDto>>,
    },
    FriendAdded {
        friend: Friend,
    },
    BoopReceived {
        friend_id: uuid::Uuid,
        boop: PendingBoopDto,
    },
    BoopReady {
        friend_id: uuid::Uuid,
        boop_id: uuid::Uuid,
    },
    BoopListenedRemote {
        friend_id: uuid::Uuid,
        boop_id: uuid::Uuid,
    },
    PeerConnected {
        friend_id: uuid::Uuid,
    },
    PlaybackStarted {
        friend_id: uuid::Uuid,
        boop_id: uuid::Uuid,
    },
    PlaybackFinished {
        friend_id: uuid::Uuid,
        boop_id: uuid::Uuid,
    },
}
