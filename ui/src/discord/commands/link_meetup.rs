use command_macro::command;
use redis::Commands;
use serenity::model::id::UserId;
use std::borrow::Cow;

#[command]
#[regex(r"link[ -]?meetup")]
fn link_meetup(
    mut context: super::CommandContext,
    _: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    let redis_key_d2m = format!("discord_user:{}:meetup_user", context.msg.author.id.0);
    // Check if there is already a meetup id linked to this user
    // and issue a warning
    let linked_meetup_id: Option<u64> = context.redis_connection()?.get(&redis_key_d2m)?;
    let runtime_lock = context.async_runtime()?.clone();
    let runtime_guard = futures::executor::block_on(runtime_lock.read());
    let async_runtime = match *runtime_guard {
        Some(ref async_runtime) => async_runtime,
        None => return Ok(()),
    };
    if let Some(linked_meetup_id) = linked_meetup_id {
        // Return value of the async block:
        // None = Meetup API unavailable
        // Some(None) = Meetup API available but no user found
        // Some(Some(user)) = User found
        let meetup_user = async_runtime.enter(|| {
            futures::executor::block_on(async {
                let meetup_client = context.meetup_client()?.lock().await.clone();
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
            })
        })?;
        let bot_id = context.bot_id()?;
        match meetup_user {
            Some(meetup_user) => match meetup_user {
                Some(meetup_user) => {
                    let _ = context.msg.author.direct_message(context.ctx, |message| {
                        message.content(lib::strings::DISCORD_ALREADY_LINKED_MESSAGE1(
                            &meetup_user.name,
                            bot_id.0,
                        ))
                    });
                    let _ = context.msg.react(context.ctx, "\u{2705}");
                }
                _ => {
                    let _ = context.msg.author.direct_message(context.ctx, |message| {
                        message.content(lib::strings::NONEXISTENT_MEETUP_LINKED_MESSAGE(bot_id.0))
                    });
                    let _ = context.msg.react(context.ctx, "\u{2705}");
                }
            },
            _ => {
                let _ = context.msg.author.direct_message(context.ctx, |message| {
                    message.content(lib::strings::DISCORD_ALREADY_LINKED_MESSAGE2(bot_id.0))
                });
                let _ = context.msg.react(context.ctx, "\u{2705}");
            }
        }
        return Ok(());
    }
    // TODO: creates a new Redis connection. Not optimal...
    let url = async_runtime.enter(|| {
        futures::executor::block_on(async {
            let user_id = context.msg.author.id.0;
            let async_redis_connection = context.async_redis_connection()?;
            lib::meetup::oauth2::generate_meetup_linking_link(async_redis_connection, user_id).await
        })
    })?;
    let dm = context.msg.author.direct_message(context.ctx, |message| {
        message.content(lib::strings::MEETUP_LINKING_MESSAGE(&url))
    });
    match dm {
        Ok(_) => {
            let _ = context.msg.react(context.ctx, "\u{2705}");
        }
        Err(why) => {
            eprintln!("Error sending Meetup linking DM: {:?}", why);
            let _ = context.msg.reply(
                context.ctx,
                "There was an error trying to send you instructions.\nDo you have direct \
                     messages disabled? In that case send me a private message with the text \
                     \"link meetup\".",
            );
        }
    }
    Ok(())
}

#[command]
#[regex(r"unlink[ -]?meetup")]
fn unlink_meetup(
    mut context: super::CommandContext,
    _: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    let user_id = context.msg.author.id;
    unlink_meetup_impl(&mut context, /*is_bot_admin_command*/ false, user_id)
}

