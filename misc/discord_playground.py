import discord

c = discord.Client()


def run(co):
    return c.loop.run_until_complete(co)


run(c.login("NjAwNzUyMTA1NTE4NzkyNzE2.XS4T_w.I58xYLSGc-Y5-i0n32Nt2Xx5icY"))

guild = run(c.fetch_guild(401856510709202945))
channel = run(c.fetch_channel(781916550515916800))
member = run(guild.fetch_member(554345195630624780))
run(
    channel.set_permissions(
        member,
        overwrite=discord.PermissionOverwrite(
            read_messages=True, mention_everyone=True, manage_messages=True
        ),
    )
)
run(
    channel.set_permissions(
        member,
        overwrite=discord.PermissionOverwrite(read_messages=True),
    )
)

hyperion = run(guild.fetch_member(600752105518792716))
daniel = run(guild.fetch_member(456545153923022849))
everyone = guild.default_role
run(
    guild.create_text_channel(
        "test channel",
        overwrites={
            everyone: discord.PermissionOverwrite(read_messages=False),
            hyperion: discord.PermissionOverwrite(
                read_messages=True, manage_permissions=True
            ),
            daniel: discord.PermissionOverwrite(read_messages=True),
        },
    )
)
