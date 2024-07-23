use std::{collections::HashMap, task::{Context, Poll}};

use http_body_util::Full;
use hyper::{body::Bytes, header::HeaderValue};
use tower::{Layer, Service};
use wsf::{Server, Result, Infallible, service_fn, Handler, Request, Response, router::{Router, get}};

#[derive(Clone)]
pub struct LogLayer {
    target: &'static str,
}
impl LogLayer {
    pub fn new(target: &'static str) -> Self {
        Self { target }
    }
}

impl<S: Clone> Layer<S> for LogLayer {
    type Service = LogService<S>;

    fn layer(&self, service: S) -> Self::Service {
        LogService {
            target: self.target,
            service
        }
    }
}

// This service implements the Log behavior
#[derive(Clone)]
pub struct LogService<S: Clone> {
    target: &'static str,
    service: S,
}

impl<S, Request> Service<Request> for LogService<S>
where
    S: Service<Request> + Clone,
    Request: std::fmt::Debug,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Insert log statement here or other functionality
        println!("request = {:?}, target = {:?}", request, self.target);
        self.service.call(request)
    }
}

async fn home() -> Response {
    let mut response = Response::new(Full::new(Bytes::from(r#"<html>
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Home</title>
        <script>
            async function fetchData() {
                const response = await fetch('/?foo=bar&baz=qux', {
                    method: 'POST',
                });

                if (response.ok) {
                    alert('Success!');
                } else {
                    alert('Failed!');
                }
            }
        </script>
    </head>
    <body>
        <h1>Hello, World!</h1>
        <button onclick="fetchData()">Fetch Data</button>
    </body>
</html>"#)));

    response.headers_mut().insert("Content-Type", HeaderValue::from_str("text/html").unwrap());
    response
}

async fn request_data(req: Request) -> Response {
    if let Some(query) = req.uri().query() {
        match serde_qs::from_str::<HashMap<String, String>>(query) {
            Err(e) => return hyper::Response::builder().status(500).body(Full::new(Bytes::from(format!("Failed to parse uri query: {e}")))).unwrap(),
            Ok(query) => println!("Query: {:#?}", query),
        }
    }

    hyper::Response::builder().status(200).body(Full::new(Bytes::default())).unwrap()
}

async fn unknown() -> Response {
    Response::new(Full::new(Bytes::from("This page is unknown :)")))
}

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    Server::bind(([127, 0, 0, 1], 3000))
        // Enable this line to use a user defined service. WSF provides some sane defaults for
        // powerful paradigms.
        //.with_router(home)
        .with_router(
            Router::default()
                .route("/", get(home).post(request_data.layer(LogLayer::new("TESTING"))))
                .route("/unknown", get(unknown))
        )
        .run()
}
