use std::{
    convert::Infallible,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use hyper::{body::{Body as HttpBody, Bytes}, header::{self, HeaderValue}};
use crate::{all_variants_with_last, Body, BoxError, Request, Response};
use crate::extract::{CookieJar, IntoResponse, FromRequest, FromParts};
use tower::{Layer, Service, ServiceExt};

use super::future;

pub trait Handler<P>: Clone + Sized + Send + 'static {
    type Future: Future<Output = Response> + Send + 'static;

    fn call(self, req: Request) -> Self::Future;

    fn into_service(self) -> HandlerService<Self, P> {
        HandlerService::new(self)
    }

    fn layer<L>(self, layer: L) -> Layered<L, Self, P>
    where
        L: Layer<HandlerService<Self, P>> + Clone,
        L::Service: Service<Request>,
    {
        Layered {
            layer,
            handler: self,
            _marker: PhantomData,
        }
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
            _marker: PhantomData,
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

impl<B, H, D> Service<Request<B>> for HandlerService<H, D>
where
    H: Handler<D> + Clone + Send + 'static,
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>
{
    type Response = Response;
    type Error = Infallible;
    type Future = future::IntoServiceFuture<H::Future>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        use futures_util::future::FutureExt;

        let handler = self.handler.clone();
        let future = Handler::call(handler, req.map(Body::new));
        let future = future.map(Ok as _);

        future::IntoServiceFuture::new(future)
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
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request) -> Self::Future {
        let svc = self.handler.into_service();
        let svc = self.layer.layer(svc);

        Box::pin(async move {
            let value = svc.oneshot(req).await.unwrap();
            value.into_response()
        })
    }
}

impl<F, R, B> Handler<((),)> for F
where
    F: Fn() -> R + Clone + Send + 'static,
    R: Future<Output = B> + Send + 'static,
    B: IntoResponse,
    Self: Sized,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, _: Request) -> Self::Future {
        let handler = self.clone();
        Box::pin(async move { handler().await.into_response() })
    }
}

macro_rules! impl_handler {
    (($($i: ident),* $(,)?), $last: ident $(,)?) => {
        impl<F, R, B, M, X $(, $i)*, $last> Handler<(X, M, $($i,)* $last,)> for F
        where
            F: Fn($($i,)* $last) -> R + Clone + Send + Sync + 'static,
            R: Future<Output = B> + Send + 'static,
            B: IntoResponse<X>,
            Self: Sized,
            $($i: FromParts + Send,)*
            $last: FromRequest<M>,
        {
            type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

            fn call(self, req: Request) -> Self::Future {
                let handler = self.clone();
                Box::pin(async move {
                    let cookies = CookieJar::default();
                    let (parts, body) = req.into_parts();

                    paste::paste! {
                        $(let [<_i_$i:lower>] = match $i::from_parts(&parts, cookies.clone()).await {
                            Ok(v) => v,
                            Err(e) => {
                                log::error!("Failed to parse handler parameter: {}", e);
                                return e.into_response()
                            },
                        };)*

                        let [<_last_$last:lower>] = match $last::from_request(Request::from_parts(parts, body), cookies.clone()).await {
                            Ok(v) => v,
                            Err(e) => {
                                log::error!("Failed to parse handler parameter: {}", e);
                                return e.into_response()
                            },
                        };

                        let mut response = handler(
                            $([<_i_$i:lower>],)*
                            [<_last_$last:lower>],
                        ).await.into_response();
                    }

                    let jar = cookies.as_ref();
                    if jar.delta().count() > 0 {
                        let headers = response.headers_mut();
                        let cookies = jar.delta().map(|v| v.stripped().encoded().to_string()).collect::<Vec<_>>().join(";");
                        headers.insert(header::SET_COOKIE, HeaderValue::from_str(cookies.as_str()).unwrap());
                    }
                    response
                })
            }
        }
    };
}

all_variants_with_last!(impl_handler);
