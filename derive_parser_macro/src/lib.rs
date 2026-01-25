use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Attribute, Data, DataEnum, DataStruct, DeriveInput, Field, Fields, parse_macro_input};

/// Derives a basic implementation of [`Token`](`derive_parser::Token`) for Tokens with no
/// additional information or span. Returns `Self` from
/// [`Token::kind`](`derive_parser::Token::kind`) and `()` as the span.
///
/// In most cases, you'll eventually want to replace the derived implementation with a custom token
/// type and implementation in order to attach additional information like spans to your tokens.
#[proc_macro_derive(Token)]
pub fn token_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let DeriveInput {
    ident, generics, ..
  } = parse_macro_input!(input as DeriveInput);

  quote! {
    impl #generics ::derive_parser::Token for #ident #generics {
      type Kind = Self;
      fn kind(&self) -> Self::Kind {
        self.clone()
      }
      type Span = ();
      fn span(&self) -> Self::Span {
        ()
      }
    }
  }
  .into()
}

#[proc_macro_derive(Parse, attributes(token, input, select, delimited, required, eoi))]
pub fn parse_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);

  impl_parse(&ast).into()
}

fn impl_parse(
  DeriveInput {
    ident,
    data,
    generics,
    attrs,
    ..
  }: &DeriveInput,
) -> TokenStream {
  let parse_fn = match data {
    Data::Struct(DataStruct { fields, .. }) => {
      field_parse_fn(fields, &format_ident!("parse"), &ident, generics, None)
    }
    Data::Enum(data) => impl_parse_for_enum(ident, data, generics, attrs),
    Data::Union(_) => {
      return syn::Error::new_spanned(ident, "Cannot derive Syntax for union types")
        .to_compile_error()
        .into();
    }
  };

  // FIXME: When specialization lands, we can `impl<T: Token> Parse for T { type Token = Self; }` and
  // use that to derive the token type without an `#[input]` annotation
  let mut input_attrs = attrs.iter().filter(|a| a.path().is_ident("input"));
  let token_type = input_attrs
    .next()
    .map(|a| {
      let ty = match a.parse_args::<syn::Type>() {
        Ok(v) => v.to_token_stream(),
        Err(v) => v.to_compile_error(),
      };
      quote! {type Token = #ty;}
    })
    .unwrap_or(
      syn::Error::new_spanned(ident, "Must have an #[input] annotation").to_compile_error(),
    );

  let token_type = if let Some(attr) = input_attrs.next() {
    syn::Error::new_spanned(attr, "Only one #[input] annotation is allowed").to_compile_error()
  } else {
    token_type
  };

  quote! {
    // impl #generics ::derive_parser::Combinator<#ident> for #ident #generics {
    //   fn apply<I, F>(input: &mut I, mut parser: F) -> ::core::result::Result<Self, ::derive_parser::Error<I>>
    //   where
    //     I: Input,
    //     F: FnMut(&mut I) -> ::core::result::Result<Self, ::derive_parser::Error<I>> {
    //     parser(input)
    //   }
    // }
    #[allow(late_bound_lifetime_arguments)]
    #[automatically_derived]
    impl #generics ::derive_parser::Parse for #ident #generics {
      type Output = #ident #generics;
      #token_type
      #parse_fn
    }
  }
  .into()
}

