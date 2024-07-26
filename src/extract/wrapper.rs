use hashbrown::HashMap;
use std::fmt::Debug;
use http_body_util::BodyExt;
use hyper::{header, body::Bytes, http::request::Parts};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::Response;

use super::{request::{FromParts, FromRequest}, IntoResponse};

/// Light wrapper around `IntoResponse` to set the `Content-Type` header to `text/html`.
pub struct Html<T>(pub T);
impl<T: IntoResponse> IntoResponse for Html<T> {
    fn into_response(self) -> crate::Response {
        let mut response = self.0.into_response();
        response.headers_mut().insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
        response
    }
}

pub struct Json<T>(pub T);
impl<T: Debug> Debug for Json<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Json").field("inner", &self.0).finish()
    }
}
impl<T: DeserializeOwned> FromRequest for Json<T> {
    async fn from_request(request: crate::Request, _jar: super::CookieJar) -> Result<Self, crate::Error> {
        let body = String::from_utf8(request.collect().await?.to_bytes().to_vec())?;
        Ok(Json(serde_json::from_str::<T>(body.as_str())?))
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> crate::Response {
        match serde_json::to_string(&self.0) {
            Ok(body) => Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.into())
                .unwrap(),
            Err(e) => {
                log::error!("Failed to serialize json response: {}", e);
                Response::builder()
                    .status(500)
                    .body(e.to_string().into())
                    .unwrap()
            },
        }
    }
}

pub struct Query<T>(pub T);
impl<T: Debug> Debug for Query<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query").field("inner", &self.0).finish()
    }
}
impl<T: DeserializeOwned> FromParts for Query<T> {
    async fn from_parts(parts: &Parts, _jar: super::CookieJar) -> Result<Self, crate::Error> {
        match parts.uri.query() {
            Some(query) => {
                let query = query.to_string();
                println!("QUERY: {}", query);
                Ok(Query(serde_urlencoded::from_str::<T>(query.as_str())?))
            },
            None => Err("(400 BAD REQUEST) No query string found".to_string().into()),
        }
    }
}

impl<T: Serialize> IntoResponse for Query<T> {
    fn into_response(self) -> crate::Response {
        match serde_urlencoded::to_string(&self.0) {
            Ok(body) => Response::builder()
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(body.into())
                .unwrap(),
            Err(e) => {
                log::error!("Failed to serialize urlencoded response: {}", e);
                Response::builder()
                    .status(500)
                    .body(e.to_string().into())
                    .unwrap()
            },
        }
    }
}

/// TODO: Needs manual implementation
///     Needs to be able to stream files without storing them in memory
#[derive(Default)]
pub struct FormData(HashMap<String, Bytes>);
impl FormData {
    pub fn new() -> Self {
        Self::default()
    }
}

//impl<T: Serialize> IntoResponse for FormData<T> {
//    fn into_response(self) -> crate::Response {
//        match serde_json::to_string(&self.0) {
//            Ok(body) => Response::builder()
//                .header(header::CONTENT_TYPE, "multipart/form-data")
//                .body(body.into())
//                .unwrap(),
//            Err(e) => {
//                log::error!("Failed to serialize json response: {}", e);
//                Response::builder()
//                    .status(500)
//                    .body(e.to_string().into())
//                    .unwrap()
//            },
//        }
//    }
//}
