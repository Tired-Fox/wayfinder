use std::path::{Path, PathBuf};

use askama::Template as Askama;
use handlebars::DirectorySourceOptions;
use serde::Serialize;

use wsf::{
    header,
    layer::LogLayer,
    mime_guess,
    prelude::*,
    server::{Handler, PathRouter, RenderError, Server, TemplateEngine, TemplateRouter, LOCAL},
    Error, Response,
};

pub struct Template<T>(pub T);
impl<T: askama::Template> IntoResponse for Template<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(content) => {
                let mut response = Response::builder();
                if let Some(mime) =
                    mime_guess::from_ext(format!(".{}", T::EXTENSION.unwrap_or("html")).as_str())
                        .first()
                {
                    response = response.header(header::CONTENT_TYPE, mime.to_string());
                }
                response.body(content.into()).unwrap()
            }
            Err(err) => {
                log::error!("(Askama) {}", err);
                Response::empty(500)
            }
        }
    }
}

struct Handlebars(pub handlebars::Handlebars<'static>);
impl Handlebars {
    pub fn new<S: AsRef<Path>>(dir: S) -> Result<Self, handlebars::TemplateError> {
        let mut engine = handlebars::Handlebars::new();
        #[cfg(debug_assertions)]
        engine.set_dev_mode(true);
        engine.register_templates_directory(dir, DirectorySourceOptions::default())?;
        Ok(Self(engine))
    }
}
impl TemplateEngine for Handlebars {
    type Error = handlebars::RenderError;
    fn render<S: Serialize>(&self, name: &str, data: &S) -> Result<String, Self::Error> {
        self.0.render(name, data)
    }

    fn get_render_error(error: Self::Error) -> RenderError {
        use handlebars::RenderErrorReason as Reason;
        match error.reason() {
            Reason::DecoratorNotFound(_)
            | Reason::TemplateNotFound(_)
            | Reason::PartialNotFound(_) => RenderError::MissingTemplate,
            Reason::ParamNotFoundForName(_, _) | Reason::ParamNotFoundForIndex(_, _) => {
                RenderError::MissingParam
            }
            other => RenderError::Other(other.to_string()),
        }
    }

    fn get_template_name_from_dir(path: PathBuf) -> PathBuf {
        path.join("index")
    }

    fn get_template_name_from_file(mut path: PathBuf) -> PathBuf {
        path.set_extension("");
        path
    }
}

pub struct Tera(pub tera::Tera);
impl Tera {
    pub fn new<S: AsRef<str>>(dir: S) -> Result<Self, tera::Error> {
        Ok(Self(tera::Tera::new(dir.as_ref())?))
    }
}
impl TemplateEngine for Tera {
    type Error = tera::Error;

    fn render<S: Serialize>(&self, name: &str, data: &S) -> Result<String, Self::Error> {
        self.0.render(name, &tera::Context::from_serialize(data)?)
    }

    fn get_render_error(error: Self::Error) -> RenderError {
        use tera::ErrorKind as Reason;
        match &error.kind {
            Reason::TemplateNotFound(_) | Reason::MissingParent { .. } => {
                RenderError::MissingTemplate
            }
            _ => RenderError::Other(error.to_string()),
        }
    }

    fn get_template_name_from_dir(path: PathBuf) -> PathBuf {
        path.join("index.html")
    }
}

#[derive(Askama)]
#[template(path = "index.html")]
struct Home {
    message: &'static str,
}

async fn home() -> impl IntoResponse {
    Template(Home {
        message: "Hello, world!",
    })
}

fn main() -> Result<(), Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    Server::bind(LOCAL, 3000)
        .with_router(
            PathRouter::default()
                // Magic _nested capture that will pass what is captured
                // to the template engine to be resolved instead of the full path
                .route(
                    "/blog/:*_nested",
                    TemplateRouter::new(Handlebars::new("templates/blog/")?),
                )
                .route(
                    "/docs/:*_nested",
                    TemplateRouter::new(Tera::new("templates/docs/**/*.html")?),
                )
                .route("/", home)
                .layer(LogLayer::new("Templating"))
                .into_service(),
        )
        .run()
}
