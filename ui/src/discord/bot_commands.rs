use futures_util::lock::Mutex as AsyncMutex;
use lib::strings;
use redis::Commands;
use regex::Regex;
use serenity::{
    model::{
        channel,
        channel::{Channel, Message, PermissionOverwrite, PermissionOverwriteType},
        id::UserId,
        user::User,
        Permissions,
    },
    prelude::*,
};
use simple_error::SimpleError;
use std::{borrow::Cow, collections::HashMap, sync::Arc};

pub struct Regexes {
    pub bot_mention: Regex,
    // pub link_meetup_dm: Regex,
    // pub link_meetup_mention: Regex,
    // pub link_meetup_bot_admin_dm: Regex,
    // pub link_meetup_bot_admin_mention: Regex,
    // pub unlink_meetup_dm: Regex,
    // pub unlink_meetup_mention: Regex,
    // pub unlink_meetup_bot_admin_dm: Regex,
    // pub unlink_meetup_bot_admin_mention: Regex,
    // pub sync_meetup_mention: Regex,
    // pub sync_discord_mention: Regex,
    // pub add_user_bot_admin_mention: Regex,
    // pub add_host_bot_admin_mention: Regex,
    // pub remove_user_mention: Regex,
    // pub remove_host_bot_admin_mention: Regex,
    // pub stop_bot_admin_dm: Regex,
    // pub stop_bot_admin_mention: Regex,
    // pub send_expiration_reminder_bot_admin_mention: Regex,
    // pub end_adventure_host_mention: Regex,
    // pub help_dm: Regex,
    // pub help_mention: Regex,
    // pub refresh_user_token_admin_dm: Regex,
    // pub rsvp_user_admin_mention: Regex,
    // pub clone_event_admin_mention: Regex,
    // pub schedule_session_mention: Regex,
    // pub whois_bot_admin_dm: Regex,
    // pub whois_bot_admin_mention: Regex,
    // pub list_players_mention: Regex,
    // pub list_stripe_subscriptions: Regex,
    // pub sync_stripe_subscriptions: Regex,
    // pub num_cached_members: Regex,
    // pub manage_channel_mention: Regex,
    // pub mention_channel_role_mention: Regex,
    // pub snooze_reminders: Regex,
    // pub list_inactive_users: Regex,
}

// impl Regexes {
//     pub fn link_meetup(&self, is_dm: bool) -> &Regex {
//         if is_dm {
//             &self.link_meetup_dm
//         } else {
//             &self.link_meetup_mention
//         }
//     }

//     pub fn link_meetup_bot_admin(&self, is_dm: bool) -> &Regex {
//         if is_dm {
//             &self.link_meetup_bot_admin_dm
//         } else {
//             &self.link_meetup_bot_admin_mention
//         }
//     }

//     pub fn unlink_meetup(&self, is_dm: bool) -> &Regex {
//         if is_dm {
//             &self.unlink_meetup_dm
//         } else {
//             &self.unlink_meetup_mention
//         }
//     }

//     pub fn unlink_meetup_bot_admin(&self, is_dm: bool) -> &Regex {
//         if is_dm {
//             &self.unlink_meetup_bot_admin_dm
//         } else {
//             &self.unlink_meetup_bot_admin_mention
//         }
//     }

//     pub fn stop_bot_admin(&self, is_dm: bool) -> &Regex {
//         if is_dm {
//             &self.stop_bot_admin_dm
//         } else {
//             &self.stop_bot_admin_mention
//         }
//     }

//     pub fn whois_bot_admin(&self, is_dm: bool) -> &Regex {
//         if is_dm {
//             &self.whois_bot_admin_dm
//         } else {
//             &self.whois_bot_admin_mention
//         }
//     }

//     pub fn help(&self, is_dm: bool) -> &Regex {
//         if is_dm {
//             &self.help_dm
//         } else {
//             &self.help_mention
//         }
//     }
// }

