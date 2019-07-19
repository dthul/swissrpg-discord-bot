//!
//! This example showcases the Github OAuth2 process for requesting access to the user's public repos and
//! email address.
//!
//! Before running it, you'll need to generate your own Github OAuth2 credentials.
//!
//! In order to run the example call:
//!
//! ```sh
//! GITHUB_CLIENT_ID=xxx GITHUB_CLIENT_SECRET=yyy cargo run --example github
//! ```
//!
//! ...and follow the instructions.
//!

use hyper::rt::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Method, Request, Response, Server};
use oauth2::basic::BasicClient;
use oauth2::reqwest::http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use std::env;
use std::sync::{Arc, Mutex};
use url::Url;

fn meetup_auth(
    oauth_client: &BasicClient,
    csrf_token: &Arc<Mutex<Option<CsrfToken>>>,
    req: Request<Body>,
) -> Response<Body> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            // Generate the authorization URL to which we'll redirect the user.
            let (authorize_url, csrf_state) = oauth_client
                .authorize_url(CsrfToken::new_random)
                // This example is requesting access to the user's public repos and email.
                .add_scope(Scope::new("ageless".to_string()))
                .add_scope(Scope::new("basic".to_string()))
                .add_scope(Scope::new("event_management".to_string()))
                .url();
            // Store the generated CSRF token so we can compare it to the one
            // returned by Meetup later
            *csrf_token.lock().unwrap() = Some(csrf_state);
            let html_body = format!("<a href=\"{}\">Login with Meetup</a>", authorize_url);
            Response::new(html_body.into())
        }
        (&Method::GET, "/redirect") => {
            let req_url = Url::parse(&req.uri().to_string()).unwrap();
            let params: Vec<_> = req_url.query_pairs().collect();
            let code = params
                .iter()
                .find_map(|(key, value)| if key == "code" { Some(value) } else { None });
            let state = params
                .iter()
                .find_map(|(key, value)| if key == "state" { Some(value) } else { None });
            let error = params
                .iter()
                .find_map(|(key, value)| if key == "error" { Some(value) } else { None });
            if let Some(error) = error {
                return Response::new(format!("OAuth error: {}", error).into());
            }
            match (code, state) {
                (Some(code), Some(state)) => {
                    if let Some(ref csrf_state) = *csrf_token.lock().unwrap() {
                        // Compare the CSRF state that was returned by Meetup to the one
                        // we have saved
                        if csrf_state.secret() == state {
                            // Exchange the code with a token.
                            let code = AuthorizationCode::new(code.to_string());
                            let token_res = oauth_client.exchange_code(code).request(http_client);
                            if let Ok(token_res) = token_res {
                                println!("Access token: {}", token_res.access_token().secret());
                                println!("Refresh token: {:?}", token_res.refresh_token().map(|t| t.secret()));
                            }
                        }
                    }
                }
                _ => (),
            };
            Response::new("Whoops, something went wrong".into())
        }
        _ => Response::new("Unknown route".into()),
    }
}

fn main() {
    let meetup_client_id = ClientId::new(
        env::var("MEETUP_CLIENT_ID").expect("Missing the MEETUP_CLIENT_ID environment variable."),
    );
    let meetup_client_secret = ClientSecret::new(
        env::var("MEETUP_CLIENT_SECRET")
            .expect("Missing the MEETUP_CLIENT_SECRET environment variable."),
    );
    let auth_url = AuthUrl::new(
        Url::parse("https://secure.meetup.com/oauth2/authorize")
            .expect("Invalid authorization endpoint URL"),
    );
    let token_url = TokenUrl::new(
        Url::parse("https://secure.meetup.com/oauth2/access").expect("Invalid token endpoint URL"),
    );

    // Set up the config for the Github OAuth2 process.
    let client = BasicClient::new(
        meetup_client_id,
        Some(meetup_client_secret),
        auth_url,
        Some(token_url),
    )
    // This example will be running its own server at localhost:8080.
    // See below for the server implementation.
    .set_redirect_url(RedirectUrl::new(
        Url::parse("http://bot.8na.de/redirect").expect("Invalid redirect URL"),
    ));
    let client = Arc::new(client);
    let csrf_token = Arc::new(Mutex::new(None));

    // let serve_fn = |req: Request<Body>| -> Response<Body> { meetup_auth(&client, req) };
    // let service_fn = service_fn_ok(serve_fn);

    let addr = ([127, 0, 0, 1], 3000).into();

    // And a MakeService to handle each connection...
    let make_service = move || {
        let client = client.clone();
        let csrf_token = csrf_token.clone();
        service_fn_ok(move |req| meetup_auth(&client, &csrf_token, req))
    };
    let server = Server::bind(&addr).serve(make_service);

    hyper::rt::run(server.map_err(|e| {
        eprintln!("server error: {}", e);
    }));

    // A very naive implementation of the redirect server.
    // let listener = TcpListener::bind("127.0.0.1:8080").unwrap();
    // for stream in listener.incoming() {
    //     if let Ok(mut stream) = stream {
    //         let code;
    //         let state;
    //         {
    //             let mut reader = BufReader::new(&stream);

    //             let mut request_line = String::new();
    //             reader.read_line(&mut request_line).unwrap();

    //             let redirect_url = request_line.split_whitespace().nth(1).unwrap();
    //             let url = Url::parse(&("http://localhost".to_string() + redirect_url)).unwrap();

    //             let code_pair = url
    //                 .query_pairs()
    //                 .find(|pair| {
    //                     let &(ref key, _) = pair;
    //                     key == "code"
    //                 })
    //                 .unwrap();

    //             let (_, value) = code_pair;
    //             code = AuthorizationCode::new(value.into_owned());

    //             let state_pair = url
    //                 .query_pairs()
    //                 .find(|pair| {
    //                     let &(ref key, _) = pair;
    //                     key == "state"
    //                 })
    //                 .unwrap();

    //             let (_, value) = state_pair;
    //             state = CsrfToken::new(value.into_owned());
    //         }

    //         let message = "Go back to your terminal :)";
    //         let response = format!(
    //             "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
    //             message.len(),
    //             message
    //         );
    //         stream.write_all(response.as_bytes()).unwrap();

    //         println!("Github returned the following code:\n{}\n", code.secret());
    //         println!(
    //             "Github returned the following state:\n{} (expected `{}`)\n",
    //             state.secret(),
    //             csrf_state.secret()
    //         );

    //         // Exchange the code with a token.
    //         let token_res = client.exchange_code(code).request(http_client);

    //         println!("Github returned the following token:\n{:?}\n", token_res);

    //         if let Ok(token) = token_res {
    //             // NB: Github returns a single comma-separated "scope" parameter instead of multiple
    //             // space-separated scopes. Github-specific clients can parse this scope into
    //             // multiple scopes by splitting at the commas. Note that it's not safe for the
    //             // library to do this by default because RFC 6749 allows scopes to contain commas.
    //             let scopes = if let Some(scopes_vec) = token.scopes() {
    //                 scopes_vec
    //                     .iter()
    //                     .map(|comma_separated| comma_separated.split(","))
    //                     .flat_map(|inner_scopes| inner_scopes)
    //                     .collect::<Vec<_>>()
    //             } else {
    //                 Vec::new()
    //             };
    //             println!("Github returned the following scopes:\n{:?}\n", scopes);
    //         }

    //         // The server will terminate itself after collecting the first code.
    //         break;
    //     }
    // }
}
