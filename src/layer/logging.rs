use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{convert::Infallible, pin::Pin};

use hashbrown::HashSet;
use hyper::{Method, StatusCode};
use tower::{Layer, Service};

use crate::{extract::IntoResponse, Request, Response};

#[derive(Debug, Default)]
pub struct LogOptions {
    headers: bool,
    sensitive: Option<HashSet<String>>,
}

pub trait IntoLogOptions<T = ()> {
    fn into_log_options(self) -> LogOptions;
}

impl IntoLogOptions for LogOptions {
    fn into_log_options(self) -> LogOptions {
        self
    }
}

impl IntoLogOptions for Option<LogOptions> {
    fn into_log_options(self) -> LogOptions {
        self.unwrap_or_default()
    }
}

impl<S: ToString, const N: usize> IntoLogOptions for [S;N] {
    fn into_log_options(self) -> LogOptions {
        LogOptions {
            headers: true,
            sensitive: Some(self.into_iter().map(|v| v.to_string()).collect())
        }
    }
}

impl<S: ToString> IntoLogOptions for &[S] {
    fn into_log_options(self) -> LogOptions {
        LogOptions {
            headers: true,
            sensitive: Some(self.iter().map(|v| v.to_string()).collect())
        }
    }
}

impl<S: ToString> IntoLogOptions for Vec<S> {
    fn into_log_options(self) -> LogOptions {
        LogOptions {
            headers: true,
            sensitive: Some(self.into_iter().map(|v| v.to_string()).collect())
        }
    }
}

impl IntoLogOptions for bool {
    fn into_log_options(self) -> LogOptions {
        LogOptions {
            headers: true,
            sensitive: None
        }
    }
}

impl LogOptions {
    pub fn with_headers() -> Self {
        Self::default().headers(true)
    }

    pub fn headers(mut self, state: bool) -> Self {
        self.headers = state;
        self
    }

    pub fn sensitive<S: ToString, I: IntoIterator<Item=S>>(mut self, keys: I) -> Self {
        self.sensitive = Some(keys.into_iter().map(|v| v.to_string()).collect());
        self
    }
}

#[derive(Clone)]
pub struct LogLayer {
    target: &'static str,
    options: Arc<LogOptions>
}

impl LogLayer {
    pub fn new<D, I: IntoLogOptions<D>>(target: &'static str, options: I) -> Self {
        Self { target, options: Arc::new(options.into_log_options()) }
    }
}

impl<S: Clone> Layer<S> for LogLayer {
    type Service = LogService<S>;

    fn layer(&self, service: S) -> Self::Service {
        LogService {
            target: self.target,
            options: self.options.clone(),
            service,
        }
    }
}

// This service implements the Log behavior
#[derive(Clone)]
pub struct LogService<S: Clone> {
    target: &'static str,
    options: Arc<LogOptions>,
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

impl<S> Service<Request> for LogService<S>
where
    S: Service<Request, Error = Infallible> + Clone + Send + 'static,
    <S as Service<Request>>::Response: IntoResponse,
    <S as Service<Request>>::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let key = format!(
            "\x1b[38;2;91;96;120m[{time}\x1b[0m {}\x1b[38;2;91;96;120m]\x1b[0m",
            self.target
        );
        let method = Self::method_to_colored_text(request.method());
        let path = request.uri().path().to_string();

        let mut service = self.service.clone();
        let options = self.options.clone();
        Box::pin(async move {
            let headers = request.headers().clone();

            let response = service.call(request).await.unwrap().into_response();
            println!(
                "{key} {method} {} {path}",
                Self::status_to_color_text(response.status()),
            );

            if options.headers {
                let mut h = HashMap::new();
                for (key, value) in headers.iter() {
                    if options.sensitive.is_some() && options.sensitive.as_ref().unwrap().contains(key.as_str()) {
                        h.insert(key.as_str(), "[**MASKED**]");
                    } else {
                        h.insert(key.as_str(), value.to_str().unwrap());
                    }
                }
                println!("{}", serde_json::to_string(&h).unwrap());
            }
            Ok(response)
        })
    }
}
