use harmony::model::id::{RoleId, UserId};
use trueskill::Rating;

use crate::config::Rank;
use crate::model::Lobby;
use crate::model::PlayerInfo;
use crate::Result;

enum Row {
    Player(UserId, Rating),
    Rank(RoleId, f64),
}

impl Row {
    fn value(&self) -> f64 {
        match self {
            Self::Player(_, x) => x.mean(),
            Self::Rank(_, x) => *x,
        }
    }

    fn is_rank(&self) -> bool {
        matches!(self, Self::Rank(_, _))
    }
}

pub fn leaderboard<F>(
    lobby: &Lobby,
    page_len: usize,
    ranks: &[Rank],
    mut f: F,
) -> Result<Vec<(String, String)>>
where
    F: FnMut(UserId) -> Result<bool>,
{
    let ratings = ratings(
        lobby,
        |user_id, &info| match f(user_id) {
            Ok(true) => Some(Ok(info.rating)),
            Ok(false) => None,
            Err(err) => Some(Err(err)),
        },
        |a, b| b.mean().partial_cmp(&a.mean()).unwrap(),
    )?;
    let pages = (ratings.len() + page_len - 1) / page_len;
    let pages = pages.max(1);
    let mut v = Vec::new();
    for (page, ratings) in ratings.chunks(page_len).enumerate() {
        let mut ratings = ratings
            .iter()
            .enumerate()
            .map(|(i, (user_id, rating))| (page * page_len + i + 1, Row::Player(*user_id, *rating)))
            .chain(ranks.iter().map(|rank| (0, Row::Rank(rank.id, rank.limit))))
            .collect::<Vec<_>>();
        ratings.sort_by(|(_, a), (_, b)| b.value().partial_cmp(&a.value()).unwrap());
        for i in (0..ratings.len()).rev() {
            match ratings[i].1 {
                Row::Rank(_, _) => continue,
                Row::Player(_, _) => {
                    ratings.truncate(i + 1);
                    break;
                }
            }
        }
        while ratings.len() > 1 && ratings[0].1.is_rank() && ratings[1].1.is_rank() {
            ratings.remove(0);
        }
        let title = format!("Leaderboard ({}/{})", page + 1, pages);
        let description = ratings
            .iter()
            .map(|(i, x)| match x {
                Row::Player(user_id, rating) => {
                    format!(
                        "{}: {} - **{:.0}** ± {:.0}",
                        i,
                        user_id.mention(),
                        rating.mean(),
                        2.0 * rating.variance().sqrt(),
                    )
                }
                Row::Rank(role_id, _) => {
                    format!("-- {} --", role_id.mention())
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        v.push((title, description));
    }
    Ok(v)
}

pub fn ratings<F, T, S>(lobby: &Lobby, mut f: F, mut s: S) -> Result<Vec<(UserId, T)>>
where
    F: FnMut(UserId, &PlayerInfo) -> Option<Result<T>>,
    S: FnMut(&T, &T) -> std::cmp::Ordering,
{
    let mut ratings = lobby
        .ratings()
        .iter()
        .filter_map(|(&user_id, info)| f(user_id, info).map(|x| Ok((user_id, x?))))
        .collect::<Result<Vec<_>>>()?;
    ratings.sort_by(|a, b| s(&a.1, &b.1));
    Ok(ratings)
}
