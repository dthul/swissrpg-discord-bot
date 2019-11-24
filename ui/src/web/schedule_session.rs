use super::server::HandlerResponse;
use askama::Template;

use warp::Filter;

pub fn create_routes(
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let route = warp::get()
        .and(warp::path!("schedule_session" / u64))
        .and_then(|_flow_id| {
            async {
                HandlerResponse::from_template(ScheduleSessionTemplate {
                    day: 12,
                    month: 12,
                    year: 2019,
                    hour: 18,
                    minute: 45,
                    selectable_years: &[2019, 2020],
                })
                .map_err(|err| warp::reject::custom(err))
            }
        });
    route
}

#[derive(Template)]
#[template(path = "schedule_session.html")]
struct ScheduleSessionTemplate<'a> {
    day: u8,
    month: u8,
    year: u16,
    hour: u8,
    minute: u8,
    selectable_years: &'a [u16],
}

pub mod filters {
    pub fn isequal<T: num_traits::PrimInt>(num: &T, val: &T) -> Result<bool, askama::Error> {
        Ok(num == val)
    }
}
