use std::{
  fmt::{Debug, Display},
  ops::Range,
};

use ariadne::{Color, Label, Report, ReportKind, Source};
use derive_parser::{Input, Parse};
use logos::Logos;

#[derive(Logos, Debug, PartialEq, Eq, Hash, Clone, Copy)]
#[logos(skip r"([ \t\n\f ]+)+")]
pub enum TokenKind {
  Error,

  #[token("def")]
  Def,
  #[token("type")]
  Typ,

  #[token("->")]
  Arrow,
  #[token(":")]
  Colon,

  #[token("(")]
  LParen,
  #[token(")")]
  RParen,
  #[token("[")]
  LBrack,
  #[token("]")]
  RBrack,

  #[regex("[a-zA-Z]+")]
  Ident,
}

impl Display for TokenKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      TokenKind::Error => write!(f, "error"),
      TokenKind::Def => write!(f, "'def'"),
      TokenKind::Typ => write!(f, "'type'"),
      TokenKind::Arrow => write!(f, "'->'"),
      TokenKind::Colon => write!(f, "':'"),
      TokenKind::LParen => write!(f, "'('"),
      TokenKind::RParen => write!(f, "')'"),
      TokenKind::LBrack => write!(f, "'['"),
      TokenKind::RBrack => write!(f, "']'"),
      TokenKind::Ident => write!(f, "identifier"),
    }
  }
}

#[derive(Clone)]
pub struct Token<'i> {
  kind: TokenKind,
  text: &'i str,
  span: Range<usize>,
}

impl<'i> derive_parser::Token for Token<'i> {
  type Kind = TokenKind;
  type Span = Range<usize>;
  fn kind(&self) -> Self::Kind {
    self.kind
  }
  fn span(&self) -> Self::Span {
    self.span.clone()
  }
}

impl<'i> Debug for Token<'i> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      " \x1b[38;5;243m{}\x1b[0m \"{}\" \x1b[38;5;243m@ {:?}\x1b[0m",
      self.kind, self.text, self.span
    )
  }
}

impl<'i> Display for Token<'i> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.text)
  }
}

use TokenKind::*;

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub struct Syntax<'i> {
  pub items: Vec<Item<'i>>,
  #[eoi]
  pub _eoi: (),
  // items: Vec<DefItem>,
}

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub struct Item<'i> {
  #[token(Typ)]
  _type: Token<'i>,
  #[token(Ident)]
  name: Token<'i>,
  typ: TypeVariants<'i>,
}

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub enum TypeVariants<'i> {
  Product(TypeAnnotation<'i>),
  Sum(Vec<TypeVariant<'i>>),
}

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub struct TypeVariant<'i> {
  #[token(Colon)]
  _col: Token<'i>,
  #[token(Ident)]
  name: Token<'i>,
  typ: TypeAnnotation<'i>,
}

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub enum TypeAnnotation<'i> {
  Stack(StackType<'i>),
  Fun(FunType<'i>),
}

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub struct StackType<'i> {
  #[token(LParen)]
  _lpar: Token<'i>,
  tys: Vec<Type<'i>>,
  #[token(RParen)]
  _rpar: Token<'i>,
}

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub struct FunType<'i> {
  #[token(LParen)]
  _lpar: Token<'i>,
  lhs: Vec<Type<'i>>,
  #[token(Arrow)]
  _arrow: Token<'i>,
  rhs: Vec<Type<'i>>,
  #[token(RParen)]
  _rpar: Token<'i>,
}

#[derive(Debug, Parse)]
#[input(Token<'i>)]
pub enum Type<'i> {
  Name(#[token(Ident)] Token<'i>),
  Stack(StackType<'i>),
  Fun(FunType<'i>),
}

#[derive(Debug)]
struct VecInput<T>(Vec<T>, usize);
impl<T: derive_parser::Token + Debug> Input for VecInput<T> {
  type Token = T;
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
  //   let source = r#"
  // def add (int int -> int): ADD
  // def foo (int): BAR

  // type Point(int int)
  // type MyOpt : Some (int)
  //            : None ()

  // type Functions(a b)
  // "#;
  let source = r#"
type Point (int)
  "#;

  match Syntax::parse(&mut VecInput(
    TokenKind::lexer(source)
      .spanned()
      .map(|(tok, span)| {
        tok.map(|tok| Token {
          kind: tok,
          span: span.clone(),
          text: &source[span],
        })
      })
      .collect::<Result<_, _>>()
      .unwrap(),
    0,
  )) {
    Ok(res) => println!("{:#?}", res.result()),
    Err(err) => {
      let span = err
        .found
        .as_ref()
        .map(|t| t.span.clone())
        .unwrap_or(source.len()..source.len());
      Report::build(ReportKind::Error, ("<source>", span.clone()))
        .with_code(1)
        .with_message(format!("{err}"))
        .with_label(
          Label::new(("<source>", span))
            .with_color(Color::Red)
            .with_message(format!("{err}")),
        )
        .finish()
        .eprint(("<source>", Source::from(source)))
        .unwrap();
    }
  }
}
