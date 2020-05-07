use futures_util::lock::Mutex as AsyncMutex;
use once_cell::sync::OnceCell;
use redis::Commands;
use regex::{Regex, RegexSet};
use serenity::{
    model::{
        channel::{Channel, Message},
        id::UserId,
    },
    prelude::*,
};
use std::sync::Arc;

mod add_user;
mod clone_event;
mod count_inactive;
mod end_adventure;
mod help;
mod link_meetup;
mod list_players;
mod list_subscriptions;
mod manage_channel;
mod mention_channel;
mod numcached;
mod refresh_meetup_token;
mod remind_expiration;
mod rsvp_user;
mod schedule_session;
mod snooze;
mod stop;
mod sync_discord;
mod sync_meetup;
mod sync_subscriptions;
mod test;
mod whois;

static ALL_COMMANDS: &[&Command] = &[
    &stop::STOP_COMMAND,
    &link_meetup::LINK_MEETUP_COMMAND,
    &link_meetup::UNLINK_MEETUP_COMMAND,
    &link_meetup::LINK_MEETUP_BOT_ADMIN_COMMAND,
    &link_meetup::UNLINK_MEETUP_BOT_ADMIN_COMMAND,
    &sync_meetup::SYNC_MEETUP_COMMAND,
    &sync_discord::SYNC_DISCORD_COMMAND,
    &remind_expiration::REMIND_EXPIRATION_COMMAND,
    &add_user::ADD_USER_COMMAND,
    &add_user::ADD_HOST_COMMAND,
    &add_user::REMOVE_USER_COMMAND,
    &add_user::REMOVE_HOST_COMMAND,
    &end_adventure::END_ADVENTURE_COMMAND,
    &help::HELP_COMMAND,
    &refresh_meetup_token::REFRESH_MEETUP_TOKEN_COMMAND,
    &schedule_session::SCHEDULE_SESSION_COMMAND,
    &whois::WHOIS_COMMAND,
    &list_players::LIST_PLAYERS_COMMAND,
    &list_subscriptions::LIST_SUBSCRIPTIONS_COMMAND,
    &sync_subscriptions::SYNC_SUBSCRIPTIONS_COMMAND,
    &numcached::NUMCACHED_COMMAND,
    &manage_channel::MANAGE_CHANNEL_COMMAND,
    &mention_channel::MENTION_CHANNEL_COMMAND,
    &snooze::SNOOZE_COMMAND,
    &count_inactive::COUNT_INACTIVE_COMMAND,
    &clone_event::CLONE_EVENT_COMMAND,
    &rsvp_user::RSVP_USER_COMMAND,
    &test::TEST_COMMAND,
];

const MENTION_PATTERN: &'static str = r"(?:<@!?(?P<mention_id>[0-9]+)>)";
const USERNAME_TAG_PATTERN: &'static str = r"(?P<discord_username_tag>[^@#:]{2,32}#[0-9]+)";
const MEETUP_ID_PATTERN: &'static str = r"(?P<meetup_user_id>[0-9]+)";

pub(crate) enum CommandLevel {
    Everybody,
    HostAndAdminOnly,
    AdminOnly,
}

pub struct RegexParts<'a> {
    mention_pattern: &'a str,
    username_tag_pattern: &'a str,
    meetup_id_pattern: &'a str,
}

pub(crate) struct Command {
    pub regex: fn(&RegexParts<'_>) -> String,
    pub level: CommandLevel,
    pub fun: fn(CommandContext<'_>, regex::Captures<'_>) -> Result<(), lib::meetup::Error>,
    pub help: &'static [HelpEntry],
}

pub(crate) struct HelpEntry {
    pub command: &'static str,
    pub explanation: &'static str,
}

pub struct CommandContext<'a> {
    pub ctx: &'a Context,
    pub msg: &'a Message,
    // pub captures: regex::Captures<'a>,
    redis_client: OnceCell<redis::Client>,
    redis_connection: OnceCell<redis::Connection>,
    async_redis_connection: OnceCell<redis::aio::Connection>,
    async_runtime: OnceCell<Arc<tokio::sync::RwLock<Option<tokio::runtime::Runtime>>>>,
    meetup_client: OnceCell<Arc<AsyncMutex<Option<Arc<lib::meetup::api::AsyncClient>>>>>,
    task_scheduler: OnceCell<Arc<AsyncMutex<white_rabbit::Scheduler>>>,
    oauth2_consumer: OnceCell<Arc<lib::meetup::oauth2::OAuth2Consumer>>,
    stripe_client: OnceCell<Arc<stripe::Client>>,
    bot_id: OnceCell<UserId>,
    channel: OnceCell<Channel>,
}

