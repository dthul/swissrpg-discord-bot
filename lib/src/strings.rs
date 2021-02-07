use serenity::model::id::ChannelId;

// ***********************
// *** Discord replies ***
// ***********************

// ** General **

pub const NOT_A_BOT_ADMIN: &'static str = "Sorry, only admins can do this. But hey, maybe one day.";

pub const UNSPECIFIED_ERROR: &'static str =
    "Oops, something went wrong :dizzy_face: Please try again.";

#[allow(non_snake_case)]
pub fn INVALID_COMMAND(bot_id: u64) -> String {
    format!(
        "Sorry, but I did not get that. I only speak Halfling, Draconic, Abyssal, and Command \
         (not Common).\nIf you also want to learn Command, type _<@{bot_id}> help_.",
        bot_id = bot_id
    )
}

#[allow(non_snake_case)]
pub fn HELP_MESSAGE_INTRO(bot_id: u64) -> String {
    format!(
        "Of course, I'm happy to serve (because I've been programmed to). Here are the commands I \
         understand.
         
***Note:*** *Unless specified, you can type all these commands in this private chat. Any commands \
         you type in a channel should start with the mention of my name <@{bot_id}>, but be \
         mindful not to spam the public channels please.*",
        bot_id = bot_id
    )
}

pub const HELP_MESSAGE_ADMIN_EMBED_TITLE: &'static str = "**Admin commands**";

pub const HELP_MESSAGE_GM_EMBED_TITLE: &'static str =
    "**Game Master commands** _(use in game channel)_";

pub const HELP_MESSAGE_PLAYER_EMBED_TITLE: &'static str = "**Player commands**";

// ** Welcome messages **

pub const WELCOME_MESSAGE: &'static str =
    "Hi there. I'm **Hyperion**, the **SwissRPG bot**. Welcome to our community.

If you have signed up for one of our games, or plan to play a game soon, you'll need to link your Meetup and Discord accounts with me. This will allow me to add you to your game's private channels where you can talk with your Game Master and other players.

Let's get you started. Just type **link meetup** below and we'll take it from there.";

// ** End of one-shot **

#[allow(non_snake_case)]
pub fn END_OF_ADVENTURE_MESSAGE(bot_id: u64, channel_role_id: Option<u64>) -> String {
    if let Some(channel_role_id) = channel_role_id {
        format!(
            "I hope everyone had fun on this adventure of <@&{channel_role_id}>.
Now that your adventure is over, it's time to close this channel.
Can the GM please confirm this by typing here:
***<@{bot_id}> end adventure***
This will set the channel for closure in the next 24 hours, which should be just enough time to \
             say thanks and goodbye.
If the adventure is not done, you can schedule a new session by typing here:
***<@{bot_id}> schedule session***",
            bot_id = bot_id,
            channel_role_id = channel_role_id
        )
    } else {
        format!(
            "I hope everyone @here had fun on this adventure.
Now that your adventure is over, it's time to close this channel.
Can the GM please confirm this by typing here:
***<@{bot_id}> end adventure***
This will set the channel for closure in the next 24 hours, which should be just enough time to \
             say thanks and goodbye.
If the adventure is not done, you can schedule a new session by typing here:
***<@{bot_id}> schedule session***",
            bot_id = bot_id
        )
    }
}

// ** End of campaign **

#[allow(non_snake_case)]
pub fn END_OF_CAMPAIGN_MESSAGE(bot_id: u64, channel_role_id: Option<u64>) -> String {
    if let Some(channel_role_id) = channel_role_id {
        format!(
            "I hope everyone had fun at the last session of <@&{channel_role_id}>!
Whenever you are ready, schedule your next session by typing:
***<@{bot_id}> schedule session***

If your adventure is over, the Game Master can inform me of this by typing here:
***<@{bot_id}> end adventure***
This will set the channel for closure in the next 24 hours, just enough to say thanks and goodbye.",
            bot_id = bot_id,
            channel_role_id = channel_role_id
        )
    } else {
        format!(
            "I hope everyone @here had fun at the last session!
Whenever you are ready, schedule your next session by typing:
***<@{bot_id}> schedule session***

If your adventure is over, the Game Master can inform me of this by typing here:
***<@{bot_id}> end adventure***
This will set the channel for closure in the next 24 hours, just enough to say thanks and goodbye.",
            bot_id = bot_id
        )
    }
}

// ** Meetup linking **

#[allow(non_snake_case)]
pub fn MEETUP_LINKING_MESSAGE(linking_url: &str) -> String {
    format!(
        "Let's get you hooked up :thumbsup:\n\n***Important note:*** If you are on mobile, please \
         copy and paste the link into your browser rather than clicking it here.\n\nUse this link \
         to connect your Meetup profile:\n{}\n***This is a private, ephemeral, one-time use link \
         and meant just for you.***\nDon't share it with anyone or bad things can happen (to you, \
         I'll be fine).",
        linking_url
    )
}

#[allow(non_snake_case)]
pub fn DISCORD_ALREADY_LINKED_MESSAGE1(meetup_name: &str, bot_id: u64) -> String {
    format!(
        "It seems you are already linked to {}'s Meetup profile. If you would like to change \
         this, please unlink your profile first by typing:\n<@{}> unlink meetup",
        meetup_name, bot_id
    )
}

#[allow(non_snake_case)]
pub fn DISCORD_ALREADY_LINKED_MESSAGE2(bot_id: u64) -> String {
    format!(
        "It seems you are already linked to a Meetup profile. If you would like to change this, \
         please unlink your profile first by typing:\n<@{}> unlink meetup",
        bot_id
    )
}

