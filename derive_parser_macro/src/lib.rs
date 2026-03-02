use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use subst::SubstMap;
use syn::{
  Attribute, Data, DataEnum, DataStruct, DeriveInput, Field, Fields, Variant, WherePredicate,
  parse::discouraged::Speculative, parse_macro_input, parse_quote, punctuated::Punctuated,
};

mod pratt;
mod subst;

#[proc_macro_derive(Precedence, attributes(pratt))]
pub fn pratt_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  pratt::derive(parse_macro_input!(input as DeriveInput)).into()
}

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
    }
    impl #generics ::derive_parser::Spanned for #ident #generics {
      type Span = ();
      fn span(&self) -> Self::Span {
        ()
      }
    }
  }
  .into()
}

#[proc_macro_derive(
  Parse,
  attributes(token, input, select, delimited, required, eoi, label)
)]
pub fn parse_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let ast = parse_macro_input!(input as DeriveInput);

  impl_parse(&ast).into()
}

struct InputArgs {
  ty: syn::Type,
  for_ty: Option<syn::Type>,
  where_clause: Option<ExtWhere>,
}

impl syn::parse::Parse for InputArgs {
  fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
    let ty: syn::Type = input.parse()?;
    let for_ty: Option<syn::Type> = input.parse().map(|f: ForClause| f.ty).ok();
    let where_clause: Option<ExtWhere> = input.parse().ok();
    Ok(InputArgs {
      ty,
      for_ty,
      where_clause,
    })
  }
}

struct ForClause {
  _kw: syn::Token![for],
  ty: syn::Type,
}

impl syn::parse::Parse for ForClause {
  fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
    let kw = input.parse()?;
    let ty = input.parse()?;
    Ok(ForClause { _kw: kw, ty })
  }
}

struct ExtWhere {
  _kw: syn::Token![where],
  predicates: Punctuated<WherePred, syn::Token![,]>,
}

impl syn::parse::Parse for ExtWhere {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let kw = input.parse()?;
    let mut predicates = Punctuated::new();
    loop {
      if input.is_empty()
        || input.peek(syn::token::Brace)
        || input.peek(syn::Token![,])
        || input.peek(syn::Token![;])
        || input.peek(syn::Token![:]) && !input.peek(syn::Token![::])
        || input.peek(syn::Token![=])
      {
        break;
      }
      predicates.push_value(input.parse()?);
      if !input.peek(syn::Token![,]) {
        break;
      }
      predicates.push_punct(input.parse()?);
    }
    Ok(ExtWhere {
      _kw: kw,
      predicates,
    })
  }
}

enum WherePred {
  Trait(syn::WherePredicate),
  Eq(EqPred),
}

impl syn::parse::Parse for WherePred {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let fork = input.fork();
    if let Ok(pred) = fork.parse() {
      input.advance_to(&fork);
      Ok(WherePred::Trait(pred))
    } else {
      let eq = input.parse()?;
      Ok(WherePred::Eq(eq))
    }
  }
}

struct EqPred {
  name: syn::Ident,
  _eq: syn::Token![=],
  ty: syn::Type,
}

