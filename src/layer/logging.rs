use std::future::Future;
use std::task::{Context, Poll};
use std::{convert::Infallible, pin::Pin};

use hyper::{body::Incoming, Method, StatusCode};
use tower::{Layer, Service};

use crate::server::{router::IntoResponse, Request, Response};

#[derive(Clone)]
pub struct LogLayer {
    target: &'static str,
}
impl LogLayer {
    pub fn new(target: &'static str) -> Self {
        Self { target }
    }
}

impl<S: Clone> Layer<S> for LogLayer {
    type Service = LogService<S>;

    fn layer(&self, service: S) -> Self::Service {
        LogService {
            target: self.target,
            service,
        }
    }
}

// This service implements the Log behavior
#[derive(Clone)]
pub struct LogService<S: Clone> {
    target: &'static str,
    service: S,
}

impl<S: Clone> LogService<S> {
    fn method_to_colored_text(method: &Method) -> String {
        let color = match *method {
            Method::GET => "36",
            Method::PUT | Method::POST | Method::OPTIONS => "35",
            Method::DELETE => "31",
            _ => "33",
        };
        format!("\x1b[{color};7m {method:?} \x1b[27;39m")
    }

    fn status_to_color_text(status: StatusCode) -> String {
        let color = if status.is_success() {
            "32"
        } else if status.is_client_error() || status.is_server_error() {
            "31"
        } else {
            "33"
        };
        format!("\x1b[{color}m{}\x1b[39m", status.as_u16())
    }
}

impl<S> Service<Request<Incoming>> for LogService<S>
where
    S: Service<Request<Incoming>, Error = Infallible> + Clone + Send + 'static,
    <S as Service<Request<Incoming>>>::Response: IntoResponse,
    <S as Service<Request<Incoming>>>::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Incoming>) -> Self::Future {
        let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let key = format!(
            "\x1b[38;2;91;96;120m[{time}\x1b[0m {}\x1b[38;2;91;96;120m]\x1b[0m",
            self.target
        );
        let method = Self::method_to_colored_text(request.method());
        let path = request.uri().path().to_string();

        let mut service = self.service.clone();
        Box::pin(async move {
            let response = service.call(request).await.unwrap().into_response().await;
            println!(
                "{key} {method} {} {path}",
                Self::status_to_color_text(response.status())
            );
            Ok(response)
        })
    }
}
