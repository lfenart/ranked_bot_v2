use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr)]
pub enum Score {
    Undecided,
    Team1,
    Team2,
    Draw,
    Cancelled,
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undecided => "undecided".fmt(f),
            Self::Team1 => "team 1".fmt(f),
            Self::Team2 => "team 2".fmt(f),
            Self::Draw => "draw".fmt(f),
            Self::Cancelled => "cancelled".fmt(f),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    id: usize,
    team1: Vec<u64>,
    team2: Vec<u64>,
    score: Score,
    datetime: DateTime<Utc>,
}

impl Game {
    pub fn create(team1: Vec<u64>, team2: Vec<u64>, datetime: DateTime<Utc>) -> Self {
        Self {
            id: 0,
            team1,
            team2,
            score: Score::Undecided,
            datetime,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn set_id(&mut self, id: usize) {
        self.id = id;
    }

    pub fn teams(&self) -> [&[u64]; 2] {
        [&self.team1, &self.team2]
    }

    pub fn score(&self) -> Score {
        self.score
    }

    pub fn set_score(&mut self, score: Score) {
        self.score = score;
    }

    pub fn datetime(&self) -> DateTime<Utc> {
        self.datetime
    }

    pub fn set_teams(&mut self, mut teams: [Vec<u64>; 2]) {
        std::mem::swap(&mut self.team1, &mut teams[0]);
        std::mem::swap(&mut self.team2, &mut teams[1]);
    }
}
