extern crate proc_macro;

use proc_macro_error::{proc_macro_error, emit_error};
use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input, Data, DataStruct, AttrStyle, Meta, MetaList, spanned::Spanned, Fields, Ident, Type};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, TokenStreamExt};

mod form_options;
mod field_options;
use form_options::FormOptions;
use field_options::FieldOptions;

#[derive(Debug, Default, Clone, Copy, strum_macros::EnumIs, PartialEq)]
enum Limit {
    #[default]
    None,
    Byte(u64),
    KB(u64),
    MB(u64),
    GB(u64),
}

impl ToTokens for Limit {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            Self::None => {},
            Self::Byte(limit) => tokens.append_all(quote::quote! { #limit }),
            Self::KB(limit) => tokens.append_all(quote::quote! { #limit * 1024 }),
            Self::MB(limit) => tokens.append_all(quote::quote! { #limit * u64::pow(1024, 2) }),
            Self::GB(limit) => tokens.append_all(quote::quote! { #limit * u64::pow(1024, 3) }),
        }
    }
}

struct Field {
    name: String,
    ident: Ident,
    ty: Type,
}

impl Field {
    fn new(ident: Ident, ty: Type, options: FieldOptions) -> Self {
        Self {
            name: options.name,
            ident,
            ty,
        }
    }
}

impl ToTokens for Field {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let name = self.name.as_str();
        let ident = self.ident.clone();
        let ty = self.ty.clone();

        tokens.append_all(quote::quote! {
            Some(#name) => if let Err(err) = <#ty as ::wayfinder::extract::FromFormCollect<_>>::collect_field(&mut form.0.#ident, field).await {
                return Self::push_error(form, err);
            }
        });
    }
}

#[proc_macro_error]
#[proc_macro_derive(Form, attributes(form, field))]
pub fn form_derive(input: TokenStream) -> TokenStream {
    let derive = parse_macro_input!(input as DeriveInput);
    let name = derive.ident.clone();
    let data: DataStruct = if let Data::Struct(data) = derive.data { data }
    else {
        emit_error!(Span::call_site(), "From can only be derived from structs");
        return TokenStream::new();
    };
    
    let mut form_options = FormOptions::default();
    for attr in derive.attrs {
        if let AttrStyle::Outer = attr.style {
            if let Meta::List(MetaList { path, tokens, .. }) = attr.meta {
                if path.segments.len() == 1 && path.segments.first().unwrap().ident.to_string().as_str() == "form" {
                    let attr = tokens.into();
                    form_options += parse_macro_input!(attr as FormOptions);
                } 
            }
        }
    }

    let mut _fields = Vec::new();
    match data.fields {
        Fields::Named(fields) => {
            for field in fields.named {
                if field.ident.is_none() {
                    emit_error!(field.span(), "Form fields must have a name");
                    return TokenStream::new();
                }

                let ident = field.ident.unwrap().clone();
                let ty = field.ty.clone();

                let mut options = FieldOptions::default();
                for attr in field.attrs {
                    if let AttrStyle::Outer = attr.style {
                        if let Meta::List(MetaList { path, tokens, .. }) = attr.meta {
                            if path.segments.len() == 1 && path.segments.first().unwrap().ident.to_string().as_str() == "field" {
                                let attr = tokens.into();
                                options += parse_macro_input!(attr as FieldOptions);
                            } 
                        }
                    }
                }

                if options.name.is_empty() {
                    options.name = ident.to_string();
                }

                form_options += &options;
                _fields.push(Field::new(ident, ty, options));
            }
        },
        fields => {
            emit_error!(fields.span(), "Only named fields are supported in a form");
            return TokenStream::new();
        }
    }

    let _constraints = form_options.into_constraints(&_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<Vec<_>>());

    quote::quote! {
        impl ::wayfinder::extract::FromForm for #name {
            type Form = (#name, Vec::<::wayfinder::Error>);

            fn init() -> Self::Form {
                (
                    #name::default(),
                    Vec::new()
                )
            }
            fn push_error(mut form: Self::Form, error: ::wayfinder::Error) -> Self::Form {
                form.1.push(error);
                form
            }
            fn finilize(mut form: Self::Form) -> std::result::Result<Self, ::wayfinder::Error> {
                if !form.1.is_empty() {
                    return Err(form.1.pop().unwrap());
                }
                Ok(form.0)
            }

            fn settings() -> multer::Constraints {
                #_constraints
            }

            async fn push_field(mut form: Self::Form, field: ::wayfinder::extract::FormField<'static>) -> Self::Form {
                match field.name() {
                    #(#_fields)*
                    _ => ()
                }
                form
            }
        }
    }.into()
}
