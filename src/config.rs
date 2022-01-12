use serde::Deserialize;
use trueskill::SimpleTrueSkill;

#[derive(Deserialize)]
pub struct Config {
    pub trueskill: SimpleTrueSkill,
    pub lobbies: Vec<Lobby>,
    pub infos: Vec<u64>,
    pub roles: Roles,
    pub ranks: Vec<Rank>,
    pub database: String,
    pub timeout: i64,
}

#[derive(Deserialize)]
pub struct Lobby {
    pub channel: u64,
    pub webhook: Option<Webhook>,
    pub capacity: usize,
}

#[derive(Deserialize)]
pub struct Roles {
    pub ranked: u64,
    pub admin: u64,
    pub banned: u64,
}

#[derive(Deserialize)]
pub struct Rank {
    pub name: String,
    pub limit: f64,
    pub color: String,
}

#[derive(Deserialize)]
pub struct Webhook {
    pub id: u64,
    pub token: String,
}
