use std::{convert::Infallible, future::Future, marker::PhantomData, pin::Pin, task::{Context, Poll}};

use tower::{Layer, Service, ServiceExt};

use crate::{future::{IntoServiceFuture, LayeredFuture}, Request, Response};
use super::future;

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

pub trait FromRequest {
    fn from_request(req: &Request) -> Self;
}

pub trait FromRequestBody {
    fn from_request_body(req: Request) -> Self;
}

impl FromRequestBody for Request {
    fn from_request_body(req: Request) -> Self {
        req
    }
}

pub trait Handler<P>: Clone + Sized + Send + 'static {
    type Future: Future<Output = Response> + Send + 'static;

    fn call(self, req: Request) -> Self::Future;

    fn into_service(self) -> HandlerService<Self, P> {
        HandlerService::new(self)
    }

    fn layer<L>(self, layer: L) -> Layered<L, Self, P>
    where
        L: Layer<HandlerService<Self, P>> + Clone,
        L::Service: Service<Request>
    {
        Layered {
            layer,
            handler: self,
            _marker: PhantomData,
        }
    }
}

impl<F, R> Handler<((),)> for F
where
    F: Fn() -> R + Clone + Send + 'static,
    R: Future<Output = Response> + Send + 'static,
    Self: Sized,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, _: Request) -> Self::Future {
        let handler = self.clone();
        Box::pin(async move {
            handler().await
        })
    }
}

impl<F, R, B> Handler<(B,)> for F
where
    F: Fn(B) -> R + Clone + Send + 'static,
    R: Future<Output = Response> + Send + 'static,
    Self: Sized,
    B: FromRequestBody,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request) -> Self::Future {
        let handler = self.clone();
        Box::pin(async move {
            handler(B::from_request_body(req)).await
        })
    }
}

pub struct HandlerService<H, D> {
    handler: H,
    _marker: PhantomData<fn() -> D>,
}

impl<H: Clone, D> Clone for HandlerService<H, D> {
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            _marker: PhantomData
        }
    }
}

impl<H, D> HandlerService<H, D> {
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            _marker: PhantomData,
        }
    }
}

impl<H, D> Service<Request> for HandlerService<H, D>
where
    H: Handler<D> + Clone + Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = IntoServiceFuture<H::Future>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        use futures_util::future::FutureExt;

        let handler = self.handler.clone();
        let future = Handler::call(handler, req);
        let future = future.map(Ok as _);

        IntoServiceFuture::new(future)
    }
}

pub struct Layered<L, H, D> {
    layer: L,
    handler: H,
    _marker: PhantomData<D>,
}

impl<L, H, D> std::fmt::Debug for Layered<L, H, D>
where
    L: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layered")
            .field("layer", &self.layer)
            .finish_non_exhaustive()
    }
}

impl<L, H, D> Clone for Layered<L, H, D>
where
    L: Clone,
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            layer: self.layer.clone(),
            handler: self.handler.clone(),
            _marker: PhantomData,
        }
    }
}

impl<L, H, D> Handler<D> for Layered<L, H, D>
where
    L: Layer<HandlerService<H, D>> + Clone + Send + 'static,
    H: Handler<D>,
    L::Service: Service<Request, Error = Infallible> + Clone + Send + 'static,
    <L::Service as Service<Request>>::Response: IntoResponse,
    <L::Service as Service<Request>>::Future: Send,
    D: Send + 'static,
{
    type Future = future::LayeredFuture<L::Service>;

    fn call(self, req: Request) -> Self::Future {
        use futures_util::future::{FutureExt, Map};

        let svc = self.handler.into_service();
        let svc = self.layer.layer(svc);

        let future: Map<
            _,
            fn(
                Result<
                    <L::Service as Service<Request>>::Response,
                    <L::Service as Service<Request>>::Error,
                >,
            ) -> _,
        > = svc.oneshot(req).map(|result| match result {
            Ok(res) => res.into_response(),
            Err(err) => match err {},
        });

        future::LayeredFuture::new(future)
    }
}
