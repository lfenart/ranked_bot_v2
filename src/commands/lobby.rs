use std::collections::{HashMap, HashSet};

use chrono::Utc;
use harmony::client::Context;
use harmony::model::id::{ChannelId, UserId};
use harmony::model::{Member, Message};
use trueskill::SimpleTrueSkill;

use crate::checks;
use crate::config::Roles;
use crate::model::{Database, Game, Lobbies, Ratings, Score};
use crate::utils;
use crate::{Error, Result};

pub fn join(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    if checks::check_banned(ctx, msg, roles)? {
        return Ok(());
    }
    join_internal(ctx, msg, msg.author.id, false, lobbies, trueskill, database)
}

pub fn forcejoin(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Outside guild but admin check passed => should never happen
        return Ok(());
    };
    let members = args
        .iter()
        .map(|arg| match Member::parse(ctx, guild_id, arg) {
            Ok(Some(x)) => Ok(x),
            Ok(None) => Err(Error::MemberNotFound(arg.clone())),
            Err(err) => Err(err.into()),
        })
        .collect::<Result<Vec<_>>>()?;
    for member in members {
        join_internal(ctx, msg, member.user.id, true, lobbies, trueskill, database)?;
    }
    Ok(())
}

fn join_internal(
    ctx: &Context,
    msg: &Message,
    user_id: UserId,
    force: bool,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let players = {
        let lobby = lobbies
            .get_mut(&msg.channel_id)
            .ok_or(Error::NotALobby(msg.channel_id))?;
        lobby.join(user_id, msg.timestamp, force)?;
        ctx.send_message(msg.channel_id, |m| {
            m.embed(|e| {
                e.description(format!(
                    "[{}/{}] {} joined the queue.",
                    lobby.len(),
                    lobby.capacity(),
                    user_id.mention()
                ))
            })
        })?;
        if lobby.len() == lobby.capacity() {
            Some(lobby.clear().into_keys().collect())
        } else {
            None
        }
    };
    if let Some(players) = players {
        start_game(ctx, msg.channel_id, lobbies, players, trueskill, database)?;
    }
    Ok(())
}

pub fn leave(
    ctx: &Context,
    msg: &Message,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    leave_internal(
        ctx,
        msg.channel_id,
        msg.author.id,
        false,
        lobbies,
        trueskill,
        database,
    )
}

pub fn forceleave(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Outside guild but admin check passed => should never happen
        return Ok(());
    };
    let members = args
        .iter()
        .map(|arg| match Member::parse(ctx, guild_id, arg) {
            Ok(Some(x)) => Ok(x),
            Ok(None) => Err(Error::MemberNotFound(arg.clone())),
            Err(err) => Err(err.into()),
        })
        .collect::<Result<Vec<_>>>()?;
    for member in members {
        leave_internal(
            ctx,
            msg.channel_id,
            member.user.id,
            true,
            lobbies,
            trueskill,
            database,
        )?;
    }
    Ok(())
}

fn leave_internal(
    ctx: &Context,
    channel_id: ChannelId,
    user_id: UserId,
    force: bool,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let players = {
        let lobby = lobbies
            .get_mut(&channel_id)
            .ok_or(Error::NotALobby(channel_id))?;
        lobby.leave(user_id, force)?;
        ctx.send_message(channel_id, |m| {
            m.embed(|e| {
                e.description(format!(
                    "[{}/{}] {} left the queue.",
                    lobby.len(),
                    lobby.capacity(),
                    user_id.mention()
                ))
            })
        })?;
        if lobby.len() == lobby.capacity() {
            Some(lobby.clear().into_keys().collect())
        } else {
            None
        }
    };
    if let Some(players) = players {
        start_game(ctx, channel_id, lobbies, players, trueskill, database)?;
    }
    Ok(())
}

pub fn players(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let players = {
        let lobby = lobbies
            .get_mut(&msg.channel_id)
            .ok_or(Error::NotALobby(msg.channel_id))?;
        let x = args
            .iter()
            .next()
            .ok_or(Error::NotEnoughArguments)?
            .parse::<usize>()?;
        lobby.set_capacity(2 * x);
        ctx.send_message(msg.channel_id, |m| {
            m.embed(|e| e.description(format!("Players per team set to {}.", x)))
        })?;
        if lobby.len() == lobby.capacity() {
            Some(lobby.clear().into_keys().collect())
        } else {
            None
        }
    };
    if let Some(players) = players {
        start_game(ctx, msg.channel_id, lobbies, players, trueskill, database)?;
    }
    Ok(())
}

