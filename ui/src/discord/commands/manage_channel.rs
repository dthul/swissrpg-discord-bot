use command_macro::command;
use redis::Commands;
use serenity::model::{
    channel::{Channel, PermissionOverwrite, PermissionOverwriteType},
    permissions::Permissions,
};

#[command]
#[regex(r"manage\s*channel")]
#[level(admin)]
#[help(
    "manage channel",
    "Enable the `[add|remove] user` and `[add|remove] host` commands for this channel."
)]
fn manage_channel(
    mut context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    let channel_id = context.msg.channel_id;
    // Step 1: Try to mark this channel as managed
    let mut is_game_channel = false;
    redis::transaction(
        context.redis_connection()?,
        &["discord_channels"],
        |con, pipe| {
            // Make sure that this is not a game channel
            is_game_channel = con.sismember("discord_channels", channel_id.0)?;
            if is_game_channel {
                // Do nothing
                pipe.query(con)
            } else {
                // Mark as managed channel
                pipe.sadd("managed_discord_channels", channel_id.0)
                    .query(con)
            }
        },
    )?;
    if is_game_channel {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, "Can not manage this channel");
        return Ok(());
    }
    let channel = if let Some(Channel::Guild(channel)) = context.msg.channel(context.ctx) {
        channel.clone()
    } else {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, "Can not manage this channel");
        return Ok(());
    };
    // Step 2: Grant the bot continued access to the channel
    channel.read().create_permission(
        context.ctx,
        &PermissionOverwrite {
            allow: Permissions::READ_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(context.bot_id()?),
        },
    )?;
    // Step 3: Grant all current users access to the channel
    let mut current_channel_members = channel.read().members(context.ctx)?;
    for member in &mut current_channel_members {
        // Don't explicitly grant access to admins
        let is_admin = {
            if let Ok(member_permissions) = member.permissions(context.ctx) {
                member_permissions.administrator()
            } else {
                false
            }
        };
        if is_admin {
            continue;
        }
        let user_id = member.user.read().id;
        channel.read().create_permission(
            context.ctx,
            &PermissionOverwrite {
                allow: Permissions::READ_MESSAGES,
                deny: Permissions::empty(),
                kind: PermissionOverwriteType::Member(user_id),
            },
        )?;
    }
    let _ = context.msg.react(context.ctx, "\u{2705}");
    Ok(())
}
