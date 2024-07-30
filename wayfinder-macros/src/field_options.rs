use proc_macro_error::abort;
use syn::{Ident, Token, LitInt, LitStr, punctuated::Punctuated};
use super::Limit;

#[allow(dead_code)]
#[derive(strum_macros::EnumIs)]
enum FieldOption {
    Limit(Limit),
    Name(String),
}

impl syn::parse::Parse for FieldOption {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?;
        match name.to_string().as_str() {
            "limit" => {
                input.parse::<Token![=]>()?;
                let value = input.parse::<LitInt>()?;
                Ok(FieldOption::Limit(match value.suffix().to_ascii_lowercase().as_str() {
                    "kb" => Limit::KB(value.base10_parse()?),
                    "mb" => Limit::MB(value.base10_parse()?),
                    "gb" => Limit::GB(value.base10_parse()?),
                    _ => Limit::Byte(value.base10_parse()?),
                }))
            },
            "name" => {
                input.parse::<Token![=]>()?;
                let value = input.parse::<LitStr>()?;
                Ok(FieldOption::Name(value.value()))
            },
            _ => { abort!(name.span(), "Unknown form field option"); }
        }
    }
}

#[derive(Default)]
pub struct FieldOptions {
    pub name: String,
    pub limit: Limit,
}

impl std::ops::AddAssign for FieldOptions {
    fn add_assign(&mut self, other: Self) {
        if self.limit != other.limit && !other.limit.is_none() {
            self.limit = other.limit;
        }
        self.name = other.name;
    }
}

impl syn::parse::Parse for FieldOptions {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let options = Punctuated::<FieldOption, Token![,]>::parse_terminated(input)?;
        let mut result = FieldOptions::default();
        for option in options {
            match option {
                FieldOption::Limit(limit) => result.limit = limit,
                FieldOption::Name(name) => result.name = name,
            }
        }
        Ok(result)
    }
}
