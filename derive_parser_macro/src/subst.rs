use std::collections::HashMap;

use syn::{
  Type,
  spanned::Spanned,
  visit_mut::{VisitMut, visit_type_mut, visit_type_path_mut},
};

#[derive(Default)]
pub struct SubstMap {
  map: HashMap<syn::Ident, Option<syn::Type>>,
}

impl SubstMap {
  pub fn new(
    param_generics: syn::Generics,
    subst_generics: syn::AngleBracketedGenericArguments,
  ) -> Result<Self, syn::Error> {
    let identifiers: Vec<_> = param_generics
      .type_params()
      .map(|p| p.ident.clone())
      .collect();
    let substitution: Vec<_> = subst_generics
      .args
      .iter()
      .map(|a| match a {
        syn::GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
      })
      .collect();

    if identifiers.len() != substitution.len() {
      return Err(syn::Error::new(
        subst_generics.span(),
        "Must provide all generic arguments",
      ));
    }

    let map = identifiers
      .into_iter()
      .zip(substitution.into_iter())
      .collect();

    Ok(SubstMap { map })
  }

  pub fn substitute(&mut self, mut ty: syn::Type) -> syn::Type {
    self.visit_type_mut(&mut ty);
    ty
  }

  pub fn contains(&self, ident: &syn::Ident) -> bool {
    self.map.contains_key(ident)
  }

  pub fn insert(&mut self, name: syn::Ident, val: syn::Type) {
    self.map.insert(name, Some(val));
  }
}

impl VisitMut for SubstMap {
  fn visit_type_path_mut(&mut self, i: &mut syn::TypePath) {
    if i.qself.is_none() && i.path.segments.len() == 1 {
      if let Some(replacement) = self.map.get(&i.path.segments[0].ident).unwrap_or(&None) {
        *i = match replacement {
          Type::Path(p) => p.clone(),
          other => {
            *i = syn::parse_quote!(#other);
            return;
          }
        };
        return;
      }
    }

    visit_type_path_mut(self, i);
  }
}
