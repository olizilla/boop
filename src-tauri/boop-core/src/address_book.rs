use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Friend {
    pub id: String, // Random unique id (local to the address book, for UI routing)
    pub endpoint_id: String, // Their public Iroh EndpointID
    pub nickname: String,
    pub emoji: String,
    pub doc_ticket: Option<String>, // The negotiated iroh-docs ticket of the shared queue
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AddressBook {
    pub friends: Vec<Friend>,
}

impl AddressBook {
    pub fn new() -> Self {
        Self { friends: Vec::new() }
    }

    pub fn add_friend(&mut self, nickname: String, endpoint_id: String) {
        let emoji = Self::emoji_for_id(&endpoint_id);
        let friend = Friend {
            id: uuid::Uuid::new_v4().to_string(),
            endpoint_id,
            nickname,
            emoji,
            doc_ticket: None,
        };
        self.friends.push(friend);
    }

    pub fn set_friend_doc(&mut self, endpoint_id: &str, doc_ticket: String) {
        if let Some(friend) = self.friends.iter_mut().find(|f| f.endpoint_id == endpoint_id) {
            friend.doc_ticket = Some(doc_ticket);
        }
    }

    fn emoji_for_id(id: &str) -> String {
        let emojis = [
            "🍎", "🍋", "🍉", "🍇", "🍓", "🍒", "🥝", "🍍", "🥥", "🥑",
            "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯",
            "🦁", "🐮", "🐷", "🐸", "🐵", "🐧", "🐦", "🐥", "🦉", "🐺",
            "⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱",
            "🪀", "🪁", "🥊", "🥋", "🛹", "⛸", "🎿", "🏄", "🏎", "🎯"
        ];
        
        // simple hash of string content
        let mut sum: usize = 0;
        for b in id.bytes() {
            sum = sum.wrapping_add(b as usize);
        }
        
        emojis[sum % emojis.len()].to_string()
    }
}
