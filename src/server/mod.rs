use std::convert::Infallible;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::pin::Pin;

pub use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio_util::codec::{BytesCodec, FramedRead};
use tower::Service;

pub mod router;
pub(crate) mod future;
pub(crate) mod handler;

pub mod request;
pub mod response;
pub mod body;

pub use handler::Handler;
pub use router::{Router, FileRouter, methods};
pub use request::Request;
pub use response::Response;
pub use body::Body;

use crate::Result;

pub static NETWORK: [u8; 4] = [0, 0, 0, 0];
pub static LOCAL: [u8; 4] = [127, 0, 0, 1];

pub mod prelude {
    use hyper::{body::Bytes, StatusCode};

    use super::{body::BoxError, Body, Response};
    pub use super::Handler;

    pub trait ResponseShortcut {
        fn empty<T>(status: T) -> Response
        where
            StatusCode: TryFrom<T>,
            <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>;

        fn ok<B>(body: B) -> Response
        where
            B: http_body::Body<Data = Bytes> + Send + 'static,
            B::Error: Into<BoxError>;

        fn error<T, B>(status: T, body: B) -> Response
        where
            B: http_body::Body<Data = Bytes> + Send + 'static,
            B::Error: Into<BoxError>,
            StatusCode: TryFrom<T>,
            <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>;
    }

    impl ResponseShortcut for Response {
        fn empty<T>(status: T) -> Response
        where
            StatusCode: TryFrom<T>,
            <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>
        {
            Response::builder()
                .status(status)
                .body(Body::empty())
                .unwrap()
        }

        fn ok<B>(body: B) -> Response
                where
                    B: http_body::Body<Data = Bytes> + Send + 'static,
                    B::Error: Into<BoxError> {

            Response::builder()
                .body(Body::new(body))
                .unwrap()
        } 

        fn error<T, B>(status: T, body: B) -> Response
        where
            B: http_body::Body<Data = Bytes> + Send + 'static,
            B::Error: Into<BoxError>,
            StatusCode: TryFrom<T>,
            <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>
        {

            Response::builder()
                .status(status)
                .body(Body::new(body))
                .unwrap()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Server<R>
where
    R: Service<Request<Incoming>, Response = Response<Body>, Error = Infallible> + Send + Clone + 'static,
    <R as Service<Request<Incoming>>>::Future: Send,
{
    address: SocketAddr,
    router: R,
}

impl Server<FileRouter> {
    pub fn bind<I: Into<IpAddr>>(address: I, port: u16) -> Self {
        Self {
            address: SocketAddr::new(address.into(), port),
            router: FileRouter::new("pages", false),
        }
    }
}

impl<R> Server<R>
where
    R: Service<Request<Incoming>, Response = Response<Body>, Error = Infallible> + Send + Clone + 'static,
    <R as Service<Request<Incoming>>>::Future: Send,
{
    pub fn with_router<N>(self, router: N) -> Server<N>
    where
        N: Service<Request<Incoming>, Response = Response<Body>, Error = Infallible> + Send + Clone + 'static,
        <N as Service<Request<Incoming>>>::Future: Send,
    {
        Server::<N> {
            address: self.address,
            router,
        } 
    }

    pub fn run(self) -> Result<()> {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()?
            .block_on(async move {
                let listener = TcpListener::bind(self.address).await?;
                log::info!("Listening to \x1b[33m{}\x1b[39m", self.address);

                let router = TowerToHyperService::new(self.router);

                loop {
                    let (stream, _) = listener.accept().await?;
                    let io = TokioIo::new(stream);
                    let router = router.clone();
                    tokio::task::spawn(async move {
                        if let Err(err) = http1::Builder::new().serve_connection(io, router).await {
                            eprintln!("Error serving connection: {:?}", err);
                        }
                    });
                }
            })
    }
}

trait IntoColorMethod {
    fn into_color_method(self) -> &'static str;
}

impl IntoColorMethod for &hyper::Method {
    fn into_color_method(self) -> &'static str {
        match *self {
            hyper::Method::GET => "\x1b[46;30m GET \x1b[49m",
            hyper::Method::POST => "\x1b[45;30m POST \x1b[49m",
            hyper::Method::PUT => "\x1b[45;30m PUT \x1b[49m",
            hyper::Method::DELETE => "\x1b[41;30m DELETE \x1b[49m",
            hyper::Method::HEAD => "\x1b[44;30m HEAD \x1b[49m",
            hyper::Method::OPTIONS => "\x1b[44;30m OPTIONS \x1b[49m",
            hyper::Method::CONNECT => "\x1b[44;30m CONNECT \x1b[49m",
            hyper::Method::PATCH => "\x1b[43;30m PATCH \x1b[49m",
            hyper::Method::TRACE => "\x1b[43;30m TRACE \x1b[49m",
            _ => "\x1b[43;30m UNKNOWN \x1b[49m",
        }
    }
}

#[derive(Clone, Copy)]
pub struct DefaultRouter;
impl Service<Request<Incoming>> for DefaultRouter {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::prelude::v1::Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(())) 
    }

    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        Box::pin(async move {
            let path = PathBuf::from("pages").join(req.uri().path().trim_start_matches('/'));
            if path.exists() {
                log::info!("{} \x1b[32m200\x1b[39m \x1b[1;33m{}\x1b[0m", req.method().into_color_method(), req.uri().path());
                if path.is_dir() && path.join("index.html").exists() {
                    if let Ok(file) = File::open(path).await {
                        let stream = FramedRead::new(file, BytesCodec::new());
                        return Ok(hyper::Response::builder()
                            .header("Content-Type", "text/html")
                            .body(Body::from_stream(stream))
                            .unwrap()
                        ) 
                    }
                } else if path.is_file() {
                    let mut res = hyper::Response::builder();
                    let guess = mime_guess::from_path(&path);
                    if let Some(guess) = guess.first() {
                        res = res.header("Content-Type", guess.as_ref());
                    }

                    if let Ok(file) = File::open(path).await {
                        let stream = FramedRead::new(file, BytesCodec::new());
                        return Ok(res
                            .body(Body::from_stream(stream))
                            .unwrap()
                        ) 
                    }
                }
            }

            log::info!("{} \x1b[31m404\x1b[39m \x1b[1;33m{}\x1b[0m", req.method().into_color_method(), req.uri().path());
            let not_found_path = PathBuf::from("pages").join("404.html");
            if not_found_path.exists() {
                if let Ok(file) = File::open(not_found_path).await {
                    let stream = FramedRead::new(file, BytesCodec::new());
                    return Ok(hyper::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(Body::from_stream(stream))
                        .unwrap()
                    ) 
                }
            }
            Ok(hyper::Response::builder()
                .status(404)
                .body(Body::empty())
                .unwrap())
        })
    }
}
