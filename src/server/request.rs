use std::{cell::{Ref, RefCell, RefMut}, future::Future, sync::Arc};

use http_body_util::BodyExt;
use hyper::{body::Incoming, header, http::request::Parts};
pub use cookie::{Cookie, PrivateJar, SignedJar};

use crate::Error;

use super::body::Body;

/// [cookie::CookieJar]
//pub type CookieJar = Arc<Mutex<cookie::CookieJar>>;
#[derive(Default, Clone)]
pub struct CookieJar(Arc<RefCell<cookie::CookieJar>>);
impl CookieJar {
    // add, get, remove, remove_force, add_original, iter
    pub fn as_ref(&self) -> Ref<'_, cookie::CookieJar> {
        self.0.borrow()
    }

    pub fn as_mut(&self) -> RefMut<'_, cookie::CookieJar> {
        self.0.borrow_mut()
    }
}
unsafe impl Send for CookieJar {}

pub type Request<T = Body> = hyper::Request<T>;

pub trait FromParts: Sized {
    fn from_parts(parts: &Parts, jar: CookieJar) -> impl Future<Output = Result<Self, Error>> + Send;
}

impl FromParts for CookieJar {
    async fn from_parts(parts: &Parts, jar: CookieJar) -> Result<Self, Error> {
        if let Some(cookies) = parts.headers.get(header::COOKIE) {
            let mut jar = jar.as_mut();
            for cookie in Cookie::split_parse_encoded(cookies.to_str().unwrap().to_string()).flatten() {
                jar.add_original(cookie);
            }
        }
        Ok(jar)
    }
}

pub trait FromRequest: Sized {
    fn from_request(request: Request<Incoming>, jar: CookieJar) -> impl Future<Output = Result<Self, Error>> + Send;
}

impl<T: FromParts> FromRequest for T {
    async fn from_request(request: Request<Incoming>, jar: CookieJar) -> Result<Self, Error> {
        T::from_parts(&request.into_parts().0, jar).await
    }
}

impl FromRequest for Request<Incoming> {
    async fn from_request(request: Request<Incoming>, _: CookieJar) -> Result<Self, Error> {
        Ok(request)
    }
}

// This allows for for the last parameter to collect the body as a string
impl FromRequest for String {
    async fn from_request(request: Request<Incoming>, _: CookieJar) -> Result<Self, Error> {
        Ok(String::from_utf8(request.collect().await.unwrap().to_bytes().to_vec())?)
    }
}