impl<'a> CommandContext<'a> {
    pub fn new(ctx: &'a Context, msg: &'a Message) -> Self {
        CommandContext {
            ctx,
            msg,
            redis_client: OnceCell::new(),
            redis_connection: OnceCell::new(),
            async_redis_connection: OnceCell::new(),
            async_runtime: OnceCell::new(),
            meetup_client: OnceCell::new(),
            task_scheduler: OnceCell::new(),
            oauth2_consumer: OnceCell::new(),
            stripe_client: OnceCell::new(),
            bot_id: OnceCell::new(),
            channel: OnceCell::new(),
        }
    }

    pub fn redis_client<'b>(&'b self) -> Result<&'b redis::Client, lib::meetup::Error> {
        self.redis_client.get_or_try_init(|| {
            let data = self.ctx.data.read();
            data.get::<super::bot::RedisClientKey>()
                .cloned()
                .ok_or_else(|| simple_error::SimpleError::new("Redis client was not set").into())
        })
    }

    pub fn redis_connection<'b>(
        &'b mut self,
    ) -> Result<&'b mut redis::Connection, lib::meetup::Error> {
        if self.redis_connection.get().is_some() {
            Ok(self
                .redis_connection
                .get_mut()
                .expect("Redis connection not set. This is a bug."))
        } else {
            let redis_connection = self.redis_client()?.get_connection()?;
            self.redis_connection
                .set(redis_connection)
                .map_err(|_| ())
                .expect("Redis connection could not be set. This is a bug.");
            Ok(self
                .redis_connection
                .get_mut()
                .expect("Redis connection not set. This is a bug."))
        }
    }

    pub async fn async_redis_connection<'b>(
        &'b mut self,
    ) -> Result<&'b mut redis::aio::Connection, lib::meetup::Error> {
        if self.async_redis_connection.get().is_some() {
            Ok(self
                .async_redis_connection
                .get_mut()
                .expect("Async redis connection not set. This is a bug."))
        } else {
            let redis_client = self.redis_client()?;
            let async_redis_connection = redis_client.get_async_connection().await?;
            self.async_redis_connection
                .set(async_redis_connection)
                .map_err(|_| ())
                .expect("Async redis connection could not be set. This is a bug.");
            Ok(self
                .async_redis_connection
                .get_mut()
                .expect("Async redis connection not set. This is a bug."))
        }
    }

    pub fn async_runtime<'b>(
        &'b self,
    ) -> Result<&'b Arc<tokio::sync::RwLock<Option<tokio::runtime::Runtime>>>, lib::meetup::Error>
    {
        self.async_runtime.get_or_try_init(|| {
            let data = self.ctx.data.read();
            data.get::<super::bot::AsyncRuntimeKey>()
                .cloned()
                .ok_or_else(|| simple_error::SimpleError::new("Async runtime was not set").into())
        })
    }

    pub fn meetup_client<'b>(
        &'b self,
    ) -> Result<&'b Arc<AsyncMutex<Option<Arc<lib::meetup::api::AsyncClient>>>>, lib::meetup::Error>
    {
        self.meetup_client.get_or_try_init(|| {
            let data = self.ctx.data.read();
            data.get::<super::bot::AsyncMeetupClientKey>()
                .cloned()
                .ok_or_else(|| simple_error::SimpleError::new("Meetup client was not set").into())
        })
    }

    pub fn task_scheduler<'b>(
        &'b self,
    ) -> Result<&'b Arc<AsyncMutex<white_rabbit::Scheduler>>, lib::meetup::Error> {
        self.task_scheduler.get_or_try_init(|| {
            let data = self.ctx.data.read();
            data.get::<super::bot::TaskSchedulerKey>()
                .cloned()
                .ok_or_else(|| simple_error::SimpleError::new("Task scheduler was not set").into())
        })
    }

    pub fn oauth2_consumer<'b>(
        &'b self,
    ) -> Result<&'b Arc<lib::meetup::oauth2::OAuth2Consumer>, lib::meetup::Error> {
        self.oauth2_consumer.get_or_try_init(|| {
            let data = self.ctx.data.read();
            data.get::<super::bot::OAuth2ConsumerKey>()
                .cloned()
                .ok_or_else(|| simple_error::SimpleError::new("OAuth2 consumer was not set").into())
        })
    }

    pub fn stripe_client<'b>(&'b self) -> Result<&'b Arc<stripe::Client>, lib::meetup::Error> {
        self.stripe_client.get_or_try_init(|| {
            let data = self.ctx.data.read();
            data.get::<super::bot::StripeClientKey>()
                .cloned()
                .ok_or_else(|| simple_error::SimpleError::new("Stripe client was not set").into())
        })
    }

    pub fn bot_id<'b>(&'b self) -> Result<UserId, lib::meetup::Error> {
        self.bot_id
            .get_or_try_init(|| {
                let data = self.ctx.data.read();
                data.get::<super::bot::BotIdKey>()
                    .copied()
                    .ok_or_else(|| simple_error::SimpleError::new("Bot ID was not set").into())
            })
            // TODO: can be replaced with a call to `copied` as soon as it's stable
            .map(|id| *id)
    }

    pub fn channel<'b>(&'b self) -> Result<&'b Channel, lib::meetup::Error> {
        self.channel
            .get_or_try_init(|| Ok(self.msg.channel_id.to_channel(self.ctx)?))
    }

    pub fn is_dm(&self) -> Result<bool, lib::meetup::Error> {
        Ok(match self.channel()? {
            Channel::Private(_) => true,
            _ => false,
        })
    }

    pub fn is_admin(&self) -> Result<bool, lib::meetup::Error> {
        Ok(self.msg.author.has_role(
            self.ctx,
            lib::discord::sync::ids::GUILD_ID,
            lib::discord::sync::ids::BOT_ADMIN_ID,
        )?)
    }

    pub fn is_host(&mut self) -> Result<bool, lib::meetup::Error> {
        let ctx = self.ctx.into();
        let channel_id = self.msg.channel_id;
        let user_id = self.msg.author.id;
        let redis_connection = self.redis_connection()?;
        lib::discord::is_host(&ctx, channel_id, user_id, redis_connection)
    }

    pub fn is_game_channel(&mut self) -> Result<bool, lib::meetup::Error> {
        let channel_id = self.msg.channel_id.0;
        Ok(self
            .redis_connection()?
            .sismember("discord_channels", channel_id)?)
    }

    pub fn is_managed_channel(&mut self) -> Result<bool, lib::meetup::Error> {
        let channel_id = self.msg.channel_id.0;
        Ok(self
            .redis_connection()?
            .sismember("managed_discord_channels", channel_id)?)
    }
}

