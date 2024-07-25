use hyper::{header, http::request::Parts};
use std::{cell::{Ref, RefCell, RefMut}, sync::Arc};

use crate::Error;

use super::request::FromParts;

#[allow(unused_imports)]
pub use cookie::{Cookie, PrivateJar, SignedJar};

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
