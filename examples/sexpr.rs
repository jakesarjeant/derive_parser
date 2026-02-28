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

  #[regex("[a-zA-Z>+-]+")]
  Ident,
  #[regex("[0-9]+(\\.[0-9]+)?")]
  Number,
}

impl Display for Token {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Token::Error => write!(f, "error"),
      Token::LParen => write!(f, "'('"),
      Token::RParen => write!(f, "')'"),
      Token::Number => write!(f, "number"),
      Token::Ident => write!(f, "identifier"),
    }
  }
}

use Token::*;

#[derive(Debug, Parse)]
#[input(Token)]
pub struct SExpr {
  #[token(LParen)]
  pub _lparen: Token,
  pub elements: Vec<Value>,
  #[token(RParen)]
  pub _rparen: Token,
}

#[derive(Debug, Parse)]
#[input(Token)]
pub enum Value {
  Symbol(#[token(Ident)] Token),
  Number(#[token(Number)] Token),
  Expr(Box<SExpr>),
}

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
  let input = r#"(define (fib n)
                   (if (> 1 n)
                       (+ (fib (- n 1) (- n 2)))
                     n))"#;
  let tokens = Token::lexer(input).collect::<Result<Vec<_>, _>>().unwrap();

  let result = SExpr::parse(&mut VecInput(tokens, 0));

  match result {
    Ok(res) => println!("{:#?}", res.result()),
    Err(err) => {
      eprintln!("{}", err)
    }
  }
}
