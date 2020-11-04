use command_macro::command;
use futures_util::FutureExt;
use redis::AsyncCommands;
use serenity::model::id::UserId;
use std::{borrow::Cow, sync::Arc};

#[command]
#[regex(r"link[ -]?meetup")]
#[help("link meetup", "starts the process to link your Meetup and Discord profiles. If you haven't yet, you should really do that now.")]
fn link_meetup<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let redis_key_d2m = format!("discord_user:{}:meetup_user", context.msg.author.id.0);
    // Check if there is already a meetup id linked to this user
    // and issue a warning
    let linked_meetup_id: Option<u64> = context
        .async_redis_connection()
        .await?
        .get(&redis_key_d2m)
        .await?;
    if let Some(linked_meetup_id) = linked_meetup_id {
        // Return value of the async block:
        // None = Meetup API unavailable
        // Some(None) = Meetup API available but no user found
        // Some(Some(user)) = User found
        let meetup_user = {
            let meetup_client = context.meetup_client().await?.lock().await.clone();
            match meetup_client {
                None => Ok::<_, lib::meetup::Error>(None),
                Some(meetup_client) => match meetup_client
                    .get_member_profile(Some(linked_meetup_id))
                    .await?
                {
                    None => Ok(Some(None)),
                    Some(user) => Ok(Some(Some(user))),
                },
            }
        }?;
        let bot_id = context.bot_id().await?;
        match meetup_user {
            Some(meetup_user) => match meetup_user {
                Some(meetup_user) => {
                    context
                        .msg
                        .author
                        .direct_message(&context.ctx, |message| {
                            message.content(lib::strings::DISCORD_ALREADY_LINKED_MESSAGE1(
                                &meetup_user.name,
                                bot_id.0,
                            ))
                        })
                        .await
                        .ok();
                    context.msg.react(&context.ctx, '\u{2705}').await.ok();
                }
                _ => {
                    context
                        .msg
                        .author
                        .direct_message(&context.ctx, |message| {
                            message
                                .content(lib::strings::NONEXISTENT_MEETUP_LINKED_MESSAGE(bot_id.0))
                        })
                        .await
                        .ok();
                    context.msg.react(&context.ctx, '\u{2705}').await.ok();
                }
            },
            _ => {
                context
                    .msg
                    .author
                    .direct_message(&context.ctx, |message| {
                        message.content(lib::strings::DISCORD_ALREADY_LINKED_MESSAGE2(bot_id.0))
                    })
                    .await
                    .ok();
                context.msg.react(&context.ctx, '\u{2705}').await.ok();
            }
        }
        return Ok(());
    };
    let user_id = context.msg.author.id.0;
    let url = lib::meetup::oauth2::generate_meetup_linking_link(
        context.async_redis_connection().await?,
        user_id,
    )
    .await?;
    let dm = context
        .msg
        .author
        .direct_message(&context.ctx, |message| {
            message.content(lib::strings::MEETUP_LINKING_MESSAGE(&url))
        })
        .await;
    match dm {
        Ok(_) => {
            context.msg.react(&context.ctx, '\u{2705}').await.ok();
        }
        Err(why) => {
            eprintln!("Error sending Meetup linking DM: {:?}", why);
            context.msg.reply(
                &context.ctx,
                "There was an error trying to send you instructions.\nDo you have direct messages \
                 disabled? In that case send me a private message with the text \"link meetup\".",
            ).await.ok();
        }
    }
    Ok(())
}

#[command]
#[regex(r"unlink[ -]?meetup")]
#[help("unlink meetup", "unlinks your Meetup and Discord profiles.")]
fn unlink_meetup<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let user_id = context.msg.author.id;
    unlink_meetup_impl(context, /*is_bot_admin_command*/ false, user_id).await
}

