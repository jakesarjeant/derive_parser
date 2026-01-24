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

  fn apply<I, F>(input: &mut I, parser: F) -> Result<Self, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<A, Error<I>>;
}

impl<T> Combinator<T> for T
where
  T: Token,
{
  fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Self, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<T, Error<I>>,
  {
    parser(input)
  }
}

impl Combinator<()> for () {
  fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Self, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<(), Error<I>>,
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
    let checkpoint = input.save();
    if let Ok(res) = P::parse(input) {
      Ok(res.map(Some))
    } else {
      input.reset(checkpoint);
      Ok(None.into())
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
  fn apply<I, F>(input: &mut I, parser: F) -> Result<Self, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<U, Error<I>>,
  {
    let checkpoint = input.save();
    if let Ok(res) = T::apply(input, parser) {
      Ok(Some(res))
    } else {
      input.reset(checkpoint);
      Ok(None)
    }
  }
}

impl<P> Parse for Vec<P>
where
  P: Parse,
{
  type Token = P::Token;
  type Output = Vec<P::Output>;

  // TODO: Pass up the last error even in the success case as a hint so that "expected" can be more
  // accurate if no other branch wants the next token either. Also applies to option.
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
          if err.committed || input.save() != chk {
            // Input was advanced = consuming failure
            return Err(err);
          } else {
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
  fn apply<I, F>(input: &mut I, mut parser: F) -> Result<Self, Error<I>>
  where
    I: Input,
    F: FnMut(&mut I) -> Result<U, Error<I>>,
  {
    let mut result = vec![];
    let mut chk = input.save();
    while let Ok(next_item) = T::apply(input, &mut parser) {
      println!("position: {chk:?}");
      result.push(next_item);
      chk = input.save();
    }
    input.reset(chk);
    Ok(result)
  }
}
