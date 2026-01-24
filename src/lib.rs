use std::{
  borrow::Cow,
  collections::BTreeSet,
  fmt::{Debug, Display, Write},
  ops::Range,
};
use thiserror::Error;

mod combinator;

pub use combinator::Combinator;
pub use derive_parser_macro::{Parse, Token};

pub trait Token: Clone {
  type Kind: Display;
  fn kind(&self) -> Self::Kind;
  type Span: Span;
  fn span(&self) -> Self::Span;
}

pub trait Span {
  /// Return a new span that encloses both spans, i.e.
  ///
  /// ```
  /// assert!((3..6).enclose(&(5..8)) == 3..8);
  /// ```
  fn enclose(&self, other: &Self) -> Self;
}

impl Span for () {
  fn enclose(&self, _other: &Self) -> Self {
    ()
  }
}

impl<N: Ord + Clone> Span for Range<N> {
  fn enclose(&self, other: &Self) -> Self {
    Range {
      start: (&self.start)
        .min(&self.end)
        .min(&other.start)
        .min(&other.end)
        .clone(),
      end: (&self.start)
        .max(&self.end)
        .max(&other.start)
        .max(&other.end)
        .clone(),
    }
  }
}

pub trait Input {
  type Token: Token;
  type Checkpoint: Copy + Ord + Debug;

  /// Returns the next token and advances the input
  fn next(&mut self) -> Option<Self::Token>;
  /// Saves the current input state. This operation should be cheap.
  fn save(&self) -> Self::Checkpoint;
  /// Restores the input to a previous state
  fn reset(&mut self, checkpoint: Self::Checkpoint);
}

pub trait Parse {
  type Token;
  type Output;

  fn parse<I>(input: &mut I) -> Result<Success<Self::Output, I>, Error<I>>
  where
    I: Input<Token = Self::Token>;
}

#[derive(Clone)]
pub struct Success<O, I: Input>(pub O, pub Option<Error<I>>);

impl<O, I, T> Debug for Success<O, I>
where
  O: Debug,
  I: Input<Token = T> + Debug,
  T: Debug,
{
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("Success")
      .field(&self.0)
      .field(&self.1)
      .finish()
  }
}

impl<O, I: Input> Success<O, I> {
  pub fn merge<P>(&mut self, other: Success<P, I>) -> P {
    let Some(_) = self.1 else {
      self.1 = other.1;
      return other.0;
    };

    let Some(e2) = other.1 else {
      return other.0;
    };

    self.1 = Some(self.1.take().unwrap().merge(e2));
    return other.0;
  }

  pub fn map<P, F>(self, fun: F) -> Success<P, I>
  where
    F: Fn(O) -> P,
  {
    Success(fun(self.0), self.1)
  }
}

impl<O, I: Input> From<O> for Success<O, I> {
  fn from(value: O) -> Self {
    Success(value, None)
  }
}

// TODO: Add `Failure { Committed(Error), Uncommited(Error) }` and the `#[commit]` attribute.
#[derive(Clone, Debug, Error)]
pub struct Error<I>
where
  I: Input,
{
  /// Position reported by the input immediately after the offending token was read.
  pub position: I::Checkpoint,
  pub expected: BTreeSet<String>,
  pub found: Option<I::Token>,
  /// Whether the error is to be treated as a consuming failure regardless of whether input was
  /// actually consumed.
  pub committed: bool,
}

impl<I> Error<I>
where
  I: Input,
{
  pub fn merge(mut self, other: Error<I>) -> Self {
    if self.position == other.position {
      self.expected.extend(other.expected.into_iter());
      self
    } else {
      std::cmp::max_by_key(self, other, |e| e.position)
    }
  }
}

impl<I> Display for Error<I>
where
  I: Input,
{
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let expected = if self.expected.len() > 1 {
      let all_but_last = self.expected.iter().take(self.expected.len() - 1);

      let mut expected = all_but_last
        .map(Cow::from)
        .reduce(|mut a, s| {
          a.to_mut().reserve(s.len() + 1);
          a.to_mut().push_str(&s);
          a.to_mut().push_str(", ");
          a
        })
        .unwrap_or_default();

      write!(
        expected.to_mut(),
        " or {}",
        self.expected.iter().last().unwrap()
      )?;

      expected
    } else {
      self.expected.first().map(Cow::from).unwrap_or_default()
    };

    if let Some(found) = &self.found {
      write!(f, "Expected {} but found {}", expected, found.kind())
    } else {
      write!(f, "Expected {} but found EOI", expected)
    }
  }
}
