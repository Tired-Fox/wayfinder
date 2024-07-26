use std::collections::HashMap;

use wsf::{
    extract::{Capture, Cookie, CookieJar, File, Json, Query},
    layer::LogLayer,
    prelude::*,
    server::{methods, FileRouter, PathRouter, Server, LOCAL},
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

async fn request_data(Json(data): Json<HashMap<String, String>>) -> impl IntoResponse {
    Json(data)
}

async fn unknown(
    Capture(rest): Capture<String>,
    q: Option<Query<HashMap<String, String>>>,
) -> impl IntoResponse {
    format!("Query: `{q:#?}`\nRest: `{rest}`")
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
                .route("/unknown/:*rest", unknown)
                .route("/blog/:*_", FileRouter::new("pages", true))
                .fallback(fallback)
                .layer(LogLayer::new("Wayfinder"))
                .into_service(),
        )
        .run()
}
