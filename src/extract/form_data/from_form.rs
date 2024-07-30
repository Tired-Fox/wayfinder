use std::{cell::{Cell, RefCell}, rc::Rc, str::FromStr, sync::{Arc, Mutex, RwLock}};

use futures_util::Future;
use hyper::body::Bytes;
use multer::{Constraints, Field};

pub trait FromForm: Sized {
    type Form: Send;

    fn settings() -> Constraints;
    fn init() -> Self::Form;
    fn push_field(form: Self::Form, field: Field<'static>) -> impl Future<Output = Self::Form> + Send;
    #[allow(dead_code)]
    fn push_error(form: Self::Form, error: crate::Error) -> Self::Form;
    fn finilize(form: Self::Form) -> Result<Self, crate::Error>;
}

pub struct Native;

pub trait FromFormField<T = Native>: Sized {
    #[allow(dead_code)]
    fn from_field(field: Field<'static>) -> impl Future<Output = Result<Self, crate::Error>> + Send;
}

pub struct FormFieldField;
impl<T> FromFormField<FormFieldField> for T
where
    T: FromForm + Send,
{
    async fn from_field(field: Field<'static>) -> Result<Self, crate::Error> {
        T::finilize(T::push_field(T::init(), field).await)
    }
}

impl FromFormField for Vec<u8> {
    async fn from_field(mut field: Field<'static>) -> Result<Self, crate::Error> {
        let mut data = Vec::new();
        while let Some(chunk) = field.chunk().await? {
            data.extend(chunk.iter());
        }
        Ok(data)
    }
}

impl FromFormField for Bytes {
    async fn from_field(field: Field<'static>) -> Result<Self, crate::Error> {
        Ok(field.bytes().await?)
    }
}

pub struct FromStrField;
impl<S> FromFormField<FromStrField> for S
where
    S: FromStr,
    S::Err: std::error::Error + Send + Sync + 'static
{
    async fn from_field(field: Field<'static>) -> Result<Self, crate::Error> {
        let data = field.text().await?;
        Ok(data.parse()?)
    }
}

pub struct OptionalField;
impl<T: FromFormField> FromFormField<OptionalField> for Option<T> {
    async fn from_field(field: Field<'static>) -> Result<Self, crate::Error> {
        Ok(T::from_field(field).await.ok())
    }
}

macro_rules! impl_from_form_with_named_impl {
    ($($ty: ty),* $(,)?) => {
        $(
            paste::paste! {
                pub struct [<$ty Field>];
                impl<T: FromFormField<D>, D> FromFormField<([<$ty Field>], D)> for $ty<T> {
                    async fn from_field(field: Field<'static>) -> Result<Self, crate::Error> {
                        Ok($ty::new(T::from_field(field).await?))
                    }
                }
            }
        )*
    }
}

impl_from_form_with_named_impl!(Box, Rc, Arc, RefCell, Mutex, Cell, RwLock);
