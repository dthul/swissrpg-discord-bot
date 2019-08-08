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

#[allow(non_snake_case)]
pub fn END_OF_ADVENTURE_MESSAGE(bot_id: u64) -> String {
    format!(
        "I hope everyone @here had fun rolling dice!
It looks like your adventure is coming to an end and so will this channel.
As soon as you are ready, the host can close this channel by typing here:
***<@{bot_id}> close channel***
This will mark the channel for closure in the next 24 hours.
In case you want to continue your adventure instead, please schedule the next session(s) \
on Meetup and I will extend the lifetime of this channel.",
        bot_id = bot_id
    )
}
