use std::{
    future::Future, path::PathBuf, pin::Pin, sync::{Arc, Mutex}, task::{Context, Poll}, fmt::{Debug, Display}, collections::HashMap,
    any::type_name, ops::Deref,
};

use http_body::Body as HttpBody;
use http_body_util::BodyExt;
use serde_json::json;
use serde::Serialize;
use tower::Service;
use hyper::body::Bytes;

use crate::extract::UriParams;

use crate::server::Handler;
use crate::{BoxError, Body, Request, Response, ResponseShortcut};

#[derive(Debug, Clone)]
pub enum RenderError {
    MissingTemplate,
    MissingParam,
    Other(String)
}

impl Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            RenderError::MissingTemplate => "Missing template",
            RenderError::MissingParam => "Missing template parameter",
            RenderError::Other(msg) => msg.as_str()
        })
    }
}
impl std::error::Error for RenderError {}

pub struct TemplateRouter<T> {
    engine: Arc<Mutex<T>>
}

impl<T> Clone for TemplateRouter<T> {
    fn clone(&self) -> Self {
        Self {
            engine: self.engine.clone()
        }
    }
}

impl<T: TemplateEngine + Debug + Sized> Debug for TemplateRouter<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let engine = &*self.engine.lock().unwrap();
        f.debug_struct("TemplateRouter")
            .field("engine", engine)
            .finish()
    }
}

impl<T> TemplateRouter<T> {
    pub fn new(engine: T) -> Self {
        Self {
            engine: Arc::new(Mutex::new(engine))
        }
    }
}

impl<T: TemplateEngine + 'static> Handler<TemplateRouter<T>> for TemplateRouter<T> {
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, req: Request) -> Self::Future {
        let router = self.clone();
        Box::pin(async move {
            let mut captures = match req.extensions().get::<UriParams>() {
                Some(UriParams::Valid(params)) => params.iter().map(|(k, v)| (k.to_string(), v.deref().to_string())).collect::<HashMap<String, String>>(),
                _ => HashMap::default(),
            };

            let path = if let Some(nested) = captures.remove("_nested") {
                PathBuf::from(nested.trim_start_matches('/'))
            } else {
                PathBuf::from(req.uri().path().trim_start_matches('/'))
            };

            let path = if path.extension().is_none() {
                T::get_template_name_from_dir(path)
            } else {
                T::get_template_name_from_file(path)
            };

            let name = path.display().to_string().replace("\\", "/");
            let content_type = path.extension().and_then(|ext| {
                mime_guess::from_ext(ext.to_str().unwrap()).first().map(|mime| mime.to_string())
            });

            let data = json! ({
                "captures": captures,
                "request": json!({
                    "version": format!("{:?}", req.version()),
                    "method": req.method().to_string(),
                    "headers": req.headers()
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
                        .collect::<HashMap<_, _>>(),
                    "uri": req.uri().path().to_string(),
                    "query": req.uri().query().map(|v| v.to_string()).unwrap_or_default(),
                    "body": String::from_utf8(req.collect().await.unwrap().to_bytes().to_vec()).unwrap(),
                })
            });

            let engine = router.engine.lock().unwrap();
            match engine.render(name.as_str(), &data) {
                Ok(result) => {
                    let mut response = Response::builder();
                    if let Some(content_type) = content_type {
                        response = response.header(hyper::header::CONTENT_TYPE, content_type);
                    }
                    response.body(result.into()).unwrap()
                },
                Err(err) => {
                    log::error!("({}) {}", type_name::<T>(), err);
                    match T::get_render_error(err) {
                        RenderError::MissingTemplate => Response::empty(404),
                        _ => Response::empty(500)
                    }
                }
            }
        })
    }
}

impl<T: TemplateEngine + 'static, B> Service<Request<B>> for TemplateRouter<T>
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future =
        Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let handler = self.clone();
        let req = req.map(Body::new);
        Box::pin(async move {
            Ok(Handler::call(handler, req).await)
        })
    }
}

pub trait TemplateEngine: Send
where
    Self: Sized
{
    type Error: std::error::Error;
    fn render<S: Serialize>(&self, name: &str, data: &S) -> Result<String, Self::Error>;
    fn get_render_error(error: Self::Error) -> RenderError;
    fn get_template_name_from_dir(path: PathBuf) -> PathBuf {
        path
    }
    fn get_template_name_from_file(path: PathBuf) -> PathBuf {
        path
    }
}
