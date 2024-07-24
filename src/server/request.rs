use std::{cell::{Ref, RefCell, RefMut}, future::Future, sync::Arc};

use http_body_util::BodyExt;
use hyper::{body::Incoming, header};
pub use cookie::{Cookie, PrivateJar, SignedJar};

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

pub trait FromRequest {
    fn from_request(req: &Request<Incoming>, jar: CookieJar) -> Self;
}

impl FromRequest for CookieJar {
    fn from_request(req: &Request<Incoming>, jar: CookieJar) -> Self {
        if let Some(cookies) = req.headers().get(header::COOKIE) {
            let mut jar = jar.as_mut();
            for cookie in Cookie::split_parse_encoded(cookies.to_str().unwrap().to_string()).flatten() {
                jar.add_original(cookie);
            }
        }
        jar
    }
}

pub trait FromRequestBody: Sized {
    fn from_request_body(req: Request<Incoming>, jar: CookieJar) -> impl Future<Output = Self> + Send;
}

impl<T: FromRequest> FromRequestBody for T {
    async fn from_request_body(req: Request<Incoming>, jar: CookieJar) -> Self {
        T::from_request(&req, jar)
    }
}

impl FromRequestBody for Request<Incoming> {
    async fn from_request_body(req: Request<Incoming>, _jar: CookieJar) -> Self {
        req
    }
}

// This allows for for the last parameter to collect the body as a string
impl FromRequestBody for String {
    async fn from_request_body(req: Request<Incoming>, _jar: CookieJar) -> Self {
        String::from_utf8(req.collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }
}