fn start_game(
    ctx: &Context,
    channel_id: ChannelId,
    lobbies: &mut Lobbies,
    players: Vec<UserId>,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let players = players
        .into_iter()
        .map(|x| {
            (
                x,
                lobbies[&channel_id]
                    .ratings()
                    .get(&x)
                    .map(|x| x.rating())
                    .unwrap_or_else(|| trueskill.create_rating()),
            )
        })
        .collect::<Vec<_>>();
    let teams = utils::balance(&players);
    let mut game = Game::create(
        teams[0].iter().map(|x| x.0.into()).collect(),
        teams[1].iter().map(|x| x.0.into()).collect(),
        Utc::now(),
    );
    database.insert_game(&mut game, channel_id)?;
    let f = |users: &[(UserId, _)]| {
        users
            .iter()
            .map(|x| x.0.mention())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let title = format!("Game {} started", game.id());
    let description = format!("Team 1:\n{}\n\nTeam 2:\n{}", f(&teams[0]), f(&teams[1]));
    ctx.send_message(channel_id, |m| {
        m.embed(|e| {
            e.title(title)
                .description(description)
                .timestamp(game.datetime())
        })
    })?;
    for (&channel_id, lobby) in lobbies.iter_mut() {
        for (user_id, _) in players.iter() {
            if lobby.leave(*user_id, true).is_ok() {
                ctx.send_message(channel_id, |m| {
                    m.embed(|e| {
                        e.description(format!(
                            "[{}/{}] {} left the queue (Game started).",
                            lobby.len(),
                            lobby.capacity(),
                            user_id.mention(),
                        ))
                    })
                })?;
            }
        }
    }
    Ok(())
}

pub fn freeze(ctx: &Context, msg: &Message, roles: &Roles, lobbies: &mut Lobbies) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    lobby.freeze();
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Queue frozen."))
    })?;
    Ok(())
}

pub fn unfreeze(ctx: &Context, msg: &Message, roles: &Roles, lobbies: &mut Lobbies) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    lobby.unfreeze();
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Queue unfrozen."))
    })?;
    Ok(())
}

pub fn queue(ctx: &Context, msg: &Message, lobbies: &Lobbies) -> Result {
    let lobby = lobbies
        .get(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    let description = lobby
        .queue()
        .keys()
        .map(|x| x.mention())
        .collect::<Vec<_>>()
        .join("\n");
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| {
            e.title(format!("Queue [{}/{}]", lobby.len(), lobby.capacity()))
                .description(description)
        })
    })?;
    Ok(())
}

pub fn score(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    initial_ratings: &HashMap<u64, f64>,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Outside guild but admin check passed => should never happen
        return Ok(());
    };
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    if args.len() < 2 {
        return Err(Error::NotEnoughArguments);
    }
    let game_id = args[0].parse()?;
    let score = match args[1].to_lowercase().as_ref() {
        "1" => Score::Team1,
        "2" => Score::Team2,
        "draw" | "d" => Score::Draw,
        _ => return Err(Error::BadArgument),
    };
    let mut game = match database.get_game(msg.channel_id.0, game_id) {
        Ok(game) => game,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::GameNotFound(game_id)),
        Err(err) => return Err(err.into()),
    };
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    game.set_score(score);
    database.update_game(&game, msg.channel_id)?;
    let games = database
        .get_games()?
        .remove(&msg.channel_id.0)
        .unwrap_or_default();
    let ratings = Ratings::from_games(&games, initial_ratings, trueskill);
    lobby.set_ratings(ratings);
    let mut ratings = Vec::new();
    for (&user_id, player_info) in lobby.ratings().iter() {
        if let Some(member) = ctx.member(guild_id, user_id)? {
            if member.roles.contains(&roles.ranked.into()) {
                ratings.push((user_id, player_info.rating()));
            }
        }
    }
    ratings.sort_by(|a, b| b.1.mean().partial_cmp(&a.1.mean()).unwrap());
    if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
        for &message in messages.iter() {
            ctx.webhook_delete_message(*webhook_id, webhook_token, message)
                .ok(); // Maybe it was already deleted
        }
        messages.clear();
        let pages = (ratings.len() + 19) / 20;
        for (i, chunk) in ratings.chunks(20).enumerate() {
            let description = chunk
                .iter()
                .enumerate()
                .map(|(j, (user_id, rating))| {
                    format!(
                        "{}: {} - ***{:.0}*** ± {:.0}",
                        20 * i + j + 1,
                        user_id.mention(),
                        rating.mean(),
                        2.0 * rating.variance().sqrt()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let message = ctx.execute_webhook(*webhook_id, webhook_token, true, |m| {
                m.embed(|e| {
                    e.description(description)
                        .title(format!("Leaderboard ({}/{})", i + 1, pages))
                })
            })?;
            if let Some(message) = message {
                messages.push(message.id);
            }
        }
    }
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Game updated"))
    })?;
    Ok(())
}

