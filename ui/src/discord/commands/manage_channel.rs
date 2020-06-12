use command_macro::command;
use futures::FutureExt;
use redis::AsyncCommands;
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
fn manage_channel<'a>(
    mut context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let channel_id = context.msg.channel_id;
    // Step 1: Try to mark this channel as managed
    let mut is_game_channel = false;
    lib::redis::async_redis_transaction(
        context.async_redis_connection().await?,
        &["discord_channels"],
        |con, mut pipe| {
            async move {
                // Make sure that this is not a game channel
                is_game_channel = con.sismember("discord_channels", channel_id.0).await?;
                if is_game_channel {
                    // Do nothing
                    pipe.query_async(con).await
                } else {
                    // Mark as managed channel
                    pipe.sadd("managed_discord_channels", channel_id.0)
                        .query_async(con)
                        .await
                }
            }
            .boxed()
        },
    )
    .await?;
    if is_game_channel {
        context
            .msg
            .channel_id
            .say(context.ctx, "Can not manage this channel")
            .await
            .ok();
        return Ok(());
    }
    let channel = if let Some(Channel::Guild(channel)) = context.msg.channel(context.ctx).await {
        channel.clone()
    } else {
        context
            .msg
            .channel_id
            .say(context.ctx, "Can not manage this channel")
            .await
            .ok();
        return Ok(());
    };
    // Step 2: Grant the bot continued access to the channel
    channel
        .create_permission(
            context.ctx,
            &PermissionOverwrite {
                allow: Permissions::READ_MESSAGES,
                deny: Permissions::empty(),
                kind: PermissionOverwriteType::Member(context.bot_id()?),
            },
        )
        .await?;
    // Step 3: Grant all current users access to the channel
    let mut current_channel_members = channel.members(context.ctx).await?;
    for member in &mut current_channel_members {
        // Don't explicitly grant access to admins
        let is_admin = {
            if let Ok(member_permissions) = member.permissions(context.ctx).await {
                member_permissions.administrator()
            } else {
                false
            }
        };
        if is_admin {
            continue;
        }
        channel
            .create_permission(
                context.ctx,
                &PermissionOverwrite {
                    allow: Permissions::READ_MESSAGES,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Member(member.user.id),
                },
            )
            .await?;
    }
    context.msg.react(&context.ctx, '\u{2705}').await.ok();
    Ok(())
}
