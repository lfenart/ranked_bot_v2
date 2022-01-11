use harmony::client::Context;
use harmony::model::id::{ChannelId, UserId};
use harmony::model::{Channel, Member, Message};

use crate::checks;
use crate::model::Lobbies;
use crate::{Error, Result};
use crate::config::Roles;

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
                    "Rating: {:.0}\nWins: {}\nLosses: {}\nDraws: {}",
                    player_info.rating().mean(),
                    player_info.wins(),
                    player_info.losses(),
                    player_info.draws()
                ))
            })
        })?;
    } else {
        ctx.send_message(msg.channel_id, |m| {
            m.embed(|e| e.description("No info yet, play more!"))
        })?;
    }
    Ok(())
}
