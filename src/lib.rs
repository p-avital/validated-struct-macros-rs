#![allow(clippy::too_many_arguments)]
use std::mem::MaybeUninit;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Attribute, Expr, Ident, Token, Type, Visibility,
};

#[derive(Clone)]
enum FieldType {
    Concrete(syn::Type),
    Structure(StructSpec),
}
impl FieldType {
    fn ty(&self) -> syn::Type {
        match self {
            FieldType::Concrete(t) => t.clone(),
            FieldType::Structure(s) => syn::parse2(s.ident.to_token_stream()).unwrap(),
        }
    }
}

#[derive(Clone)]
struct FieldSpec {
    attributes: Vec<Attribute>,
    vis: Visibility,
    ident: Ident,
    ty: FieldType,
    constraint: Option<Expr>,
}
mod kw {
    syn::custom_keyword!(recursive_attrs);
}
#[derive(Clone)]
struct StructSpec {
    attrs: Vec<Attribute>,
    recursive_attrs: Vec<Attribute>,
    ident: Ident,
    fields: Punctuated<FieldSpec, Token![,]>,
}
const SEPARATOR: char = if cfg!(feature = "dot_separator") {
    '.'
} else {
    '/'
};
impl StructSpec {
    fn flatten_go(
        mut self,
        list: &mut Vec<Self>,
        recursive_attrs: &mut Vec<Vec<Attribute>>,
    ) -> Self {
        let mut self_rec_attrs = Vec::new();
        std::mem::swap(&mut self_rec_attrs, &mut self.recursive_attrs);
        recursive_attrs.push(self_rec_attrs);
        self.attrs.extend(recursive_attrs.iter().flatten().cloned());
        for field in self.fields.iter_mut() {
            if let FieldType::Structure(s) = &mut field.ty {
                let mut tmp = MaybeUninit::uninit();
                std::mem::swap(s, unsafe { &mut *tmp.as_mut_ptr() });
                tmp = MaybeUninit::new(
                    unsafe { tmp.assume_init() }.flatten_go(list, recursive_attrs),
                );
                std::mem::swap(s, unsafe { &mut *tmp.as_mut_ptr() });
            }
        }
        recursive_attrs.pop();
        list.push(self.clone());
        self
    }
    fn flatten(self) -> Vec<Self> {
        let mut list = Vec::with_capacity(1);
        let mut rec_attrs = Vec::with_capacity(1);
        self.flatten_go(&mut list, &mut rec_attrs);
        list
    }
    fn structure(&self) -> impl quote::ToTokens {
        unzip_n::unzip_n!(9);
        let ident = &self.ident;
        let mut notifying = false;
        let sattrs: Vec<_> = self
            .attrs
            .iter()
            .filter_map(|a| {
                if a.path.is_ident("notifying") {
                    notifying = true;
                    None
                } else {
                    Some(a.clone())
                }
            })
            .collect();
        let (
            fields,
            args,
            associations,
            accessors,
            constructor_validations,
            constructor_rec_validations,
            serde_match,
            get_match,
            get_keys,
        ) = self
            .fields
            .iter()
            .map(|spec| {
                let id = &spec.ident;
                let field_name = id;
                let ty = spec.ty.ty();
                let predicate = spec.constraint.as_ref().map(|e| quote! {#e(&value)});
                let str_id = format!("{}", id);
                let set_id = quote::format_ident!("set_{}", id);
                let validate_id = quote::format_ident!("validate_{}", id);
                let validate_id_rec = quote::format_ident!("validate_{}_rec", id);
                (
                    field(spec, field_name),
                    quote! {#id: #ty},
                    quote! {#field_name: #id},
                    accessors(
                        spec,
                        id,
                        field_name,
                        &ty,
                        &set_id,
                        &validate_id,
                        &validate_id_rec,
                        &predicate,
                    ),
                    if predicate.is_some() {
                        Some(quote! {Self::#validate_id(self.#id())})
                    } else {
                        None
                    },
                    quote! {Self::#validate_id_rec(self.#id())},
                    serde_match(id, spec, &str_id, set_id, field_name),
                    get_match(spec, id, &str_id, field_name),
                    keys_match(spec, field_name, &str_id),
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
            .unzip_n_vec();
        let serde_access = serde_access(ident, &serde_match, &get_match, &get_keys);
        let constructor_validations = constructor_validations
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        main_implementation(
            &sattrs,
            ident,
            &fields,
            &constructor_validations,
            &constructor_rec_validations,
            &args,
            &associations,
            &accessors,
            &serde_access,
        )
    }
}

fn main_implementation(
    sattrs: &[Attribute],
    ident: &Ident,
    fields: &[proc_macro2::TokenStream],
    constructor_validations: &[proc_macro2::TokenStream],
    constructor_rec_validations: &[proc_macro2::TokenStream],
    args: &[proc_macro2::TokenStream],
    associations: &[proc_macro2::TokenStream],
    accessors: &[proc_macro2::TokenStream],
    serde_access: &Option<proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    quote! {
        #(#sattrs)*
        pub struct #ident {
            #(#fields),*
        }
        impl #ident {
            pub fn validate(&self) -> bool {
                true #(&& #constructor_validations)*
            }
            fn validate_rec(&self) -> bool {
                true #(&& #constructor_rec_validations)*
            }
            #[allow(clippy::too_many_arguments)]
            pub fn new(#(#args),*) -> Result<Self, Self> {
                let constructed = #ident {
                    #(#associations),*
                };
                if constructed.validate() {Ok(constructed)} else {Err(constructed)}
            }
            #(#accessors)*
        }
        #serde_access
    }
}

fn field(spec: &FieldSpec, field: &Ident) -> proc_macro2::TokenStream {
    let ty = spec.ty.ty();
    let attrs = &spec.attributes;
    let vis = &spec.vis;
    quote! {#(#attrs)* #vis #field: #ty}
}

fn serde_access(
    ident: &Ident,
    serde_match: &[proc_macro2::TokenStream],
    get_match: &[proc_macro2::TokenStream],
    get_keys: &[proc_macro2::TokenStream],
) -> Option<proc_macro2::TokenStream> {
    cfg!(feature = "serde").then(|| quote! {
        impl #ident {
            pub fn from_deserializer<'d, D: serde::Deserializer<'d>>(
                d: D,
            ) -> Result<Self, Result<Self, D::Error>>
            where
                Self: serde::Deserialize<'d>,
            {
                match <Self as serde::Deserialize>::deserialize(d) {
                    Ok(value) => {
                        if value.validate_rec() {
                            Ok(value)
                        } else {
                            Err(Ok(value))
                        }
                    }
                    Err(e) => Err(Err(e)),
                }
            }
        }
        impl validated_struct::ValidatedMap for #ident {
            fn insert<'d, D: serde::Deserializer<'d>>(&mut self, key: &str, value: D) -> Result<(), validated_struct::InsertionError>
            where
                validated_struct::InsertionError: From<D::Error> {
                if let Some(e) = match key.split_once(#SEPARATOR).unwrap_or((key, "")) {
                    #(#serde_match)*
                    _ => Some("unknown key".into())
                } {return Err(e)};
                Ok(())
            }
            fn get<'a>(&'a self, key: &str) -> Result<&dyn std::any::Any, validated_struct::GetError>{
                use std::any::Any;
                match key.split_once(#SEPARATOR).unwrap_or((key, "")) {
                    #(#get_match)*
                    ("", key) if !key.is_empty() => self.get(key),
                    _ => Err(validated_struct::GetError::NoMatchingKey),
                }
            }
            type Keys = std::vec::Vec<String>;
            fn keys(&self) -> Self::Keys {
                let mut keys = std::vec::Vec::new();
                #(#get_keys)*
                keys
            }
        }
    })
}

fn accessors(
    spec: &FieldSpec,
    id: &Ident,
    field: &Ident,
    ty: &Type,
    set_id: &Ident,
    validate_id: &Ident,
    validate_id_rec: &Ident,
    predicate: &Option<proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    let doc_attrs: Vec<_> = spec
        .attributes
        .iter()
        .filter(|&attr| attr.path.is_ident("doc"))
        .cloned()
        .collect();
    let validate_id_rec_impl =
        implement_validation(spec, ty, predicate, validate_id_rec, validate_id);
    match predicate {
        Some(predicate) => quote! {
            #[inline(always)]
            #(#doc_attrs)*
            pub fn #id(&self) -> & #ty {
                &self.#field
            }
            #[allow(clippy::ptr_arg)]
            pub fn #validate_id(value: &#ty) -> bool {
                #predicate
            }
            #validate_id_rec_impl
            #(#doc_attrs)*
            pub fn #set_id(&mut self, mut value: #ty) -> Result<#ty, #ty> {
                if Self::#validate_id(&value) {
                    std::mem::swap(&mut self.#field, &mut value);
                    Ok(value)
                } else {
                    Err(value)
                }
            }
        },
        None => quote! {
            #[inline(always)]
            #(#doc_attrs)*
            pub fn #id(&self) -> & #ty {
                &self.#field
            }
            #validate_id_rec_impl
            #(#doc_attrs)*
            pub fn #set_id(&mut self, mut value: #ty) -> Result<#ty, #ty> {
                std::mem::swap(&mut self.#field, &mut value);
                Ok(value)
            }
        },
    }
}

fn keys_match(spec: &FieldSpec, field: &Ident, str_id: &str) -> proc_macro2::TokenStream {
    match spec.ty {
        FieldType::Concrete(_) => quote! {keys.push(#str_id.into());},
        FieldType::Structure(_) => quote! {
            keys.push(#str_id.into());
            keys.extend(self.#field.keys().into_iter().map(|s|format!("{}{}{}",#str_id, #SEPARATOR, s.as_str())));
        },
    }
}

fn get_match(
    spec: &FieldSpec,
    id: &Ident,
    str_id: &str,
    field: &Ident,
) -> proc_macro2::TokenStream {
    match spec.ty {
        FieldType::Concrete(_) => quote! {(#str_id, "") => Ok(self.#id() as &dyn Any),},
        FieldType::Structure(_) => quote! {
            (#str_id, "") => Ok(self.#id() as &dyn Any),
            (#str_id, key) => self.#field.get(key),
        },
    }
}

fn serde_match(
    id: &Ident,
    spec: &FieldSpec,
    str_id: &str,
    set_id: Ident,
    field: &Ident,
) -> proc_macro2::TokenStream {
    let serde_set_err = format!("Predicate rejected value for {}", id);
    match spec.ty {
        FieldType::Concrete(_) => {
            quote! {(#str_id, "") => self.#set_id(serde::Deserialize::deserialize(value)?).is_err().then(||#serde_set_err.into()),}
        }
        FieldType::Structure(_) => quote! {
            (#str_id, "") => self.#set_id(serde::Deserialize::deserialize(value)?).is_err().then(||#serde_set_err.into()),
            (#str_id, key) => self.#field.insert(key, value).err(),
        },
    }
}

fn implement_validation(
    f: &FieldSpec,
    ty: &Type,
    predicate: &Option<proc_macro2::TokenStream>,
    validate_id_rec: &Ident,
    validate_id: &Ident,
) -> proc_macro2::TokenStream {
    if let FieldType::Structure(_) = f.ty {
        match predicate {
            Some(predicate) => quote! {
                fn #validate_id_rec(value: &#ty) -> bool {
                    value.validate_rec() && #predicate
                }
            },
            None => quote! {
                fn #validate_id_rec(value: &#ty) -> bool {
                    value.validate_rec()
                }
            },
        }
    } else {
        let validate_rec_inner = match *predicate {
            Some(_) => quote! {Self::#validate_id(value)},
            None => quote! {true},
        };
        quote! {
            #[allow(clippy::ptr_arg)]
            fn #validate_id_rec(value: &#ty) -> bool {
                #validate_rec_inner
            }
        }
    }
}

#[proc_macro]
pub fn validator(stream: TokenStream) -> TokenStream {
    let spec: StructSpec = syn::parse(stream).unwrap();
    let structure: Vec<_> = spec.flatten().iter().map(StructSpec::structure).collect();
    (quote! {
        #(#structure)*
    })
    .into()
}
mod display;
mod parsing;
