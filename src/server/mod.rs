use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use tower::ServiceExt as _;
pub use hyper::body::Incoming;
pub use hyper::body::Body as HttpBody;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tower::Service;

pub mod router;
pub(crate) mod future;
pub(crate) mod handler;

pub mod body;

pub use handler::Handler;
pub use router::{PathRouter, FileRouter, methods, TemplateRouter, TemplateEngine, RenderError};
pub use body::Body;

use crate::Result;

pub static NETWORK: [u8; 4] = [0, 0, 0, 0];
pub static LOCAL: [u8; 4] = [127, 0, 0, 1];

pub type Request<T = Body> = hyper::Request<T>;
pub type Response<B = Body> = hyper::Response<B>;

pub mod prelude {
    use hyper::{body::Bytes, StatusCode};

    use super::{body::BoxError, Body, Response};
    pub use crate::extract::response::IntoResponse;
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
    R: Service<Request, Response = Response<Body>, Error = Infallible> + Send + Clone + 'static,
    <R as Service<Request>>::Future: Send,
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
    R: Service<Request, Response = Response<Body>, Error = Infallible> + Send + Clone + 'static,
    <R as Service<Request>>::Future: Send,
{
    pub fn with_router<N>(self, router: N) -> Server<N>
    where
        N: Service<Request, Response = Response<Body>, Error = Infallible> + Send + Clone + 'static,
        <N as Service<Request>>::Future: Send,
    {
        Server {
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

                let router = TowerToHyperService::new(self.router
                    .map_request(|req: Request<Incoming>| req.map(Body::new)));

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
