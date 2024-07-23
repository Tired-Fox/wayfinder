use std::convert::Infallible;

use futures_util::{future::Map, Future};
use pin_project_lite::pin_project;
use tower::{util::Oneshot, Service};

use crate::{Request, Response};

macro_rules! opaque_future {
    ($(#[$m:meta])* pub type $name:ident = $actual:ty;) => {
        opaque_future! {
            $(#[$m])*
            pub type $name<> = $actual;
        }
    };

    ($(#[$m:meta])* pub type $name:ident<$($param:ident),*> = $actual:ty;) => {
        pin_project_lite::pin_project! {
            $(#[$m])*
            pub struct $name<$($param),*> {
                #[pin] future: $actual,
            }
        }

        impl<$($param),*> $name<$($param),*> {
            pub(crate) fn new(future: $actual) -> Self {
                Self { future }
            }
        }

        impl<$($param),*> std::fmt::Debug for $name<$($param),*> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }

        impl<$($param),*> std::future::Future for $name<$($param),*>
        where
            $actual: std::future::Future,
        {
            type Output = <$actual as std::future::Future>::Output;

            #[inline]
            fn poll(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Self::Output> {
                self.project().future.poll(cx)
            }
        }
    };
}

opaque_future! {
     /// The response future for [`IntoService`](super::IntoService).
    pub type IntoServiceFuture<F> =
        Map<
            F,
            fn(Response) -> Result<Response, Infallible>,
        >;
}

pin_project! {
    pub struct LayeredFuture<S>
    where
        S: Service<Request>
    {
        #[pin]
        inner: Map<Oneshot<S, Request>, fn(Result<S::Response, S::Error>) -> Response>
    }
}

impl<S> LayeredFuture<S>
where
    S: Service<Request>,
{
    pub fn new(inner: Map<Oneshot<S, Request>, fn(Result<S::Response, S::Error>) -> Response>) -> Self {
        Self { inner }
    }
}

impl<S> Future for LayeredFuture<S>
where
    S: Service<Request>,
{
    type Output = Response;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}
