# Redis Schema

## Meetup Events

`meetup_events`: set of string\
Set of all currently tracked Meetup event IDs

`meetup_event:{}:meetup_users`: set of u64\
1:N relationship between an event and guests that RSVP'd 'yes'

`meetup_event:{}:meetup_hosts`: set of u64\
1:N relationship between an event and the event hosts

`meetup_event:{}:event_series`: string\
N:1 relationship between an event and the series it belongs to.\
See `event_series:{}:meetup_events` for the inverse relationship.

`meetup_event:{}`: hash
* `name`: string. Title of the event as it appears on Meetup
* `time`: string. Date and time of the event in RFC3339 format
* `link`: string. URL to the Meetup event page
* `urlname`: string. 'urlname' of the Meetup group this event belongs to
* `is_online`: string, optional. "true" or "false"

## Meetup Users

`meetup_users`: set of u64\
Set of linked Meetup users

`meetup_user:{}:discord_user`: u64\
1:1 relationship between a Meetup user and a Discord user.\
See `discord_user:{}:meetup_user` for the inverse relationship.

`meetup_user:{}:oauth2_tokens`: hash
* `access_token`: string. OAuth2 access token for this Meetup user
* `refresh_token`: string. OAuth2 refresh token for this Meetup user

`meetup_user:{}:oauth2_tokens:last_refresh_time`: string, optional\
The last time the tokens were refreshed.

## Discord Users

`discord_users`: set of u64\
Set of linked Discord users

`discord_user:{}:meetup_user`: u64\
1:1 relationship between a Discord user and a Meetup user.\
See `meetup_user:{}:discord_user` for the inverse relationship.

## Event Series

`event_series`: set of string\
Set of all currently tracked event series

`event_series:{}:meetup_events`: set of string\
Set of all currently tracked events that are part of this event series.\
See `meetup_event:{}:event_series` for the inverse relationship.

`event_series:{}:discord_channel`: u64\
1:1 relationship between an event series and its bot controlled channel.\
See `discord_channel:{}:event_series` for the inverse relationship.

`event_series:{}:discord_voice_channel`: u64, optional\
1:1 relationship between an event series and its bot controlled voice channel.\
See `discord_voice_channel:{}:event_series` for the inverse relationship.

`event_series:{}:type`: string\
'campaign' or 'adventure'. Should probably be part of the `event_series:{}` hash instead

`event_series:{}:is_online`: bool\
`true` for online campaigns, non-existent or "falsy" otherwise

## Discord Channels

`discord_channels`: set of u64\
Set of all bot controlled Discord game channels

`discord_channel:{}:event_series`: string\
1:1 relationship between a Discord channel and the event series it belongs to.\
See `event_series:{}:discord_channel` for the inverse relationship.

`discord_channel:{}:discord_role`: u64\
1:1 relationship between a Discord channel and its Discord guest role.\
See `discord_role:{}:discord_channel` for the inverse relationship.

`discord_channel:{}:discord_host_role`: u64\
1:1 relationship between a Discord channel and its Discord host role.\
See `discord_host_role:{}:discord_channel` for the inverse relationship.

`discord_channel:{}:removed_hosts`: set of u64\
Set of hosts (Discord ID) that have been manually removed from this channel. These users might still be part of the channel, but should not be automatically promoted to hosts of this channel anymore.

`discord_channel:{}:removed_users`: set of u64\
Set of users (Discord ID) that have been manually removed from this channel. These users should not be automatically added back to this channel anymore.

`discord_channel:{}:expiration_time`: string\
Expiration time of this channel

`discord_channel:{}:last_expiration_reminder_time`: string\
Time the channel was last reminded of its expiration

`discord_channel:{}:snooze_until`: string\
Time until which this channel should not be reminded of its expiration

`discord_channel:{}:deletion_time`: string\
Time after which this channel will be deleted

`orphaned_discord_channels`: set of u64\
Set of Discord channels that were created by the bot but could not be successfully deleted in the past

## Discord Voice Channels

A voice channel uses the same roles as the event series' associated text channel.

`discord_voice_channels`: set of u64\
Set of all bot controlled Discord voice channels

`discord_voice_channel:{}:event_series`: string\
1:1 relationship between a Discord voice channel and the event series it belongs to.\
See `event_series:{}:discord_voice_channel` for the inverse relationship.

`discord_voice_channel:{}:deletion_time`: string\
Time after which this channel will be deleted

`orphaned_discord_voice_channels`: set of u64\
Set of Discord voice channels that were created by the bot but could not be successfully deleted in the past

`user_topic_voice_channel_topic_time`: string\
Time the topic of the user topic voice channel has been set

## Managed Discord Channels

These are channels managed by Hyperion but independent of Meetup events.

`managed_discord_channels`: set of u64\
Set of all bot managed Discord (non-game) channels

## Discord Roles

`discord_roles`: set of 64\
Set of all bot controlled Discord guest roles

`discord_host_roles`: set of u64\
Set of all bot controlled Discord host roles

`discord_role:{}:discord_channel`: u64\
1:1 relationship between a Discord guest role and the Discord channel it belongs to.\
See `discord_channel:{}:discord_role` for the inverse relationship.

`discord_host_role:{}:discord_channel`: u64\
1:1 relationship between a Discord host role and the Discord channel it belongs to.\
See `discord_channel:{}:discord_host_role` for the inverse relationship.

`orphaned_discord_roles`: set of u64\
Set of Discord roles that were created by the bot but could not be successfully deleted in the past

## Linking

`meetup_linking:{}:discord_user`: u64\
Short lived N:1 relationship between one or more ephemeral linking IDs (string) and a Discord user

`csrf:{}`: string\
Short lived CSRF token belonging to some transient 'user_id' (string) that will be stored in a cookie during the linking process

## OAuth2 Organizer Token

`meetup_access_token`: string\
OAuth2 access token of someone who is organizer in all our Meetup groups

`meetup_refresh_token`: string\
OAuth2 refresh token of someone who is organizer in all our Meetup groups

`meetup_access_token_refresh_time`: string\
Date and time of the next scheduled token refresh in RFC3339 format