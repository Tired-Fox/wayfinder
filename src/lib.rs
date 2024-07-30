use std::{ops::Deref, sync::Arc};

mod body;

pub mod server;
pub mod layer;
pub mod extract;

use hyper::body::Bytes;
pub use mime_guess;
pub use hyper::{header, StatusCode};
pub use body::{Body, BoxError};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

pub type Request<T = Body> = hyper::Request<T>;
pub type Response<B = Body> = hyper::Response<B>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PercentDecodedStr(Arc<str>);

impl PercentDecodedStr {
    pub(crate) fn new<S>(s: S) -> Option<Self>
    where
        S: AsRef<str>,
    {
        percent_encoding::percent_decode(s.as_ref().as_bytes())
            .decode_utf8()
            .ok()
            .map(|decoded| Self(decoded.as_ref().into()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for PercentDecodedStr {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

pub trait ResponseShortcut {
    fn empty<T>(status: T) -> Response
    where
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>;

    fn ok<B>(body: B) -> Response
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: Into<BoxError>;

    fn error<T, B>(status: T, body: B) -> Response
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: Into<BoxError>,
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>;
}

impl ResponseShortcut for Response {
    fn empty<T>(status: T) -> Response
    where
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>
    {
        Response::builder()
            .status(status)
            .body(Body::empty())
            .unwrap()
    }

    fn ok<B>(body: B) -> Response
            where
                B: http_body::Body<Data = Bytes> + Send + 'static,
                B::Error: Into<BoxError> {

        Response::builder()
            .body(Body::new(body))
            .unwrap()
    } 

    fn error<T, B>(status: T, body: B) -> Response
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: Into<BoxError>,
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<hyper::http::Error>
    {

        Response::builder()
            .status(status)
            .body(Body::new(body))
            .unwrap()
    }
}

pub mod prelude {
    pub use crate::extract::IntoResponse;
    pub use crate::server::Handler;
    pub use super::ResponseShortcut;
}

#[macro_export]
macro_rules! all_variants {
    ($macro: ident) => {
        $macro!(T1);
        $macro!(T1, T2);
        $macro!(T1, T2, T3);
        $macro!(T1, T2, T3, T4);
        $macro!(T1, T2, T3, T4, T5);
        $macro!(T1, T2, T3, T4, T5, T6);
        $macro!(T1, T2, T3, T4, T5, T6, T7);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
        $macro!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);
    };
}

#[macro_export]
macro_rules! all_variants_with_last {
    ($macro: ident) => {
        $macro!((), T1);
        $macro!((T1), T2);
        $macro!((T1, T2), T3);
        $macro!((T1, T2, T3), T4);
        $macro!((T1, T2, T3, T4), T5);
        $macro!((T1, T2, T3, T4, T5), T6);
        $macro!((T1, T2, T3, T4, T5, T6), T7);
        $macro!((T1, T2, T3, T4, T5, T6, T7), T8);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8), T9);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8, T9), T10);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10), T11);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11), T12);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12), T13);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13), T14);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14), T15);
        $macro!((T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15), T16);
    };
}