pub fn cancel(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &Lobbies,
    database: &Database,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let game_id = args[0].parse()?;
    let mut game = database.get_game(msg.channel_id.into(), game_id)?;
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    game.set_score(Score::Cancelled);
    database.update_game(&game, msg.channel_id)?;
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Game cancelled."))
    })?;
    Ok(())
}

pub fn undo(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
    initial_ratings: &HashMap<u64, f64>,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Outside guild but admin check passed => should never happen
        return Ok(());
    };
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let game_id = args[0].parse()?;
    let mut game = match database.get_game(msg.channel_id.0, game_id) {
        Ok(game) => game,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::GameNotFound(game_id)),
        Err(err) => return Err(err.into()),
    };
    let prev_score = game.score();
    game.set_score(Score::Undecided);
    database.update_game(&game, msg.channel_id)?;
    let games = database
        .get_games()?
        .remove(&msg.channel_id.0)
        .unwrap_or_default();
    let ratings = Ratings::from_games(&games, initial_ratings, trueskill);
    lobby.set_ratings(ratings);
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Game updated"))
    })?;
    if prev_score == Score::Cancelled || prev_score == Score::Undecided {
        return Ok(());
    }
    let mut ratings = Vec::new();
    for (&user_id, player_info) in lobby.ratings().iter() {
        if let Some(member) = ctx.member(guild_id, user_id)? {
            if member.roles.contains(&roles.ranked.into()) {
                ratings.push((user_id, player_info.rating()));
            }
        }
    }
    ratings.sort_by(|a, b| b.1.mean().partial_cmp(&a.1.mean()).unwrap());
    if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
        for &message in messages.iter() {
            ctx.webhook_delete_message(*webhook_id, webhook_token, message)
                .ok(); // Maybe it was already deleted
        }
        messages.clear();
        let pages = (ratings.len() + 19) / 20;
        for (i, chunk) in ratings.chunks(20).enumerate() {
            let description = chunk
                .iter()
                .enumerate()
                .map(|(j, (user_id, rating))| {
                    format!(
                        "{}: {} - ***{:.0}*** ± {:.0}",
                        20 * i + j + 1,
                        user_id.mention(),
                        rating.mean(),
                        2.0 * rating.variance().sqrt()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let message = ctx.execute_webhook(*webhook_id, webhook_token, true, |m| {
                m.embed(|e| {
                    e.description(description)
                        .title(format!("Leaderboard ({}/{})", i + 1, pages))
                })
            })?;
            if let Some(message) = message {
                messages.push(message.id);
            }
        }
    }
    Ok(())
}

pub fn gamelist(ctx: &Context, msg: &Message, lobbies: &Lobbies, database: &Database) -> Result {
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
    let games = database
        .get_games()?
        .remove(&msg.channel_id.0)
        .unwrap_or_default();
    let description = games
        .into_iter()
        .rev()
        .take(20)
        .map(|(id, game)| format!("Game {}: {}", id, game.score()))
        .collect::<Vec<_>>()
        .join("\n");
    ctx.send_message(msg.channel_id, |m| m.embed(|e| e.description(description)))?;
    Ok(())
}

pub fn gameinfo(
    ctx: &Context,
    msg: &Message,
    lobbies: &Lobbies,
    database: &Database,
    args: &[String],
) -> Result {
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
    let game_id = args[0].parse()?;
    let game = match database.get_game(msg.channel_id.into(), game_id) {
        Ok(game) => game,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::GameNotFound(game_id)),
        Err(err) => return Err(err.into()),
    };
    let title = format!("Game {}", game.id());
    let f = |users: &[u64]| {
        users
            .iter()
            .map(|x| format!("<@{}>", x))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let description = format!(
        "Team 1:\n{}\n\nTeam 2:\n{}",
        f(game.teams()[0]),
        f(game.teams()[1])
    );
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| {
            e.title(title)
                .description(description)
                .timestamp(game.datetime())
        })
    })?;
    Ok(())
}