#[command]
#[regex(
    r"link[ -]?meetup\s+{mention_pattern}\s+(?P<meetupid>[0-9]+)",
    mention_pattern
)]
#[level(admin)]
#[help(
    "link meetup `@some-user` `meetup-ID`",
    "link another user's Meetup and Discord profile."
)]
fn link_meetup_bot_admin<'a>(
    context: &'a mut super::CommandContext,
    captures: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let discord_id = captures.name("mention_id").unwrap().as_str();
    let meetup_id = captures.name("meetupid").unwrap().as_str();
    let bot_id = context.bot_id().await?;
    // Try to convert the specified ID to an integer
    let (discord_id, meetup_id) = match (discord_id.parse::<u64>(), meetup_id.parse::<u64>()) {
        (Ok(id1), Ok(id2)) => (id1, id2),
        _ => {
            let _ = context.msg.channel_id.say(
                &context.ctx,
                "Seems like the specified Discord or Meetup ID is invalid",
            );
            return Ok(());
        }
    };
    let redis_key_d2m = format!("discord_user:{}:meetup_user", discord_id);
    let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_id);
    // let redis_connection = redis_client.get_connection()?;
    // Check if there is already a meetup id linked to this user
    // and issue a warning
    let linked_meetup_id: Option<u64> = context
        .async_redis_connection()
        .await?
        .get(&redis_key_d2m)
        .await?;
    if let Some(linked_meetup_id) = linked_meetup_id {
        if linked_meetup_id == meetup_id {
            context
                .msg
                .channel_id
                .say(
                    &context.ctx,
                    format!(
                        "All good, this Meetup account was already linked to <@{}>",
                        discord_id
                    ),
                )
                .await
                .ok();
            return Ok(());
        } else {
            // TODO: answer in DM?
            context
                .msg
                .channel_id
                .say(
                    &context.ctx,
                    format!(
                    "<@{discord_id}> is already linked to a different Meetup account. If you want \
                     to change this, unlink the currently linked Meetup account first by \
                     writing:\n<@{bot_id}> unlink meetup <@{discord_id}>",
                    discord_id = discord_id,
                    bot_id = bot_id
                ),
                )
                .await
                .ok();
            return Ok(());
        }
    }
    // Check if this meetup id is already linked
    // and issue a warning
    let linked_discord_id: Option<u64> = context
        .async_redis_connection()
        .await?
        .get(&redis_key_m2d)
        .await?;
    if let Some(linked_discord_id) = linked_discord_id {
        let _ = context
            .msg
            .author
            .direct_message(&context.ctx, |message_builder| {
                message_builder.content(format!(
                    "This Meetup account is alread linked to <@{linked_discord_id}>. If you want \
                     to change this, unlink the Meetup account first by writing\n<@{bot_id}> \
                     unlink meetup <@{linked_discord_id}>",
                    linked_discord_id = linked_discord_id,
                    bot_id = bot_id
                ))
            });
        return Ok(());
    }
    // The user has not yet linked their meetup account.
    // Test whether the specified Meetup user actually exists.
    let meetup_client = context.meetup_client().await?.lock().await.clone();
    let meetup_user = match meetup_client {
        None => {
            return Err(lib::meetup::Error::from(simple_error::SimpleError::new(
                "Meetup API unavailable",
            )))
        }
        Some(meetup_client) => meetup_client.get_member_profile(Some(meetup_id)).await?,
    };
    match meetup_user {
        None => {
            let _ = context.msg.channel_id.say(
                &context.ctx,
                "It looks like this Meetup profile does not exist",
            );
            return Ok(());
        }
        Some(meetup_user) => {
            let successful = Arc::new(std::sync::atomic::AtomicBool::new(false));
            // Try to atomically set the meetup id
            let redis_connection = context.async_redis_connection().await?;
            lib::redis::async_redis_transaction(
                redis_connection,
                &[&redis_key_d2m, &redis_key_m2d],
                |con, mut pipe| {
                    let redis_key_d2m = redis_key_d2m.clone();
                    let redis_key_m2d = redis_key_m2d.clone();
                    let successful = Arc::clone(&successful);
                    async move {
                        let linked_meetup_id: Option<u64> = con.get(&redis_key_d2m).await?;
                        let linked_discord_id: Option<u64> = con.get(&redis_key_m2d).await?;
                        if linked_meetup_id.is_some() || linked_discord_id.is_some() {
                            // The meetup id was linked in the meantime, abort
                            successful.store(false, std::sync::atomic::Ordering::Release);
                            // Execute empty transaction just to get out of the closure
                            pipe.query_async(con).await
                        } else {
                            pipe.sadd("meetup_users", meetup_id)
                                .sadd("discord_users", discord_id)
                                .set(&redis_key_d2m, meetup_id)
                                .set(&redis_key_m2d, discord_id);
                            successful.store(true, std::sync::atomic::Ordering::Release);
                            pipe.query_async(con).await
                        }
                    }
                    .boxed()
                },
            )
            .await?;
            if successful.load(std::sync::atomic::Ordering::Acquire) {
                let photo_url = meetup_user.photo.as_ref().map(|p| p.thumb_link.as_str());
                let _ = context
                    .msg
                    .channel_id
                    .send_message(&context.ctx, |message| {
                        message.embed(|embed| {
                            embed.title("Linked Meetup account");
                            embed.description(format!(
                                "Successfully linked <@{}> to {}'s Meetup account",
                                discord_id, meetup_user.name
                            ));
                            if let Some(photo_url) = photo_url {
                                embed.image(photo_url)
                            } else {
                                embed
                            }
                        })
                    });
                return Ok(());
            } else {
                let _ = context
                    .msg
                    .channel_id
                    .say(&context.ctx, "Could not assign meetup id (timing error)");
                return Ok(());
            }
        }
    }
}

#[command]
#[regex(r"unlink[ -]?meetup\s+{mention_pattern}", mention_pattern)]
#[level(admin)]
#[help(
    "unlink meetup `@some-user`",
    "unlink another user's Meetup and Discord profile."
)]
fn unlink_meetup_bot_admin<'a>(
    context: &'a mut super::CommandContext,
    captures: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => UserId(id),
        _ => {
            let _ = context.msg.channel_id.say(
                &context.ctx,
                "Seems like the specified Discord ID is invalid",
            );
            return Ok(());
        }
    };
    unlink_meetup_impl(context, /*is_bot_admin_command*/ true, discord_id).await
}

async fn unlink_meetup_impl(
    context: &mut super::CommandContext,
    is_bot_admin_command: bool,
    user_id: UserId,
) -> Result<(), lib::meetup::Error> {
    let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
    // Check if there is actually a meetup id linked to this user
    let linked_meetup_id: Option<u64> = context
        .async_redis_connection()
        .await?
        .get(&redis_key_d2m)
        .await?;
    match linked_meetup_id {
        Some(meetup_id) => {
            let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_id);
            context
                .async_redis_connection()
                .await?
                .del(&[&redis_key_d2m, &redis_key_m2d])
                .await?;
            let message = if is_bot_admin_command {
                format!("Unlinked <@{}>'s Meetup account", user_id)
            } else {
                lib::strings::MEETUP_UNLINK_SUCCESS(context.bot_id().await?.0)
            };
            context.msg.channel_id.say(&context.ctx, message).await.ok();
        }
        None => {
            let message = if is_bot_admin_command {
                Cow::Owned(format!(
                    "There was seemingly no meetup account linked to <@{}>",
                    user_id
                ))
            } else {
                Cow::Borrowed(lib::strings::MEETUP_UNLINK_NOT_LINKED)
            };
            context.msg.channel_id.say(&context.ctx, message).await.ok();
        }
    }
    Ok(())
}