pub(crate) struct PreparedCommands {
    pub regex_set: RegexSet,
    pub regexes: Vec<Regex>,
    pub commands: Vec<&'static Command>,
    pub bot_mention: Regex,
}

pub(crate) fn prepare_commands(
    bot_id: UserId,
    bot_name: &str,
) -> Result<PreparedCommands, lib::meetup::Error> {
    let regex_parts = RegexParts {
        mention_pattern: MENTION_PATTERN,
        username_tag_pattern: USERNAME_TAG_PATTERN,
        meetup_id_pattern: MEETUP_ID_PATTERN,
    };
    let bot_mention = format!(
        r"(?:<@!?{bot_id}>|(@|#)(?i){bot_name})",
        bot_id = bot_id.0,
        bot_name = regex::escape(bot_name)
    );
    let mut commands = vec![];
    let mut regexes = vec![];
    for &command in ALL_COMMANDS {
        let command_partial_regex = (command.regex)(&regex_parts);
        let command_dm_regex = format!(r"^\s*(?i){command}\s*$", command = command_partial_regex);
        let command_mention_regex = format!(
            r"^\s*{bot_mention}\s+(?i){command}\s*$",
            bot_mention = bot_mention,
            command = command_partial_regex
        );
        match (
            Regex::new(&command_dm_regex),
            Regex::new(&command_mention_regex),
        ) {
            (Ok(dm_regex), Ok(mention_regex)) => {
                regexes.push(dm_regex);
                commands.push(command);
                regexes.push(mention_regex);
                commands.push(command);
            }
            (res1, res2) => {
                let err = res1.err().unwrap_or_else(|| res2.unwrap_err());
                eprintln!(
                    "Could not compile command regex \"{}\":\n{:#?}",
                    command_partial_regex, err
                )
            }
        }
    }
    let regex_set = regex::RegexSet::new(regexes.iter().map(Regex::as_str))?;
    Ok(PreparedCommands {
        regex_set,
        regexes,
        commands,
        bot_mention: Regex::new(&format!(r"^\s*{}", bot_mention))?,
    })
}
