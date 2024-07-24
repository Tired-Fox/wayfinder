use std::{
    convert::Infallible,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use hyper::{body::Incoming, header::{self, HeaderValue}};
use super::{Request, Response, request::CookieJar};
use tower::{Layer, Service, ServiceExt};

use super::{future, request::{FromRequestBody, FromRequest}, response::IntoResponse};

pub trait Handler<P>: Clone + Sized + Send + 'static {
    type Future: Future<Output = Response> + Send + 'static;

    fn call(self, req: Request<Incoming>) -> Self::Future;

    fn into_service(self) -> HandlerService<Self, P> {
        HandlerService::new(self)
    }

    fn layer<L>(self, layer: L) -> Layered<L, Self, P>
    where
        L: Layer<HandlerService<Self, P>> + Clone,
        L::Service: Service<Request<Incoming>>,
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

impl<H, D> Service<Request<Incoming>> for HandlerService<H, D>
where
    H: Handler<D> + Clone + Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = future::IntoServiceFuture<H::Future>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        use futures_util::future::FutureExt;

        let handler = self.handler.clone();
        let future = Handler::call(handler, req);
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
    L::Service: Service<Request<Incoming>, Error = Infallible> + Clone + Send + 'static,
    <L::Service as Service<Request<Incoming>>>::Response: IntoResponse,
    <L::Service as Service<Request<Incoming>>>::Future: Send,
    D: Send + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request<Incoming>) -> Self::Future {
        let svc = self.handler.into_service();
        let svc = self.layer.layer(svc);

        Box::pin(async move {
            let value = svc.oneshot(req).await.unwrap();
            value.into_response().await
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

    fn call(self, _: Request<Incoming>) -> Self::Future {
        let handler = self.clone();
        Box::pin(async move { handler().await.into_response().await })
    }
}

macro_rules! impl_handler {
    ($([($($i: ident),* $(,)?) , $last: ident $(,)?]),* $(,)?) => {
        $(
            impl<F, R, B $(, $i)*, $last> Handler<($($i,)* $last,)> for F
            where
                F: Fn($($i,)* $last) -> R + Clone + Send + Sync + 'static,
                R: Future<Output = B> + Send + 'static,
                B: IntoResponse,
                Self: Sized,
                $($i: FromRequest + Send,)*
                $last: FromRequestBody,
            {
                type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

                fn call(self, req: Request<Incoming>) -> Self::Future {
                    let handler = self.clone();
                    Box::pin(async move {
                        let cookie_jar = CookieJar::default();

                        let mut response = handler(
                            $($i::from_request(&req, cookie_jar.clone()),)*
                            $last::from_request_body(req, cookie_jar.clone()).await
                        ).await.into_response().await;

                        let jar = cookie_jar.as_ref();
                        if jar.delta().count() > 0 {
                            let headers = response.headers_mut();
                            let cookies = jar.delta().map(|v| v.stripped().encoded().to_string()).collect::<Vec<_>>().join(";");
                            headers.insert(header::SET_COOKIE, HeaderValue::from_str(cookies.as_str()).unwrap());
                        }
                        response
                    })
                }
            }
        )*
    };
}

impl_handler!(
    [(), T1],
    [(T1), T2],
    [(T1, T2), T3],
    [(T1, T2, T3), T4],
    [(T1, T2, T3, T4), T5],
    [(T1, T2, T3, T4, T5), T6],
    [(T1, T2, T3, T4, T5, T6), T7],
    [(T1, T2, T3, T4, T5, T6, T7), T8],
    [(T1, T2, T3, T4, T5, T6, T7, T8), T9],
    [(T1, T2, T3, T4, T5, T6, T7, T8, T9), T10],
);


