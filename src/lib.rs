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

pub trait Input: Debug {
  type Token: Token + Debug;
  type Checkpoint: Copy + Ord + Debug;

  /// Returns the next token and advances the input
  fn next(&mut self) -> Option<Self::Token>;
  /// Saves the current input state. This operation should be cheap.
  fn save(&self) -> Self::Checkpoint;
  /// Restores the input to a previous state
  fn reset(&mut self, checkpoint: Self::Checkpoint);
}

pub trait Parse {
  type Token: Debug;
  type Output;

  fn parse<I>(input: &mut I) -> Result<Success<Self::Output, I>, Error<I>>
  where
    I: Input<Token = Self::Token>;
}

#[derive(Clone)]
pub struct Success<O, I: Input>(#[doc(hidden)] pub O, #[doc(hidden)] pub Option<Error<I>>);

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
    F: FnOnce(O) -> P,
  {
    Success(fun(self.0), self.1)
  }

  pub fn result(self) -> O {
    self.0
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
    let committed = self.committed || other.committed;
    // println!("Merging:\n\t{:?} and\n\t{:?}\n", self, other);
    if self.position == other.position {
      self.expected.extend(other.expected.into_iter());
      self.committed = committed;
      self
    } else {
      let mut err = std::cmp::max_by_key(self, other, |e| e.position);
      err.committed = committed;
      err
    }
  }
}

impl<I> Display for Error<I>
where
  I: Input,
{
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Expected ")?;
    if self.expected.len() > 1 {
      let mut all_but_last = self.expected.iter().take(self.expected.len() - 1);

      write!(f, "one of {}", all_but_last.next().unwrap())?;

      for exp in all_but_last {
        write!(f, ", {}", exp)?;
      }

      write!(f, " or {}", self.expected.iter().last().unwrap())?;
    } else if let Some(first) = self.expected.first() {
      write!(f, "{first}")?;
    } else {
      write!(f, "end of input")?;
    };

    if let Some(found) = &self.found {
      write!(f, " but found {}", found.kind())
    } else {
      write!(f, " but found end of input")
    }
  }
}
