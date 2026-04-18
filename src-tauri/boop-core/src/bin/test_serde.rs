use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub struct PendingBoopDto {
    pub id: Uuid,
    pub is_ready: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CoreEvent {
    BoopReceived {
        friend_id: Uuid,
        boop: PendingBoopDto,
    },
    BoopReady {
        friend_id: Uuid,
        boop_id: Uuid,
    },
}

fn main() {
    let evt = CoreEvent::BoopReady {
        friend_id: Uuid::new_v4(),
        boop_id: Uuid::new_v4(),
    };
    println!("{}", serde_json::to_string(&evt).unwrap());
    
    let evt2 = CoreEvent::BoopReceived {
        friend_id: Uuid::new_v4(),
        boop: PendingBoopDto { id: Uuid::new_v4(), is_ready: false },
    };
    println!("{}", serde_json::to_string(&evt2).unwrap());
}
