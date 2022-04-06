use harmony::client::Context;
use harmony::model::Message;

use crate::Result;

pub fn ping(ctx: &Context, msg: &Message) -> Result {
    let reply = ctx.create_message(msg.channel_id, |m| m.content("Pong!"))?;
    let duration = reply.timestamp - msg.timestamp;
    ctx.edit_message(reply.channel_id, reply.id, |m| {
        m.content(format!(
            "Pong! That took {} ms.",
            duration.num_milliseconds()
        ))
    })?;
    Ok(())
}
