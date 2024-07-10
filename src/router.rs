use std::{collections::HashMap, convert::Infallible, future::Future, pin::Pin, sync::{Arc, Mutex}};

use http_body_util::Full;
use hyper::{body::Bytes, service::Service, Method};

use crate::{Request, Response};

pub trait Handler<P> {
    fn call(&self, req: Request) -> Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;
}

impl<F, R> Handler<()> for F
where
    F: Fn() -> R + Clone + Send + 'static,
    R: Future<Output = Result<Response, Infallible>> + Send + 'static,
    Self: Sized,
{
    fn call(&self, _: Request) -> Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>> {
        let handler = self.clone();
        Box::pin(async move {
            handler().await
        })
    }
}

pub struct MakeErasedHandler<D>(Box<dyn Handler<D> + Send + Sync>);
impl<D> MakeErasedHandler<D> {
    pub fn new<H>(handler: H) -> Self
    where
        H: Handler<D> + Send + Sync + 'static,
    {
        Self(Box::new(handler))
    }

    pub fn call(&self, req: Request) -> Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>> {
        self.0.call(req)
    }
}

pub trait ErasedHandler {
    fn call(&self, req: Request) -> Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;
}

impl<D> ErasedHandler for MakeErasedHandler<D>
{
    fn call(&self, req: Request) -> Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>> {
        MakeErasedHandler::call(self, req)
    }
}

#[derive(Default)]
pub struct Route {
    get: Option<Box<dyn ErasedHandler + Send + Sync>>,
    post: Option<Box<dyn ErasedHandler + Send + Sync>>,
    put: Option<Box<dyn ErasedHandler + Send + Sync>>,
    delete: Option<Box<dyn ErasedHandler + Send + Sync>>,
}

impl Route {
    pub fn get<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + Sync + 'static,
        D: 'static
    {
        self.get = Some(Box::new(MakeErasedHandler::new(handler)));
        self
    }

    pub fn post<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + Sync + 'static,
        D: 'static
    {
        self.get = Some(Box::new(MakeErasedHandler::new(handler)));
        self
    }

    pub fn put<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + Sync + 'static,
        D: 'static
    {
        self.get = Some(Box::new(MakeErasedHandler::new(handler)));
        self
    }

    pub fn delete<H, D>(mut self, handler: H) -> Self
    where
        H: Handler<D> + Send + Sync + 'static,
        D: 'static
    {
        self.get = Some(Box::new(MakeErasedHandler::new(handler)));
        self
    }
}

#[derive(Default, Clone)]
pub struct Router {
    handlers: Arc<Mutex<HashMap<String, Route>>>,
}

impl Router {
    pub fn route<S: AsRef<str>>(self, path: S, route: Route) -> Self {
        self.handlers.lock().unwrap().insert(path.as_ref().to_string(), route);
        self
    }
}

impl Service<Request> for Router {
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request) -> Self::Future {
        let handlers = self.handlers.clone();
        if let Some(route) = handlers.lock().unwrap().get(req.uri().path()) {
            match *req.method() {
                Method::GET => if let Some(handler) = route.get.as_ref() {
                    return handler.call(req);
                }
                Method::POST => if let Some(handler) = route.post.as_ref() {
                    return handler.call(req);
                }
                Method::PUT => if let Some(handler) = route.put.as_ref() {
                    return handler.call(req);
                }
                Method::DELETE => if let Some(handler) = route.delete.as_ref() {
                    return handler.call(req);
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

pub fn get<H, D>(handler: H) -> Route
where
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Route {
        get: Some(Box::new(MakeErasedHandler::new(handler))),
        ..Default::default()
    }
}
pub fn post<H, D>(handler: H) -> Route
where
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Route {
        post: Some(Box::new(MakeErasedHandler::new(handler))),
        ..Default::default()
    }
}
pub fn put<H, D>(handler: H) -> Route
where
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Route {
        put: Some(Box::new(MakeErasedHandler::new(handler))),
        ..Default::default()
    }
}
pub fn delete<H, D>(handler: H) -> Route
where 
    H: Handler<D> + Send + Sync + 'static,
    D: 'static
{
    Route { delete: Some(Box::new(MakeErasedHandler::new(handler))), ..Default::default() }
}
