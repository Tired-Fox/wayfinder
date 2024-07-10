use http_body_util::Full;
use hyper::body::Bytes;
use wsf::{Server, Result, Infallible, service_fn, Request, Response, router::{Router, get}};

async fn home() -> Infallible<Response> {
    Ok(Response::new(Full::new(Bytes::from("Hello, World!"))))
}

async fn unknown() -> Infallible<Response> {
    Ok(Response::new(Full::new(Bytes::from("This page is unknown :)"))))
}

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    Server::bind(([127, 0, 0, 1], 3000))
        // Enable this line to use a user defined service. WSF provides some sane defaults for
        // powerful paradigms.
        //.with_router(service_fn(handler))
        .with_router(Router::default()
            .route("/", get(home))
            .route("/unknown", get(unknown))
        )
        .run()
}
