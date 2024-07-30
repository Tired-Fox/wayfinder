use std::collections::HashMap;
use proc_macro_error::abort;
use syn::{Ident, punctuated::Punctuated, Token, LitBool, LitInt};
use proc_macro2::TokenStream as TokenStream2;
use quote::{TokenStreamExt, ToTokens};

use super::field_options::FieldOptions;
use super::Limit;

#[allow(dead_code)]
#[derive(strum_macros::EnumIs)]
enum FormOption {
    Limit(Limit),
    FieldLimit(Limit),
    Strict(bool),
}

impl syn::parse::Parse for FormOption {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?;
        match name.to_string().as_str() {
            "limit" => {
                input.parse::<Token![=]>()?;
                let value = input.parse::<LitInt>()?;
                Ok(FormOption::Limit(match value.suffix().to_ascii_lowercase().as_str() {
                    "kb" => Limit::KB(value.base10_parse()?),
                    "mb" => Limit::MB(value.base10_parse()?),
                    "gb" => Limit::GB(value.base10_parse()?),
                    _ => Limit::Byte(value.base10_parse()?),
                }))
            },
            "field_limit" => {
                input.parse::<Token![=]>()?;
                let value = input.parse::<LitInt>()?;
                Ok(FormOption::FieldLimit(match value.suffix().to_ascii_lowercase().as_str() {
                    "kb" => Limit::KB(value.base10_parse()?),
                    "mb" => Limit::MB(value.base10_parse()?),
                    "gb" => Limit::GB(value.base10_parse()?),
                    _ => Limit::Byte(value.base10_parse()?),
                }))
            },
            "strict" => {
                let strict = if input.parse::<Token![=]>().is_ok() {
                    let value = input.parse::<LitBool>()?;
                    value.value
                } else {
                    true
                };
                Ok(FormOption::Strict(strict))
            },
            _ => { abort!(name.span(), "Unknown form option"); }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct FormOptions {
    pub limit: Limit,
    pub field_limit: Limit,
    pub strict: bool,
    pub debug: Vec<String>,
    pub field_limits: HashMap<String, Limit>,
}

impl std::ops::AddAssign for FormOptions {
    fn add_assign(&mut self, other: Self) {
        if self.limit != other.limit && !other.limit.is_none() {
            self.limit = other.limit;
        }

        if self.field_limit != other.field_limit && !other.field_limit.is_none() {
            self.field_limit = other.field_limit;
        }

        if other.strict {
            self.strict = other.strict;
        }

        self.debug.extend(other.debug);
    }
}

impl std::ops::AddAssign<&FieldOptions> for FormOptions {
    fn add_assign(&mut self, other: &FieldOptions) {
        self.field_limits.insert(other.name.clone(), other.limit);
    }
}

impl syn::parse::Parse for FormOptions {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let options = Punctuated::<FormOption, Token![,]>::parse_terminated(input)?;
        let mut result = FormOptions::default();
        for option in options {
            match option {
                FormOption::Limit(limit) => result.limit = limit,
                FormOption::FieldLimit(limit) => result.field_limit = limit,
                FormOption::Strict(strict) => result.strict = strict,
            }
        }
        Ok(result)
    }
}

impl FormOptions {
    pub fn into_constraints(self, names: &[&str]) -> TokenStream2 {
        if !self.debug.is_empty() {
            let debug = self.debug.clone();
            return quote::quote! { #(#debug)* }
        }

        let mut result = quote::quote! { multer::Constraints::default() };

        if !names.is_empty() && self.strict {
            result.append_all(quote::quote! { .allowed_fields(vec![#(#names,)*]) });
        }

        if !self.limit.is_none() || !self.field_limit.is_none() || !self.field_limits.is_empty() {
            let mut limits = TokenStream2::new();
            limits.append_all(quote::quote! { ::wayfinder::extract::SizeLimit::default() });
            if !self.limit.is_none() {
                limits.append_all({
                        let limit = self.limit.into_token_stream();
                        quote::quote! { .whole_stream(#limit) }
                });
            }
            if !self.field_limit.is_none() {
                limits.append_all({
                    let limit = self.field_limit.into_token_stream();
                    quote::quote! { .per_field(#limit) }
                });
            }
            if !self.field_limits.is_empty() {
                for (name, limit) in self.field_limits.iter() {
                    if !limit.is_none() {
                        limits.append_all(quote::quote! { .for_field(#name, #limit) });
                    }
                }
            }
            result.append_all(quote::quote! { .size_limit(#limits) });
        }
        result
    }
}
