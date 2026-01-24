trait Parse {
  type Token: Default;

  fn default_tk() -> Self::Token {
    Self::Token::default()
  }
}

#[derive(Default, Debug)]
struct Token;
impl Parse for Token {
  type Token = Self;
}

impl<T> Parse for Box<T>
where
  T: Parse,
{
  type Token = T::Token;
}

// #[derive(Default, Debug)]
// struct OtherToken;
// impl Parse for OtherToken {
//   type Token = Self;
// }

struct And<A, B>(std::marker::PhantomData<(A, B)>);
trait TokenHelper<X>
where
  X: Parse,
{
  type Token;
}
impl<A, B, T> TokenHelper<B> for And<A, B>
where
  B: Parse<Token = T>,
  T: Default,
{
  type Token = T;
}
impl<A, B, T> Parse for And<A, B>
where
  A: Parse<Token = T>,
  B: Parse,
  T: Default,
{
  type Token = <Self as TokenHelper<B>>::Token;
}

struct Atom;

struct Sum {
  lhs: Box<Sum>,
  kw_eq: Token,
  val: Atom,
}

impl Parse for Sum {
  type Token = <And<Box<Sum>, Token> as Parse>::Token;
}

fn main() {
  println!("{:?}", Sum::default_tk());
}
