use redis::AsyncCommands;

pub struct SyncStats {
    pub num_events_added: u64,
    pub num_participants_added: u64,
    pub num_hosts_added: u64,
    pub num_links_added: u64,
    pub num_errors: u64,
}

pub async fn sync_redis_to_postgres(
    con: &mut redis::aio::Connection,
    pool: &sqlx::PgPool,
    discord_api: &mut crate::discord::CacheAndHttp,
) -> Result<SyncStats, crate::meetup::Error> {
    // Move all game events to Postgres
    // let mut series_id_iter: redis::AsyncIter<'_, String> = con.sscan("event_series").await?;
    // while let Some(series_id) = series_id_iter.next_item().await {
    let mut num_events_added = 0;
    let mut num_participants_added = 0;
    let mut num_hosts_added = 0;
    let mut num_links_added = 0;
    let mut num_errors = 0u64;
    let series_ids: Vec<String> = con.smembers("event_series").await?;
    for series_id in &series_ids {
        // Not stored per-game at the moment
        let is_online: Option<String> = con
            .get(format!("event_series:{}:is_online", series_id))
            .await?;
        let is_online = is_online.map(|v| v == "true").unwrap_or(false);
        // Get event information from Redis
        let events = crate::meetup::util::get_events_for_series_async(con, series_id).await?;
        for event in &events {
            // Ignore the event if it is in the future
            if event.time > chrono::Utc::now() {
                continue;
            }
            match sqlx::query!("INSERT INTO game_events(meetup_id, \"start\", \"end\", name, is_online, event_series_id, urlname) VALUES($1, $2, $3, $4, $5, $6, $7) ON CONFLICT DO NOTHING", &event.id, &event.time, Option::<chrono::DateTime<chrono::Utc>>::None, &event.name, is_online, series_id, &event.urlname).execute(pool).await {
                Err(err) => {
                    eprintln!("Failed to write event information to Postgres:\n{:#}", err);
                    num_errors += 1;
                }
                Ok(res) => num_events_added += res.rows_affected()
            }
            // Also store the event participants and hosts in Postgres
            match crate::redis::get_events_participants(
                &[event.id.as_str()],
                /*hosts*/ false,
                con,
            )
            .await
            {
                Err(err) => {
                    eprintln!(
                        "Could not get the a list of event participants from Redis:\n{:#}",
                        err
                    );
                    num_errors += 1;
                }
                Ok(user_ids) => {
                    for &user_id in &user_ids {
                        match sqlx::query!("INSERT INTO game_event_participants(event_meetup_id, user_meetup_id) VALUES($1, $2) ON CONFLICT DO NOTHING", &event.id, user_id as i64).execute(pool).await {
                            Err(err) => eprintln!("Failed to add event-participant mapping to Postgres:\n{:#}", err),
                            Ok(res) => num_participants_added += res.rows_affected()
                        }
                    }
                }
            }
            match crate::redis::get_events_participants(
                &[event.id.as_str()],
                /*hosts*/ true,
                con,
            )
            .await
            {
                Err(err) => {
                    eprintln!(
                        "Could not get the a list of event hosts from Redis:\n{:#}",
                        err
                    );
                    num_errors += 1;
                }
                Ok(user_ids) => {
                    for &user_id in &user_ids {
                        match sqlx::query!("INSERT INTO game_event_hosts(event_meetup_id, user_meetup_id) VALUES($1, $2) ON CONFLICT DO NOTHING", &event.id, user_id as i64).execute(pool).await {
                            Err(err) => eprintln!("Failed to add event-host mapping to Postgres:\n{:#}", err),
                            Ok(res) => num_hosts_added += res.rows_affected()
                        }
                    }
                }
            }
        }
    }
    // Get linking information
    let user_meetup_ids: Vec<u64> = con.smembers("meetup_users").await?;
    for &user_meetup_id in &user_meetup_ids {
        let user_discord_id: Option<u64> = con
            .get(format!("meetup_user:{}:discord_user", user_meetup_id))
            .await?;
        if let Some(user_discord_id) = user_discord_id {
            let mut transaction = pool.begin().await?;
            let count = sqlx::query!(
                "SELECT COUNT(*) FROM meetup_discord_linking WHERE meetup_id = $1 AND discord_id = $2",
                user_meetup_id as i64,
                user_discord_id as i64
            )
            .fetch_one(&mut transaction)
            .await?.count.unwrap_or(0);
            if count == 1 {
                // Nothing to do, the correct mapping is already in the database
                continue;
            }
            // Clear any potentially stale mapping and write the new one
            sqlx::query!(
                "DELETE FROM meetup_discord_linking WHERE meetup_id = $1 OR discord_id = $2",
                user_meetup_id as i64,
                user_discord_id as i64
            )
            .execute(&mut transaction)
            .await?;
            sqlx::query!(
                "INSERT INTO meetup_discord_linking(meetup_id, discord_id) VALUES($1, $2)",
                user_meetup_id as i64,
                user_discord_id as i64
            )
            .execute(&mut transaction)
            .await?;
            transaction.commit().await?;
            num_links_added += 1;
        }
    }
    // Get nickname information
    for user in discord_api.cache.users().await.values() {
        sqlx::query!(
            "INSERT INTO discord_nicknames(discord_id, nick) VALUES($1, $2) ON CONFLICT (discord_id) DO UPDATE SET nick = EXCLUDED.nick",
            user.id.0 as i64,
            &user.name
        ).execute(pool).await?;
    }
    Ok(SyncStats {
        num_events_added,
        num_participants_added,
        num_hosts_added,
        num_links_added,
        num_errors,
    })
}