fn field_parse_fn(
  fields: &syn::Fields,
  ident: &syn::Ident,
  struct_ident: &syn::Ident,
  generics: &syn::Generics,
  variant_ident: Option<&syn::Ident>,
) -> proc_macro2::TokenStream {
  let (names, steps): (Vec<_>, Vec<_>) = fields
    .iter()
    .enumerate()
    .map(|(j, f @ Field { ident, ty, .. })| {
      let field_parser = field_parse_expr(&f);

      // FIXME: remove debug
      let variant_ident = variant_ident.iter();

      let field_ident = format_ident!(
        "__field_{}",
        ident
          .clone()
          .map(|i| i.to_string())
          .unwrap_or(j.to_string())
      );
      (
        (field_ident.clone(), ident.clone()),
        quote! {
          println!("Parsing field {}", stringify!(#field_ident));
          let #field_ident : #ty = match #field_parser {
            Ok(res) => __res.merge(res),
            Err(e) => {
              println!(
                "Aborting {} on field {}",
                stringify!(#struct_ident #(:: #variant_ident)*),
                stringify!(#field_ident)
              );
              return match __res.1 {
                Some(e2) => Err(e.merge(e2)),
                None => Err(e)
              }
            }
          };
        },
      )
    })
    .unzip();

  let field_assignments = match fields {
    Fields::Unnamed(_) => {
      let var_names = names.iter().map(|(v, _)| v);
      quote! { ( #(#var_names),* ) }
    }
    Fields::Named(_) => {
      let (vars, fields): (Vec<_>, Vec<_>) = names.iter().cloned().unzip();
      quote! { {#(#fields: #vars),*} }
    }
    Fields::Unit => quote! { () },
  };

  let lifetimes = variant_ident
    .is_some()
    .then(|| generics.lifetimes())
    .into_iter()
    .flatten();
  let variant_ident1 = variant_ident.iter();
  let variant_ident = variant_ident.iter();

  quote! {
    fn #ident<#(#lifetimes,)* I>(input: &mut I) -> Result<
      ::derive_parser::Success<#struct_ident #generics, I>,
      ::derive_parser::Error<I>
    >
    where
      I: ::derive_parser::Input<
        Token = <#struct_ident #generics as ::derive_parser::Parse>::Token
      >
    {
      let mut __res = ::derive_parser::Success((), None);
      println!(
        "Trying {} from {:?}",
        stringify!(#struct_ident #(:: #variant_ident1)*),
        input.save()
      );
      #(#steps)*
      // println!("Aggregate error in {}: {:?}", stringify!(#struct_ident), &__res.1);
      Ok(__res.map(|_| #struct_ident #(:: #variant_ident)* #field_assignments))
    }
  }
}

fn impl_parse_for_enum(
  ident: &syn::Ident,
  DataEnum { variants, .. }: &DataEnum,
  generics: &syn::Generics,
  _attrs: &Vec<Attribute>,
) -> proc_macro2::TokenStream {
  let (parse_fns, parsers): (Vec<_>, Vec<_>) = variants
    .iter()
    .map(|v| {
      let fn_ident = format_ident!("__parse_{}", v.ident);
      (
        field_parse_fn(&v.fields, &fn_ident, ident, generics, Some(&v.ident)),
        fn_ident,
      )
    })
    .unzip();

  let mut parsers = parsers.iter();
  // let first = parsers.next();

  let others = parsers.map(|p| {
    let lifetimes = generics.lifetimes();
    quote! { #p::<#(#lifetimes),*> }
  });

  let lifetimes = generics.lifetimes();

  quote! {
      fn parse<I>(input: &mut I) -> Result<
        ::derive_parser::Success<Self, I>,
        ::derive_parser::Error<I>
      >
      where
        I: ::derive_parser::Input<Token = Self::Token>
      {
        // FIXME: See rust-lang/rust#42868. The late bounds in the inner functions are not able to
        // be bypassed by elision, so we can't get rid of them until `for<'a> fn foo() { ... }`
        // syntax becomes stable, which will probably coincide with late bounds becoming a hard
        // error. At that point, the fix will be easy (just move the lifetime generics into a
        // `for<...>` prefix).
        // NOTE: A corresponding #[allow] exists on the whole impl (because clippy is weird); it
        // will need to be removed as well.
        #(
          #[inline(always)]
          #[allow(non_snake_case,late_bound_lifetime_arguments)]
          #parse_fns
        )*

        // TODO: Pray that this is correct
        // let checkpoint = input.save();
        // let res = #first::<#(#lifetimes),*>(input);
        // #(
        //   let Err(err) = res else { return res; };
        //   input.reset(checkpoint);
        //   let res = #others(input)
        //     .map_err(|e2| err.merge(e2))
        //     .map(|mut res2| { res2.merge(res); res2 });
        //   // if input.save() != checkpoint { return Err(err) };
        // )*
        //
        let start = input.save();
        let mut end = None;
        // let res = #first::<#(#lifetimes),*>(input);
        let mut err = None;
        let mut res = None;
        #(
          let branch = #others(input);
          match branch {
            Err(e2) => 'e: {
              println!("Branch {} fails at {:?}", stringify!(#others), input.save());
              let Some(e1) = err else { err = Some(e2); break 'e; };
              err = Some(e1.merge(e2));
            }
            Ok(r2) => 'o: {
              let Some(mut r1) = res else {
                res = Some(r2);
                end = Some(input.save());
                break 'o;
              };
              let _ = r1.merge(r2);
              // Pacify the borrow checker
              res = Some(r1);
              println!("BRANCH {} HAS SUCCEEDED UP TO {end:?}", stringify!(#others));
              // res = Some(::derive_parser::Success(v, r1.1));
            }
          };
          input.reset(start);
          // println!("Branch error: {:?}", err);
        )*
        println!("Aggregate Error in {}: {:?}", stringify!(#ident), err);

        // res.ok_or_else(|| err.unwrap()).map(|v| { input.reset(end.unwrap()); v })
        match res {
          Some(mut r) => {
            input.reset(end.unwrap());
            r.merge(::derive_parser::Success((), err));
            Ok(r)
          },
          None => Err(err.unwrap())
        }
        // match res {
        //   Ok(v) => Ok(v),
        //   Err(mut e) => {
        //     // e.committed = true;
        //     Err(e)
        //   }
        // }
      }
  }
}

fn field_parse_expr(field @ Field { ty, .. }: &Field) -> TokenStream {
  let parser = attribute_parser(field)
    .map(|base_parser| {
      quote! {
        <#ty as ::derive_parser::Combinator<_>>::apply(input, |input| #base_parser)
      }
    })
    .unwrap_or_else(|| {
      quote! {
        <#ty>::parse(input)
      }
    });

  // TODO: #[delimited], #[required]
  parser
}

fn attribute_parser(Field { attrs, .. }: &Field) -> Option<proc_macro2::TokenStream> {
  let mut parsers = attrs
    .iter()
    .filter_map(|a| {
      if a.path().is_ident("token") {
        Some(token_parser(a))
      } else if a.path().is_ident("select") {
        Some(select_parser(a))
      } else if a.path().is_ident("eoi") {
        Some(eoi_parser(a))
      } else {
        None
      }
    })
    .peekable();

  let first = parsers.next()?;

  let checkpoint = parsers.peek().map(|_| {
    quote! {
      let checkpoint = input.save();
    }
  });

  // {
  //   let checkpoint = input.save();
  //   parser_1()
  //     .or_else(|e1| { input.reset(checkpoint); parser_2().map_err(|e2| e1.merge(e2)) })
  //     .or_else(|e1| { input.reset(checkpoint); parser_3().map_err(|e2| e1.merge(e2)) })
  //     // ...
  // }
  Some(quote! {{
    #checkpoint
    #first
    #(.or_else(|e1| { input.reset(checkpoint); #parsers.map_err(|e2| e1.merge(e2)) }))*
  }})
}

fn eoi_parser(_attr: &Attribute) -> proc_macro2::TokenStream {
  quote! {
    let position = input.save();
    match input.next() {
      Some(tok) => {println!("expected EOI at {:?}", tok);Err(::derive_parser::Error {
        position,
        expected: ::std::collections::BTreeSet::from(["end of input".to_string()]),
        found: Some(tok),
        committed: false,
      })},
      None => Ok(::derive_parser::Success(Default::default(), None))
    }
  }
}

fn token_parser(attr: &Attribute) -> proc_macro2::TokenStream {
  let token_expr = attr
    .parse_args::<syn::Expr>()
    .map(|expr| expr.to_token_stream())
    .unwrap_or_else(|err| err.to_compile_error());

  quote! {{
    let checkpoint = input.save();
    let tok = input.next();
    if tok.as_ref().map(::derive_parser::Token::kind) == Some(#token_expr) {
      Ok(::derive_parser::Success::from(tok.unwrap()))
    } else {
      input.reset(checkpoint);
      Err(::derive_parser::Error {
        position: input.save(),
        expected: ::std::collections::BTreeSet::from([format!("{}", #token_expr)]),
        found: tok,
        committed: false,
      })
    }
  }}
}

fn select_parser(_attr: &Attribute) -> proc_macro2::TokenStream {
  todo!()
}
