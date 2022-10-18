import psycopg
import redis
import dateutil.parser
import os
import sys

conninfo = os.environ.get("CONNINFO")
if conninfo is None:
    print("Missing database connection information")
    sys.exit(-1)

r = redis.Redis(host="localhost", port=6380, db=1)

with psycopg.connect(conninfo, autocommit=True) as conn:
    cur = conn.cursor()

    discord_user_ids = [
        int(user_id.decode("utf8")) for user_id in r.smembers("discord_users")
    ]
    for discord_user_id in discord_user_ids:
        meetup_user_id = r.get(f"discord_user:{discord_user_id}:meetup_user")
        if meetup_user_id is not None:
            meetup_user_id = int(meetup_user_id.decode("utf8"))
            access_token = r.hget(
                f"meetup_user:{meetup_user_id}:oauth2_tokens", "access_token"
            )
            if access_token is not None:
                access_token = access_token.decode("utf8")
            refresh_token = r.hget(
                f"meetup_user:{meetup_user_id}:oauth2_tokens", "refresh_token"
            )
            if refresh_token is not None:
                refresh_token = refresh_token.decode("utf8")
            last_refresh_time = r.get(
                f"meetup_user:{meetup_user_id}:oauth2_tokens:last_refresh_time"
            )
            if last_refresh_time is not None:
                last_refresh_time = dateutil.parser.isoparse(
                    last_refresh_time.decode("utf8")
                )
            try:
                with conn.transaction():
                    cur.execute(
                        "INSERT INTO member (meetup_id, discord_id, meetup_oauth2_access_token, meetup_oauth2_refresh_token, meetup_oauth2_last_token_refresh_time) VALUES (%s, %s, %s, %s, %s)",
                        (
                            meetup_user_id,
                            discord_user_id,
                            access_token,
                            refresh_token,
                            last_refresh_time,
                        ),
                    )
            except psycopg.errors.UniqueViolation:
                result_row = cur.execute(
                    "SELECT meetup_id, discord_id FROM member WHERE meetup_id = %s",
                    (meetup_user_id,),
                ).fetchone()
                if result_row[0] == meetup_user_id and result_row[1] == discord_user_id:
                    continue
                else:
                    raise RuntimeError(
                        f"{meetup_user_id}<->{discord_user_id} was supposed to be inserted but {result_row[0]}<->{result_row[1]} already exists"
                    )

    print("Transferring Discord roles")
    discord_role_ids = [
        int(role_id.decode("utf8")) for role_id in r.smembers("discord_roles")
    ]
    for i, discord_role_id in enumerate(discord_role_ids):
        print(f"\r{i+1} / {len(discord_role_ids)}", end="", flush=True)
        with conn.transaction():
            cur.execute(
                "INSERT INTO event_series_role (discord_id) VALUES (%s)",
                (discord_role_id,),
            )
    print()

    print("Transferring Discord host roles")
    discord_host_role_ids = [
        int(role_id.decode("utf8")) for role_id in r.smembers("discord_host_roles")
    ]
    for i, discord_host_role_id in enumerate(discord_host_role_ids):
        print(f"\r{i+1} / {len(discord_host_role_ids)}", end="", flush=True)
        with conn.transaction():
            cur.execute(
                "INSERT INTO event_series_host_role (discord_id) VALUES (%s)",
                (discord_host_role_id,),
            )
    print()

    print("Transferring Discord text channels")
    discord_channel_ids = [
        int(channel_id.decode("utf8")) for channel_id in r.smembers("discord_channels")
    ]
    for i, discord_channel_id in enumerate(discord_channel_ids):
        print(f"\r{i+1} / {len(discord_channel_ids)}", end="", flush=True)
        expiration_time = r.get(f"discord_channel:{discord_channel_id}:expiration_time")
        if expiration_time is not None:
            expiration_time = dateutil.parser.isoparse(expiration_time.decode("utf8"))
        last_expiration_reminder_time = r.get(
            f"discord_channel:{discord_channel_id}:last_expiration_reminder_time"
        )
        if last_expiration_reminder_time is not None:
            last_expiration_reminder_time = dateutil.parser.isoparse(
                last_expiration_reminder_time.decode("utf8")
            )
        snooze_until = r.get(f"discord_channel:{discord_channel_id}:snooze_until")
        if snooze_until is not None:
            snooze_until = dateutil.parser.isoparse(snooze_until.decode("utf8"))
        deletion_time = r.get(f"discord_channel:{discord_channel_id}:deletion_time")
        if deletion_time is not None:
            deletion_time = dateutil.parser.isoparse(deletion_time.decode("utf8"))
        discord_role_id = r.get(f"discord_channel:{discord_channel_id}:discord_role")
        if discord_role_id is not None:
            discord_role_id = int(discord_role_id.decode("utf8"))
        discord_host_role_id = r.get(
            f"discord_channel:{discord_channel_id}:discord_host_role"
        )
        if discord_host_role_id is not None:
            discord_host_role_id = int(discord_host_role_id.decode("utf8"))
        with conn.transaction():
            cur.execute(
                "INSERT INTO event_series_text_channel (discord_id, expiration_time, last_expiration_reminder_time, snooze_until, deletion_time) VALUES (%s, %s, %s, %s, %s)",
                (
                    discord_channel_id,
                    expiration_time,
                    last_expiration_reminder_time,
                    snooze_until,
                    deletion_time,
                ),
            )
            if deletion_time is not None and discord_role_id is not None:
                cur.execute(
                    "UPDATE event_series_role SET deletion_time = %s WHERE discord_id = %s",
                    (
                        deletion_time,
                        discord_role_id,
                    ),
                )
            if deletion_time is not None and discord_host_role_id is not None:
                cur.execute(
                    "UPDATE event_series_host_role SET deletion_time = %s WHERE discord_id = %s",
                    (
                        deletion_time,
                        discord_host_role_id,
                    ),
                )
    print()

    print("Transferring Discord voice channels")
    discord_voice_channel_ids = [
        int(channel_id.decode("utf8"))
        for channel_id in r.smembers("discord_voice_channels")
    ]
    for i, discord_voice_channel_id in enumerate(discord_voice_channel_ids):
        print(f"\r{i+1} / {len(discord_voice_channel_ids)}", end="", flush=True)
        deletion_time = r.get(
            f"discord_voice_channel:{discord_voice_channel_id}:deletion_time"
        )
        if deletion_time is not None:
            deletion_time = dateutil.parser.isoparse(deletion_time.decode("utf8"))
        with conn.transaction():
            cur.execute(
                "INSERT INTO event_series_voice_channel (discord_id, deletion_time) VALUES (%s, %s)",
                (
                    discord_voice_channel_id,
                    deletion_time,
                ),
            )
    print()

    print("Transferring managed Discord channels")
    discord_managed_channel_ids = [
        int(channel_id.decode("utf8"))
        for channel_id in r.smembers("managed_discord_channels")
    ]
    for i, discord_managed_channel_id in enumerate(discord_managed_channel_ids):
        print(f"\r{i+1} / {len(discord_managed_channel_ids)}", end="", flush=True)
        with conn.transaction():
            cur.execute(
                "INSERT INTO managed_channel (discord_id) VALUES (%s)",
                (discord_managed_channel_id,),
            )
    print()

    print("Transferring event series")
    event_series_ids = [
        series_id.decode("utf8") for series_id in r.smembers("event_series")
    ]
    for i, event_series_id in enumerate(event_series_ids):
        print(f"\r{i+1} / {len(event_series_ids)}", end="", flush=True)
        discord_channel_id = r.get(f"event_series:{event_series_id}:discord_channel")
        discord_role_id = None
        discord_host_role_id = None
        if discord_channel_id is not None:
            discord_channel_id = int(discord_channel_id)
            discord_role_id = r.get(
                f"discord_channel:{discord_channel_id}:discord_role"
            )
            if discord_role_id is not None:
                discord_role_id = int(discord_role_id)
            discord_host_role_id = r.get(
                f"discord_channel:{discord_channel_id}:discord_host_role"
            )
            if discord_host_role_id is not None:
                discord_host_role_id = int(discord_host_role_id)
        discord_voice_channel_id = r.get(
            f"event_series:{event_series_id}:discord_voice_channel"
        )
        if discord_voice_channel_id is not None:
            discord_voice_channel_id = int(discord_voice_channel_id.decode("utf8"))
        discord_category_id = r.get(f"event_series:{event_series_id}:discord_category")
        if discord_category_id is not None:
            discord_category_id = int(discord_category_id.decode("utf8"))
        series_type = r.get(f"event_series:{event_series_id}:type")
        if series_type is not None:
            series_type = series_type.decode("utf8")
        if series_type not in ["adventure", "campaign"]:
            series_type = "adventure"
        with conn.transaction():
            result_row = cur.execute(
                "SELECT id FROM event_series WHERE redis_series_id = %s",
                (event_series_id,),
            ).fetchone()
            if result_row is None:
                cur.execute(
                    'INSERT INTO event_series (discord_text_channel_id, discord_voice_channel_id, discord_role_id, discord_host_role_id, discord_category_id, "type", redis_series_id) VALUES (%s, %s, %s, %s, %s, %s, %s)',
                    (
                        discord_channel_id,
                        discord_voice_channel_id,
                        discord_role_id,
                        discord_host_role_id,
                        discord_category_id,
                        series_type,
                        event_series_id,
                    ),
                )
    print()

    print("Transferring removed hosts and users")
    for i, discord_channel_id in enumerate(discord_channel_ids):
        print(f"\r{i+1} / {len(discord_channel_ids)}", end="", flush=True)
        sql_event_series_id = cur.execute(
            "SELECT id FROM event_series WHERE discord_text_channel_id = %s",
            (discord_channel_id,),
        ).fetchall()
        if len(sql_event_series_id) == 0:
            print("Found no event series for channel", discord_channel_id)
            continue
        elif len(sql_event_series_id) > 1:
            print("Found more than one event series for channel", discord_channel_id)
            continue
        else:
            sql_event_series_id = sql_event_series_id[0][0]
        removed_host_discord_ids = [
            int(user_id.decode("utf8"))
            for user_id in r.smembers(
                f"discord_channel:{discord_channel_id}:removed_hosts"
            )
        ]
        removed_user_discord_ids = [
            int(user_id.decode("utf8"))
            for user_id in r.smembers(
                f"discord_channel:{discord_channel_id}:removed_users"
            )
        ]
        for discord_user_id in removed_host_discord_ids:
            result_row = member_id = cur.execute(
                "SELECT id FROM member WHERE discord_id = %s", (discord_user_id,)
            ).fetchone()
            if result_row is None:
                print("\nAdding new user")
                result_row = cur.execute(
                    "INSERT INTO member (discord_id) VALUES (%s) RETURNING id",
                    (discord_user_id,),
                ).fetchone()
            member_id = result_row[0]
            with conn.transaction():
                cur.execute(
                    "INSERT INTO event_series_removed_host (event_series_id, member_id) VALUES (%s, %s)",
                    (sql_event_series_id, member_id),
                )
        for discord_user_id in removed_user_discord_ids:
            result_row = member_id = cur.execute(
                "SELECT id FROM member WHERE discord_id = %s", (discord_user_id,)
            ).fetchone()
            if result_row is None:
                print("\nAdding new user")
                result_row = cur.execute(
                    "INSERT INTO member (discord_id) VALUES (%s) RETURNING id",
                    (discord_user_id,),
                ).fetchone()
            member_id = result_row[0]
            with conn.transaction():
                cur.execute(
                    "INSERT INTO event_series_removed_user (event_series_id, member_id) VALUES (%s, %s)",
                    (sql_event_series_id, member_id),
                )
    print()

    meetup_event_ids = [
        event_id.decode("utf8") for event_id in r.smembers("meetup_events")
    ]
    print("Transferring events")
    for i, meetup_event_id in enumerate(meetup_event_ids):
        print(f"\r{i+1} / {len(meetup_event_ids)}", end="", flush=True)
        if (
            cur.execute(
                "SELECT COUNT(*) FROM meetup_event WHERE meetup_id = %s",
                (meetup_event_id,),
            ).fetchone()[0]
            == 1
        ):
            continue
        event_series_id = r.get(f"meetup_event:{meetup_event_id}:event_series").decode(
            "utf8"
        )
        sql_event_series_id = cur.execute(
            "SELECT id FROM event_series WHERE redis_series_id = %s", (event_series_id,)
        ).fetchone()[0]
        title = r.hget(f"meetup_event:{meetup_event_id}", "name").decode("utf8")
        time = dateutil.parser.isoparse(
            r.hget(f"meetup_event:{meetup_event_id}", "time").decode("utf8")
        )
        link = r.hget(f"meetup_event:{meetup_event_id}", "link").decode("utf8")
        urlname = r.hget(f"meetup_event:{meetup_event_id}", "urlname").decode("utf8")
        discord_category = r.hget(f"meetup_event:{meetup_event_id}", "discord_category")
        is_online = r.hget(f"meetup_event:{meetup_event_id}", "is_online")
        description = r.get(f"meetup_event:{meetup_event_id}:description")
        if description is not None:
            description = description.decode("utf8")
        else:
            description = ""
        if is_online is not None:
            is_online = is_online.decode("utf8")
        is_online = is_online == "true"
        if discord_category is not None:
            discord_category = int(discord_category.decode("utf8"))
        with conn.transaction():
            result_row = cur.execute(
                "INSERT INTO event (event_series_id, start_time, title, description, is_online) VALUES (%s, %s, %s, %s, %s) RETURNING id",
                (sql_event_series_id, time, title, description, is_online),
            ).fetchone()
            sql_event_id = result_row[0]
            cur.execute(
                "INSERT INTO meetup_event (event_id, meetup_id, url, urlname) VALUES (%s, %s, %s, %s)",
                (sql_event_id, meetup_event_id, link, urlname),
            )
    print()

    def member_for_meetup_id(meetup_id: int):
        with conn.transaction():
            result_row = cur.execute(
                "SELECT id FROM member WHERE meetup_id = %s", (meetup_id,)
            ).fetchone()
            if result_row is not None:
                member_id = result_row[0]
                return member_id
            # Create a new member
            discord_id = r.get(f"meetup_user:{meetup_id}:discord_user")
            if discord_id is not None:
                discord_id = int(discord_id.decode("utf8"))
                result_row = cur.execute(
                    "INSERT INTO member (meetup_id, discord_id) VALUES (%s, %s) RETURNING id",
                    (meetup_id,),
                    discord_id,
                ).fetchone()
                member_id = result_row[0]
                return member_id
            else:
                result_row = cur.execute(
                    "INSERT INTO member (meetup_id) VALUES (%s) RETURNING id",
                    (meetup_id,),
                ).fetchone()
                member_id = result_row[0]
                return member_id

    print("Transferring event hosts")
    for i, meetup_event_id in enumerate(meetup_event_ids):
        print(f"\r{i+1} / {len(meetup_event_ids)}", end="", flush=True)
        result_row = cur.execute(
            "SELECT event.id FROM event INNER JOIN meetup_event ON event.id = meetup_event.event_id WHERE meetup_event.meetup_id = %s",
            (meetup_event_id,),
        ).fetchone()
        event_id = result_row[0]
        meetup_host_ids = [
            int(host_id.decode("utf8"))
            for host_id in r.smembers(f"meetup_event:{meetup_event_id}:meetup_hosts")
        ]
        for meetup_host_id in meetup_host_ids:
            member_id = member_for_meetup_id(meetup_host_id)
            with conn.transaction():
                cur.execute(
                    "INSERT INTO event_host (event_id, member_id) VALUES (%s, %s)",
                    (event_id, member_id),
                )
    print()

    print("Transferring event participants")
    for i, meetup_event_id in enumerate(meetup_event_ids):
        print(f"\r{i+1} / {len(meetup_event_ids)}", end="", flush=True)
        result_row = cur.execute(
            "SELECT event.id FROM event INNER JOIN meetup_event ON event.id = meetup_event.event_id WHERE meetup_event.meetup_id = %s",
            (meetup_event_id,),
        ).fetchone()
        event_id = result_row[0]
        meetup_participant_ids = [
            int(participant_id.decode("utf8"))
            for participant_id in r.smembers(
                f"meetup_event:{meetup_event_id}:meetup_users"
            )
        ]
        for meetup_participant_id in meetup_participant_ids:
            member_id = member_for_meetup_id(meetup_participant_id)
            with conn.transaction():
                cur.execute(
                    "INSERT INTO event_participant (event_id, member_id) VALUES (%s, %s)",
                    (event_id, member_id),
                )
    print()

    meetup_access_token = r.get("meetup_access_token")
    if meetup_access_token is not None:
        print("Transferring organizer token")
        meetup_access_token = meetup_access_token.decode("utf8")
        meetup_refresh_token = r.get("meetup_refresh_token")
        if meetup_refresh_token is not None:
            meetup_refresh_token = meetup_refresh_token.decode("utf8")
        meetup_access_token_refresh_time = r.get("meetup_access_token_refresh_time")
        if meetup_access_token_refresh_time is not None:
            meetup_access_token_refresh_time = dateutil.parser.isoparse(
                meetup_access_token_refresh_time.decode("utf8")
            )
        with conn.transaction():
            cur.execute(
                "INSERT INTO organizer_token (meetup_access_token, meetup_refresh_token, meetup_access_token_refresh_time) VALUES (%s, %s, %s)",
                (
                    meetup_access_token,
                    meetup_refresh_token,
                    meetup_access_token_refresh_time,
                ),
            )
