use std::{
    convert::Infallible, future::Future, pin::Pin, sync::{Arc, Mutex}, task::{Context, Poll}
};

use http_body::Body as HttpBody;
use hyper::{
    body::{Bytes, SizeHint},
    header::{self, HeaderValue, CONTENT_LENGTH},
    HeaderMap, Method,
};
use pin_project_lite::pin_project;
use regex::Regex;
use tower::{
    util::{BoxCloneService, Oneshot},
    Service, ServiceExt,
};
use hyper::http::Extensions;

use crate::{extract::UriParams, PercentDecodedStr};

use crate::{BoxError, Body, Request, Response, extract::response::IntoResponse};
pub use super::Handler;

mod file;
mod template;
pub use file::FileRouter;
pub use template::{TemplateRouter, TemplateEngine, RenderError};

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

pub struct Route<E = Infallible>(Mutex<BoxCloneService<Request, Response, E>>);
impl Route {
    pub(crate) fn new<T>(svc: T) -> Self
    where
        T: Service<Request, Error = Infallible> + Clone + Send + 'static,
        T::Response: IntoResponse + 'static,
        T::Future: Send + 'static,
    {
        Self(Mutex::new(BoxCloneService::new(
            svc.map_future(|f| async move {
                let result = f.await.unwrap();
                Ok(result.into_response())
            })
        )))
    }

    pub(crate) fn oneshot_inner(
        &mut self,
        req: Request,
    ) -> Oneshot<BoxCloneService<Request, Response, Infallible>, Request> {
        self.0.get_mut().unwrap().clone().oneshot(req)
    }

    //pub(crate) fn layer<L>(self, layer: L) -> Route
    //where
    //    L: Layer<Route> + Clone + Send + 'static,
    //    L::Service: Service<Request, Error = Infallible> + Clone + Send + 'static,
    //    <L::Service as Service<Request>>::Response: IntoResponse + 'static,
    //    <L::Service as Service<Request>>::Future: Send + 'static,
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

impl Service<Request> for Route {
    type Response = Response;
    type Error = Infallible;
    type Future = RouteFuture;

    #[inline]
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn call(&mut self, req: Request) -> Self::Future {
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
                BoxCloneService<Request, Response, Infallible>,
                Request,
            >,
        },
        Response {
            response: Option<Response>,
        }
    }
}

impl RouteFuture {
    pub(crate) fn from_future(
        future: Oneshot<BoxCloneService<Request, Response, Infallible>, Request>,
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
    fn call(self: Box<Self>, request: Request) -> RouteFuture;
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

    fn call(self: Box<Self>, req: Request) -> RouteFuture {
        self.into_route().call(req)
    }
}

pub struct BoxedRoute(Mutex<Box<dyn ErasedHandler + Send>>);
impl BoxedRoute {
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
impl Clone for BoxedRoute {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.0.lock().unwrap().clone_box()))
    }
}

impl std::fmt::Debug for BoxedRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BoxedIntoRoute").finish()
    }
}

#[derive(Default, Clone)]
pub struct Endpoint {
    get: Option<BoxedRoute>,
    head: Option<BoxedRoute>,
    post: Option<BoxedRoute>,
    put: Option<BoxedRoute>,
    delete: Option<BoxedRoute>,
    connect: Option<BoxedRoute>,
    options: Option<BoxedRoute>,
    trace: Option<BoxedRoute>,
    patch: Option<BoxedRoute>,
    fallback: Option<BoxedRoute>,
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
                    self.$method = Some(BoxedRoute::new(handler));
                    self
                }
            )*

            pub fn fallback<H, D>(mut self, handler: H) -> Self
            where
                H: Handler<D> + Send + 'static,
                D: 'static
            {
                self.fallback = Some(BoxedRoute::new(handler));
                self
            }
        }

        pub mod methods {
            use super::{Endpoint, Handler, BoxedRoute};
            $(
                pub fn $method<H, D>(handler: H) -> Endpoint
                where
                    H: Handler<D> + Send + Sync + 'static,
                    D: 'static
                {
                    Endpoint {
                        $method: Some(BoxedRoute::new(handler)),
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

    fn call(self, req: Request) -> Self::Future {
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
            Some(handler) => Box::pin(async move { handler.into_route().call(req).await.unwrap() }),
            None => {
                if let Some(fallback) = self.fallback.clone() {
                    Box::pin(async move { fallback.into_route().call(req).await.unwrap() })
                } else {
                    Box::pin(async move {
                        hyper::Response::builder()
                            .status(404)
                            .body(Body::empty())
                            .unwrap()
                    })
                }
            }
        }
    }
}

impl Service<Request> for Endpoint {
    type Error = Infallible;
    type Response = Response;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let handler = self.clone();
        Box::pin(async move {
            Ok(Handler::call(handler, req).await)
        })
    }
}

// A dynamic route path representation
//
// Mainly used to match agains actual routes served from a request.
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
    
    /// Try to match the dynamic route path to the served uri
    ///
    /// # Returns
    ///
    /// Some, if it matches with a list of captures from the url and a ranking based on how many characters where
    /// captured. None if it does not match. 
    pub fn match_path<'a>(&'a self, path: &'a str) -> Option<(Vec<(&'a str, &'a str)>, usize)> {
        self.pattern.captures(path).map(|captures| {
            let captures = self.pattern.capture_names().skip(1).zip(captures.iter().skip(1)).map(|(name, capture)| {
                (name.unwrap(), capture.unwrap().as_str())
            });
            let captures: Vec<(&'a str, &'a str)> = captures.collect();
            let total = captures.iter().map(|v| v.1.len()).sum();
            (captures, total)
        })
    }
}

