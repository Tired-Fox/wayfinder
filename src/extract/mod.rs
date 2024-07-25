use std::{fmt::Display, sync::Arc};

use hyper::http::request::Parts;
pub use hyper::{Method, StatusCode, body::Incoming};
use serde::de::DeserializeOwned;
pub use tokio::fs::File;

use crate::{Error, PercentDecodedStr};


pub mod request;
pub mod response;

mod cookies;
mod de;
#[cfg(feature="askama")]
mod template;

use request::FromParts;
use de::{ErrorKind, PathDeserializationError, PathDeserializer};

pub use cookies::{CookieJar, Cookie};
#[cfg(feature="askama")]
pub use template::Template;


#[derive(Debug, Clone)]
pub enum UriParams {
    Valid(Vec<(Arc<str>, PercentDecodedStr)>),
    InvalidEncoding(Arc<str>)
}

#[derive(Debug)]
pub struct MissingPathParams;
impl Display for MissingPathParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Missing URI Path Params")
    }
}
impl std::error::Error for MissingPathParams {}

pub struct Capture<T>(pub T);

impl<T> FromParts for Capture<T>
where
    T: DeserializeOwned + Send,
{
    async fn from_parts(parts: &Parts, _: CookieJar) -> Result<Self, Error> {
        let params = match parts.extensions.get::<UriParams>() {
            Some(UriParams::Valid(captures)) => captures,
            Some(UriParams::InvalidEncoding(key)) => {
                return Err(PathDeserializationError {
                    kind: ErrorKind::InvalidEncoding(key.to_string())
                }.into())
            },
            None => {
                return Err(MissingPathParams.into())
            }
        };

        Ok(T::deserialize(PathDeserializer::new(params)).map(Capture)?)
    }
}
