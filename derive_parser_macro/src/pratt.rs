use quote::quote;
use syn::{
  Data, DataEnum, DeriveInput, Fields, LitInt, Token, Variant, parenthesized, parse::Parse,
  punctuated::Punctuated,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Assoc {
  Left,
  Right,
}

enum Arg {
  Assoc(Assoc),
  Precedence(LitInt),
  Prefix(LitInt),
  Postfix(LitInt),
}

impl Parse for Arg {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    if input.peek(LitInt) {
      return Ok(Arg::Precedence(input.parse()?));
    }

    let id = input.parse::<syn::Ident>()?;
    if id == "left_assoc" {
      Ok(Arg::Assoc(Assoc::Left))
    } else if id == "right_assoc" {
      Ok(Arg::Assoc(Assoc::Right))
    } else if id == "prefix" {
      let val;
      parenthesized!(val in input);
      Ok(Arg::Prefix(val.parse()?))
    } else if id == "postfix" {
      let val;
      parenthesized!(val in input);
      Ok(Arg::Postfix(val.parse()?))
    } else {
      Err(syn::Error::new(
        id.span(),
        "Expected left_assoc, right_assoc, prefix or postfix",
      ))
    }
  }
}

#[derive(Clone, Copy)]
struct PrattArgs {
  assoc: Option<Assoc>,
  prec: Option<u8>,
  pre_prec: Option<u8>,
  post_prec: Option<u8>,
}

impl Parse for PrattArgs {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let args = Punctuated::<_, Token![,]>::parse_terminated(input)?;
    // TODO: Error if an attr is specified multiple times
    let mut res = PrattArgs {
      assoc: None,
      prec: None,
      pre_prec: None,
      post_prec: None,
    };

    for arg in args.into_iter() {
      match arg {
        Arg::Precedence(p) => {
          if res.prec.is_none() {
            res.prec = Some(p.base10_parse()?);
          } else {
            return Err(syn::Error::new(
              input.span(),
              "Cannot specify infix precedence twice",
            ));
          }
        }
        Arg::Assoc(a) => {
          if res.assoc.is_none() {
            res.assoc = Some(a)
          } else {
            return Err(syn::Error::new(
              input.span(),
              "Cannot specify associativity twice",
            ));
          }
        }
        Arg::Prefix(p) => {
          if res.pre_prec.is_none() {
            res.pre_prec = Some(p.base10_parse()?);
          } else {
            return Err(syn::Error::new(
              input.span(),
              "Cannot specify prefix precedence twice",
            ));
          }
        }
        Arg::Postfix(p) => {
          if res.post_prec.is_none() {
            res.post_prec = Some(p.base10_parse()?);
          } else {
            return Err(syn::Error::new(
              input.span(),
              "Cannot specify postfix precedence twice",
            ));
          }
        }
      }
    }

    Ok(res)
  }
}

pub fn derive(
  DeriveInput {
    ident,
    generics,
    attrs,
    data,
    ..
  }: DeriveInput,
) -> proc_macro2::TokenStream {
  let outer_attr: Option<PrattArgs> = match attrs
    .iter()
    .find_map(|a| a.path().is_ident("pratt").then(|| a.parse_args()))
    .transpose()
  {
    Ok(ok) => ok,
    Err(err) => return err.into_compile_error(),
  };

  let Data::Enum(DataEnum { variants, .. }) = data else {
    return syn::Error::new(
      ident.span(),
      "`#[derive(Presedence)]` only applies to enums",
    )
    .into_compile_error();
  };

  let (infix, prepost): (Vec<_>, Vec<_>) = variants
    .into_iter()
    .map(
      |Variant {
         attrs,
         ident,
         fields,
         ..
       }| {
        let placeholders = match fields {
          Fields::Unit => quote! {},
          Fields::Unnamed(_) => quote! { (..) },
          Fields::Named(_) => quote! { {..} },
        };

        let args = match attrs
          .into_iter()
          .find_map(|a| {
            a.path()
              .is_ident("pratt")
              .then(|| a.parse_args::<PrattArgs>())
          })
          .transpose()
        {
          Ok(ok) => ok,
          Err(err) => return (err.into_compile_error(), (quote! {}, quote! {})),
        };

        let assoc = args
          .and_then(|a| a.assoc)
          .or_else(|| outer_attr.and_then(|a| a.assoc))
          .unwrap_or_else(|| Assoc::Left);

        let prec = args.and_then(|a| a.prec);

        let infix = match prec.map(|p| {
          (
            (p * 2) + (assoc == Assoc::Right) as u8,
            (p * 2) + (assoc == Assoc::Left) as u8,
          )
        }) {
          None => quote! { None },
          Some((l, r)) => quote! (Some((#l, #r))),
        };
        let prefix = match args.and_then(|a| a.pre_prec).map(|p| p * 2) {
          None => quote! { None },
          Some(p) => quote!(Some(#p)),
        };
        let postfix = match args.and_then(|a| a.post_prec.map(|p| p * 2)) {
          None => quote! { None },
          Some(p) => quote!(Some(#p)),
        };

        (
          quote! {
            Self::#ident #placeholders => #infix,
          },
          (
            quote! {
              Self::#ident #placeholders => #prefix,
            },
            quote! {
              Self::#ident #placeholders => #postfix,
            },
          ),
        )
      },
    )
    .unzip();
  let (prefix, postfix): (Vec<_>, Vec<_>) = prepost.into_iter().unzip();

  quote! {
    impl #generics ::derive_parser::Precedence for #ident #generics
    where
      Self: ::derive_parser::Parse<Output = Self>
    {
      fn prefix_precedence(val: &Self::Output) -> Option<u8> {
        match val {
          #(#prefix)*
          _ => None
        }
      }
      fn postfix_precedence(val: &Self::Output) -> Option<u8> {
        match val {
          #(#postfix)*
          _ => None
        }
      }
      fn infix_precedence(val: &Self::Output) -> Option<(u8, u8)> {
        match val {
          #(#infix)*
          _ => None
        }
      }
    }
  }
}
