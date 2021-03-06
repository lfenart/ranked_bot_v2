use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};

use harmony::model::id::UserId;
use trueskill::{self, Rating, SimpleTrueSkill as TrueSkill};

use super::{Game, Score};

#[derive(Debug, Clone, Default)]
pub struct Ratings(HashMap<UserId, PlayerInfo>);

impl Ratings {
    pub fn from_games(
        games: &BTreeMap<usize, Game>,
        initial: &HashMap<UserId, f64>,
        trueskill: TrueSkill,
    ) -> Self {
        let mut ratings = HashMap::new();
        for (user_id, rating) in initial.iter() {
            ratings.insert(
                *user_id,
                PlayerInfo::new(Rating::new(*rating, trueskill.sigma().powi(2))),
            );
        }
        let default_rating = trueskill.create_rating();
        let default_info = PlayerInfo::new(default_rating);
        for game in games.values() {
            let score = match game.score() {
                Score::Undecided | Score::Cancelled => continue,
                Score::Team1 => trueskill::Score::Win,
                Score::Team2 => trueskill::Score::Loss,
                Score::Draw => trueskill::Score::Draw,
            };
            let teams = game.teams();
            let mut team1_ratings = teams[0]
                .iter()
                .map(|&x| {
                    ratings
                        .get(&x)
                        .map(|y: &PlayerInfo| y.rating)
                        .unwrap_or_else(|| default_rating)
                })
                .collect::<Vec<_>>();
            let mut team2_ratings = teams[1]
                .iter()
                .map(|&x| {
                    ratings.get(&x).map(|x| x.rating).unwrap_or_else(|| {
                        if let Some(&rating) = initial.get(&x) {
                            Rating::new(rating, default_rating.variance())
                        } else {
                            default_rating
                        }
                    })
                })
                .collect::<Vec<_>>();
            trueskill.update(&mut team1_ratings, &mut team2_ratings, score);
            for (i, &user_id) in teams[0].iter().enumerate() {
                let player_info = ratings.entry(user_id).or_insert(default_info);
                match score {
                    trueskill::Score::Win => player_info.wins += 1,
                    trueskill::Score::Loss => player_info.losses += 1,
                    trueskill::Score::Draw => player_info.draws += 1,
                };
                player_info.rating = team1_ratings[i];
            }
            for (i, &user_id) in teams[1].iter().enumerate() {
                let player_info = ratings.entry(user_id).or_insert(default_info);
                match score {
                    trueskill::Score::Win => player_info.losses += 1,
                    trueskill::Score::Loss => player_info.wins += 1,
                    trueskill::Score::Draw => player_info.draws += 1,
                };
                player_info.rating = team2_ratings[i];
            }
        }
        Self(ratings)
    }
}

impl Deref for Ratings {
    type Target = HashMap<UserId, PlayerInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Ratings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerInfo {
    pub rating: Rating,
    pub wins: usize,
    pub losses: usize,
    pub draws: usize,
}

impl PlayerInfo {
    pub fn new(rating: Rating) -> Self {
        Self {
            rating,
            wins: 0,
            losses: 0,
            draws: 0,
        }
    }
}
