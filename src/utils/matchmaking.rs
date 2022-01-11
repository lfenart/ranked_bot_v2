use std::iter::FromIterator;

use itertools::Itertools;
use trueskill::{Rating, SimpleTrueSkill};

#[inline]
fn balance_internal(ratings: &[Rating]) -> Vec<usize> {
    let goal = Rating::from_iter(ratings).mean() / 2.0 - ratings.last().unwrap().mean();
    let len = ratings.len() - 2;
    let mut best_score = f64::INFINITY;
    let mut best_team1 = None;
    for team1 in (0..len).combinations(len / 2) {
        let score = team1
            .iter()
            .fold(goal, |acc, &x| acc - ratings[x].mean())
            .abs();
        if score < best_score {
            best_score = score;
            best_team1 = Some(team1);
        }
    }
    let mut team1 = best_team1.unwrap();
    team1.push(ratings.len() - 1);
    team1
}

pub fn balance<T: Copy>(players: &[(T, Rating)]) -> [Vec<(T, Rating)>; 2] {
    let len = players.len();
    if len < 2 {
        panic!("Not enough players");
    }
    let mut players = players.to_vec();
    players.sort_by(|a, b| a.1.mean().partial_cmp(&b.1.mean()).unwrap());
    let team1 = balance_internal(&players.iter().map(|x| x.1).collect::<Vec<_>>());
    let mut index = 0;
    let mut teams = [
        Vec::with_capacity(len / 2),
        Vec::with_capacity((len + 1) / 2),
    ];
    for (i, player) in players.into_iter().enumerate() {
        if team1[index] == i {
            index += 1;
            &mut teams[0]
        } else {
            &mut teams[1]
        }
        .push(player);
    }
    teams
}

pub fn quality<T: Copy>(teams: &[Vec<(T, Rating)>; 2], trueskill: SimpleTrueSkill) -> f64 {
    trueskill.quality(
        &teams[0].iter().map(|x| x.1).collect::<Vec<_>>(),
        &teams[1].iter().map(|x| x.1).collect::<Vec<_>>(),
    )
}
