// ***********************
// *** Discord replies ***
// ***********************

// ** General **

pub const NOT_A_BOT_ADMIN: &'static str = "Only admins can do this";

pub const UNSPECIFIED_ERROR: &'static str = "Something went wrong";

pub const INVALID_COMMAND: &'static str = "Sorry, I do not understand that command";

// ** Welcome messages **

pub const WELCOME_MESSAGE_PART1: &'static str =
    "Welcome to **SwissRPG**! We hope you'll enjoy rolling dice with us. \
Here is some basic info to get you started.

**Got a question about SwissRPG?**
Ask at the #tavern and someone will help you out. If you need help from an \
organiser, just hit them up with the \\@Organiser tag.

**Are you a GM or aspire to be one?**
- If you would like be a GM for our community, please get in touch with \\@Alp#5068. \
We offer great resources to our GMs like access to public venues, most D&D books \
and PDF adventures.
- If you're interested in being a GM but don't feel ready, let us know. \
We've provided support to many beginner GMs to get them started.

**Support our group**
SwissRPG aims to enable people to play role playing games. If you find value \
in what we do, please consider supporting us here (https://rebi.me/swissrpg) \
with a donation. Every donation makes a big difference. Thank you for your support.

**Basic rules of our community**
**1.** If you would like to promote an event, another tabletop group, \
a discord server etc. please ask an \\@Organiser first.
**2.** Be inclusive and respectful of others and their differences.
**3.** Stay positive and love each other. Life is good.

**_Don’t forget to introduce yourself to our community in the #tavern channel. \
Don't be shy, we're all very nice._**";

pub const WELCOME_MESSAGE_PART2_EMBED_TITLE: &'static str = "**Get access to a game's channel**";
pub const WELCOME_MESSAGE_PART2_EMBED_CONTENT: &'static str =
    "If you've signed up for a game, or plan to do so soon, you'll need access to \
the adventure's channel. To do that, please link your Meetup profile to your \
Discord profile. Let’s try this now shall we?
Reply with ***link meetup*** here to get the process started.";

// ** End of adventure **

#[allow(non_snake_case)]
pub fn END_OF_ADVENTURE_MESSAGE(bot_id: u64) -> String {
    format!(
        "I hope everyone @here had fun rolling dice!
It looks like your adventure is coming to an end and so will this channel.
As soon as you are ready, the host can close this channel by typing here:
***<@{bot_id}> end adventure***
This will mark the channel for closure in the next 24 hours.
In case you want to continue your adventure instead, please schedule the next session(s) \
on Meetup and I will extend the lifetime of this channel.",
        bot_id = bot_id
    )
}

// ** Meetup linking **

#[allow(non_snake_case)]
pub fn MEETUP_LINKING_MESSAGE(linking_url: &str) -> String {
    format!(
        "Visit the following website to link your Meetup profile: {}\n\
        ***This is a private, ephemeral, one-time use link and meant just for you.***\n\
        Don't share it with others or they might link your Discord account to their Meetup profile.",
        linking_url
    )
}

#[allow(non_snake_case)]
pub fn MEETUP_ALREADY_LINKED_SUCCESS(linking_url: &str) -> String {
    format!(
        "Visit the following website to link your Meetup profile: {}\n\
        ***This is a private, ephemeral, one-time use link and meant just for you.***\n\
        Don't share it with others or they might link your Discord account to their Meetup profile.",
        linking_url
    )
}

#[allow(non_snake_case)]
pub fn DISCORD_ALREADY_LINKED_MESSAGE1(meetup_name: &str, bot_id: u64) -> String {
    format!(
        "You are already linked to {}'s Meetup account. \
         If you really want to change this, unlink your currently \
         linked meetup account first by writing:\n\
         <@{}> unlink meetup",
        meetup_name, bot_id
    )
}

#[allow(non_snake_case)]
pub fn DISCORD_ALREADY_LINKED_MESSAGE2(bot_id: u64) -> String {
    format!(
        "You are already linked to a Meetup account. \
         If you really want to change this, unlink your currently \
         linked meetup account first by writing:\n\
         {} unlink meetup",
        bot_id
    )
}

#[allow(non_snake_case)]
pub fn NONEXISTENT_MEETUP_LINKED_MESSAGE(bot_id: u64) -> String {
    format!(
        "You are linked to a seemingly non-existent Meetup account. \
         If you want to change this, unlink the currently \
         linked meetup account by writing:\n\
         {} unlink meetup",
        bot_id
    )
}

pub const MEETUP_UNLINK_SUCCESS: &'static str = "Unlinked your Meetup account";
pub const MEETUP_UNLINK_NOT_LINKED: &'static str =
    "There was seemingly no meetup account linked to you";

// ** Channel administration **

pub const NOT_A_CHANNEL_ADMIN: &'static str = "Only channel hosts and admins can do that";

pub const CHANNEL_NOT_BOT_CONTROLLED: &'static str =
    "This channel does not seem to be under my control";

pub const CHANNEL_NOT_YET_CLOSEABLE: &'static str = "The channel cannot be closed yet";

pub const CHANNEL_MARKED_FOR_CLOSING: &'static str =
    "I marked this channel the be closed in the next 24 hours.\n\
     Thanks for playing and hope to see you soon!";

pub const CHANNEL_ALREADY_MARKED_FOR_CLOSING: &'static str =
    "Channel is already marked for closing";

pub const CHANNEL_ROLE_ADD_ERROR: &'static str = "Something went wrong assigning the channel role";

pub const CHANNEL_ROLE_REMOVE_ERROR: &'static str =
    "Something went wrong removing the channel role";

#[allow(non_snake_case)]
pub fn CHANNEL_ADDED_PLAYERS(discord_user_ids: &[u64]) -> String {
    let mentions = itertools::join(
        discord_user_ids.iter().map(|&id| format!("<@{}>", id)),
        ", ",
    );
    format!("Welcome {}! Please check this channel's pinned messages (if any) for basic information about the adventure.", mentions)
}

#[allow(non_snake_case)]
pub fn CHANNEL_ADDED_HOSTS(discord_user_ids: &[u64]) -> String {
    let mentions = itertools::join(
        discord_user_ids.iter().map(|&id| format!("<@{}>", id)),
        ", ",
    );
    if discord_user_ids.len() > 1 {
        format!(
            "{} are the Game Masters of this channel! All hail to you!",
            mentions
        )
    } else {
        format!(
            "{} is the Game Master of this channel! All hail to thee!",
            mentions
        )
    }
}

#[allow(non_snake_case)]
pub fn CHANNEL_ADDED_NEW_HOST(discord_id: u64) -> String {
    format!("<@{}> is now a host of this channel", discord_id)
}

pub const CHANNEL_ADD_USER_INVALID_DISCORD: &'static str =
    "Seems like the specified Discord ID is invalid";

// **************************************
// *** Meetup linking webpage replies ***
// **************************************

#[allow(non_snake_case)]
pub fn OAUTH2_AUTHORISATION_DENIED(linking_url: &str) -> String {
    format!(
        "Looks like you declined the authorisation. If you want to \
         start over, click the button below to give it another go. \
         If you are still having issues, please contact an organiser \
         by email (organisers@swissrpg.ch) or on Discord (@Organiser).<br>\
         <a href=\"{linking_url}\" class=\"button\" style=\"margin-top: 1em;\">Start Over</a>",
        linking_url = linking_url
    )
}

pub const OAUTH2_LINK_EXPIRED_TITLE: &'static str = "This link seems to have expired";
pub const OAUTH2_LINK_EXPIRED_CONTENT: &'static str =
    "Get a new link from the bot with the \"link meetup\" command";

pub const OAUTH2_LINKING_SUCCESS_TITLE: &'static str = "Linking Success!";
#[allow(non_snake_case)]
pub fn OAUTH2_LINKING_SUCCESS_CONTENT(name: &str) -> String {
    format!("Successfully linked to {}'s Meetup account", name)
}

pub const OAUTH2_ALREADY_LINKED_SUCCESS_TITLE: &'static str = "All good!";
pub const OAUTH2_ALREADY_LINKED_SUCCESS_CONTENT: &'static str =
    "Your Meetup account was already linked";

pub const OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_TITLE: &'static str = "Linking Failure";
#[allow(non_snake_case)]
pub fn OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_CONTENT(bot_name: &str) -> String {
    format!(
        "You are already linked to a different Meetup account. \
         If you really want to change this, unlink your currently \
         linked meetup account first by writing:\n\
         {} unlink meetup",
        bot_name
    )
}

pub const OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_TITLE: &'static str = "Linking Failure";
#[allow(non_snake_case)]
pub fn OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_CONTENT(bot_name: &str) -> String {
    format!(
        "This Meetup account is already linked to a different Discord user. \
         Did you link this Meetup account to another Discord account in the past? \
         In that case you can first unlink this Meetup account from the other Discord \
         account by writing \"@{} unlink meetup\" from the other Discord account. \
         After that you can link this Meetup account again. \
         If you did not link this Meetup account before, please contact an @Organiser",
        bot_name
    )
}

pub const INTERNAL_SERVER_ERROR: &'static str = "Internal Server Error";