impl syn::parse::Parse for EqPred {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let name = input.parse()?;
    let eq = input.parse()?;
    let ty = input.parse()?;
    Ok(EqPred { name, _eq: eq, ty })
  }
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
  let (mut subst, new_generics, where_preds, for_ty, token_type) =
    input_attr(&ident, &generics, &attrs[..]);

  let parse_fn = match data {
    Data::Struct(DataStruct { fields, .. }) => field_parse_fn(
      fields,
      &format_ident!("parse"),
      &ident,
      &subst.substitute(parse_quote!(#ident #generics)),
      generics,
      None,
      attrs,
      &mut subst,
    ),
    Data::Enum(data) => impl_parse_for_enum(ident, data, generics, attrs, &mut subst),
    Data::Union(_) => {
      return syn::Error::new_spanned(ident, "Cannot derive Syntax for union types")
        .to_compile_error()
        .into();
    }
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
    impl #new_generics ::derive_parser::Parse for #for_ty
    where
      #token_type: ::derive_parser::Token,
      #(#where_preds),*
    {
      type Output = #for_ty;
      type Token = #token_type;
      #parse_fn
    }
  }
  .into()
}

fn input_attr(
  ident: &syn::Ident,
  generics: &syn::Generics,
  attrs: &[Attribute],
) -> (
  SubstMap,
  syn::Generics,
  Vec<WherePredicate>,
  syn::Type,
  proc_macro2::TokenStream,
) {
  // FIXME: When specialization lands, we can `impl<T: Token> Parse for T { type Token = Self; }` and
  // use that to derive the token type without an `#[input]` annotation
  let mut input_attrs = attrs.iter().filter(|a| a.path().is_ident("input"));
  let (token_type, for_ty, where_clause) = input_attrs
    .next()
    .map(|a| match a.parse_args::<InputArgs>() {
      Ok(v) => (Ok(v.ty), v.for_ty, v.where_clause),
      Err(v) => (Err(v), None, None),
    })
    .unwrap_or((
      Err(syn::Error::new_spanned(
        ident,
        "Must have an #[input] annotation",
      )),
      None,
      None,
    ));

  let token_type = if let Some(attr) = input_attrs.next() {
    Err(syn::Error::new_spanned(
      attr,
      "Only one #[input] annotation is allowed",
    ))
  } else {
    token_type
  };

  let for_generics = for_ty.as_ref().map(|ty| match ty {
    syn::Type::Path(p) => {
      // TODO: Error if more segments or qpath
      if let syn::PathArguments::AngleBracketed(args) = &p.path.segments[0].arguments {
        args.clone()
      } else {
        // TODO: Error
        todo!()
      }
    }
    // TODO: Emit Error
    _ => todo!(),
  });

  let mut subst = for_generics
    .map(|args| SubstMap::new(generics.clone(), args).unwrap_or_default())
    .unwrap_or_default();
  let where_preds = where_clause
    .into_iter()
    .flat_map(|w| w.predicates)
    .filter_map(|pred| match pred {
      WherePred::Trait(t) => Some(t),
      WherePred::Eq(eq) => {
        subst.insert(eq.name, eq.ty);
        None
      }
    })
    .collect::<Vec<_>>();

  let token_type = match token_type {
    Ok(ty) => subst.substitute(ty).to_token_stream(),
    Err(err) => err.to_compile_error(),
  };

  let for_ty = subst.substitute(for_ty.unwrap_or_else(|| parse_quote!(#ident #generics)));

  let mut new_generics = generics.clone();
  new_generics.params = generics
    .params
    .pairs()
    .filter_map(|p| match p.value() {
      syn::GenericParam::Type(t) if subst.contains(&t.ident) => None,
      _ => Some(p.cloned()),
    })
    .collect();

  (subst, new_generics, where_preds, for_ty, token_type)
}

fn field_parse_fn(
  fields: &syn::Fields,
  ident: &syn::Ident,
  struct_ident: &syn::Ident,
  struct_ty: &syn::Type,
  generics: &syn::Generics,
  variant_ident: Option<&syn::Ident>,
  attrs: &Vec<Attribute>,
  subst: &mut SubstMap,
) -> proc_macro2::TokenStream {
  let label = attrs.iter().find_map(|a| {
    a.path().is_ident("label").then(|| {
      let value = a
        .parse_args::<syn::LitStr>()
        .map(|expr| expr.to_token_stream())
        .unwrap_or_else(|err| err.to_compile_error());

      quote! {
        .label(#value.to_string(), __label_start)
      }
    })
  });

  let (names, steps): (Vec<_>, Vec<_>) = fields
    .iter()
    .enumerate()
    .map(|(j, f @ Field { ident, ty, .. })| {
      let field_parser = field_parse_expr(&f, subst);

      let field_ident = format_ident!(
        "__field_{}",
        ident
          .clone()
          .map(|i| i.to_string())
          .unwrap_or(j.to_string())
      );

      let ty = subst.substitute(ty.clone());

      (
        (field_ident.clone(), ident.clone()),
        quote! {
          // println!("Parsing field {}", stringify!(#field_ident));
          let #field_ident : #ty = match #field_parser {
            Ok(res) => __res.merge(res),
            Err(e) => {
              // println!(
              //   "Aborting {} on field {}",
              //   stringify!(#struct_ident #(:: #variant_ident)*),
              //   stringify!(#field_ident)
              // );
              return match __res.1 {
                Some(e2) => Err(e.merge(e2)#label),
                None => Err(e #label)
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
  let variant_ident = variant_ident.iter();

  quote! {
    fn #ident<#(#lifetimes,)* I>(input: &mut I) -> ::core::result::Result<
      ::derive_parser::Success<#struct_ty, I>,
      ::derive_parser::Error<I>
    >
    where
      I: ::derive_parser::Input<
        Token = <#struct_ty as ::derive_parser::Parse>::Token
      >
    {
      let mut __res = ::derive_parser::Success((), None);
      let __label_start = input.save();
      // println!(
      //   "Trying {} from {:?}",
      //   stringify!(#struct_ident #(:: #variant_ident1)*),
      //   input.save()
      // );
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
  subst: &mut SubstMap,
) -> proc_macro2::TokenStream {
  let (parse_fns, parsers): (Vec<_>, Vec<_>) = variants
    .iter()
    .map(|v| {
      let fn_ident = format_ident!("__parse_{}", v.ident);
      (
        field_parse_fn(
          &v.fields,
          &fn_ident,
          ident,
          &subst.substitute(parse_quote!(#ident #generics)),
          generics,
          Some(&v.ident),
          &v.attrs,
          subst,
        ),
        fn_ident,
      )
    })
    .unzip();

  let parsers = parsers.iter();

  let others = parsers.map(|p| {
    let lifetimes = generics.lifetimes();
    quote! { #p::<#(#lifetimes),*> }
  });

  quote! {
      fn parse<I>(input: &mut I) -> ::core::result::Result<
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
              // println!("Branch {} fails at {:?}", stringify!(#others), input.save());
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
              // println!("BRANCH {} HAS SUCCEEDED UP TO {end:?}", stringify!(#others));
              // res = Some(::derive_parser::Success(v, r1.1));
            }
          };
          input.reset(start);
          // println!("Branch error: {:?}", err);
        )*
        // println!("Aggregate Error in {}: {:?}", stringify!(#ident), err);

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

fn field_parse_expr(field @ Field { ty, .. }: &Field, subst: &mut SubstMap) -> TokenStream {
  let ty = subst.substitute(ty.clone());
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
      Some(tok) => {Err(::derive_parser::Error {
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
    // .parse_args_with(syn::Pat::parse_multi)
    .map(|expr| expr.to_token_stream())
    .unwrap_or_else(|err| err.to_compile_error());

  quote! {{
    let checkpoint = input.save();
    let tok = input.next();
    if tok.as_ref().map(::derive_parser::Token::kind) == Some(#token_expr) {
    // if tok.as_ref().map(::derive_parser::Token::kind).map(|k| matches!(k, #token_expr)).unwrap_or(false) {
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

// TODO: Roll this into `derive(Parse)`?
#[proc_macro_derive(Spanned, attributes(input, eoi))]
pub fn spanned_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let DeriveInput {
    ident,
    generics,
    attrs,
    data,
    ..
  } = parse_macro_input!(input as DeriveInput);

  let (_subst, new_generics, where_preds, for_ty, token_type) =
    input_attr(&ident, &generics, &attrs[..]);

  let mut requires_default = false;
  let span_ty: syn::Type = parse_quote!(<#token_type as ::derive_parser::Spanned>::Span);

  let body = match data {
    Data::Union(_) => unimplemented!("Unions are not supported"),
    Data::Enum(enum_data) => {
      let arms = enum_data
        .variants
        .iter()
        .map(|Variant { ident, fields, .. }| {
          let body = span_for_fields(&mut requires_default, span_ty.clone(), fields);
          let fields = expand_fields(fields);
          quote! {
            Self::#ident #fields => {
              #body
              span
            }
          }
        });

      quote! {
        let span = match &self {
          #(#arms),*
        };
      }
    }
    Data::Struct(DataStruct { fields, .. }) => {
      let body = span_for_fields(&mut requires_default, span_ty.clone(), &fields);
      let fields = expand_fields(&fields);
      quote! {
        let Self #fields = self;
        #body
      }
    }
  };

  fn expand_fields(fields: &Fields) -> proc_macro2::TokenStream {
    let names = fields.iter().map(|f| f.ident.clone());
    match fields {
      Fields::Unnamed(_) => {
        let names = (0..fields.len()).map(|i| format_ident!("i{i}"));
        quote! { ( #(#names),* ) }
      }
      Fields::Named(_) => {
        quote! { {#(#names),*} }
      }
      Fields::Unit => quote! { () },
    }
  }

  fn span_for_fields(
    requires_default: &mut bool,
    span_ty: syn::Type,
    fields: &Fields,
  ) -> proc_macro2::TokenStream {
    if fields.len() == 0 {
      *requires_default = true;
      return quote! {
        let span = #span_ty::default();
      };
    }

    let mut fields = fields
      .iter()
      .enumerate()
      // Filter out #[eoi] fields (their type is (), which doesn't implement Spanned)
      .filter(|(_, Field { attrs, .. })| !attrs.iter().any(|attr| attr.path().is_ident("eoi")));
    let first = fields.next().map(|(i, Field { ident, .. })| {
      let ident = ident.clone().unwrap_or_else(|| format_ident!("i{i}"));
      quote! {
        let span = ::derive_parser::Spanned::span(#ident);
      }
    });

    let rest = fields.map(|(i, Field { ident, .. })| {
      let ident = ident.clone().unwrap_or_else(|| format_ident!("i{i}"));
      quote! {
        let span = ::derive_parser::Span::enclose(&#ident.span(), &span);
      }
    });

    quote! {
      #first
      #(#rest)*
    }
    .into()
  }

  let default_constraint = requires_default.then(|| quote! { #span_ty: Default, });

  quote! {
    impl #new_generics ::derive_parser::Spanned for #for_ty
    where
      #token_type: ::derive_parser::Spanned,
      #default_constraint
      #(#where_preds),*
    {
      type Span = <#token_type as ::derive_parser::Spanned>::Span;
      fn span(&self) -> Self::Span {
        #body
        span
      }
    }
  }
  .into()
}
