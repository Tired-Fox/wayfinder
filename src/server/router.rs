use std::{
    convert::Infallible, future::Future, path::{Path, PathBuf}, pin::Pin, sync::Mutex, task::{Context, Poll}
};

use hashbrown::HashMap;
use http_body::Body;
use hyper::{
    body::{Bytes, Incoming, SizeHint},
    header::{self, HeaderValue, CONTENT_LENGTH},
    HeaderMap, Method,
};
use pin_project_lite::pin_project;
use regex::Regex;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};
use tower::{
    util::{BoxCloneService, Oneshot},
    Layer, Service, ServiceExt,
};

use super::{Request, Response, Body as HttpBody};
pub use super::{handler::Handler, response::IntoResponse};

lazy_static::lazy_static! {
    static ref CATCH_ALL: Regex =  Regex::new(":\\*([a-zA-Z_][a-zA-Z_\\d]*)").unwrap();
    static ref CAPTURE: Regex = Regex::new(":([a-zA-Z_][a-zA-Z_\\d]*)").unwrap();
}

pub struct MakeErasedHandler<H> {
    pub handler: H,
    pub into_route: fn(H) -> Route,
}

impl<H: Clone> Clone for MakeErasedHandler<H> {
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            into_route: self.into_route,
        }
    }
}

pub struct Route<E = Infallible>(Mutex<BoxCloneService<Request<Incoming>, Response, E>>);
impl Route {
    pub(crate) fn new<T>(svc: T) -> Self
    where
        T: Service<Request<Incoming>, Error = Infallible> + Clone + Send + 'static,
        T::Response: IntoResponse + 'static,
        T::Future: Send + 'static,
    {
        Self(Mutex::new(BoxCloneService::new(
            svc.map_future(|f| async move {
                let result = f.await.unwrap();
                Ok(result.into_response().await)
            })
        )))
    }

    pub(crate) fn oneshot_inner(
        &mut self,
        req: Request<Incoming>,
    ) -> Oneshot<BoxCloneService<Request<Incoming>, Response, Infallible>, Request<Incoming>> {
        self.0.get_mut().unwrap().clone().oneshot(req)
    }

    //pub(crate) fn layer<L>(self, layer: L) -> Route
    //where
    //    L: Layer<Route> + Clone + Send + 'static,
    //    L::Service: Service<Request<Incoming>, Error = Infallible> + Clone + Send + 'static,
    //    <L::Service as Service<Request<Incoming>>>::Response: IntoResponse + 'static,
    //    <L::Service as Service<Request<Incoming>>>::Future: Send + 'static,
    //{
    //    Route::new(layer.layer(self))
    //}
}

impl<E> Clone for Route<E> {
    #[track_caller]
    fn clone(&self) -> Self {
        Self(Mutex::new(self.0.lock().unwrap().clone()))
    }
}

impl<E> std::fmt::Debug for Route<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Route").finish()
    }
}

impl Service<Request<Incoming>> for Route {
    type Response = Response;
    type Error = Infallible;
    type Future = RouteFuture;

    #[inline]
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        RouteFuture::from_future(self.oneshot_inner(req))
    }
}

pin_project! {
    /// Response future for [`Route`].
    pub struct RouteFuture {
        #[pin]
        kind: RouteFutureKind,
        strip_body: bool,
        allow_header: Option<Bytes>,
    }
}

pin_project! {
    #[project = RouteFutureKindProj]
    enum RouteFutureKind {
        Future {
            #[pin]
            future: Oneshot<
                BoxCloneService<Request<Incoming>, Response, Infallible>,
                Request<Incoming>,
            >,
        },
        Response {
            response: Option<Response>,
        }
    }
}

impl RouteFuture {
    pub(crate) fn from_future(
        future: Oneshot<BoxCloneService<Request<Incoming>, Response, Infallible>, Request<Incoming>>,
    ) -> Self {
        Self {
            kind: RouteFutureKind::Future { future },
            strip_body: false,
            allow_header: None,
        }
    }
}

