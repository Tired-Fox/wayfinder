use std::{collections::HashMap, convert::Infallible, future::Future, pin::Pin, sync::{Arc, Mutex}, task::{Context, Poll}};

use http_body_util::Full;
use hyper::{body::{Body, Bytes, SizeHint}, header::{self, HeaderValue, CONTENT_LENGTH}, HeaderMap, Method};
use pin_project_lite::pin_project;
use tower::{util::{BoxCloneService, MapErrLayer, MapRequestLayer, MapResponseLayer, Oneshot}, Layer, Service, ServiceExt};

use crate::{Request, Response};
pub use super::handler::{Handler, IntoResponse};

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
            svc.map_response(IntoResponse::into_response),
        )))
    }

    pub(crate) fn oneshot_inner(
        &mut self,
        req: Request,
    ) -> Oneshot<BoxCloneService<Request, Response, Infallible>, Request> {
        self.0.get_mut().unwrap().clone().oneshot(req)
    }

    pub(crate) fn layer<L>(self, layer: L) -> Route
    where
        L: Layer<Route> + Clone + Send + 'static,
        L::Service: Service<Request, Error=Infallible> + Clone + Send + 'static,
        <L::Service as Service<Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        let layer = (
            MapRequestLayer::new(|req: Request| req),
            MapErrLayer::new(Into::into),
            MapResponseLayer::new(IntoResponse::into_response),
            layer,
        );

        Route::new(layer.layer(self))
    }
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
    H: Clone + Send + 'static 
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

pub struct BoxedIntoRoute(Mutex<Box<dyn ErasedHandler + Send>>);
impl BoxedIntoRoute {
    pub fn new<H, T>(handler: H) -> Self
    where
        H: Handler<T>,
        T: 'static
    {
        Self(Mutex::new(Box::new(MakeErasedHandler {
            handler,
            into_route: |handler| Route::new(handler.into_service())
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

#[derive(Default)]
pub struct Endpoint {
    get: Option<BoxedIntoRoute>,
    post: Option<BoxedIntoRoute>,
    put: Option<BoxedIntoRoute>,
    delete: Option<BoxedIntoRoute>,
}

impl Endpoint {
    pub fn get<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + 'static,
        D: 'static
    {
        self.get = Some(BoxedIntoRoute::new(handler));
        self
    }

    pub fn post<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + 'static,
        D: 'static
    {
        self.post = Some(BoxedIntoRoute::new(handler));
        self
    }

    pub fn put<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + 'static,
        D: 'static
    {
        self.put = Some(BoxedIntoRoute::new(handler));
        self
    }

    pub fn delete<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + 'static,
        D: 'static
    {
        self.delete = Some(BoxedIntoRoute::new(handler));
        self
    }
}

#[derive(Default, Clone)]
pub struct Router {
    handlers: Arc<Mutex<HashMap<String, Endpoint>>>,
}

impl Router {
    pub fn route<S: AsRef<str>>(self, path: S, route: Endpoint) -> Self {
        self.handlers.lock().unwrap().insert(path.as_ref().to_string(), route);
        self
    }
}

impl Service<Request> for Router {
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let handlers = self.handlers.clone();
        if let Some(route) = handlers.lock().unwrap().get(req.uri().path()) {
            match *req.method() {
                Method::GET => if let Some(handler) = route.get.as_ref() {
                    let handler = handler.clone();
                    return Box::pin(async move {
                        handler.into_route().call(req).await
                    });
                }
                Method::POST => if let Some(handler) = route.post.as_ref() {
                    let handler = handler.clone();
                    return Box::pin(async move {
                        handler.clone().into_route().call(req).await
                    });
                }
                Method::PUT => if let Some(handler) = route.put.as_ref() {
                    let handler = handler.clone();
                    return Box::pin(async move {
                        handler.clone().into_route().call(req).await
                    });
                }
                Method::DELETE => if let Some(handler) = route.delete.as_ref() {
                    let handler = handler.clone();
                    return Box::pin(async move {
                        handler.clone().into_route().call(req).await
                    });
                }
                _ => {}
            }
        }
        Box::pin(async move {
            Ok(hyper::Response::builder()
                .status(404)
                .body(Full::new(Bytes::default()))
                .unwrap())
        })
    }
}

pub fn get<H, D>(handler: H) -> Endpoint
where
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Endpoint {
        get: Some(BoxedIntoRoute::new(handler)),
        ..Default::default()
    }
}
pub fn post<H, D>(handler: H) -> Endpoint
where
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Endpoint {
        post: Some(BoxedIntoRoute::new(handler)),
        ..Default::default()
    }
}
pub fn put<H, D>(handler: H) -> Endpoint
where
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Endpoint {
        put: Some(BoxedIntoRoute::new(handler)),
        ..Default::default()
    }
}
pub fn delete<H, D>(handler: H) -> Endpoint
where 
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Endpoint {
        delete: Some(BoxedIntoRoute::new(handler)),
        ..Default::default()
    }
}
