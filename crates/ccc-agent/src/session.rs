use ccc_core::types::Message;

/// A conversation session.
pub struct Session {
    pub history: Vec<Message>,
    pub system_prompt: String,
}

impl Session {
    pub fn new(system_prompt: String) -> Self {
        Self {
            history: Vec::new(),
            system_prompt,
        }
    }

    pub fn add_message(&mut self, msg: Message) {
        self.history.push(msg);
    }
}
