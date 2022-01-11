use std::fmt;
use std::num::ParseIntError;

use harmony::model::id::{ChannelId, UserId};

use crate::model::LobbyError;

#[derive(Debug)]
pub enum Error {
    Harmony(harmony::Error),
    Rusqlite(rusqlite::Error),
    ParseInt(ParseIntError),
    Lobby(LobbyError),
    NotALobby(ChannelId),
    NotEnoughArguments,
    BadArgument,
    GameAlreadySet,
    MemberNotFound(String),
    ChannelNotFound(String),
    GameNotFound(usize),
    NotPlaying(UserId),
    SameTeam,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Harmony(err) => err.fmt(f),
            Self::Rusqlite(err) => err.fmt(f),
            Self::ParseInt(err) => err.fmt(f),
            Self::Lobby(err) => err.fmt(f),
            Self::NotALobby(channel_id) => write!(f, "{} is not a lobby.", channel_id.mention()),
            Self::NotEnoughArguments => "Not enough arguments.".fmt(f),
            Self::BadArgument => "Bad argument.".fmt(f),
            Self::GameAlreadySet => "Undo before modifying the game.".fmt(f),
            Self::MemberNotFound(member) => write!(f, "Member {} not found.", member),
            Self::ChannelNotFound(channel) => write!(f, "Channel {} not found.", channel),
            Self::GameNotFound(game) => write!(f, "Game {} not found.", game),
            Self::NotPlaying(user) => write!(f, "{} is not playing.", user.mention()),
            Self::SameTeam => "The players are in the same team.".fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<harmony::Error> for Error {
    fn from(err: harmony::Error) -> Self {
        Self::Harmony(err)
    }
}

impl From<ParseIntError> for Error {
    fn from(err: ParseIntError) -> Self {
        Self::ParseInt(err)
    }
}

impl From<LobbyError> for Error {
    fn from(err: LobbyError) -> Self {
        Self::Lobby(err)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Self::Rusqlite(err)
    }
}
