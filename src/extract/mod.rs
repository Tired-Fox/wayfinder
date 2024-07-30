use http_body_util::BodyExt;
pub use hyper::{Method, StatusCode, body::{Incoming, Bytes}};
pub use tokio::fs::File;

mod request;
mod response;
mod cookies;
mod capture;
mod redirect;
mod wrapper;
mod form_data;

pub use cookies::{CookieJar, Cookie};
pub use capture::{Capture, UriParams};
pub use redirect::Redirect;
pub use response::IntoResponse;
pub use request::{FromRequest, FromParts};
pub use wrapper::{Html, Json, Query};
pub use form_data::{Form as Multipart, FromFormField, FromForm, SizeLimit, Field as FormField, TempFile};
pub use wayfinder_macros::Form;

impl FromRequest for Bytes {
    async fn from_request(request: crate::Request, _: CookieJar) -> Result<Self, crate::Error> {
        Ok(request.into_body().collect().await?.to_bytes())
    }
}

impl IntoResponse for Bytes {
    fn into_response(self) -> crate::Response {
        crate::Response::builder()
            .body(crate::Body::from(self))
            .unwrap()
    }
}