impl Future for RouteFuture {
    type Output = Result<Response, Infallible>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let mut res = match this.kind.project() {
            RouteFutureKindProj::Future { future } => match future.poll(cx) {
                Poll::Ready(Ok(res)) => res,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Pending => return Poll::Pending,
            },
            RouteFutureKindProj::Response { response } => {
                response.take().expect("future polled after completion")
            }
        };

        set_allow_header(res.headers_mut(), this.allow_header);

        // make sure to set content-length before removing the body
        set_content_length(res.size_hint(), res.headers_mut());

        Poll::Ready(Ok(res))
    }
}

fn set_allow_header(headers: &mut HeaderMap, allow_header: &mut Option<Bytes>) {
    match allow_header.take() {
        Some(allow_header) if !headers.contains_key(header::ALLOW) => {
            headers.insert(
                header::ALLOW,
                HeaderValue::from_maybe_shared(allow_header).expect("invalid `Allow` header"),
            );
        }
        _ => {}
    }
}

fn set_content_length(size_hint: SizeHint, headers: &mut HeaderMap) {
    if headers.contains_key(CONTENT_LENGTH) {
        return;
    }

    if let Some(size) = size_hint.exact() {
        let header_value = if size == 0 {
            #[allow(clippy::declare_interior_mutable_const)]
            const ZERO: HeaderValue = HeaderValue::from_static("0");

            ZERO
        } else {
            let mut buffer = itoa::Buffer::new();
            HeaderValue::from_str(buffer.format(size)).unwrap()
        };

        headers.insert(CONTENT_LENGTH, header_value);
    }
}

pub trait ErasedHandler {
    fn clone_box(&self) -> Box<dyn ErasedHandler + Send>;
    fn into_route(self: Box<Self>) -> Route;

    #[allow(dead_code)]
    fn call(self: Box<Self>, request: Request<Incoming>) -> RouteFuture;
}

impl<H> ErasedHandler for MakeErasedHandler<H>
where
    H: Clone + Send + 'static,
{
    fn clone_box(&self) -> Box<dyn ErasedHandler + Send> {
        Box::new(self.clone())
    }

    fn into_route(self: Box<Self>) -> Route {
        (self.into_route)(self.handler)
    }

    fn call(self: Box<Self>, req: Request<Incoming>) -> RouteFuture {
        self.into_route().call(req)
    }
}

pub struct BoxedIntoRoute(Mutex<Box<dyn ErasedHandler + Send>>);
impl BoxedIntoRoute {
    pub fn new<H, T>(handler: H) -> Self
    where
        H: Handler<T>,
        T: 'static,
    {
        Self(Mutex::new(Box::new(MakeErasedHandler {
            handler,
            into_route: |handler| Route::new(handler.into_service()),
        })))
    }

    pub(crate) fn into_route(self) -> Route {
        self.0.into_inner().unwrap().into_route()
    }
}
impl Clone for BoxedIntoRoute {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.0.lock().unwrap().clone_box()))
    }
}

impl std::fmt::Debug for BoxedIntoRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BoxedIntoRoute").finish()
    }
}

#[derive(Default, Clone)]
pub struct Endpoint {
    get: Option<BoxedIntoRoute>,
    head: Option<BoxedIntoRoute>,
    post: Option<BoxedIntoRoute>,
    put: Option<BoxedIntoRoute>,
    delete: Option<BoxedIntoRoute>,
    connect: Option<BoxedIntoRoute>,
    options: Option<BoxedIntoRoute>,
    trace: Option<BoxedIntoRoute>,
    patch: Option<BoxedIntoRoute>,
    fallback: Option<BoxedIntoRoute>,
}