#[allow(non_snake_case)]
pub fn NONEXISTENT_MEETUP_LINKED_MESSAGE(bot_id: u64) -> String {
    format!(
        "You are linked to a seemingly non-existent Meetup account. If you want to change this, \
         unlink the currently linked meetup account by writing:\n<@{}> unlink meetup",
        bot_id
    )
}

#[allow(non_snake_case)]
pub fn MEETUP_UNLINK_SUCCESS(bot_id: u64) -> String {
    format!(
        "Your Meetup profile is now unlinked from your Discord profile. If you want to link it \
         again, please type:\n<@{bot_id}> link meetup.",
        bot_id = bot_id
    )
}

pub const MEETUP_UNLINK_NOT_LINKED: &'static str =
    "There doesn't seem to be anything to unlink. But thanks for the effort :smiley:";

// ** Channel administration **

pub const NOT_A_CHANNEL_ADMIN: &'static str =
    "Only this channel's Game Master and admins can do that. How about running your own game?";

pub const CHANNEL_NOT_BOT_CONTROLLED: &'static str =
    "This channel does not seem to be under my control. But one day... one day :smiling_imp:";

pub const CHANNEL_NOT_YET_CLOSEABLE: &'static str = "Too soon mate. Please wait for my request \
                                                     for deletion first. This is to avoid \
                                                     accidental deletion of channels :grimacing:";

pub const CHANNEL_NO_EXPIRATION: &'static str =
    "This channel has no expiration date, so I will not close it.";

pub const CHANNEL_MARKED_FOR_CLOSING: &'static str =
    "Roger that. I've marked this channel to be closed in the next 24 hours.\nThanks for playing \
     and hope to see you at another game soon.";

pub const CHANNEL_ALREADY_MARKED_FOR_CLOSING: &'static str =
    "Deja vu! This channel is already marked for closing. The black hole is on its way. Patience.";

#[allow(non_snake_case)]
pub fn CHANNEL_MARKED_FOR_CLOSING_ALERT(channel_id: ChannelId) -> String {
    format!("<#{}> just ended their adventure!", channel_id.0)
}

pub const CHANNEL_ROLE_ADD_ERROR: &'static str = "Something went wrong assigning the channel role";

pub const CHANNEL_ROLE_REMOVE_ERROR: &'static str =
    "Something went wrong removing the channel role";

#[allow(non_snake_case)]
pub fn CHANNEL_ADDED_PLAYERS(discord_user_ids: &[u64]) -> String {
    let mentions = itertools::join(
        discord_user_ids.iter().map(|&id| format!("<@{}>", id)),
        ", ",
    );
    format!(
        "Welcome {}! Please check this channel's pinned messages (if any) for basic information \
         about the adventure.",
        mentions
    )
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
    format!(
        "<@{}> is now a Game Master for this channel. With great power comes great responsibility \
         :spider:",
        discord_id
    )
}

pub const CHANNEL_ADD_USER_INVALID_DISCORD: &'static str =
    "Seems like the specified Discord ID is invalid";

// **************************************
// *** Meetup linking webpage replies ***
// **************************************

#[allow(non_snake_case)]
pub fn OAUTH2_AUTHORISATION_DENIED(linking_url: &str) -> String {
    format!(
        "Looks like you declined the authorisation. If you want to start over, click the button \
         below to give it another go. If you are still having issues, please contact an organiser \
         by email (organisers@swissrpg.ch) or on Discord (@Organiser).<br><a \
         href=\"{linking_url}\" class=\"button\" style=\"margin-top: 1em;\">Start Over</a>",
        linking_url = linking_url
    )
}

pub const OAUTH2_LINK_EXPIRED_TITLE: &'static str = "This link seems to have expired";
pub const OAUTH2_LINK_EXPIRED_CONTENT: &'static str =
    "Get a new link with the \"link meetup\" command";

pub const OAUTH2_LINKING_SUCCESS_TITLE: &'static str = "Linking Success!";
#[allow(non_snake_case)]
pub fn OAUTH2_LINKING_SUCCESS_CONTENT(name: &str) -> String {
    format!(
        "You are now linked to {}'s Meetup profile. Enjoy rolling dice with us!",
        name
    )
}

pub const OAUTH2_ALREADY_LINKED_SUCCESS_TITLE: &'static str = "All good!";
pub const OAUTH2_ALREADY_LINKED_SUCCESS_CONTENT: &'static str =
    "Your Meetup account was already linked";

pub const OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_TITLE: &'static str = "Linking Failure";
#[allow(non_snake_case)]
pub fn OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_CONTENT(bot_name: &str) -> String {
    format!(
        "It seems you are already linked to a different Meetup profile. If you would like to \
         change this, please unlink your profile first by typing:\n@{} unlink meetup",
        bot_name
    )
}

pub const OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_TITLE: &'static str = "Linking Failure";
#[allow(non_snake_case)]
pub fn OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_CONTENT(bot_name: &str) -> String {
    format!(
        "Deja vu! This Meetup profile is already linked to a different Discord user. Did you link \
         it to another Discord profile in the past? If so, you should first unlink this Meetup \
         profile from the other Discord profile by writing \"@{} unlink meetup\". Make sure you \
         do this with the other Discord account. After that you can link this Meetup account \
         again. If you did not link this Meetup account before, please contact an @Organiser on \
         Discord.",
        bot_name
    )
}

pub const INTERNAL_SERVER_ERROR: &'static str =
    "Tiamat just crit on our server. Please try again soon.";