pub fn compile_regexes(bot_id: u64, bot_name: &str) -> Regexes {
    let bot_mention = format!(
        r"^\s*(?:<@!?{bot_id}>|(@|#)(?i){bot_name})",
        bot_id = bot_id,
        bot_name = regex::escape(bot_name)
    );
    // let link_meetup_dm = r"^(?i)link[ -]?meetup\s*$";
    // let link_meetup_mention = format!(
    //     r"^{bot_mention}\s+(?i)link[ -]?meetup\s*$",
    //     bot_mention = bot_mention
    // );
    // let link_meetup_bot_admin = format!(
    //     r"(?i)link[ -]?meetup\s+{mention_pattern}\s+(?P<meetupid>[0-9]+)",
    //     mention_pattern = MENTION_PATTERN
    // );
    // let link_meetup_bot_admin_dm = format!(
    //     r"^{link_meetup_bot_admin}\s*$",
    //     link_meetup_bot_admin = link_meetup_bot_admin
    // );
    // let link_meetup_bot_admin_mention = format!(
    //     r"^{bot_mention}\s+{link_meetup_bot_admin}\s*$",
    //     bot_mention = bot_mention,
    //     link_meetup_bot_admin = link_meetup_bot_admin
    // );
    // let unlink_meetup = r"(?i)unlink[ -]?meetup";
    // let unlink_meetup_dm = format!(r"^{unlink_meetup}\s*$", unlink_meetup = unlink_meetup);
    // let unlink_meetup_mention = format!(
    //     r"^{bot_mention}\s+{unlink_meetup}\s*$",
    //     bot_mention = bot_mention,
    //     unlink_meetup = unlink_meetup
    // );
    // let unlink_meetup_bot_admin = format!(
    //     r"(?i)unlink[ -]?meetup\s+{mention_pattern}",
    //     mention_pattern = MENTION_PATTERN
    // );
    // let unlink_meetup_bot_admin_dm = format!(
    //     r"^{unlink_meetup_bot_admin}\s*$",
    //     unlink_meetup_bot_admin = unlink_meetup_bot_admin
    // );
    // let unlink_meetup_bot_admin_mention = format!(
    //     r"^{bot_mention}\s+{unlink_meetup_bot_admin}\s*$",
    //     bot_mention = bot_mention,
    //     unlink_meetup_bot_admin = unlink_meetup_bot_admin
    // );
    // let sync_meetup_mention = format!(
    //     r"^{bot_mention}\s+(?i)sync\s+meetup\s*$",
    //     bot_mention = bot_mention
    // );
    // let sync_discord_mention = format!(
    //     r"^{bot_mention}\s+(?i)sync\s+discord\s*$",
    //     bot_mention = bot_mention
    // );
    // let add_user_bot_admin_mention = format!(
    //     r"^{bot_mention}\s+(?i)add\s+{mention_pattern}\s*$",
    //     bot_mention = bot_mention,
    //     mention_pattern = MENTION_PATTERN,
    // );
    // let add_host_bot_admin_mention = format!(
    //     r"^{bot_mention}\s+(?i)add\s+host\s+{mention_pattern}\s*$",
    //     bot_mention = bot_mention,
    //     mention_pattern = MENTION_PATTERN,
    // );
    // let remove_user_mention = format!(
    //     r"^{bot_mention}\s+(?i)remove\s+{mention_pattern}\s*$",
    //     bot_mention = bot_mention,
    //     mention_pattern = MENTION_PATTERN,
    // );
    // let remove_host_bot_admin_mention = format!(
    //     r"^{bot_mention}\s+(?i)remove\s+host\s+{mention_pattern}\s*$",
    //     bot_mention = bot_mention,
    //     mention_pattern = MENTION_PATTERN,
    // );
    // let stop_bot_admin_dm = r"^(?i)stop\s*$";
    // let stop_bot_admin_mention =
    //     format!(r"^{bot_mention}\s+(?i)stop\s*$", bot_mention = bot_mention);
    // let send_expiration_reminder_bot_admin_mention = format!(
    //     r"^{bot_mention}\s+(?i)remind\s+expiration\s*$",
    //     bot_mention = bot_mention
    // );
    // let end_adventure_host_mention = format!(
    //     r"^{bot_mention}\s+(?i)end\s+adventure\s*$",
    //     bot_mention = bot_mention
    // );
    // let help_dm = r"^(?i)help\s*$";
    // let help_mention = format!(r"^{bot_mention}\s+(?i)help\s*$", bot_mention = bot_mention);
    // let refresh_user_token_admin_dm = format!(
    //     r"^{bot_mention}\s+(?i)refresh\s+meetup(-|\s*)token\s+{mention_pattern}\s*$",
    //     bot_mention = bot_mention,
    //     mention_pattern = MENTION_PATTERN
    // );
    // let rsvp_user_admin_mention = format!(
    //     r"^{bot_mention}\s+(?i)rsvp\s+{mention_pattern}\s+(?P<meetup_event_id>[^\s]+)\s*$",
    //     bot_mention = bot_mention,
    //     mention_pattern = MENTION_PATTERN,
    // );
    // let clone_event_admin_mention = format!(
    //     r"^{bot_mention}\s+(?i)clone\s+event\s+(?P<meetup_event_id>[^\s]+)\s*$",
    //     bot_mention = bot_mention,
    // );
    // let schedule_session_mention = format!(
    //     r"^{bot_mention}\s+(?i)schedule\s+session\s*$",
    //     bot_mention = bot_mention,
    // );
    // let whois_bot_admin = format!(
    //     r"(?i)whois\s+{mention_pattern}|{username_tag_pattern}|{meetup_id_pattern}",
    //     mention_pattern = MENTION_PATTERN,
    //     username_tag_pattern = USERNAME_TAG_PATTERN,
    //     meetup_id_pattern = MEETUP_ID_PATTERN,
    // );
    // let whois_bot_admin_dm = format!(r"^{whois_bot_admin}\s*$", whois_bot_admin = whois_bot_admin);
    // let whois_bot_admin_mention = format!(
    //     r"^{bot_mention}\s+{whois_bot_admin}\s*$",
    //     bot_mention = bot_mention,
    //     whois_bot_admin = whois_bot_admin
    // );
    // let list_players_mention = format!(
    //     r"^{bot_mention}\s+(?i)list\s+players\s*$",
    //     bot_mention = bot_mention,
    // );
    // let list_stripe_subscriptions = format!(
    //     r"^{bot_mention}\s+(?i)list\s+subscriptions\s*$",
    //     bot_mention = bot_mention,
    // );
    // let sync_stripe_subscriptions = format!(
    //     r"^{bot_mention}\s+(?i)sync\s+subscriptions\s*$",
    //     bot_mention = bot_mention,
    // );
    // let num_cached_members = format!(
    //     r"^{bot_mention}\s+(?i)numcached\s*$",
    //     bot_mention = bot_mention,
    // );
    // let manage_channel_mention = format!(
    //     r"^{bot_mention}\s+(?i)manage\s+channel\s*$",
    //     bot_mention = bot_mention,
    // );
    // let mention_channel_role_mention = format!(
    //     r"^{bot_mention}\s+(?i)mention\s+channel\s*$",
    //     bot_mention = bot_mention,
    // );
    // let snooze_until = format!(
    //     r"^{bot_mention}\s+(?i)snooze\s+(?P<num_days>[0-9]+)\s*d(ay)?s?\s*$",
    //     bot_mention = bot_mention,
    // );
    // let list_inactive_users = format!(
    //     r"^{bot_mention}\s+(?i)list\sinactive\s*$",
    //     bot_mention = bot_mention,
    // );
    Regexes {
        bot_mention: Regex::new(&format!("^{}", bot_mention)).unwrap(),
        // link_meetup_dm: Regex::new(link_meetup_dm).unwrap(),
        // link_meetup_mention: Regex::new(link_meetup_mention.as_str()).unwrap(),
        // link_meetup_bot_admin_dm: Regex::new(link_meetup_bot_admin_dm.as_str()).unwrap(),
        // link_meetup_bot_admin_mention: Regex::new(link_meetup_bot_admin_mention.as_str()).unwrap(),
        // unlink_meetup_dm: Regex::new(unlink_meetup_dm.as_str()).unwrap(),
        // unlink_meetup_mention: Regex::new(unlink_meetup_mention.as_str()).unwrap(),
        // unlink_meetup_bot_admin_dm: Regex::new(unlink_meetup_bot_admin_dm.as_str()).unwrap(),
        // unlink_meetup_bot_admin_mention: Regex::new(unlink_meetup_bot_admin_mention.as_str())
        //     .unwrap(),
        // sync_meetup_mention: Regex::new(sync_meetup_mention.as_str()).unwrap(),
        // sync_discord_mention: Regex::new(sync_discord_mention.as_str()).unwrap(),
        // add_user_bot_admin_mention: Regex::new(add_user_bot_admin_mention.as_str()).unwrap(),
        // add_host_bot_admin_mention: Regex::new(add_host_bot_admin_mention.as_str()).unwrap(),
        // remove_user_mention: Regex::new(remove_user_mention.as_str()).unwrap(),
        // remove_host_bot_admin_mention: Regex::new(remove_host_bot_admin_mention.as_str()).unwrap(),
        // stop_bot_admin_dm: Regex::new(stop_bot_admin_dm).unwrap(),
        // stop_bot_admin_mention: Regex::new(stop_bot_admin_mention.as_str()).unwrap(),
        // send_expiration_reminder_bot_admin_mention: Regex::new(
        //     send_expiration_reminder_bot_admin_mention.as_str(),
        // )
        // .unwrap(),
        // end_adventure_host_mention: Regex::new(end_adventure_host_mention.as_str()).unwrap(),
        // help_dm: Regex::new(help_dm).unwrap(),
        // help_mention: Regex::new(help_mention.as_str()).unwrap(),
        // refresh_user_token_admin_dm: Regex::new(refresh_user_token_admin_dm.as_str()).unwrap(),
        // rsvp_user_admin_mention: Regex::new(rsvp_user_admin_mention.as_str()).unwrap(),
        // clone_event_admin_mention: Regex::new(clone_event_admin_mention.as_str()).unwrap(),
        // schedule_session_mention: Regex::new(schedule_session_mention.as_str()).unwrap(),
        // whois_bot_admin_dm: Regex::new(whois_bot_admin_dm.as_str()).unwrap(),
        // whois_bot_admin_mention: Regex::new(whois_bot_admin_mention.as_str()).unwrap(),
        // list_players_mention: Regex::new(list_players_mention.as_str()).unwrap(),
        // list_stripe_subscriptions: Regex::new(list_stripe_subscriptions.as_str()).unwrap(),
        // sync_stripe_subscriptions: Regex::new(sync_stripe_subscriptions.as_str()).unwrap(),
        // num_cached_members: Regex::new(num_cached_members.as_str()).unwrap(),
        // manage_channel_mention: Regex::new(manage_channel_mention.as_str()).unwrap(),
        // mention_channel_role_mention: Regex::new(mention_channel_role_mention.as_str()).unwrap(),
        // snooze_reminders: Regex::new(snooze_until.as_ref()).unwrap(),
        // list_inactive_users: Regex::new(list_inactive_users.as_ref()).unwrap(),
    }
}

