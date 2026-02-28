use crate::{Error, Input, Parse, Success, Token};

/// Trait for combinators that work together with `#[token]` and `#[select]`. The implementation is
/// almost always very similar to the corresponding `Parse` implementation; the fact that both are
/// needed is an unfortunate side effect of the lack of specialization support, as are many of this
/// crate's problems.
///
/// **Note:** As of right now, combinators don't compose, so `#[token(..)] foo: Vec<Option<Token>>`
/// won't work, but `foo: Vec<Option<SomeParser>>` will.
///
/// Every combinator should both implement this trait and have a custom implementation of `Parse`
/// with the same behavior. The implementation for `Option` might give you an idea of how this
/// should look:
///
/// ```
/// impl<P> Parse for Option<P>
/// where
///   P: Parse,
/// {
///   type Token = P::Token;
///   type Output = Option<P::Output>;
///
///   fn parse<I>(input: &mut I) -> Result<Self::Output, Error<I>>
///   where
///     I: Input<Token = Self::Token>,
///   {
///     let checkpoint = input.save();
///     if let Ok(res) = P::parse(input) {
///       Ok(Some(res))
///     } else {
///       input.reset(checkpoint);
///       Ok(None)
///     }
///   }
/// }
///
///
/// impl<T> Combinator<T> for Option<T> {
///   type Output = Option<T>;
///   fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Self::Output, Error<I>>
///   where
///     I: Input,
///     F: FnMut(&mut I) -> Result<T, Error<I>>,
///   {
///     let checkpoint = input.save();
///     if let Ok(res) = parser(input) {
///       Ok(Some(res))
///     } else {
///       input.reset(checkpoint);
///       Ok(None)
///     }
///   }
/// }
/// ```
pub trait Combinator<A>: Sized {
  // type Output;

  fn apply<I, F>(input: &mut I, parser: F) -> Result<Success<Self, I>, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<Success<A, I>, Error<I>>;
}

impl<T> Combinator<T> for T
where
  T: Token,
{
  fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Success<Self, I>, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<Success<Self, I>, Error<I>>,
  {
    parser(input)
  }
}

impl Combinator<()> for () {
  fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Success<Self, I>, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<Success<(), I>, Error<I>>,
  {
    parser(input)
  }
}

// impl<T> Combinator<T> for T {
//   // type Output = T;

//   fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Self, Error<I>>
//   where
//     I: Input,
//     F: FnMut(&mut I) -> Result<T, Error<I>>,
//   {
//     parser(input)
//   }
// }

// pub trait Combinator<I, F>: Sized
// where
//   I: Input,
// {
//   fn apply(input: &mut I, parser: F) -> Result<Self, Error<I>>;
// }

// impl<I, F, T> Combinator<I, F> for T
// where
//   I: Input,
//   F: FnMut(&mut I) -> Result<T, Error<I>>,
// {
//   fn apply(input: &mut I, mut parser: F) -> Result<Self, Error<I>> {
//     parser(input)
//   }
// }

impl<P> Parse for Option<P>
where
  P: Parse,
{
  type Token = P::Token;
  type Output = Option<P::Output>;

  fn parse<I>(input: &mut I) -> Result<Success<Self::Output, I>, Error<I>>
  where
    I: Input<Token = Self::Token>,
  {
    let chk = input.save();
    match P::parse(input) {
      Ok(res) => Ok(res.map(Some)),
      Err(err) => {
        input.reset(chk);
        Ok(Success(None, Some(err)))

        // if err.committed || input.save() != chk {
        //   // Input was advanced = consuming failure
        //   return Err(err);
        // } else {
        //   Ok(Success(None, Some(err)))
        // }
      }
    }
  }
}

// impl<I, F, T> Combinator<I, F> for Option<T>
// where
//   I: Input,
//   F: FnMut(&mut I) -> Result<U, Error<I>>,
//   T: Combinator<I, F>,
// {
//   fn apply(input: &mut I, parser: F) -> Result<Self, Error<I>> {
//     let checkpoint = input.save();
//     if let Ok(res) = U::apply(input, parser) {
//       Ok(Some(res))
//     } else {
//       input.reset(checkpoint);
//       Ok(None)
//     }
//   }
// }

// impl<T> Combinator<T> for Option<T>
// where
//   T: Combinator<T>,
// {
//   // impl<T, U> Combinator<T> for Option<U>
//   // where
//   //   T: Combinator<T, Output = U>,
//   // {
//   //   type Output = Option<U>;
impl<T, U> Combinator<U> for Option<T>
where
  T: Combinator<U>,
{
  fn apply<I, F>(input: &mut I, parser: F) -> Result<Success<Self, I>, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<Success<U, I>, Error<I>>,
  {
    let checkpoint = input.save();
    if let Ok(res) = T::apply(input, parser) {
      Ok(res.map(Some))
    } else {
      input.reset(checkpoint);
      Ok(None.into())
    }
  }
}

impl<T> Spanned for Option<T>
where
  T: Spanned,
  T::Span: Default,
{
  type Span = T::Span;

  fn span(&self) -> Self::Span {
    self.as_ref().map(Spanned::span).unwrap_or_default()
  }
}

impl<P> Parse for Vec<P>
where
  P: Parse,
{
  type Token = P::Token;
  type Output = Vec<P::Output>;

  fn parse<I>(input: &mut I) -> Result<Success<Self::Output, I>, Error<I>>
  where
    I: Input<Token = Self::Token>,
  {
    let mut result = Success::from(vec![]);
    let mut chk = input.save();
    loop {
      match P::parse(input) {
        Ok(next_res) => {
          let next_item = result.merge(next_res);
          result.0.push(next_item);
          chk = input.save();
        }
        Err(err) => {
          // println!("Failing: {:?} at {:?}", err, input.save());
          if err.committed || input.save() != chk {
            // Input was advanced = consuming failure
            return Err(match result.1 {
              None => err,
              Some(e) => err.merge(e),
            });
          } else {
            result.merge(Success((), Some(err)));
            break;
          }
        }
      }
    }
    input.reset(chk);
    Ok(result)
  }
}

impl<T, U> Combinator<U> for Vec<T>
where
  T: Combinator<U>,
{
  fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Success<Self, I>, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<Success<U, I>, Error<I>>,
  {
    // let mut result = Success::from(vec![]);
    // let mut chk = input.save();
    // while let Ok(next_res) = T::apply(input, &mut parser) {
    //   let next_item = result.merge(next_res);
    //   result.0.push(next_item);
    //   chk = input.save();
    // }
    // input.reset(chk);
    // Ok(result)

    let mut result = Success::from(vec![]);
    let mut chk = input.save();
    loop {
      match T::apply(input, &mut parser) {
        Ok(next_res) => {
          let next_item = result.merge(next_res);
          result.0.push(next_item);
          chk = input.save();
        }
        Err(err) => {
          // println!("Failing: {:?} at {:?}", err, input.save());
          if err.committed || input.save() != chk {
            // Input was advanced = consuming failure
            return Err(match result.1 {
              None => err,
              Some(e) => err.merge(e),
            });
          } else {
            result.merge(Success((), Some(err)));
            break;
          }
        }
      }
    }
    input.reset(chk);
    Ok(result)
  }
}

impl<T> Spanned for Vec<T>
where
  T: Spanned,
  T::Span: Default,
{
  type Span = T::Span;

  fn span(&self) -> Self::Span {
    let first = self.first().map(Spanned::span).unwrap_or_default();
    let last = self.last().map(Spanned::span).unwrap_or_default();
    first.enclose(&last)
  }
}
