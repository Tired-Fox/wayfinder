use http_body_util::{Empty, Full};
use hyper::{body::Bytes, StatusCode};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::server::{body::Body, Response};

pub trait IntoResponse<T = ()> {
    fn into_response(self) -> Response;
}

static WAYFINDER_INERNAL_ERROR: &str = "WAYFINDER-INTERNAL-ERROR";

impl IntoResponse for crate::Error {
    fn into_response(self) -> Response {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(WAYFINDER_INERNAL_ERROR, self.to_string())
            .body(Body::empty())
            .unwrap()
    }
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for Response<Full<Bytes>> {
    fn into_response(self) -> Response {
        self.map(Body::new)
    }
}

impl IntoResponse for Response<Empty<Bytes>> {
    fn into_response(self) -> Response {
        self.map(|_| Body::empty())
    }
}

impl IntoResponse for File {
    fn into_response(self) -> Response {
        let stream = FramedRead::new(self, BytesCodec::new());
        hyper::Response::builder()
            .body(Body::from_stream(stream))
            .unwrap()
    }    
}

impl<B: IntoResponse + Send> IntoResponse for (u16, B) {
    fn into_response(self) -> Response {
        let mut response = self.1.into_response();
        *response.status_mut() = StatusCode::from_u16(self.0).unwrap();
        response
    }
}

impl<B: IntoResponse + Send> IntoResponse for (StatusCode, B) {
    fn into_response(self) -> Response {
        let mut response = self.1.into_response();
        *response.status_mut() = self.0;
        response
    }
}

impl IntoResponse for &str {
    fn into_response(self) -> Response {
        Response::builder()
            .body(Body::new(self.to_string()))
            .unwrap()
    }
}
