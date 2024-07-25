use std::collections::HashMap;

use wsf::{
    extract::{File, Capture, CookieJar, Cookie},
    layer::LogLayer,
    server::{
        methods, prelude::*, FileRouter, Request, Response, PathRouter, Server, LOCAL
    },
    Result,
};

async fn home(jar: CookieJar) -> impl IntoResponse {
    for cookie in jar.as_ref().iter() {
        println!("{} = {}", cookie.name(), cookie.value())
    }

    if jar.as_ref().get("last_wayfinder_page").is_none() {
        jar.as_mut().add(Cookie::new("last_wayfinder_page", "home"));
    }

    File::open("index.html").await.unwrap()
}

async fn request_data(req: Request) -> impl IntoResponse {
    if let Some(query) = req.uri().query() {
        match serde_qs::from_str::<HashMap<String, String>>(query) {
            Err(e) => {
                return Response::error(500, format!("Failed to parse uri query: {e}"));
            }
            Ok(query) => println!("Query: {:#?}", query),
        }
    }

    Response::empty(200)
}

async fn unknown(Capture((sub, rest)): Capture<(String, String)>) -> impl IntoResponse {
    format!("Sub: `{sub}`\nRest: `{rest}`")
}

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    let fallback = || async { (404, File::open("./pages/404.html").await.unwrap()) };

    Server::bind(LOCAL, 3000)
        .with_router(
            PathRouter::default()
                .route("/", methods::get(home).post(request_data))
                .route("/unknown/:sub/:*rest", unknown)
                .route("/blog/:*_", FileRouter::new("pages", true))
                .fallback(fallback)
                .layer(LogLayer::new("Wayfinder"))
                .into_service(),
        )
        .run()
}
