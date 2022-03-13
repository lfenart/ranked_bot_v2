use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::mem;
use std::ops::{Deref, DerefMut};

use chrono::{DateTime, Utc};
use harmony::model::id::{ChannelId, MessageId, UserId, WebhookId};

use super::Ratings;

#[derive(Debug, Clone)]
pub enum LobbyError {
    NotInQueue(UserId),
    AlreadyInQueue(UserId),
    Frozen,
}

impl Error for LobbyError {}

impl fmt::Display for LobbyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInQueue(user_id) => write!(f, "{} is not in the queue.", user_id.mention()),
            Self::AlreadyInQueue(user_id) => {
                write!(f, "{} is already in the queue.", user_id.mention())
            }
            Self::Frozen => "The queue is frozen.".fmt(f),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Lobbies(HashMap<ChannelId, Lobby>);

impl Deref for Lobbies {
    type Target = HashMap<ChannelId, Lobby>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Lobbies {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone)]
pub struct Lobby {
    queue: HashMap<UserId, DateTime<Utc>>,
    name: String,
    ratings: Ratings,
    webhook: Option<(WebhookId, String, Vec<MessageId>)>,
    capacity: usize,
    frozen: bool,
}

impl Lobby {
    pub fn new(name: String, capacity: usize, ratings: Ratings) -> Self {
        Self {
            queue: HashMap::default(),
            name,
            ratings,
            webhook: None,
            capacity,
            frozen: false,
        }
    }

    pub fn ratings(&self) -> &Ratings {
        &self.ratings
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn join(
        &mut self,
        user_id: UserId,
        timestamp: DateTime<Utc>,
        force: bool,
    ) -> Result<(), LobbyError> {
        if !force && self.frozen {
            return Err(LobbyError::Frozen);
        }
        if self.queue.insert(user_id, timestamp).is_some() {
            return Err(LobbyError::AlreadyInQueue(user_id));
        }
        Ok(())
    }

    pub fn leave(&mut self, user_id: UserId, force: bool) -> Result<(), LobbyError> {
        if !force && self.frozen {
            return Err(LobbyError::Frozen);
        }
        if self.queue.remove(&user_id).is_none() {
            return Err(LobbyError::NotInQueue(user_id));
        }
        Ok(())
    }

    pub fn queue(&self) -> &HashMap<UserId, DateTime<Utc>> {
        &self.queue
    }

    pub fn queue_mut(&mut self) -> &mut HashMap<UserId, DateTime<Utc>> {
        &mut self.queue
    }

    pub fn clear(&mut self) -> HashMap<UserId, DateTime<Utc>> {
        mem::take(&mut self.queue)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn unfreeze(&mut self) {
        self.frozen = false;
    }

    pub fn webhook_mut(&mut self) -> &mut Option<(WebhookId, String, Vec<MessageId>)> {
        &mut self.webhook
    }

    pub fn set_webhook(&mut self, webhook_id: WebhookId, webhook_token: String) {
        self.webhook = Some((webhook_id, webhook_token, Vec::new()));
    }

    pub fn set_ratings(&mut self, ratings: Ratings) {
        self.ratings = ratings;
    }
}