pub fn clear(ctx: &Context, msg: &Message, roles: &Roles, lobbies: &mut Lobbies) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    lobby.clear();
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Queue cleared"))
    })?;
    Ok(())
}

pub fn rebalance(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    let mut game = match database.get_games()?.remove(&msg.channel_id.0) {
        Some(games) => {
            if let Some(game) = games.into_iter().last() {
                game.1
            } else {
                return Ok(());
            }
        }
        None => return Ok(()),
    };
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    let players = game
        .teams()
        .into_iter()
        .flatten()
        .map(|x| {
            (
                x,
                lobby
                    .ratings()
                    .get(&(*x).into())
                    .map(|x| x.rating())
                    .unwrap_or_else(|| trueskill.create_rating()),
            )
        })
        .collect::<Vec<_>>();
    let teams = utils::balance(&players);
    let quality = utils::quality(&teams, trueskill);
    let team1 = teams[0].iter().map(|x| x.0).copied().collect::<Vec<_>>();
    let team2 = teams[1].iter().map(|x| x.0).copied().collect::<Vec<_>>();
    game.set_teams([team1, team2]);
    database.update_game(&game, msg.channel_id)?;
    let title = format!("Game {}", game.id());
    let f = |users: &[u64]| {
        users
            .iter()
            .map(|x| format!("<@{}>", x))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let description = format!(
        "Quality: {:.0}\n\nTeam 1:\n{}\n\nTeam 2:\n{}",
        100.0 * quality,
        f(game.teams()[0]),
        f(game.teams()[1])
    );
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| {
            e.title(title)
                .description(description)
                .timestamp(game.datetime())
        })
    })?;
    Ok(())
}

pub fn swap(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &Lobbies,
    database: &Database,
    args: &[String],
) -> Result {
    if !checks::check_admin(ctx, msg, roles)? {
        return Ok(());
    }
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Outside guild but admin check passed => should never happen
        return Ok(());
    };
    let member1 = if let Some(member) = Member::parse(ctx, guild_id, &args[0])? {
        member
    } else {
        return Err(Error::MemberNotFound(args[0].to_owned()));
    };
    let member2 = if let Some(member) = Member::parse(ctx, guild_id, &args[1])? {
        member
    } else {
        return Err(Error::MemberNotFound(args[1].to_owned()));
    };
    let mut game = match database.get_games()?.remove(&msg.channel_id.0) {
        Some(games) => {
            if let Some(game) = games.into_iter().last() {
                game.1
            } else {
                return Ok(());
            }
        }
        None => return Ok(()),
    };
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    let teams = game.teams();
    let mut team1 = teams[0].iter().copied().collect::<HashSet<_>>();
    let mut team2 = teams[1].iter().copied().collect::<HashSet<_>>();
    if team1.contains(&member1.user.id.0) {
        if team1.contains(&member2.user.id.0) {
            return Err(Error::SameTeam);
        } else {
            team1.remove(&member1.user.id.0);
            team1.insert(member2.user.id.0);
            if team2.contains(&member2.user.id.0) {
                team2.remove(&member2.user.id.0);
                team2.insert(member1.user.id.0);
            }
        }
    } else if team2.contains(&member1.user.id.0) {
        if team2.contains(&member2.user.id.0) {
            return Err(Error::SameTeam);
        } else {
            team2.remove(&member1.user.id.0);
            team2.insert(member2.user.id.0);
            if team1.contains(&member2.user.id.0) {
                team1.remove(&member2.user.id.0);
                team1.insert(member1.user.id.0);
            }
        }
    } else {
        return Err(Error::NotPlaying(member1.user.id));
    }
    game.set_teams([
        team1.into_iter().collect::<Vec<_>>(),
        team2.into_iter().collect::<Vec<_>>(),
    ]);
    database.update_game(&game, msg.channel_id)?;
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Players swapped"))
    })?;
    Ok(())
}
