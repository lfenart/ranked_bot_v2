use std::collections::HashMap;

use harmony::client::Context;
use harmony::model::id::{ChannelId, UserId};
use harmony::model::{Channel, Member, Message};
use inline_python::python;
use trueskill::{Rating, SimpleTrueSkill};

use crate::checks;
use crate::config::{Rank, Roles};
use crate::model::{Database, Lobbies, PlayerInfo, Score};
use crate::{Error, Result};

pub fn info(ctx: &Context, msg: &Message, lobbies: &Lobbies, args: &[String]) -> Result {
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let channel = if let Some(channel) = Channel::parse(ctx, msg.guild_id, &args[0])? {
        channel
    } else {
        return Err(Error::ChannelNotFound(args[0].to_string()));
    };
    info_internal(ctx, msg, lobbies, msg.author.id, channel.id)?;
    Ok(())
}

pub fn forceinfo(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &Lobbies,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    if args.len() < 2 {
        return Err(Error::NotEnoughArguments);
    }
    let channel = if let Some(channel) = Channel::parse(ctx, msg.guild_id, &args[0])? {
        channel
    } else {
        return Err(Error::ChannelNotFound(args[0].to_string()));
    };
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Outside guild but admin check passed => should never happen
        return Ok(());
    };
    for arg in args.iter().skip(1) {
        if let Some(member) = Member::parse(ctx, guild_id, arg)? {
            info_internal(ctx, msg, lobbies, member.user.id, channel.id)?;
        }
    }
    Ok(())
}

fn info_internal(
    ctx: &Context,
    msg: &Message,
    lobbies: &Lobbies,
    member_id: UserId,
    channel: ChannelId,
) -> Result {
    let lobby = lobbies.get(&channel).ok_or(Error::NotALobby(channel))?;
    if let Some(player_info) = lobby.ratings().get(&member_id) {
        ctx.send_message(msg.channel_id, |m| {
            m.embed(|e| {
                e.description(format!(
                    "{}\nRating: {:.0} ± {:.0}\nWins: {}\nLosses: {}\nDraws: {}",
                    member_id.mention(),
                    player_info.rating.mean(),
                    2.0 * player_info.rating.variance().sqrt(),
                    player_info.wins,
                    player_info.losses,
                    player_info.draws
                ))
                .title("Info")
            })
        })?;
    } else {
        ctx.send_message(msg.channel_id, |m| {
            m.embed(|e| {
                e.description(format!("{}\nNo info yet, play more!", member_id.mention()))
                    .title("Info")
            })
        })?;
    }
    Ok(())
}

pub fn history(
    ctx: &Context,
    msg: &Message,
    ranks: &[Rank],
    lobbies: &Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
    args: &[String],
) -> Result {
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let channel = if let Some(channel) = Channel::parse(ctx, msg.guild_id, &args[0])? {
        channel
    } else {
        return Err(Error::ChannelNotFound(args[0].to_string()));
    };
    let limit = if let Some(limit) = args.get(1) {
        Some(limit.parse()?)
    } else {
        None
    };
    history_internal(
        ctx,
        msg,
        ranks,
        lobbies,
        database,
        trueskill,
        msg.author.id,
        channel.id,
        limit,
    )?;
    Ok(())
}

pub fn forcehistory(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    ranks: &[Rank],
    lobbies: &Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    if args.len() < 2 {
        return Err(Error::NotEnoughArguments);
    }
    let channel = if let Some(channel) = Channel::parse(ctx, msg.guild_id, &args[0])? {
        channel
    } else {
        return Err(Error::ChannelNotFound(args[0].to_string()));
    };
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Outside guild but admin check passed => should never happen
        return Ok(());
    };
    let member = if let Some(member) = Member::parse(ctx, guild_id, &args[1])? {
        member
    } else {
        return Err(Error::MemberNotFound(args[1].to_owned()));
    };
    let limit = if let Some(limit) = args.get(2) {
        Some(limit.parse()?)
    } else {
        None
    };
    history_internal(
        ctx,
        msg,
        ranks,
        lobbies,
        database,
        trueskill,
        member.user.id,
        channel.id,
        limit,
    )?;
    Ok(())
}

