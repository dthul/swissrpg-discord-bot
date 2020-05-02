use command_macro::command;

#[command]
#[regex(r"clone\s+event\s+(?P<meetup_event_id>[^\s]+)")]
#[level(admin)]
fn clone_event(
    context: super::CommandContext<'_>,
    captures: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    // Get the mentioned Meetup event
    let meetup_event_id = captures.name("meetup_event_id").unwrap().as_str();
    let async_meetup_client = context.meetup_client()?.clone();
    let async_runtime_mutex = context.async_runtime()?.clone();
    let urlname = "SwissRPG-Zurich";
    let future = async {
        // Clone the async meetup client
        let guard = async_meetup_client.lock().await;
        let client = guard.clone();
        drop(guard);
        let client = match client {
            None => {
                return Err(lib::meetup::Error::from(simple_error::SimpleError::new(
                    "Async Meetup client not set",
                )))
            }
            Some(client) => client,
        };
        let new_event_hook = Box::new(|new_event: lib::meetup::api::NewEvent| {
            let date_time = new_event.time.clone();
            lib::flow::ScheduleSessionFlow::new_event_hook(
                new_event,
                date_time,
                meetup_event_id,
                false,
            )
        });
        let new_event = lib::meetup::util::clone_event(
            urlname,
            meetup_event_id,
            client.as_ref(),
            Some(new_event_hook),
        )
        .await?;
        // Try to transfer the RSVPs to the new event
        let mut redis_connection = context.redis_client()?.get_async_connection().await?;
        if let Err(_) = lib::meetup::util::clone_rsvps(
            urlname,
            meetup_event_id,
            &new_event.id,
            &mut redis_connection,
            client.as_ref(),
            context.oauth2_consumer()?,
        )
        .await
        {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, "Could not transfer all RSVPs to the new event");
        }
        Ok(new_event)
    };
    let runtime_guard = futures::executor::block_on(async_runtime_mutex.read());
    let async_runtime = match *runtime_guard {
        Some(ref async_runtime) => async_runtime,
        None => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                "Could not submit asynchronous event cloning task",
            );
            return Ok(());
        }
    };
    match async_runtime.enter(|| futures::executor::block_on(future)) {
        Ok(new_event) => {
            let _ = context.msg.react(context.ctx, "\u{2705}");
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!("Created new Meetup event: {}", new_event.link),
            );
        }
        Err(err) => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, "Something went wrong");
            eprintln!(
                "Could not clone Meetup event {}. Error:\n{:#?}",
                meetup_event_id, err
            );
        }
    }
    Ok(())
}
