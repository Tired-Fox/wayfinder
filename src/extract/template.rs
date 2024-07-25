use hyper::header;

use crate::server::{prelude::ResponseShortcut, Response};

use super::response::IntoResponse;

pub struct Template<T>(pub T);

impl<T: askama::Template> IntoResponse for Template<T>  {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(content) => {
                let mut response = Response::builder();
                if let Some(mime) = mime_guess::from_ext(format!(".{}", T::EXTENSION.unwrap_or("html")).as_str()).first() {
                    response = response.header(header::CONTENT_TYPE, mime.to_string());
                }
                response.body(content.into()).unwrap() 
            },
            Err(err) => {
                log::error!("(Askama) {}", err);
                Response::empty(500)
            } 
        }
    }
}
