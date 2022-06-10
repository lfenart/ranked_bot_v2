use itertools::Itertools;
use rand::Rng;
use trueskill::{Rating, SimpleTrueSkill as TrueSkill};

fn balance_internal(players: &[f64]) -> u64 {
    let goal = players.iter().sum::<f64>() / 2.0 - players[0];
    let len = players.len();
    let mut best_score = f64::INFINITY;
    let mut best_team1 = vec![];
    for team1 in (1..len).combinations(len / 2 - 1) {
        let score = (goal - team1.iter().map(|&x| players[x]).sum::<f64>()).abs();
        if score < best_score {
            best_score = score;
            best_team1 = team1;
        }
    }
    let mut bitmap = 1;
    for x in best_team1 {
        bitmap ^= 1 << x;
    }
    bitmap
}

pub fn balance<T: Copy>(players: &[(T, Rating)]) -> [Vec<(T, Rating)>; 2] {
    let len = players.len();
    if len < 2 {
        panic!("Not enough players");
    }
    let mut players = players.to_vec();
    players.sort_by(|a, b| b.1.mean().partial_cmp(&a.1.mean()).unwrap());
    let team1 = balance_internal(&players.iter().map(|x| x.1.mean()).collect::<Vec<_>>());
    let team1 = if rand::thread_rng().gen_bool(0.5) {
        !team1
    } else {
        team1
    };
    let mut teams = [
        Vec::with_capacity(len / 2),
        Vec::with_capacity((len + 1) / 2),
    ];
    for (i, player) in players.into_iter().enumerate() {
        if team1 & (1 << i) != 0 {
            &mut teams[0]
        } else {
            &mut teams[1]
        }
        .push(player);
    }
    teams
}

pub fn quality<T: Copy>(teams: &[Vec<(T, Rating)>; 2], trueskill: TrueSkill) -> f64 {
    trueskill.quality(
        &teams[0].iter().map(|x| x.1).collect::<Vec<_>>(),
        &teams[1].iter().map(|x| x.1).collect::<Vec<_>>(),
    )
}
