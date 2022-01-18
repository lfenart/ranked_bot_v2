use harmony::model::id::UserId;

use crate::model::Lobby;
use crate::model::PlayerInfo;
use crate::Result;

pub fn leaderboard<F>(lobby: &Lobby, page_len: usize, mut f: F) -> Result<Vec<(String, String)>>
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
        let title = format!("Leaderboard ({}/{})", page + 1, pages);
        let description = ratings
            .iter()
            .enumerate()
            .map(|(i, (user_id, x))| {
                format!(
                    "{}: {} - **{:.0}** ± {:.0}",
                    page * page_len + i + 1,
                    user_id.mention(),
                    x.mean(),
                    2.0 * x.variance().sqrt(),
                )
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