impl super::bot::Handler {
    pub fn send_welcome_message(ctx: &Context, user: &User) {
        let _ = user.direct_message(ctx, |message_builder| {
            message_builder
                .content(strings::WELCOME_MESSAGE_PART1)
                .embed(|embed_builder| {
                    embed_builder
                        .colour(serenity::utils::Colour::new(0xFF1744))
                        .title(strings::WELCOME_MESSAGE_PART2_EMBED_TITLE)
                        .description(strings::WELCOME_MESSAGE_PART2_EMBED_CONTENT)
                })
        });
    }

    pub fn clone_event(
        ctx: &Context,
        msg: &Message,
        urlname: &str,
        meetup_event_id: &str,
        redis_client: redis::Client,
        async_meetup_client: Arc<AsyncMutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
        oauth2_consumer: Arc<lib::meetup::oauth2::OAuth2Consumer>,
    ) {
        let async_runtime = {
            let data = ctx.data.read();
            data.get::<super::bot::AsyncRuntimeKey>()
                .expect("Async runtime was not set")
                .clone()
        };
        let future = async {
            // Clone the async meetup client
            let guard = async_meetup_client.lock().await;
            let client = guard.clone();
            drop(guard);
            let client = match client {
                None => {
                    return Err(lib::meetup::Error::from(simple_error::SimpleError::new(
                        "Async Meetup client not set",
                    )))
                }
                Some(client) => client,
            };
            let new_event_hook = Box::new(|new_event: lib::meetup::api::NewEvent| {
                let date_time = new_event.time.clone();
                lib::flow::ScheduleSessionFlow::new_event_hook(
                    new_event,
                    date_time,
                    meetup_event_id,
                )
            });
            let new_event = lib::meetup::util::clone_event(
                urlname,
                meetup_event_id,
                client.as_ref(),
                Some(new_event_hook),
            )
            .await?;
            // Try to transfer the RSVPs to the new event
            let mut redis_connection = redis_client.get_async_connection().await?;
            if let Err(_) = lib::meetup::util::clone_rsvps(
                urlname,
                meetup_event_id,
                &new_event.id,
                &mut redis_connection,
                client.as_ref(),
                oauth2_consumer.as_ref(),
            )
            .await
            {
                let _ = msg
                    .channel_id
                    .say(&ctx.http, "Could not transfer all RSVPs to the new event");
            }
            Ok(new_event)
        };
        let runtime_guard = futures::executor::block_on(async_runtime.read());
        let async_runtime = match *runtime_guard {
            Some(ref async_runtime) => async_runtime,
            None => return,
        };
        match async_runtime.enter(|| futures::executor::block_on(future)) {
            Ok(new_event) => {
                let _ = msg.react(ctx, "\u{2705}");
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!("Created new Meetup event: {}", new_event.link),
                );
            }
            Err(err) => {
                let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                eprintln!(
                    "Could not clone Meetup event {}. Error:\n{:#?}",
                    meetup_event_id, err
                );
            }
        }
    }

    pub fn rsvp_user_to_event(
        ctx: &Context,
        msg: &Message,
        user_id: UserId,
        urlname: &str,
        meetup_event_id: &str,
        redis_client: redis::Client,
        oauth2_consumer: Arc<lib::meetup::oauth2::OAuth2Consumer>,
    ) {
        let async_runtime = {
            let data = ctx.data.read();
            data.get::<super::bot::AsyncRuntimeKey>()
                .expect("Async runtime was not set")
                .clone()
        };
        // Look up the Meetup ID for this user
        let mut redis_connection = match redis_client.get_connection() {
            Ok(redis_connection) => redis_connection,
            Err(err) => {
                eprintln!("Redis error when obtaining a redis connection: {}", err);
                // TODO reply
                return;
            }
        };
        let redis_discord_user_meetup_user_key = format!("discord_user:{}:meetup_user", user_id);
        let res: redis::RedisResult<Option<u64>> =
            redis_connection.get(&redis_discord_user_meetup_user_key);
        let meetup_id = match res {
            Ok(Some(meetup_id)) => meetup_id,
            Ok(None) => {
                // TODO reply
                return;
            }
            Err(err) => {
                eprintln!("Redis error when looking up the user's meetup ID: {}", err);
                // TODO reply
                return;
            }
        };
        // Try to RSVP the user
        let runtime_guard = futures::executor::block_on(async_runtime.read());
        let async_runtime = match *runtime_guard {
            Some(ref async_runtime) => async_runtime,
            None => return,
        };
        match async_runtime.enter(|| {
            futures::executor::block_on(async {
                let mut redis_connection = redis_client.get_async_connection().await?;
                lib::meetup::util::rsvp_user_to_event(
                    meetup_id,
                    urlname,
                    meetup_event_id,
                    &mut redis_connection,
                    oauth2_consumer.as_ref(),
                )
                .await
            })
        }) {
            Ok(rsvp) => {
                let _ = msg.react(ctx, "\u{2705}");
                println!(
                    "RSVP'd Meetup user {} to Meetup event {}. RSVP:\n{:#?}",
                    meetup_id, meetup_event_id, rsvp
                );
            }
            Err(err) => {
                let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                eprintln!(
                    "Could not RSVP Meetup user {} to Meetup event {}. Error:\n{:#?}",
                    meetup_id, meetup_event_id, err
                );
            }
        }
    }

    pub fn manage_channel(
        ctx: &Context,
        msg: &Message,
        redis_client: &redis::Client,
        bot_id: UserId,
    ) -> Result<(), lib::meetup::Error> {
        let mut redis_connection = redis_client.get_connection()?;
        let channel_id = msg.channel_id;
        // Step 1: Try to mark this channel as managed
        let mut is_game_channel = false;
        redis::transaction(&mut redis_connection, &["discord_channels"], |con, pipe| {
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
        })?;
        if is_game_channel {
            let _ = msg.channel_id.say(ctx, "Can not manage this channel");
            return Ok(());
        }
        let channel = if let Some(Channel::Guild(channel)) = msg.channel(ctx) {
            channel.clone()
        } else {
            let _ = msg.channel_id.say(ctx, "Can not manage this channel");
            return Ok(());
        };
        // Step 2: Grant the bot continued access to the channel
        channel.read().create_permission(
            ctx,
            &PermissionOverwrite {
                allow: Permissions::READ_MESSAGES,
                deny: Permissions::empty(),
                kind: PermissionOverwriteType::Member(bot_id),
            },
        )?;
        // Step 3: Grant all current users access to the channel
        let mut current_channel_members = channel.read().members(&ctx)?;
        for member in &mut current_channel_members {
            // Don't explicitly grant access to admins
            let is_admin = {
                if let Ok(member_permissions) = member.permissions(ctx) {
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
                ctx,
                &PermissionOverwrite {
                    allow: Permissions::READ_MESSAGES,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Member(user_id),
                },
            )?;
        }
        let _ = msg.react(ctx, "\u{2705}");
        Ok(())
    }
}
