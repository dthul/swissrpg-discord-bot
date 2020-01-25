pub async fn list_active_subscriptions(
    client: &stripe::Client,
) -> Result<Vec<stripe::Subscription>, crate::meetup::Error> {
    let mut subscription_cursor = stripe::Subscription::list(
        client,
        stripe::ListSubscriptions {
            status: Some(stripe::SubscriptionStatusFilter::Active),
            ..Default::default()
        },
    )
    .await?;
    let mut all_subscriptions = vec![];
    loop {
        if subscription_cursor.has_more {
            let next_cursor = subscription_cursor.next(client).await?;
            all_subscriptions.extend(subscription_cursor.data);
            subscription_cursor = next_cursor;
        } else {
            all_subscriptions.extend(subscription_cursor.data);
            break;
        }
    }
    Ok(all_subscriptions)
}
