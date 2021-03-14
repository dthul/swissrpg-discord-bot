import requests

application_id = 643523617702936596
guild_id = 601070848446824509
bot_token = "NjQzNTIzNjE3NzAyOTM2NTk2.XcmuEg.gZV5ETFdZiaPa90YmoCFtfodlJM"
url = "https://discord.com/api/v8/applications/{}/guilds/{}/commands".format(
    application_id, guild_id)

# json = {
#     "name": "link-meetup",
#     "description": "Links your Meetup account to grant access to game channels",
# }
json = {
    "name": "unlink-meetup",
    "description": "Unlinks your Meetup account",
}

# For authorization, you can use either your bot token
headers = {
    "Authorization": "Bot {}".format(bot_token)
}

# or a client credentials token for your app with the applications.commands.update scope
# headers = {
#     "Authorization": "Bearer abcdefg"
# }

r = requests.post(url, headers=headers, json=json)

print(r.text)
