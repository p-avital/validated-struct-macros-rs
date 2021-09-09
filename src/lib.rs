use std::mem::MaybeUninit;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{Attribute, Expr, Ident, Token, Visibility, parenthesized, parse::{Parse, ParseStream}, punctuated::Punctuated};

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
        const SEPARATOR: char = if cfg!(feature = "dot_separator") {'.'} else {'/'};
        unzip_n::unzip_n!(9);
        let ident = &self.ident;
        let sattrs = &self.attrs;
        let (
            fields,
            args,
            associations,
            accessors,
            constructor_validations,
            constructor_rec_validations,
            serde_match,
            get_match,
            get_keys
        ) = self
            .fields
            .iter()
            .map(|f| {
                let attrs = &f.attributes;
                let doc_attrs: Vec<_> = attrs.iter().filter(|&attr| attr.path.is_ident("doc")).cloned().collect();
                let id = &f.ident;
                let field = id;//quote::format_ident!("_{}", id);
                let ty = f.ty.ty();
                let vis = &f.vis;
                let predicate = f
                    .constraint
                    .as_ref()
                    .map(|e| quote! {#e(&value)});
                let str_id = format!("{}", id);
                let serde_set_err = format!("Predicate rejected value for {}", id);
                let set_id = quote::format_ident!("set_{}", id);
                let validate_id = quote::format_ident!("validate_{}", id);
                let validate_id_rec = quote::format_ident!("validate_{}_rec", id);
                let validate_id_rec_impl = if let FieldType::Structure(_) = f.ty {
                    match &predicate {
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
                    let validate_rec_inner = match predicate {
                        Some(_) => quote!{Self::#validate_id(value)},
                        None => quote!{true},
                    };
                    quote! {
                        #[allow(clippy::ptr_arg)]
                        fn #validate_id_rec(value: &#ty) -> bool {
                            #validate_rec_inner
                        }
                    }
                };
                (
                    quote! {#(#attrs)* #vis #field: #ty},
                    quote! {#id: #ty},
                    quote! {#field: #id},
                    match &predicate {
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
                    ,
                    if predicate.is_some() {Some(quote! {Self::#validate_id(self.#id())})} else {None},
                    quote! {Self::#validate_id_rec(self.#id())},
                    match f.ty {
                        FieldType::Concrete(_) => 
                        quote! {(#str_id, "") => self.#set_id(serde::Deserialize::deserialize(value)?).is_err().then(||#serde_set_err.into()),},
                        FieldType::Structure(_) => quote!{
                            (#str_id, "") => self.#set_id(serde::Deserialize::deserialize(value)?).is_err().then(||#serde_set_err.into()),
                            (#str_id, key) => self.#field.insert(key, value).err(),
                        },
                    },
                    match f.ty {
                        FieldType::Concrete(_) => 
                        quote! {(#str_id, "") => Ok(self.#id() as &dyn Any),},
                        FieldType::Structure(_) => quote!{
                            (#str_id, "") => Ok(self.#id() as &dyn Any),
                            (#str_id, key) => self.#field.get(key),
                        },
                    },
                    match f.ty {
                        FieldType::Concrete(_) => quote!{keys.push(#str_id.into());},
                        FieldType::Structure(_) => quote!{
                            keys.push(#str_id.into());
                            keys.extend(self.#field.keys().into_iter().map(|s|format!("{}{}{}",#str_id, #SEPARATOR, s.as_str())));
                        },
                    }
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
            .unzip_n_vec();
        let serde_access = cfg!(feature = "serde").then(|| quote! {
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
        });
        let constructor_validations = constructor_validations.into_iter().flatten().collect::<Vec<_>>();
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
