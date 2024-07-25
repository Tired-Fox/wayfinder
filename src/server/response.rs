use std::{future::Future, os::windows::fs::MetadataExt};
use http_body_util::{Empty, Full};
use hyper::{body::Bytes, header, StatusCode};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use super::body::Body;

pub type Response<B = Body> = hyper::Response<B>;

pub trait IntoResponse {
    fn into_response(self) -> impl Future<Output = Response> + Send;
}

impl IntoResponse for Response {
    async fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for Response<Full<Bytes>> {
    async fn into_response(self) -> Response {
        self.map(Body::new)
    }
}

impl IntoResponse for Response<Empty<Bytes>> {
    async fn into_response(self) -> Response {
        self.map(|_| Body::empty())
    }
}

impl<B: Into<Body> + Send> IntoResponse for B {
    async fn into_response(self) -> Response {
        hyper::Response::builder().body(self.into()).unwrap()
    }
}

impl IntoResponse for File {
    async fn into_response(self) -> Response {

        let mut builder = hyper::Response::builder();
        if let Ok(metadata) = self.metadata().await {
            builder = builder
                .header(header::CONTENT_LENGTH, metadata.file_size());
        }

        let stream = FramedRead::new(self, BytesCodec::new());
        builder
            .body(Body::from_stream(stream))
            .unwrap()
    }    
}

impl<B: IntoResponse + Send> IntoResponse for (u16, B) {
    async fn into_response(self) -> Response {
        let mut response = self.1.into_response().await;
        *response.status_mut() = StatusCode::from_u16(self.0).unwrap();
        response
    }
}

impl<B: IntoResponse + Send> IntoResponse for (StatusCode, B) {
    async fn into_response(self) -> Response {
        let mut response = self.1.into_response().await;
        *response.status_mut() = self.0;
        response
    }
}
