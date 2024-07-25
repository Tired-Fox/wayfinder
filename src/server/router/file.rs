use std::{
    future::Future, path::{Path, PathBuf}, pin::Pin, task::{Context, Poll}
};

use http_body::Body;
use tower::Service;
use hyper::{
    body::Bytes,
    header,
};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::server::{body::BoxError, Handler, Body as HttpBody, Request, Response};

#[derive(Debug, Clone)]
pub struct FileRouter {
    enforce_slash: bool,
    path: PathBuf,
}

impl FileRouter {
    pub fn new<P: AsRef<Path>>(path: P, enforce: bool) -> Self {
        Self {
            path: path.as_ref().into(),
            enforce_slash: enforce
        }
    }
}

impl Handler<FileRouter> for FileRouter {
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request) -> Self::Future {
        let router = self.clone();
        Box::pin(async move {
            // Enforce ending paths that match index.html with a slash `/`
            if router.enforce_slash && !req.uri().path().ends_with('/') {
                return hyper::Response::builder()
                    .status(308)
                    .header(header::LOCATION, format!("{}/", req.uri().path()))
                    .body(HttpBody::empty())
                    .unwrap()
            }

            let path = router.path.join(req.uri().path().trim_start_matches('/'));
            if path.exists() {
                if path.is_dir() && path.join("index.html").exists() {
                    if let Ok(file) = File::open(path.join("index.html")).await {
                        let stream = FramedRead::new(file, BytesCodec::new());
                        return hyper::Response::builder()
                            .header(header::CONTENT_TYPE, "text/html")
                            .body(HttpBody::from_stream(stream))
                            .unwrap()
                    }
                } else if path.is_file() {
                    let mut res = hyper::Response::builder();
                    let guess = mime_guess::from_path(&path);
                    if let Some(guess) = guess.first() {
                        res = res.header("Content-Type", guess.as_ref());
                    }

                    if let Ok(file) = File::open(path).await {
                        let stream = FramedRead::new(file, BytesCodec::new());
                        return res
                            .body(HttpBody::from_stream(stream))
                            .unwrap()
                    }
                }
            }

            let not_found_path = router.path.join("404.html");
            if not_found_path.exists() {
                if let Ok(file) = File::open(not_found_path).await {
                    let stream = FramedRead::new(file, BytesCodec::new());
                    return hyper::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(HttpBody::from_stream(stream))
                        .unwrap()
                }
            }

            hyper::Response::builder()
                .status(404)
                .body(HttpBody::empty())
                .unwrap()
        })
    }
}

impl<B> Service<Request<B>> for FileRouter
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let handler = self.clone();
        let req = req.map(HttpBody::new);
        Box::pin(async move {
            Ok(Handler::call(handler, req).await)
        })
    }
}
