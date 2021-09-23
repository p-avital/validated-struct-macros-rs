use super::*;
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
        let attributes = input.call(Attribute::parse_outer)?;
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
        let content;
        let Attrs {
            local: attrs,
            recursive: recursive_attrs,
        } = input.parse()?;
        let ident = input.parse()?;
        syn::braced!(content in input);
        Ok(StructSpec {
            attrs,
            recursive_attrs,
            ident,
            fields: content.parse_terminated(FieldSpec::parse)?,
        })
    }
}
