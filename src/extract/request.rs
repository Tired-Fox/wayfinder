use std::future::Future;

use http_body_util::BodyExt;
use hyper::http::request::Parts;
#[allow(unused_imports)]
pub use cookie::{Cookie, PrivateJar, SignedJar};

use crate::Error;

use crate::Request;

use super::CookieJar;

pub trait FromParts: Sized {
    fn from_parts(parts: &Parts, jar: CookieJar) -> impl Future<Output = Result<Self, Error>> + Send;
}

impl<T: FromParts> FromParts for Option<T> {
    async fn from_parts(parts: &Parts, jar: CookieJar) -> Result<Self, Error> {
        match T::from_parts(parts, jar).await {
            Ok(t) => Ok(Some(t)),
            Err(e) => {
                log::error!("Error while parsing handler params: {}", e);
                Ok(None)
            },
        }
    }
}

pub struct ViaRequest;
pub struct ViaParts;

pub trait FromRequest<M = ViaRequest>: Sized {
    fn from_request(request: Request, jar: CookieJar) -> impl Future<Output = Result<Self, Error>> + Send;
}

impl<T: FromRequest> FromRequest for Option<T> {
    async fn from_request(request: Request, jar: CookieJar) -> Result<Self, Error> {
        match T::from_request(request, jar).await {
            Ok(t) => Ok(Some(t)),
            Err(e) => {
                log::error!("Error while parsing handler params: {}", e);
                Ok(None)
            },
        }
    }
}

impl<T: FromParts> FromRequest<ViaParts> for T {
    async fn from_request(request: Request, jar: CookieJar) -> Result<Self, Error> {
        T::from_parts(&request.into_parts().0, jar).await
    }
}

impl FromRequest for Request {
    async fn from_request(request: Request, _: CookieJar) -> Result<Self, Error> {
        Ok(request)
    }
}

// This allows for for the last parameter to collect the body as a string
impl FromRequest for String {
    async fn from_request(request: Request, _: CookieJar) -> Result<Self, Error> {
        Ok(String::from_utf8(request.collect().await.unwrap().to_bytes().to_vec())?)
    }
}