#[derive(Default, Clone)]
pub struct PathRouter {
    paths: Vec<RoutePath>,
    routes: Vec<BoxedRoute>,
    fallback: Option<BoxedRoute>,
}

impl PathRouter {
    pub fn route<S, H, D>(mut self, path: S, route: H) -> Self
    where
        S: AsRef<str>,
        H: Handler<D> + Send + 'static,
        D: 'static,
    {
        self.paths.push(RoutePath::new(path.as_ref()));
        self.routes.push(BoxedRoute::new(route));
        self
    }

    pub fn fallback<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Clone + Send + 'static,
        D: 'static,
    {
        self.fallback = Some(BoxedRoute::new(handler));
        self
    }
}

impl Handler<PathRouter> for PathRouter {
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, mut req: Request) -> Self::Future {
        let path = req.uri().path().to_string();
        let mut matches = Vec::new();
        for (i, route) in self.paths.iter().enumerate() {
            if let Some((captures, rank)) = route.match_path(path.as_str()) {
                matches.push((i, captures, rank));
            }
        }
        matches.sort_by(|(_, _, a), (_, _, b)| a.cmp(b));

        let best = matches.first();
        match best {
            Some((i, captures, _)) => {
                let route = self.routes.get(*i).unwrap().clone();
                // Add captures and original path to request extensions to be used in extractors
                // later
                insert_url_params(req.extensions_mut(), captures);
                Box::pin(async move { route.into_route().call(req).await.unwrap() })
            },
            None => if let Some(fallback) = self.fallback.clone() {
                Box::pin(async move { fallback.into_route().call(req).await.unwrap() })
            } else {
                Box::pin(async move {
                    hyper::Response::builder()
                        .status(404)
                        .body(Body::empty())
                        .unwrap()
                })
            }
        }
    }
}

impl<B> Service<Request<B>> for PathRouter
where
    B: HttpBody<Data = Bytes> + Send + 'static,
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
        let req = req.map(Body::new);
        Box::pin(async move {
            Ok(Handler::call(handler, req).await)
        })
    }
}

fn insert_url_params(extensions: &mut Extensions, params: &[(&str, &str)]) {
    let current = extensions.get_mut::<UriParams>();

    // If there was an error in a prefious extraction then do nothing
    if let Some(UriParams::InvalidEncoding(_)) = current {
        return;
    }

    let params = params
        .iter()
        .map(|(k, v)| {
            if let Some(decoded) = PercentDecodedStr::new(v) {
                Ok((Arc::from(*k), decoded))
            } else {
                Err(Arc::from(*k))
            }
        })
        .collect::<Result<Vec<_>, _>>();
    match (current, params) {
        (_, Err(key)) => {
            extensions.insert(UriParams::InvalidEncoding(key));
        }
        (Some(UriParams::Valid(prev)), Ok(params)) => {
            prev.extend(params);
        }
        (None, Ok(params)) => {
            extensions.insert(UriParams::Valid(params));
        },
        _ => unreachable!("Should never reach this point becuase of prevous check for invalid encoding")
    }
}
