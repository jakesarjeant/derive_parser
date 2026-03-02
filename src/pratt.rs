use std::collections::BTreeSet;

use crate::{Error, Input, Parse, Span, Spanned, Success};

pub trait Precedence: Parse {
  fn prefix_precedence(_val: &Self::Output) -> Option<u8> {
    None
  }
  fn postfix_precedence(_val: &Self::Output) -> Option<u8> {
    None
  }
  fn infix_precedence(_val: &Self::Output) -> Option<(u8, u8)> {
    Some((1, 2))
  }

  fn expect_prefix() -> BTreeSet<String> {
    ["prefix operator".to_string()].into()
  }

  fn expect_infix() -> BTreeSet<String> {
    ["infix operator".to_string()].into()
  }

  fn expect_postfix() -> BTreeSet<String> {
    ["postfix operator".to_string()].into()
  }
}

#[derive(Debug, Clone)]
pub enum Pratt<O, T>
where
  O: Precedence,
  T: Parse<Token = O::Token>,
{
  Infix {
    lhs: Box<Pratt<O, T>>,
    op: O::Output,
    rhs: Box<Pratt<O, T>>,
  },
  Prefix {
    op: O::Output,
    rhs: Box<Pratt<O, T>>,
  },
  Postfix {
    lhs: Box<Pratt<O, T>>,
    op: O::Output,
  },
  Atom(T::Output),
}

impl<O, T> Parse for Pratt<O, T>
where
  O: Precedence,
  T: Parse<Token = O::Token>,
{
  type Output = Pratt<O, T>;
  type Token = T::Token;

  fn parse<I>(input: &mut I) -> Result<crate::Success<Self::Output, I>, crate::Error<I>>
  where
    I: crate::Input<Token = Self::Token>,
  {
    fn parse_pratt<I, O, T>(input: &mut I, min_pre: u8) -> Result<Success<Pratt<O, T>, I>, Error<I>>
    where
      O: Precedence,
      T: Parse<Token = O::Token>,
      I: Input<Token = T::Token>,
    {
      let chk = input.save();
      let mut res = match T::parse(input) {
        Ok(ok) => ok.map(|lhs| Pratt::Atom(lhs)),
        // Handle prefix operators
        Err(err) => match O::parse(input) {
          Ok(mut res) => {
            let Some(r_pre) = O::prefix_precedence(&res.0) else {
              // TODO: Maybe instead allow strings for `found`
              let position = input.save();
              input.reset(chk);
              let tk = input.next();
              return Err(err.merge(Error {
                position,
                found: tk,
                expected: O::expect_prefix(),
                committed: false,
              }));
            };
            let rhs = res.merge(parse_pratt(input, r_pre)?);
            res.map(|op| Pratt::Prefix {
              op,
              rhs: Box::new(rhs),
            })
          }
          Err(err2) => return Err(err.merge(err2)),
        },
      };

      loop {
        let chk = input.save();
        let op = match O::parse(input) {
          Ok(ok) => res.merge(ok),
          Err(err) => {
            res.1 = Some(res.1.map(|e| e.merge(err.clone())).unwrap_or(err));
            break;
          }
        };

        let Some(l_pre) = O::infix_precedence(&op)
          .map(|p| p.0)
          .or_else(|| O::postfix_precedence(&op))
        else {
          let position = input.save();
          input.reset(chk);
          let tk = input.next();
          res.merge(Success(
            (),
            Some(Error {
              committed: false,
              position,
              expected: O::expect_infix()
                .union(&O::expect_postfix())
                .cloned()
                .collect(),
              found: tk,
            }),
          ));
          return Ok(res);
        };
        if l_pre < min_pre {
          input.reset(chk);
          break;
        }

        if let Some((_, r_pre)) = O::infix_precedence(&op) {
          let rhs = res.merge(parse_pratt(input, r_pre)?);
          res.0 = Pratt::Infix {
            lhs: Box::new(res.0),
            op: op,
            rhs: Box::new(rhs),
          };
        } else {
          res.0 = Pratt::Postfix {
            lhs: Box::new(res.0),
            op: op,
          }
        }
      }

      Ok(res)
    }

    parse_pratt(input, 0)
  }
}

impl<O, T> Spanned for Pratt<O, T>
where
  O: Precedence,
  T: Parse<Token = O::Token>,
  O::Output: Spanned,
  T::Output: Spanned<Span = <O::Output as Spanned>::Span>,
{
  type Span = <O::Output as Spanned>::Span;

  fn span(&self) -> Self::Span {
    match self {
      Pratt::Atom(a) => a.span(),
      Pratt::Infix { lhs, op, rhs } => lhs.span().enclose(&op.span()).enclose(&rhs.span()),
      Pratt::Prefix { op, rhs } => op.span().enclose(&rhs.span()),
      Pratt::Postfix { lhs, op } => lhs.span().enclose(&op.span()),
    }
  }
}
