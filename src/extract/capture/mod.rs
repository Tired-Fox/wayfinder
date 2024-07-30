use std::{fmt::Display, sync::Arc};

use hyper::http::request::Parts;
use serde::de::DeserializeOwned;
use crate::{Error, PercentDecodedStr};

mod de;

use super::{request::FromParts, CookieJar};
use de::{ErrorKind, PathDeserializationError, PathDeserializer};

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