macro_rules! impl_endpoint_methods {
    ($($method: ident),* $(,)?) => {
        impl Endpoint {
            $(
                pub fn $method<H, D>(mut self, handler: H) -> Self
                where
                    H: Handler<D> + Send + 'static,
                    D: 'static
                {
                    self.$method = Some(BoxedIntoRoute::new(handler));
                    self
                }
            )*

            pub fn fallback<H, D>(mut self, handler: H) -> Self
            where
                H: Handler<D> + Send + 'static,
                D: 'static
            {
                self.fallback = Some(BoxedIntoRoute::new(handler));
                self
            }
        }

        pub mod methods {
            use super::{Endpoint, Handler, BoxedIntoRoute};
            $(
                pub fn $method<H, D>(handler: H) -> Endpoint
                where
                    H: Handler<D> + Send + Sync + 'static,
                    D: 'static
                {
                    Endpoint {
                        $method: Some(BoxedIntoRoute::new(handler)),
                        ..Default::default()
                    }
                }
            )*
        }
    };
}

impl_endpoint_methods!(get, post, put, delete, options, head, patch, trace, connect);

impl Handler<Endpoint> for Endpoint {
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request<Incoming>) -> Self::Future {
        let mut handler = self.clone();
        Box::pin(async move {
            match Service::<Request<Incoming>>::call(&mut handler, req).await {
                Ok(response) => response,
                Err(err) => hyper::Response::builder()
                    .status(500)
                    .header("WAYFINDER-ERROR", err.to_string())
                    .body(HttpBody::empty())
                    .unwrap(),
            }
        })
    }
}

impl Service<Request<Incoming>> for Endpoint {
    type Error = Infallible;
    type Response = Response;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        let handler = match *req.method() {
            Method::GET => self.get.clone(),
            Method::POST => self.post.clone(),
            Method::PUT => self.put.clone(),
            Method::DELETE => self.delete.clone(),
            Method::OPTIONS => self.options.clone(),
            Method::HEAD => self.head.clone(),
            Method::PATCH => self.patch.clone(),
            Method::TRACE => self.trace.clone(),
            Method::CONNECT => self.connect.clone(),
            _ => None,
        };

        match handler {
            Some(handler) => Box::pin(async move { handler.into_route().call(req).await }),
            None => {
                if let Some(fallback) = self.fallback.clone() {
                    Box::pin(async move { fallback.into_route().call(req).await })
                } else {
                    Box::pin(async move {
                        Ok(hyper::Response::builder()
                            .status(404)
                            .body(HttpBody::empty())
                            .unwrap())
                    })
                }
            }
        }
    }
}

/**

    `/some/:catch-all*`
        captured all parts of the path between `/some` and the next matching part after the catch all
    `/some/:cap/other/parts` capture a specific part with specific name
*/
#[derive(Debug, Clone)]
pub struct RoutePath {
    path: String,
    pattern: Regex,
}

impl RoutePath {
    pub fn new(pattern: &str) -> Self {
        let reg = pattern.split('/').map(|part| {
            if CATCH_ALL.is_match(part) {
                let name = &part[2..];
                if name == "_" {
                    "?.*".to_string()
                } else {
                    format!("?(?<{name}>.*)")
                }
            } else if CAPTURE.is_match(part) {
                let name = &part[1..];
                if name == "_" {
                    "[^/]+".to_string()
                } else {
                    format!("(?<{name}>[^/]+)")
                }
            } else {
                regex::escape(part)
            }
        }).collect::<Vec<String>>().join("/");

        Self {
            path: pattern.to_string(),
            pattern: Regex::new(format!("^{reg}$").as_str()).expect("Invalid uri path regex"),
        }
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }
    
    pub fn match_path(&self, path: &str) -> Option<(HashMap<String, String>, usize)> {
        self.pattern.captures(path).map(|captures| {
            let captures = self.pattern.capture_names().skip(1).zip(captures.iter().skip(1)).map(|(name, capture)| {
                (name.unwrap().to_string(), capture.unwrap().as_str().to_string())
            });
            let captures: HashMap<String, String> = captures.collect();
            let total = captures.values().map(|v| v.len()).sum();
            (captures, total)
        })
    }
}

