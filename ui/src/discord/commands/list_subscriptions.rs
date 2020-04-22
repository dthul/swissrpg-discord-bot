use command_macro::command;

#[command]
#[regex(r"list\s*subscriptions")]
#[level(admin)]
fn list_subscriptions(
    context: super::CommandContext,
    _: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    let _ = context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder.content("Sure! This might take a moment...")
        });
    let runtime_mutex = context.async_runtime()?.clone();
    let runtime_guard = futures::executor::block_on(runtime_mutex.read());
    let async_runtime = match *runtime_guard {
        Some(ref async_runtime) => async_runtime,
        None => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                "Could not submit asynchronous list subscription task",
            );
            return Ok(());
        }
    };
    let subscriptions = {
        let stripe_client = context.stripe_client()?;
        async_runtime.enter(|| {
            futures::executor::block_on(lib::stripe::list_active_subscriptions(stripe_client))
        })?
    };
    let mut message = String::new();
    for subscription in &subscriptions {
        // First, figure out which product was bought
        // Subscription -> Plan -> Product
        let product = match &subscription.plan {
            Some(plan) => match &plan.product {
                Some(product) => match product {
                    stripe::Expandable::Object(product) => Some(*product.clone()),
                    stripe::Expandable::Id(product_id) => {
                        let product = {
                            let stripe_client = context.stripe_client()?;
                            async_runtime.enter(|| {
                                futures::executor::block_on(stripe::Product::retrieve(
                                    stripe_client,
                                    product_id,
                                    &[],
                                ))
                            })?
                        };
                        Some(product)
                    }
                },
                _ => None,
            },
            _ => None,
        };
        // Now, figure out who the customer is
        let customer = match &subscription.customer {
            stripe::Expandable::Object(customer) => *customer.clone(),
            stripe::Expandable::Id(customer_id) => {
                let customer = {
                    let stripe_client = context.stripe_client()?;
                    async_runtime.enter(|| {
                        futures::executor::block_on(stripe::Customer::retrieve(
                            stripe_client,
                            customer_id,
                            &[],
                        ))
                    })?
                };
                customer
            }
        };
        let discord_handle = customer.metadata.get("Discord");
        message.push_str(&format!(
            "Customer: {:?}, Discord: {:?}, Product: {:?}\n",
            &customer.email,
            discord_handle,
            product.map(|p| p.name)
        ));
    }
    let _ = context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder.content(format!("Active subscriptions:\n{}", message))
        });
    Ok(())
}
