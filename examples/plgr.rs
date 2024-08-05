use std::collections::HashMap;

use tokio::io::AsyncReadExt;
use wayfinder::{
    extract::{Capture, Cookie, CookieJar, File, Form, Json, Multipart, Query, TempFile, UrlEncoded},
    layer::LogLayer,
    prelude::*,
    server::{methods, FileRouter, PathRouter, Server, LOCAL, NETWORK},
    Result, StatusCode
};

async fn home(jar: CookieJar) -> impl IntoResponse {
    for cookie in jar.as_ref().iter() {
        println!("{} = {}", cookie.name(), cookie.value())
    }

    if jar.as_ref().get("last_wayfinder_page").is_none() {
        jar.as_mut().add(Cookie::new("last_wayfinder_page", "home"));
    }

    File::open("index.html").await.unwrap()
}

async fn request_data(Json(data): Json<HashMap<String, String>>) -> impl IntoResponse {
    Json(data)
}

#[derive(Default, Form)]
struct MyForm {
    text: String,
    // This becomes the limit per item in the collection
    #[field(limit = 6kb)]
    file1: Vec<TempFile>,
    #[field(limit = 6mb)]
    file2: TempFile,
}

async fn handle_form(jar: CookieJar, Multipart(mut form): Multipart<MyForm>) -> impl IntoResponse {
    //println!("BODY:\n\n{}", body);
    println!("TEXT: {}", form.text);

    // can read data from file object. It is automatically seeked to the beginning of the file.
    for temp in form.file1.iter_mut() {
        if let Some(file) = temp.as_mut() {
            let mut buff = String::new();
            file.read_to_string(&mut buff).await.unwrap();
            println!("{:?}:\n{}", temp.file_name(), buff);
        }
    }

    // can read from other source like tokio::fs::read_to_string or std::fs::read_to_string
    let data = std::fs::read_to_string(form.file2.path()).unwrap();
    println!("FILE2:\n{}", data);

    home(jar).await
}

async fn handle_query(UrlEncoded(data): UrlEncoded) -> impl IntoResponse {
    println!("{:?}", data);
    UrlEncoded(data)
}

async fn unknown(
    Capture(rest): Capture<String>,
    q: Option<Query<HashMap<String, String>>>,
) -> impl IntoResponse {
    format!("Query: `{q:#?}`\nRest: `{rest}`")
}

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
        .init();

    let fallback = || async { (StatusCode::NOT_FOUND, File::open("./pages/404.html").await.unwrap()) };

    Server::bind(NETWORK, 3000)
        .with_router(
            PathRouter::default()
                .route("/", methods::get(home).put(request_data).post(handle_form).delete(handle_query))
                .route("/unknown/:*rest", unknown)
                .route("/blog/:*_", FileRouter::new("pages").enforce_slash(true))
                .fallback(fallback)
                .layer(LogLayer::new("Wayfinder", None))
                .into_service(),
        )
        .run()
}