fn history_internal(
    ctx: &Context,
    msg: &Message,
    ranks: &[Rank],
    lobbies: &Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
    member_id: UserId,
    channel_id: ChannelId,
    limit: Option<usize>,
) -> Result {
    if lobbies.get(&channel_id).is_none() {
        return Err(Error::NotALobby(channel_id));
    }
    let games = database
        .get_games()?
        .remove(&channel_id.0)
        .unwrap_or_default();
    let mut info_history = Vec::new();
    let initials = database.get_initial_ratings()?;
    let mut ratings: HashMap<UserId, _> = HashMap::new();
    let default_rating = trueskill.create_rating();
    let default_info = PlayerInfo::new(default_rating);
    info_history.push(if let Some(&rating) = initials.get(&member_id.0) {
        PlayerInfo::new(Rating::new(rating, default_rating.variance()))
    } else {
        default_info
    });
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
                    .get(&x.into())
                    .map(|x: &PlayerInfo| x.rating)
                    .unwrap_or_else(|| {
                        if let Some(&rating) = initials.get(&x) {
                            Rating::new(rating, default_rating.variance())
                        } else {
                            default_rating
                        }
                    })
            })
            .collect::<Vec<_>>();
        let mut team2_ratings = teams[1]
            .iter()
            .map(|&x| {
                ratings.get(&x.into()).map(|x| x.rating).unwrap_or_else(|| {
                    if let Some(&rating) = initials.get(&x) {
                        Rating::new(rating, default_rating.variance())
                    } else {
                        default_rating
                    }
                })
            })
            .collect::<Vec<_>>();
        trueskill.update(&mut team1_ratings, &mut team2_ratings, score);
        for (i, &user_id) in teams[0].iter().enumerate() {
            let player_info = ratings.entry(user_id.into()).or_insert(default_info);
            match score {
                trueskill::Score::Win => player_info.wins += 1,
                trueskill::Score::Loss => player_info.losses += 1,
                trueskill::Score::Draw => player_info.draws += 1,
            };
            player_info.rating = team1_ratings[i];
            if user_id == member_id.0 {
                info_history.push(*player_info);
            }
        }
        for (i, &user_id) in teams[1].iter().enumerate() {
            let player_info = ratings.entry(user_id.into()).or_insert(default_info);
            match score {
                trueskill::Score::Win => player_info.losses += 1,
                trueskill::Score::Loss => player_info.wins += 1,
                trueskill::Score::Draw => player_info.draws += 1,
            };
            player_info.rating = team2_ratings[i];
            if user_id == member_id.0 {
                info_history.push(*player_info);
            }
        }
    }
    let xs = (0..info_history.len()).collect::<Vec<_>>();
    let ys = info_history
        .iter()
        .map(|x| x.rating.mean())
        .collect::<Vec<_>>();
    let (xs, ys) = if let Some(limit) = limit {
        (
            xs.into_iter()
                .rev()
                .take(limit + 1)
                .rev()
                .collect::<Vec<_>>(),
            ys.into_iter()
                .rev()
                .take(limit + 1)
                .rev()
                .collect::<Vec<_>>(),
        )
    } else {
        (xs, ys)
    };
    let ymin = ys.iter().fold(f64::INFINITY, |acc, x| x.min(acc));
    let ymax = ys.iter().fold(f64::NEG_INFINITY, |acc, x| x.max(acc));
    let ydelta = (ymax - ymin) / 20.0;
    let ymin = ymin - ydelta;
    let ymax = ymax + ydelta;
    let ranks = ranks
        .iter()
        .map(|x| (&x.name, &x.color, x.limit))
        .collect::<Vec<_>>();
    let title = "test";
    python! {
        import matplotlib.pyplot as plt
        import numpy as np
        plt.figure()
        plt.clf()
        plt.ylim(['ymin, 'ymax])
        plt.grid()
        y = 0
        for rank in 'ranks:
            limit = rank[2]
            plt.axhspan(y, limit, alpha=0.3, color=rank[1])
            y = limit
        plt.plot('ys, "black", marker='o')
        step = max(1, round(len('xs) / 15))
        plt.xticks(np.arange(0, len('xs), step), 'xs[::step], rotation=45, ha="right")
        plt.title('title)
        plt.ylabel("rating")
        plt.savefig(fname="plot")
    }
    ctx.send_files(msg.channel_id, &["plot.png"], |m| m)?;
    Ok(())
}
