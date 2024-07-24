use std::collections::HashMap;

// Needed to write a new custom layer
//use tower::{Layer, Service};

// TODO: Remove the need and use re-exported values
use hyper::body::Incoming;
//use hyper::{Method, StatusCode};

// TODO: Re-export from crate
use tokio::fs::File;

use wsf::server::body::Body;
use wsf::server::request::CookieJar;
use wsf::{
    server::{Response, Request, request::Cookie, router::method, Handler, FileRouter, Router, Server, LOCAL},
    layer::LogLayer,
    Result,
};

async fn home(jar: CookieJar) -> File {
    for cookie in jar.as_ref().iter() {
        println!("{} = {}", cookie.name(), cookie.value())
    }

    if jar.as_ref().get("last_wayfinder_page").is_none() {
        jar.as_mut().add(Cookie::new("last_wayfinder_page", "home"));
    }

    File::open("index.html").await.unwrap()
}

async fn request_data(req: Request<Incoming>) -> Response {
    if let Some(query) = req.uri().query() {
        match serde_qs::from_str::<HashMap<String, String>>(query) {
            Err(e) => {
                return hyper::Response::builder()
                    .status(500)
                    .body(Body::from(format!(
                        "Failed to parse uri query: {e}"
                    )))
                    .unwrap()
            }
            Ok(query) => println!("Query: {:#?}", query),
        }
    }

    hyper::Response::builder()
        .body(Body::empty())
        .unwrap()
}

async fn unknown() -> &'static str {
    "This page is unknown :)"
}

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    let fallback = || async { File::open("./pages/404.html").await.unwrap() };

    Server::bind(LOCAL, 3000)
        .with_router(
            Router::default()
                .route(
                    "/",
                    method::get(home).post(request_data),
                )
                .route("/unknown", unknown)
                .route("/blog/:*_", FileRouter::new("pages", true))
                .fallback(fallback)
                .layer(LogLayer::new("Wayfinder"))
                .into_service()
        )
        .run()
}
