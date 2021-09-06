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
            ident,
            ty,
            constraint,
        })
    }
}
trait AtomicParse: Sized {
    fn atomic_parse(input: ParseStream) -> syn::Result<Self>;
}
impl AtomicParse for RecursionMarker {
    fn atomic_parse(input: ParseStream) -> syn::Result<Self> {
        RecursionMarker::parse(&input.fork())?;
        RecursionMarker::parse(input)
    }
}
impl AtomicParse for Attribute {
    fn atomic_parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        let pound_token = input.parse()?;
        let style = syn::AttrStyle::Outer;
        let bracket_token = syn::bracketed!(content in input);
        let path = content.call(syn::Path::parse_mod_style)?;
        let tokens = content.parse()?;
        Ok(Attribute {
            pound_token,
            style,
            bracket_token,
            path,
            tokens,
        })
    }
}
struct RecursionMarker;
impl Parse for RecursionMarker {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<Token![#]>()?;
        let content;
        syn::bracketed!(content in input);
        content.parse::<kw::recursive_attrs>()?;
        Ok(Self)
    }
}
#[derive(Default)]
struct Attrs {
    local: Vec<Attribute>,
    recursive: Vec<Attribute>,
}
impl Parse for Attrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut s = Self::default();
        let mut recursive = false;
        for _ in 0..10000 {
            if RecursionMarker::atomic_parse(input).is_ok() {
                recursive = true;
                continue;
            }
            match Attribute::atomic_parse(input) {
                Ok(attr) => {
                    if recursive {
                        s.recursive.push(attr)
                    } else {
                        s.local.push(attr)
                    }
                }
                Err(_) => return Ok(s),
            }
        }
        Ok(s)
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
