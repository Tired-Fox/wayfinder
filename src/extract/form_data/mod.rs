use std::{fmt::Debug, io::SeekFrom, path::PathBuf};

use futures_util::StreamExt;
use hyper::header;

mod from_form;

use tokio::{fs::{File, OpenOptions}, io::{AsyncSeekExt, AsyncWriteExt}};
use uuid::Uuid;
#[allow(unused_imports)]
pub use wayfinder_macros::Form;
#[allow(unused_imports)]
pub use from_form::{FromForm, FromFormField};
pub use multer::Field;

use super::request::FromRequest;

#[allow(unused_imports)]
pub use multer::{Constraints, SizeLimit};

pub struct Form<T = multer::Multipart<'static>>(pub T);
impl<T: Debug> Debug for Form<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form")
            .field("inner", &self.0)
            .finish()
    }
}
impl<T: PartialEq> PartialEq for Form<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}
impl<T: Clone> Clone for Form<T> {
    fn clone(&self) -> Self {
        Form(self.0.clone())
    }
}

impl<T: FromForm + Send> FromRequest for Form<T> {
    async fn from_request(request: crate::Request, _jar: super::CookieJar) -> Result<Self, crate::Error> {
        // Extract the `multipart/form-data` boundary from the headers.
        let boundary = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|ct| ct.to_str().ok())
            .and_then(|ct| multer::parse_boundary(ct).ok());

        if boundary.is_none() {
            return Err("BAD REQUEST: Invalid content type".into());
        }


        // Convert the body into a stream of data frames.
        let body_stream = request.into_body().into_data_stream();
        let mut multipart = multer::Multipart::with_constraints(body_stream, boundary.unwrap(), T::settings());

        let mut form: T::Form = T::init();
        while let Some(field) = multipart.next_field().await? {
            form = T::push_field(form, field).await;
        }

        Ok(Form(T::finilize(form)?))
    }
}

impl FromRequest for Form {
    async fn from_request(request: crate::Request, _jar: super::CookieJar) -> Result<Self, crate::Error> {
        // Extract the `multipart/form-data` boundary from the headers.
        let boundary = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|ct| ct.to_str().ok())
            .and_then(|ct| multer::parse_boundary(ct).ok());

        if boundary.is_none() {
            return Err("BAD REQUEST: Invalid content type".into());
        }


        // Convert the body into a stream of data frames.
        let body_stream = request.into_body().into_data_stream();
        let multipart = multer::Multipart::with_constraints(body_stream, boundary.unwrap(), Constraints::default());

        Ok(Form(multipart))
    }
}

#[derive(Default, Debug)]
pub struct TempFile {
    path: PathBuf,
    file: Option<File>,
}

impl TempFile {
    pub fn as_ref(&self) -> Option<&File> {
        self.file.as_ref()
    }

    pub fn as_mut(&mut self) -> Option<&mut File> {
        self.file.as_mut()
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl FromFormField for TempFile {
    async fn from_field(mut field: Field<'static>) -> Result<Self, crate::Error> {
        let base = std::env::temp_dir().join("wayfinder");
        let path = base.join(format!("{}-{}", field.name().unwrap(), Uuid::now_v7()));

        if !base.exists() {
            std::fs::create_dir_all(&base)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true)
            .open(&path)
            .await?;

        while let Some(Ok(chunk)) = field.next().await {
            file.write_all(&chunk).await?;
        }

        file.flush().await?;
        file.seek(SeekFrom::Start(0)).await?;
        Ok(Self { path, file: Some(file) })
    }
}
