use rand::Rng;
use redis::AsyncCommands;

use crate::db;

pub struct ScheduleSessionFlow {
    pub id: u64,
    pub event_series_id: db::EventSeriesId,
}

impl ScheduleSessionFlow {
    pub async fn new(
        redis_connection: &mut redis::aio::Connection,
        event_series_id: db::EventSeriesId,
    ) -> Result<Self, crate::meetup::Error> {
        let id: u64 = rand::thread_rng().gen();
        let redis_key = format!("flow:schedule_session:{}", id);
        let mut pipe = redis::pipe();
        let _: () = pipe
            .hset(&redis_key, "event_series_id", event_series_id.0)
            .ignore()
            .expire(&redis_key, 10 * 60)
            .query_async(redis_connection)
            .await?;
        Ok(ScheduleSessionFlow {
            id,
            event_series_id,
        })
    }

    pub async fn retrieve(
        redis_connection: &mut redis::aio::Connection,
        id: u64,
    ) -> Result<Option<Self>, crate::meetup::Error> {
        let redis_key = format!("flow:schedule_session:{}", id);
        let event_series_id: Option<i32> =
            redis_connection.hget(&redis_key, "event_series_id").await?;
        let flow = event_series_id.map(|event_series_id| ScheduleSessionFlow {
            id: id,
            event_series_id: db::EventSeriesId(event_series_id),
        });
        Ok(flow)
    }

    pub async fn delete(
        self,
        redis_connection: &mut redis::aio::Connection,
    ) -> Result<(), crate::meetup::Error> {
        let redis_key = format!("flow:schedule_session:{}", self.id);
        let () = redis_connection.del(&redis_key).await?;
        Ok(())
    }
}
