use std::fmt::Debug;

use crate::{Parse, Span, Spanned, Success};

/// Parses a delimited list, accepting leading and trailing delimiters.
#[derive(Clone)]
pub struct Delimited<P, D> {
  pub leading: Option<D>,
  pub first: Option<P>,
  pub rest: Vec<(D, P)>,
  pub trailing: Option<D>,
}

impl<P, D> Delimited<P, D> {
  pub fn new() -> Self {
    Delimited {
      leading: None,
      first: None,
      rest: vec![],
      trailing: None,
    }
  }

  /// Returns owning an iterator over pairs of `(delimiter, item)`. Note that it will never return
  /// the trailing delimiter if there is one.
  pub fn into_iter(self) -> impl Iterator<Item = (Option<D>, P)> {
    let mut first = self.first.map(|first| (self.leading, first));
    let mut rest = self.rest.into_iter();
    std::iter::from_fn(move || {
      first
        .take()
        .or_else(|| rest.next().map(|(del, item)| (Some(del), item)))
    })
    .fuse()
  }

  /// Returns an iterator over pairs of `(delimiter, item)`. Note that it will never return the
  /// trailing delimiter if there is one.
  pub fn iter<'a>(&'a self) -> impl Iterator<Item = (Option<&'a D>, &'a P)> {
    let mut first = self
      .first
      .as_ref()
      .map(|first| (self.leading.as_ref(), first));
    let mut rest = self.rest.iter();
    std::iter::from_fn(move || {
      first
        .take()
        .or_else(|| rest.next().map(|(del, item)| (Some(del), item)))
    })
    .fuse()
  }

  /// Push a value. Returns `Err(())` if a delimiter was expected.
  pub fn push_value(&mut self, val: P) -> Result<(), ()> {
    if self.first.is_none() {
      self.first = Some(val);
      return Ok(());
    }

    let delim = self.trailing.take().ok_or(())?;
    self.rest.push((delim, val));

    Ok(())
  }

  /// Push a delimiter. Returns `Err(())` if a value was expected.
  pub fn push_delim(&mut self, delim: D) -> Result<(), ()> {
    if self.first.is_none() {
      return if self.leading.is_none() {
        self.leading = Some(delim);
        Ok(())
      } else {
        Err(())
      };
    }

    if self.trailing.is_none() {
      self.trailing = Some(delim);
      Ok(())
    } else {
      Err(())
    }
  }

  pub fn leading(&self) -> &Option<D> {
    &self.leading
  }

  pub fn trailing(&self) -> &Option<D> {
    &self.trailing
  }
}

impl<P, D> Parse for Delimited<P, D>
where
  P: Parse<Output = P>,
  D: Parse<Output = D, Token = P::Token>,
{
  type Token = P::Token;
  type Output = Self;

  fn parse<I>(input: &mut I) -> Result<crate::Success<Self::Output, I>, crate::Error<I>>
  where
    I: crate::Input<Token = Self::Token>,
  {
    let mut result = Success::from(Delimited {
      leading: None,
      first: None,
      rest: vec![],
      trailing: None,
    });
    let chk = input.save();
    result.0.leading = D::parse(input)
      .map(|res| result.merge(res))
      .map_err(|err| {
        input.reset(chk);
        result.merge(Success((), Some(err)))
      })
      .ok();
    result.0.first = Option::<P>::parse(input)
      .map(|res| result.merge(res))
      .map_err(|err| match &result.1 {
        None => err,
        Some(err2) => err.merge(err2.clone()),
      })?;

    if result.0.first.is_none() {
      return Ok(result);
    }

    let mut rest = vec![];
    let mut chk = input.save();
    loop {
      let Some(delimiter) = D::parse(input)
        .map(|res| Some(result.merge(res)))
        .or_else(|err| {
          if err.committed || input.save() != chk {
            Err(match &result.1 {
              None => err,
              Some(err2) => err.merge(err2.clone()),
            })
          } else {
            result.merge(Success((), Some(err)));
            Ok(None)
          }
        })?
      else {
        break;
      };
      chk = input.save();

      let Some(item) = P::parse(input)
        .map(|res| Some(result.merge(res)))
        .or_else(|err| {
          if err.committed || input.save() != chk {
            Err(match &result.1 {
              None => err,
              Some(err2) => err.merge(err2.clone()),
            })
          } else {
            result.merge(Success((), Some(err)));
            Ok(None)
          }
        })?
      else {
        result.0.trailing = Some(delimiter);
        break;
      };
      chk = input.save();

      rest.push((delimiter, item));
    }
    input.reset(chk);

    result.0.rest = rest;

    Ok(result)
  }
}

impl<P: Debug, D: Debug> Debug for Delimited<P, D> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let mut list = f.debug_list();

    self.leading.as_ref().map(|leading| list.entry(leading));
    self.first.as_ref().map(|first| list.entry(first));
    for (del, item) in &self.rest {
      list.entry(del);
      list.entry(item);
    }
    self.trailing.as_ref().map(|trailing| list.entry(trailing));

    list.finish()
  }
}

impl<P, D> Spanned for Delimited<P, D>
where
  P: Spanned,
  D: Spanned<Span = P::Span>,
  P::Span: Default,
{
  type Span = P::Span;

  fn span(&self) -> Self::Span {
    self
      .leading
      .span()
      .enclose(&self.first.span())
      .enclose(&self.rest.span())
      .enclose(&self.trailing.span())
  }
}
