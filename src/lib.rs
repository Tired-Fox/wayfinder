use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

pub use hyper::service::service_fn;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;
pub type Infallible<T> = std::result::Result<T, std::convert::Infallible>;
pub type Response = hyper::Response::<http_body_util::Full<hyper::body::Bytes>>;
pub type Request = hyper::Request::<hyper::body::Incoming>;

pub trait IntoSocketAddr {
    fn into_socket_addr(self) -> SocketAddr;
}

impl IntoSocketAddr for ([u8;4], u16) {
    fn into_socket_addr(self) -> SocketAddr {
        SocketAddr::from(self)
    }
}

impl IntoSocketAddr for SocketAddr {
    fn into_socket_addr(self) -> SocketAddr {
        self
    }
}

#[derive(Debug, Clone)]
pub struct Server<R>
where
    R: Service<Request, Response = Response, Error = std::convert::Infallible> + Send + Clone + 'static,
    <R as Service<Request>>::Future: Send,
{
    address: SocketAddr,
    router: R,
}

impl Server<DefaultRouter> {
    pub fn bind<S: IntoSocketAddr>(address: S) -> Self {
        Self {
            address: address.into_socket_addr(),
            router: DefaultRouter,
        }
    }
}

impl<R> Server<R>
where
    R: Service<Request, Response = Response, Error = std::convert::Infallible> + Send + Clone + 'static,
    <R as Service<Request>>::Future: Send,
{
    pub fn with_router<N>(self, router: N) -> Server<N>
    where
        N: Service<Request, Response = Response, Error = std::convert::Infallible> + Send + Clone + 'static,
        <N as Service<Request>>::Future: Send,
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
                let addr = self.address.into_socket_addr();
                let listener = TcpListener::bind(addr).await?;
                log::info!("Listening to \x1b[33m{}\x1b[39m", addr);

                loop {
                    let (stream, _) = listener.accept().await?;
                    let io = TokioIo::new(stream);
                    let router = self.router.clone();
                    tokio::task::spawn(async move {
                        if let Err(err) = http1::Builder::new().serve_connection(io, router).await {
                            eprintln!("Error serving connection: {:?}", err);
                        }
                    });
                }
            })
    }
}

fn full<S: Into<Bytes>>(body: S) -> Full<Bytes> {
    Full::new(body.into())
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
impl Service<Request> for DefaultRouter {
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request) -> Self::Future {
        Box::pin(async move {
            let path = PathBuf::from("pages").join(req.uri().path().trim_start_matches('/'));
            if path.exists() {
                log::info!("{} \x1b[32m200\x1b[39m \x1b[1;33m{}\x1b[0m", req.method().into_color_method(), req.uri().path());
                if path.is_dir() && path.join("index.html").exists() {
                    return Ok(hyper::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(full(std::fs::read_to_string(path.join("index.html")).unwrap()))
                        .unwrap()
                    ) 
                } else if path.is_file() {
                    let mut res = hyper::Response::builder();
                    let guess = mime_guess::from_path(&path);
                    if let Some(guess) = guess.first() {
                        res = res.header("Content-Type", guess.as_ref());
                    }
                    return Ok(res
                        .body(full(std::fs::read_to_string(path).unwrap()))
                        .unwrap()
                    ) 
                }
            }

            log::info!("{} \x1b[31m404\x1b[39m \x1b[1;33m{}\x1b[0m", req.method().into_color_method(), req.uri().path());
            let not_found_path = PathBuf::from("pages").join("404.html");
            if not_found_path.exists() {
                return Ok(hyper::Response::builder()
                    .header("Content-Type", "text/html")
                    .body(full(std::fs::read_to_string(not_found_path).unwrap()))
                    .unwrap()
                ) 
            }
            Ok(hyper::Response::builder()
                .status(404)
                .body(Full::new(Bytes::default()))
                .unwrap())
        })
    }
}