#[derive(Default, Clone)]
pub struct Router {
    paths: Vec<RoutePath>,
    routes: Vec<BoxedIntoRoute>,
    fallback: Option<BoxedIntoRoute>,
}

impl Router {
    pub fn route<S, H, D>(mut self, path: S, route: H) -> Self
    where
        S: AsRef<str>,
        H: Handler<D> + Send + 'static,
        D: 'static,
    {
        self.paths.push(RoutePath::new(path.as_ref()));
        self.routes.push(BoxedIntoRoute::new(route));
        self
    }

    pub fn fallback<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Clone + Send + 'static,
        D: 'static,
    {
        self.fallback = Some(BoxedIntoRoute::new(handler));
        self
    }
}

impl Handler<Router> for Router {
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request<Incoming>) -> Self::Future {
        let mut handler = self.clone();
        Box::pin(async move {
            Service::<Request<Incoming>>::call(&mut handler, req).await.unwrap()
        })
    }
}

impl Service<Request<Incoming>> for Router {
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Incoming>) -> Self::Future {

        let path = req.uri().path();
        let mut matches = Vec::new();
        for (i, route) in self.paths.iter().enumerate() {
            if let Some((capturs, rank)) = route.match_path(path) {
                matches.push((i, capturs, rank));
            }
        }
        matches.sort_by(|(_, _, a), (_, _, b)| a.cmp(b));

        let best = matches.first();
        match best {
            // TODO: Pass captures to Handler: Use a custom Request object from this crate
            Some((i, _captures, _)) => {
                let route = self.routes.get(*i).unwrap().clone();
                Box::pin(async move { route.into_route().call(req).await })
            },
            None => if let Some(fallback) = self.fallback.clone() {
                Box::pin(async move { fallback.into_route().call(req).await })
            } else {
                Box::pin(async move {
                    Ok(hyper::Response::builder()
                        .status(404)
                        .body(HttpBody::empty())
                        .unwrap())
                })
            }
        }
    }
}

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

    fn call(self, req: Request<Incoming>) -> Self::Future {
        let mut handler = self.clone();
        Box::pin(async move {
            Service::<Request<Incoming>>::call(&mut handler, req).await.unwrap()
        })
    }
}

impl Service<Request<Incoming>> for FileRouter {
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        let router = self.clone();
        Box::pin(async move {
            // Enforce ending paths that match index.html with a slash `/`
            if router.enforce_slash && !req.uri().path().ends_with('/') {
                return Ok(hyper::Response::builder()
                    .status(308)
                    .header(header::LOCATION, format!("{}/", req.uri().path()))
                    .body(HttpBody::empty())
                    .unwrap()
                )
            }

            let path = router.path.join(req.uri().path().trim_start_matches('/'));
            if path.exists() {
                if path.is_dir() && path.join("index.html").exists() {
                    if let Ok(file) = File::open(path.join("index.html")).await {
                        let stream = FramedRead::new(file, BytesCodec::new());
                        return Ok(hyper::Response::builder()
                            .header(header::CONTENT_TYPE, "text/html")
                            .body(HttpBody::from_stream(stream))
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
                            .body(HttpBody::from_stream(stream))
                            .unwrap()
                        ) 
                    }
                }
            }

            let not_found_path = router.path.join("404.html");
            if not_found_path.exists() {
                if let Ok(file) = File::open(not_found_path).await {
                    let stream = FramedRead::new(file, BytesCodec::new());
                    return Ok(hyper::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(HttpBody::from_stream(stream))
                        .unwrap()
                    ) 
                }
            }

            Ok(hyper::Response::builder()
                .status(404)
                .body(HttpBody::empty())
                .unwrap())
        })
    }
}
