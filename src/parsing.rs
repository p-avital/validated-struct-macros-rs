use super::*;

mod kw {
    syn::custom_keyword!(recursive_accessors);
}

enum FieldAttributes {
    RecurseAccessors,
}
impl Parse for FieldAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::recursive_accessors>()?;
        Ok(Self::RecurseAccessors)
    }
}

impl Parse for FieldType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fork = input.fork();
        match fork.parse::<StructSpec>() {
            Ok(_) => Ok(FieldType::Structure(input.parse()?)),
            Err(_) => Ok(FieldType::Concrete(input.parse()?)),
        }
    }
}
impl Parse for FieldSpec {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attributes = input.call(Attribute::parse_outer)?;
        let mut is_validated_map = false;
        attributes = attributes
            .into_iter()
            .filter(|attr| {
                if attr.path.is_ident("validated") {
                    match attr.parse_args::<FieldAttributes>() {
                        Ok(args) => match args {
                            FieldAttributes::RecurseAccessors => is_validated_map = true,
                        },
                        Err(e) => panic!("{}", e),
                    };
                    false
                } else {
                    true
                }
            })
            .collect();
        let vis = input.parse()?;
        let ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        let constraint = match input.parse::<syn::token::Where>() {
            Ok(_) => {
                let content;
                parenthesized!(content in input);
                Some(content.parse()?)
            }
            Err(_) => None,
        };
        Ok(FieldSpec {
            attributes,
            is_validated_map,
            vis,
            ident,
            ty,
            constraint,
        })
    }
}

#[derive(Default)]
struct Attrs {
    local: Vec<Attribute>,
    recursive: Vec<Attribute>,
}
impl Parse for Attrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut local = input.call(Attribute::parse_outer)?;
        let split = local
            .iter()
            .position(|a| a.path.is_ident("recursive_attrs"));
        let recursive = if let Some(split) = split {
            local.split_off(split + 1)
        } else {
            Vec::new()
        };
        if split.is_some() {
            local.pop();
        }
        Ok(Attrs { local, recursive })
    }
}
impl Parse for StructSpec {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let visibility: Visibility = input.parse()?;
        let Attrs {
            local: attrs,
            recursive: recursive_attrs,
        } = input.parse()?;
        let ident = input.parse()?;
        let content;
        syn::braced!(content in input);
        Ok(StructSpec {
            visibility,
            attrs,
            recursive_attrs,
            ident,
            fields: content.parse_terminated(FieldSpec::parse)?,
        })
    }
}
