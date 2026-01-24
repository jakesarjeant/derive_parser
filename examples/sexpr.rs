use std::fmt::Display;

use derive_parser::{Input, Parse};
use derive_parser_macro::Token;
use logos::Logos;

#[derive(Token, Logos, Debug, PartialEq, Eq, Hash, Clone)]
#[logos(skip r"([ \t\n\f ]+)+")]
pub enum Token {
  Error,

  #[token("(")]
  LParen,
  #[token(")")]
  RParen,

  #[token("<")]
  LTri,

  #[regex("[a-zA-Z]+")]
  Ident,
}

impl Display for Token {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Token::Error => write!(f, "error"),
      Token::LParen => write!(f, "'('"),
      Token::LTri => write!(f, "'<'"),
      Token::RParen => write!(f, "')'"),
      Token::Ident => write!(f, "identifier"),
    }
  }
}

use Token::*;

#[derive(Debug, Parse)]
#[input(Token)]
pub struct SExpr {
  // open: Open,
  #[token(LParen)]
  _lparen: Token,
  // #[token(Ident)]
  // elements: Vec<Option<Token>>,
  elements: Vec<Option<Id>>,
  // elements: Vec<SExpr>,
  #[token(RParen)]
  _rparen: Token,
}

#[derive(Debug, Parse)]
#[input(Token)]
pub struct Id {
  #[token(Ident)]
  _id: Token,
}

#[derive(Debug, Parse)]
#[input(Token)]
pub struct Open {
  #[token(LTri)]
  #[token(LParen)]
  _lparen: Token,
}

// #[derive(Debug, Parse)]
// #[input(Token)]
// pub struct Name(#[token(Ident)] Token);

#[derive(Debug)]
struct VecInput(Vec<Token>, usize);
impl Input for VecInput {
  type Token = Token;
  type Checkpoint = usize;

  fn next(&mut self) -> Option<Self::Token> {
    let val = self.0.get(self.1);
    self.1 += 1;
    val.cloned()
  }

  fn save(&self) -> Self::Checkpoint {
    self.1
  }

  fn reset(&mut self, checkpoint: Self::Checkpoint) {
    self.1 = checkpoint
  }
}

fn main() {
  println!(
    "{:?}",
    SExpr::parse(&mut VecInput(vec![Token::LParen, Token::RParen], 0))
      .map_err(|err| format!("{err}"))
  )
}
