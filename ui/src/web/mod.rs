pub mod linking;
pub mod schedule_session;
pub mod server;

use askama::Template;

#[derive(Template)]
#[template(path = "message.html")]
struct MessageTemplate<'a> {
    title: &'a str,
    content: Option<&'a str>,
    safe_content: Option<&'a str>,
    img_url: Option<&'a str>,
}
