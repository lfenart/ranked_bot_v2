use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use harmony::model::id::ChannelId;
use rusqlite::{params, Connection};

use super::Game;

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> rusqlite::Result<Self> {
        Ok(Self {
            connection: Connection::open(path)?,
        })
    }

    fn last_game_id(&self, channel: ChannelId) -> rusqlite::Result<usize> {
        let mut stmt = self
            .connection
            .prepare("SELECT MAX(id) as id FROM games WHERE channel = ?1;")?;
        stmt.query_row(params![channel.0], |row| Ok(row.get(0).unwrap_or_default()))
    }

    pub fn insert_game(&self, game: &mut Game, channel: ChannelId) -> rusqlite::Result<()> {
        let game_id = self.last_game_id(channel)? + 1;
        game.set_id(game_id);
        self.connection.execute(
            "INSERT INTO games (id, channel, game) VALUES (?1, ?2, ?3);",
            params![game_id, channel.0, serde_json::to_string(game).unwrap()],
        )?;
        Ok(())
    }

    pub fn update_game(&self, game: &Game, channel: ChannelId) -> rusqlite::Result<()> {
        self.connection.execute(
            "UPDATE games SET game = ?3 WHERE channel = ?2 AND id = ?1;",
            params![game.id(), channel.0, serde_json::to_string(game).unwrap()],
        )?;
        Ok(())
    }

    pub fn get_games(&self) -> rusqlite::Result<HashMap<u64, BTreeMap<usize, Game>>> {
        let mut stmt = self
            .connection
            .prepare("SELECT id, channel, game FROM games;")?;
        let games_raw = stmt.query_map([], |row| {
            Ok((
                row.get::<_, u64>(1)?,
                row.get::<_, usize>(0)?,
                serde_json::from_str::<Game>(&row.get::<_, String>(2)?).unwrap(),
            ))
        })?;
        let mut games = HashMap::<_, BTreeMap<_, _>>::new();
        for game in games_raw {
            let game = game?;
            games.entry(game.0).or_default().insert(game.1, game.2);
        }
        Ok(games)
    }

    pub fn get_game(&self, channel_id: u64, game_id: usize) -> rusqlite::Result<Game> {
        let mut stmt = self
            .connection
            .prepare("SELECT game FROM games WHERE channel = ?1 AND id = ?2")?;
        stmt.query_row(params![channel_id, game_id], |row| {
            Ok(serde_json::from_str(&row.get::<_, String>(0)?).unwrap())
        })
    }

    pub fn get_initial_ratings(&self) -> rusqlite::Result<HashMap<u64, f64>> {
        let mut stmt = self
            .connection
            .prepare("SELECT player, rating FROM initial;")?;
        let intials_raw = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        intials_raw.collect()
    }
}