#[command]
#[regex(
    r"link[ -]?meetup\s+{mention_pattern}\s+(?P<meetupid>[0-9]+)",
    mention_pattern
)]
#[level(admin)]
fn link_meetup_bot_admin(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    let discord_id = captures.name("mention_id").unwrap().as_str();
    let meetup_id = captures.name("meetupid").unwrap().as_str();
    let bot_id = context.bot_id()?;
    // Try to convert the specified ID to an integer
    let (discord_id, meetup_id) = match (discord_id.parse::<u64>(), meetup_id.parse::<u64>()) {
        (Ok(id1), Ok(id2)) => (id1, id2),
        _ => {
            let _ = context.msg.channel_id.say(
                context.ctx,
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
    let linked_meetup_id: Option<u64> = context.redis_connection()?.get(&redis_key_d2m)?;
    if let Some(linked_meetup_id) = linked_meetup_id {
        if linked_meetup_id == meetup_id {
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!(
                    "All good, this Meetup account was already linked to <@{}>",
                    discord_id
                ),
            );
            return Ok(());
        } else {
            // TODO: answer in DM?
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!(
                    "<@{discord_id}> is already linked to a different Meetup account. If you \
                     want to change this, unlink the currently linked Meetup account first by \
                     writing:\n<@{bot_id}> unlink meetup <@{discord_id}>",
                    discord_id = discord_id,
                    bot_id = bot_id
                ),
            );
            return Ok(());
        }
    }
    // Check if this meetup id is already linked
    // and issue a warning
    let linked_discord_id: Option<u64> = context.redis_connection()?.get(&redis_key_m2d)?;
    if let Some(linked_discord_id) = linked_discord_id {
        let _ = context
            .msg
            .author
            .direct_message(context.ctx, |message_builder| {
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
    let runtime_mutex = context.async_runtime()?.clone();
    let runtime_guard = futures::executor::block_on(runtime_mutex.read());
    let async_runtime = match *runtime_guard {
        Some(ref async_runtime) => async_runtime,
        None => return Ok(()),
    };
    let meetup_user = async_runtime.enter(|| {
        futures::executor::block_on(async {
            let meetup_client = context.meetup_client()?.lock().await.clone();
            match meetup_client {
                None => {
                    return Err(lib::meetup::Error::from(simple_error::SimpleError::new(
                        "Meetup API unavailable",
                    )))
                }
                Some(meetup_client) => meetup_client
                    .get_member_profile(Some(meetup_id))
                    .await
                    .map_err(Into::into),
            }
        })
    })?;
    drop(runtime_guard);
    match meetup_user {
        None => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                "It looks like this Meetup profile does not exist",
            );
            return Ok(());
        }
        Some(meetup_user) => {
            let mut successful = false;
            // Try to atomically set the meetup id
            let redis_connection = context.redis_connection()?;
            redis::transaction(
                redis_connection,
                &[&redis_key_d2m, &redis_key_m2d],
                |con, pipe| {
                    let linked_meetup_id: Option<u64> = con.get(&redis_key_d2m)?;
                    let linked_discord_id: Option<u64> = con.get(&redis_key_m2d)?;
                    if linked_meetup_id.is_some() || linked_discord_id.is_some() {
                        // The meetup id was linked in the meantime, abort
                        successful = false;
                        // Execute empty transaction just to get out of the closure
                        pipe.query(con)
                    } else {
                        pipe.sadd("meetup_users", meetup_id)
                            .sadd("discord_users", discord_id)
                            .set(&redis_key_d2m, meetup_id)
                            .set(&redis_key_m2d, discord_id);
                        successful = true;
                        pipe.query(con)
                    }
                },
            )?;
            if successful {
                let photo_url = meetup_user.photo.as_ref().map(|p| p.thumb_link.as_str());
                let _ = context.msg.channel_id.send_message(context.ctx, |message| {
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
                    .say(context.ctx, "Could not assign meetup id (timing error)");
                return Ok(());
            }
        }
    }
}

#[command]
#[regex(r"unlink[ -]?meetup\s+{mention_pattern}", mention_pattern)]
#[level(admin)]
fn unlink_meetup_bot_admin(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => UserId(id),
        _ => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                "Seems like the specified Discord ID is invalid",
            );
            return Ok(());
        }
    };
    unlink_meetup_impl(&mut context, /*is_bot_admin_command*/ true, discord_id)
}

fn unlink_meetup_impl(
    context: &mut super::CommandContext,
    is_bot_admin_command: bool,
    user_id: UserId,
) -> Result<(), lib::meetup::Error> {
    let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
    // Check if there is actually a meetup id linked to this user
    let linked_meetup_id: Option<u64> = context.redis_connection()?.get(&redis_key_d2m)?;
    match linked_meetup_id {
        Some(meetup_id) => {
            let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_id);
            context
                .redis_connection()?
                .del(&[&redis_key_d2m, &redis_key_m2d])?;
            let message = if is_bot_admin_command {
                format!("Unlinked <@{}>'s Meetup account", user_id)
            } else {
                lib::strings::MEETUP_UNLINK_SUCCESS(context.bot_id()?.0)
            };
            let _ = context.msg.channel_id.say(context.ctx, message);
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
            let _ = context.msg.channel_id.say(context.ctx, message);
        }
    }
    Ok(())
}
