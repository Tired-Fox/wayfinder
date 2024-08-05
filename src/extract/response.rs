use std::{borrow::Cow, convert::Infallible};

use hyper::http::response::Parts;
use http_body_util::{Empty, Full};
use hyper::{body::Bytes, header::{self, HeaderName, HeaderValue}, HeaderMap, StatusCode};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{all_variants, Body, BoxError, Response};

pub trait IntoResponse<S = ()> {
    fn into_response(self) -> Response;
}

pub trait IntoResponseParts {
    type Error: IntoResponse;
    fn into_response_parts(self, res: Response) -> Result<Response, Self::Error>;
}

impl<K, V, const N: usize> IntoResponseParts for [(K, V); N]
where
    K: TryInto<HeaderName>,
    K::Error: std::error::Error + Send + Sync + 'static,
    V: TryInto<HeaderValue>,
    V::Error: std::error::Error + Send + Sync + 'static,
{
    type Error = crate::Error;

    fn into_response_parts(self, mut res: Response) -> Result<Response, Self::Error> {
        for (k, v) in self {
            res.headers_mut().insert(k.try_into()?, v.try_into()?);
        }
        Ok(res)
    }
}

impl IntoResponseParts for hyper::http::HeaderMap {
    type Error = Infallible;
    fn into_response_parts(self, mut res: Response) -> Result<Response, Self::Error> {
        res.headers_mut().extend(self);
        Ok(res)
    }
}

impl IntoResponseParts for hyper::http::Extensions {
    type Error = Infallible;
    fn into_response_parts(self, mut res: Response) -> Result<Response, Self::Error> {
        *res.extensions_mut() = self;
        Ok(res)
    }
}

impl IntoResponseParts for hyper::http::Version {
    type Error = Infallible;
    fn into_response_parts(self, mut res: Response) -> Result<Response, Self::Error> {
        *res.version_mut() = self;
        Ok(res)
    }
}

impl IntoResponseParts for () {
    type Error = Infallible;
    fn into_response_parts(self, res: Response) -> Result<Response, Self::Error> {
        Ok(res)
    }
}

impl<T: IntoResponseParts> IntoResponseParts for Option<T> {
    type Error = T::Error;
    fn into_response_parts(self, res: Response) -> Result<Response, Self::Error> {
        match self {
            Some(inner) => inner.into_response_parts(res),
            None => Ok(res)
        }
    }
}

macro_rules! impl_into_response_parts {
    ($($ty: ident),* $(,)?) => {
        impl<$($ty,)*> IntoResponseParts for ($($ty,)*)
        where
            $($ty: IntoResponseParts,)*
        {
            type Error = Response;
            fn into_response_parts(self, res: Response) -> Result<Response, Self::Error> {
                #[allow(non_snake_case)]
                let ($($ty,)*) = self;

                $(
                    let res = match $ty.into_response_parts(res) {
                        Ok(parts) => parts,
                        Err(err) => {
                            return Err(err.into_response());
                        }
                    };
                )*

                Ok(res)
            }
        }
    };
}

all_variants!(impl_into_response_parts);

static WAYFINDER_INERNAL_ERROR: &str = "X-WAYFINDER-INTERNAL-ERROR";

impl IntoResponse for crate::Error {
    fn into_response(self) -> Response {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(WAYFINDER_INERNAL_ERROR, self.to_string())
            .body(Body::empty())
            .unwrap()
    }
}

impl IntoResponse for Infallible {
    fn into_response(self) -> Response {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap()
    }
}

impl IntoResponse for () {
    fn into_response(self) -> Response {
        Response::new(Body::empty())
    }
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for Response<Full<Bytes>> {
    fn into_response(self) -> Response {
        self.map(Body::new)
    }
}

impl IntoResponse for Response<Empty<Bytes>> {
    fn into_response(self) -> Response {
        self.map(|_| Body::empty())
    }
}

pub struct ResponseFromStream;
impl<S> IntoResponse<ResponseFromStream> for S
where
    S: futures_util::TryStream + Send + 'static,
    S::Ok: Into<Bytes>,
    S::Error: Into<BoxError>
{
    fn into_response(self) -> Response {
        hyper::Response::builder()
            .body(Body::from_stream(self))
            .unwrap()
    }    
}

impl IntoResponse for File {
    fn into_response(self) -> Response {
        FramedRead::new(self, BytesCodec::new()).into_response()
    }    
}

impl IntoResponse for Cow<'static, [u8]> {
    fn into_response(self) -> Response {
        Response::builder()
            .header(header::CONTENT_TYPE, HeaderValue::from_static(mime::APPLICATION_OCTET_STREAM.as_ref()))
            .body(Body::from(self))
            .unwrap()
    }
}
impl IntoResponse for Vec<u8> {
    fn into_response(self) -> Response {
        Cow::<'static, [u8]>::Owned(self).into_response()
    }
}
impl IntoResponse for &'static [u8] {
    fn into_response(self) -> Response {
        Cow::<'static, [u8]>::Borrowed(self).into_response()
    }
}
impl<const N: usize> IntoResponse for [u8;N] {
    fn into_response(self) -> Response {
        self.to_vec().into_response()
    }
}
impl IntoResponse for Box<[u8]> {
    fn into_response(self) -> Response {
        Vec::from(self).into_response()
    }
}

