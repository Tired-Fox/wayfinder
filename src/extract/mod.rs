use std::str::FromStr;

use hyper::http::request::Parts;
pub use hyper::{Method, StatusCode, body::Incoming};
pub use tokio::fs::File;

use crate::{server::request::{CookieJar, FromParts}, Error};

#[derive(Debug, Clone)]
pub struct MatchedPath(pub String, pub Vec<(String, String)>);

pub struct Capture<T>(pub T);
impl<T1> FromParts for Capture<T1>
where
    T1: FromStr,
    <T1 as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    async fn from_parts(parts: &Parts, _: CookieJar) -> Result<Self, Error> {
        let params = match parts.extensions.get::<MatchedPath>() {
            Some(MatchedPath(_path, captures)) => captures,
            None => {
                return Err("Missing URL Path Params".into());
            }
        };

        let mut citer = params.iter();
        Ok(Capture(
            citer.next().unwrap().1.parse::<T1>()?,
        ))
    }
}
