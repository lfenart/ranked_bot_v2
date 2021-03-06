use harmony::model::id::{ChannelId, RoleId, WebhookId};
use serde::Deserialize;
use trueskill::SimpleTrueSkill as TrueSkill;

#[derive(Deserialize)]
pub struct Config {
    pub prefix: String,
    pub trueskill: TrueSkill,
    pub lobbies: Vec<Lobby>,
    pub infos: Vec<ChannelId>,
    pub roles: Roles,
    pub ranks: Vec<Rank>,
    pub database: String,
    pub timeout: Timeout,
    pub bridge: ChannelId,
    pub game: Option<String>,
}

#[derive(Deserialize)]
pub struct Lobby {
    pub channel: ChannelId,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub webhook: Option<Webhook>,
    pub capacity: usize,
}

#[derive(Deserialize)]
pub struct Roles {
    pub ranked: RoleId,
    pub admin: RoleId,
    pub banned: RoleId,
}

#[derive(Deserialize)]
pub struct Rank {
    pub id: RoleId,
    pub name: String,
    pub limit: f64,
    pub color: String,
}

#[derive(Deserialize)]
pub struct Webhook {
    pub id: WebhookId,
    pub token: String,
}

#[derive(Clone, Copy, Deserialize)]
pub struct Timeout {
    pub default: u64,
    pub maximum: u64,
    pub warn: u64,
}
