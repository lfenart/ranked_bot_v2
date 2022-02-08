use harmony::model::id::UserId;
use serde::{de, Deserialize, Deserializer, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum OpCode {
    GameStarted,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum BridgeEvent {
    GameStarted(GameStarted),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GameStarted {
    pub players: Vec<UserId>,
}

impl From<GameStarted> for BridgeEvent {
    fn from(game_started: GameStarted) -> Self {
        Self::GameStarted(game_started)
    }
}

impl<'de> Deserialize<'de> for BridgeEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = serde_json::value::Map::deserialize(deserializer)?;
        let t = map
            .remove("t")
            .ok_or_else(|| de::Error::missing_field("t"))
            .and_then(OpCode::deserialize)
            .map_err(de::Error::custom)?;
        let d = map
            .remove("d")
            .ok_or_else(|| de::Error::missing_field("d"))?;
        match t {
            OpCode::GameStarted => GameStarted::deserialize(d).map(Into::into),
        }
        .map_err(de::Error::custom)
    }
}