impl IntoResponse for Cow<'static, str> {
    fn into_response(self) -> Response {
        Response::builder()
            .header(header::CONTENT_TYPE, HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()))
            .body(self.into())
            .unwrap()
    }
}
impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Cow::<'static, str>::Borrowed(self).into_response()
    }
}
impl IntoResponse for String {
    fn into_response(self) -> Response {
        Cow::<'static, str>::Owned(self).into_response()
    }
}

impl IntoResponse for HeaderMap {
    fn into_response(self) -> Response {
        let mut res = ().into_response();
        *res.headers_mut() = self;
        res
    }
}

impl<K, V, const N: usize> IntoResponse for [(K, V); N]
where
    K: TryInto<HeaderName>,
    K::Error: std::fmt::Display + std::fmt::Debug,
    V: TryInto<HeaderValue>,
    V::Error: std::fmt::Display + std::fmt::Debug,
{
    fn into_response(self) -> Response {
        let mut headers = HeaderMap::new();
        for (k, v) in self {
            headers.insert(k.try_into().unwrap(), v.try_into().unwrap());
        }

        let mut res = ().into_response();
        *res.headers_mut() = headers;
        res
    }
}

impl<R> IntoResponse for (Parts, R)
where
    R: IntoResponse,
{
    fn into_response(self) -> Response {
        let (parts, res) = self;
        (parts.status, parts.headers, parts.extensions, res).into_response()
    }
}

impl<R> IntoResponse for (StatusCode, Parts, R)
where
    R: IntoResponse,
{
    fn into_response(self) -> Response {
        let (status, parts, res) = self;
        (status, parts.headers, parts.extensions, res).into_response()
    }
}

impl<R> IntoResponse for (StatusCode, R)
where
    R: IntoResponse,
{
    fn into_response(self) -> Response {
        let mut res = self.1.into_response();
        *res.status_mut() = self.0;
        res
    }
}

macro_rules! impl_into_response {
    ( $($ty:ident),* $(,)? ) => {
        #[allow(non_snake_case)]
        impl<R, $($ty,)*> IntoResponse for ($($ty),*, R)
        where
            $( $ty: IntoResponseParts, )*
            R: IntoResponse,
        {
            fn into_response(self) -> Response {
                let ($($ty),*, res) = self;

                let res = res.into_response();

                $(
                    let res = match $ty.into_response_parts(res) {
                        Ok(parts) => parts,
                        Err(err) => {
                            return err.into_response();
                        }
                    };
                )*

                res
            }
        }

        #[allow(non_snake_case)]
        impl<R, $($ty,)*> IntoResponse for (StatusCode, $($ty),*, R)
        where
            $( $ty: IntoResponseParts, )*
            R: IntoResponse,
        {
            fn into_response(self) -> Response {
                let (status, $($ty),*, res) = self;

                let res = res.into_response();

                $(
                    let res = match $ty.into_response_parts(res) {
                        Ok(parts) => parts,
                        Err(err) => {
                            return err.into_response();
                        }
                    };
                )*

                (status, res).into_response()
            }
        }

        #[allow(non_snake_case)]
        impl<R, $($ty,)*> IntoResponse for (Parts, $($ty),*, R)
        where
            $( $ty: IntoResponseParts, )*
            R: IntoResponse,
        {
            fn into_response(self) -> Response {
                let (parts, $($ty),*, res) = self;

                let res = res.into_response();
                $(
                    let res = match $ty.into_response_parts(res) {
                        Ok(parts) => parts,
                        Err(err) => {
                            return err.into_response();
                        }
                    };
                )*

                (parts, res).into_response()
            }
        }
    }
}

all_variants!(impl_into_response);
