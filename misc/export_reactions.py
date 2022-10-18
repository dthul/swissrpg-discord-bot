import discord

c = discord.Client()


def run(co):
    return c.loop.run_until_complete(co)


run(c.login("NjAwNzUyMTA1NTE4NzkyNzE2.XS4T_w.I58xYLSGc-Y5-i0n32Nt2Xx5icY"))
# channel = run(c.fetch_channel(603487434952671232))
# message = run(channel.fetch_message(902522856137711646))

# channel = run(c.fetch_channel(613638356542423071))
# message = run(channel.fetch_message(918228392241922058))

# channel = run(c.fetch_channel(603487434952671232))
# message = run(channel.fetch_message(907279173448523866))

# channel = run(c.fetch_channel(603487434952671232))
# message = run(channel.fetch_message(920683780673507328))

# channel = run(c.fetch_channel(603487434952671232))
# message = run(channel.fetch_message(920694815098830938))

# channel = run(c.fetch_channel(603487434952671232))
# message = run(channel.fetch_message(922209594590249061))

# channel = run(c.fetch_channel(603487434952671232))
# message = run(channel.fetch_message(925019403270107207))

# channel = run(c.fetch_channel(603487434952671232))
# message = run(channel.fetch_message(931963550275080193))

channel = run(c.fetch_channel(685792265301655578))
message = run(channel.fetch_message(955415274838949908))


async def create_users_csv_for_message(message, output_path_prefix):
    def reaction_matches(reaction, name: str):
        if isinstance(reaction.emoji, str):
            return reaction.emoji == name
        elif hasattr(reaction.emoji, "name"):
            return reaction.emoji.name == name
        else:
            return False

    # reaction = next(filter(lambda reaction: reaction_matches(
    #     reaction, reaction_name), message.reactions))
    def reaction_to_string(reaction):
        if isinstance(reaction.emoji, str):
            return reaction.emoji.encode("unicode_escape")[1:].decode("utf8")
        elif hasattr(reaction.emoji, "name"):
            return reaction.emoji.name
        else:
            raise RuntimeError("Could not get string for reaction: " + str(reaction))

    guild = await c.fetch_guild(message.guild.id)
    for reaction in message.reactions:
        # if not reaction_matches(reaction, "SwissRPG"):
        #     continue
        print(f"Reaction {reaction_to_string(reaction)}")
        users = []
        async for user in reaction.users():
            users.append(user)
        members = [await guild.fetch_member(user.id) for user in users]
        output_path = f"{output_path_prefix}_{reaction_to_string(reaction)}.csv"
        with open(output_path, "w") as f:
            f.write("Id,Discord Name,GM,Champion\n")
            member_num = 0
            for member in members:
                print(f"\rMember {member_num + 1}", end="")
                is_gm = any(role.name == "Game Master" for role in member.roles)
                is_champion = any(
                    role.name.endswith("Champion") for role in member.roles
                )
                escaped_name = member.name.replace('"', '""')
                f.write(
                    f'{member.id},"@{escaped_name}#{member.discriminator}",{is_gm},{is_champion}\n'
                )
                member_num += 1
            print()
    print("done")


run(create_users_csv_for_message(message, "reaction_users"))
