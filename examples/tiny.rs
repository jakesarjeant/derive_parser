use std::{collections::HashMap, fmt::Debug, io::stdout, ops::Range};

use ariadne::{Color, Label, Report, ReportKind, Source};
use crossterm::{
  ExecutableCommand, cursor,
  terminal::{self, ClearType},
};
use derive_more::Display;
use derive_parser::{Delimited, Input, Parse, Pratt, Spanned};
use derive_parser_macro::Precedence;
use logos::Logos;
use rustyline::{Config, Editor, error::ReadlineError, history::MemHistory};

type Result<T> = std::result::Result<T, (Option<Range<usize>>, String)>;

#[derive(Logos, Debug, Display, PartialEq, Eq, Hash, Clone, Copy)]
#[logos(skip r"([ \t\n\f ]+)+")]
pub enum TokenKind {
  #[display("error")]
  Error,

  #[display("identifier")]
  #[regex("[a-zA-Z_][a-zA-Z0-9_]*")]
  Ident,
  #[display("number")]
  #[regex("[0-9]*\\.?[0-9]+")]
  Number,

  #[display("','")]
  #[token(",")]
  Comma,
  #[display("':='")]
  #[token(":=")]
  Defvar,
  #[display("'::'")]
  #[token("::")]
  Defconst,
  #[display("'='")]
  #[token("=")]
  Assign,
  #[display("'+'")]
  #[token("+")]
  Add,
  #[display("'-'")]
  #[token("-")]
  Sub,
  #[display("'*'")]
  #[token("*")]
  Mul,
  #[display("'/'")]
  #[token("/")]
  Div,

  #[display("'('")]
  #[token("(")]
  LParen,
  #[display("')'")]
  #[token(")")]
  RParen,
  #[display("'{{'")]
  #[token("{")]
  LBrace,
  #[display("'}}'")]
  #[token("}")]
  RBrace,

  #[display("'fn'")]
  #[token("fn")]
  Fn,
}
use TokenKind::*;

trait Eval {
  fn eval(&self, cx: &mut Context) -> Result<Value>;
}

#[derive(Debug, Parse, Spanned)]
#[input(Token)]
pub struct Syntax {
  expr: Expression,
  #[eoi]
  _eoi: (),
}

#[derive(Debug, Clone, Parse, Spanned)]
#[input(Token)]
pub enum Expression {
  Definition(Definition),
  // Number(#[token(Number)] Token),
  Arithmetic(Pratt<Operator, Atom>),
  Function(Function),
}

impl Eval for Expression {
  fn eval(&self, cx: &mut Context) -> Result<Value> {
    match self {
      Expression::Arithmetic(expr) => expr.eval(cx),
      Expression::Definition(defn) => defn.eval(cx),
      Expression::Function(e_fn) => e_fn.eval(cx),
    }
  }
}

#[derive(Debug, Clone, Parse, Spanned)]
#[input(Token)]
pub struct Definition {
  #[token(Ident)]
  pub ident: Token,
  #[token(Defvar)]
  #[token(Defconst)]
  pub _define: Token,
  #[label("Expression")]
  pub value: Box<Expression>,
}

impl Eval for Definition {
  fn eval(&self, cx: &mut Context) -> Result<Value> {
    let val = self.value.eval(cx)?;
    cx.insert(self.ident.text.clone(), val);
    Ok(Value::Unit(Some(self.span())))
  }
}

#[derive(Debug, Clone, Parse, Spanned, Precedence)]
#[input(Token)]
pub enum Operator {
  #[pratt(1)]
  Add(#[token(Add)] Token),
  #[pratt(1, prefix(5))]
  Sub(#[token(Sub)] Token),

  #[pratt(2)]
  Mul(#[token(Mul)] Token),
  #[pratt(2)]
  Div(#[token(Div)] Token),
}

impl Eval for Pratt<Operator, Atom> {
  fn eval(&self, cx: &mut Context) -> Result<Value> {
    match self {
      Pratt::Atom(a) => a.eval(cx),
      Pratt::Infix { lhs, op, rhs } => {
        let lhs = lhs.eval(cx)?.as_number()?;
        let rhs = rhs.eval(cx)?.as_number()?;
        match op {
          Operator::Add(t) => Ok(Value::Number(lhs + rhs, Some(t.span()))),
          Operator::Sub(t) => Ok(Value::Number(lhs - rhs, Some(t.span()))),
          Operator::Mul(t) => Ok(Value::Number(lhs * rhs, Some(t.span()))),
          Operator::Div(t) => Ok(Value::Number(lhs / rhs, Some(t.span()))),
        }
      }
      Pratt::Prefix {
        op: Operator::Sub(_),
        rhs,
      } => Ok(Value::Number(-rhs.eval(cx)?.as_number()?, Some(rhs.span()))),
      _ => unreachable!(),
    }
  }
}

#[derive(Debug, Clone, Parse, Spanned)]
#[input(Token)]
#[label("atom")]
pub enum Atom {
  Paren(
    #[token(LParen)] Token,
    Box<Expression>,
    #[token(RParen)] Token,
  ),
  Number(#[token(Number)] Token),
  Call {
    #[token(Ident)]
    ident: Token,
    #[token(LParen)]
    _lpar: Token,
    args: Delimited<Box<Expression>, Delim>,
    #[token(RParen)]
    _rpar: Token,
  },
  Reference(#[token(Ident)] Token),
}

impl Eval for Atom {
  fn eval(&self, cx: &mut Context) -> Result<Value> {
    match self {
      Atom::Number(n) => Ok(Value::Number(
        n.text.parse::<f64>().unwrap(),
        Some(n.span()),
      )),
      Atom::Paren(_, expr, _) => expr.eval(cx),
      Atom::Reference(ident) => cx
        .get(&ident.text)
        .cloned()
        .ok_or_else(|| (Some(ident.span()), "Undefined variable".to_string())),
      Atom::Call { ident, args, .. } => {
        let (params, exprs) = cx
          .get(&ident.text)
          .ok_or_else(|| (Some(ident.span()), "Undefined function".to_string()))?
          .as_fun()
          .map(|(a, b)| (a.clone(), b.clone()))?;

        let arg_scope = params
          .iter()
          .zip(args.iter())
          .map(|(p, a)| Ok((p.text.clone(), a.1.eval(cx)?)))
          .collect::<Result<_>>()?;

        cx.push_scope(arg_scope);
        let results = exprs
          .iter()
          .map(|e| e.eval(cx))
          .collect::<Result<Vec<_>>>()?;
        let res = results.last().unwrap_or(&Value::Unit(None));
        cx.pop_scope();

        Ok(res.clone())
      }
    }
  }
}

#[derive(Debug, Clone, Parse, Spanned)]
#[input(Token)]
pub struct Function {
  #[token(Fn)]
  pub _fn: Token,
  #[token(LParen)]
  pub _lpar: Token,
  pub params: Delimited<Param, Delim>,
  #[token(RParen)]
  pub _rpar: Token,
  #[token(LBrace)]
  pub _lbrace: Token,
  pub body: Vec<Expression>,
  #[token(RBrace)]
  pub _rbrace: Token,
}

#[derive(Debug, Parse, Spanned, Clone)]
#[input(Token)]
pub struct Param(#[token(Ident)] pub Token);
#[derive(Debug, Parse, Spanned, Clone)]
#[input(Token)]
pub struct Delim(#[token(Comma)] pub Token);

impl Eval for Function {
  fn eval(&self, _cx: &mut Context) -> Result<Value> {
    Ok(Value::Function(
      self
        .params
        .iter()
        .map(|(_, param)| param.0.clone())
        .collect(),
      self.body.clone(),
      Some(self.span()),
    ))
  }
}

#[derive(Clone)]
pub struct Token {
  kind: TokenKind,
  span: Range<usize>,
  text: String,
}

impl<'s> derive_parser::Token for Token {
  type Kind = TokenKind;
  fn kind(&self) -> Self::Kind {
    self.kind
  }
}

impl Spanned for Token {
  type Span = Range<usize>;
  fn span(&self) -> Self::Span {
    self.span.clone()
  }
}

impl Debug for Token {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      " \x1b[38;5;243m{}\x1b[0m \"{}\" \x1b[38;5;243m@ {:?}\x1b[0m",
      self.kind, self.text, self.span
    )
  }
}

#[derive(Debug)]
pub struct TokenInput(Vec<Token>, usize);

impl TokenInput {
  pub fn from_source<S: AsRef<str>>(source: S) -> std::result::Result<TokenInput, Range<usize>> {
    TokenKind::lexer(source.as_ref())
      .spanned()
      .map(|(tok, span)| {
        tok
          .map(|tok| Token {
            kind: tok,
            span: span.clone(),
            text: source.as_ref()[span.clone()].into(),
          })
          .map_err(|_| span)
      })
      .collect::<std::result::Result<_, _>>()
      .map(|v| TokenInput(v, 0))
  }
}

impl Input for TokenInput {
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

#[derive(Debug, Clone)]
enum Value {
  Unit(Option<Range<usize>>),
  Number(f64, Option<Range<usize>>),
  Function(Vec<Token>, Vec<Expression>, Option<Range<usize>>),
}

impl Value {
  fn as_number(&self) -> Result<f64> {
    match self {
      Value::Number(n, _span) => Ok(*n),
      x => Err((x.span(), format!("{x:?} is not a number"))),
    }
  }
  fn as_fun(&self) -> Result<(&Vec<Token>, &Vec<Expression>)> {
    match self {
      Value::Function(params, exprs, _span) => Ok((params, exprs)),
      x => Err((x.span(), format!("{x:?} is not a function"))),
    }
  }

  fn span(&self) -> Option<Range<usize>> {
    match self {
      Value::Number(_, span) => span.clone(),
      Value::Unit(span) => span.clone(),
      Value::Function(_, _, span) => span.clone(),
    }
  }
}

#[derive(Default, Debug)]
struct Context {
  scopes: Vec<HashMap<String, Value>>,
}

impl Context {
  fn insert(&mut self, name: String, val: Value) {
    self.scopes.last_mut().unwrap().insert(name, val);
  }

  fn get(&self, name: &str) -> Option<&Value> {
    self
      .scopes
      .iter()
      .rfold(None, |acc, scope| acc.or_else(|| scope.get(name)))
  }

  fn push_scope(&mut self, map: HashMap<String, Value>) {
    self.scopes.push(map);
  }

  fn pop_scope(&mut self) {
    self.scopes.pop();
  }
}

fn eval(source: &str, cx: &mut Context) -> ::core::result::Result<(), ()> {
  let mut tokens = TokenInput::from_source(source).map_err(|span| {
    Report::build(ReportKind::Error, ("<repl>", span.clone()))
      .with_message(format!("Invalid character"))
      .with_label(
        Label::new(("<repl>", span))
          .with_color(Color::Red)
          .with_message(format!("Invalid character")),
      )
      .finish()
      .eprint(("<repl>", Source::from(source)))
      .unwrap();
  })?;

  let syntax = match Syntax::parse(&mut tokens) {
    Ok(ok) => ok.result(),
    // Pretty print errors
    Err(err) => {
      let span = err
        .found
        .as_ref()
        .map(|t| t.span.clone())
        .unwrap_or(source.len()..source.len());
      Report::build(ReportKind::Error, ("<repl>", span.clone()))
        .with_message(format!("{err}"))
        .with_label(
          Label::new(("<repl>", span))
            .with_color(Color::Red)
            .with_message(format!("{err}")),
        )
        .finish()
        .eprint(("<repl>", Source::from(source)))
        .unwrap();
      return Err(());
    }
  };

  let result = match syntax.expr.eval(cx) {
    Ok(ok) => ok,
    Err((span, msg)) => {
      let span = span.unwrap_or(source.len()..source.len());
      Report::build(ReportKind::Error, ("<repl>", span.clone()))
        .with_message(format!("{msg}"))
        .with_label(
          Label::new(("<repl>", span))
            .with_color(Color::Red)
            .with_message(format!("{msg}")),
        )
        .finish()
        .eprint(("<repl>", Source::from(source)))
        .unwrap();
      return Err(());
    }
  };

  // println!("{syntax:?}");
  println!("{result:?}");

  Ok(())
}

// Just a repl loop, nothing to see here
fn main() -> anyhow::Result<()> {
  let mut rl = Editor::<(), _>::with_history(Config::default(), MemHistory::new())?;

  let mut stdout = stdout();
  let mut last_ctrl_c = false;
  let mut buffer = String::new();
  let mut cx = Context {
    scopes: vec![HashMap::new()],
  };
  loop {
    match rl.readline_with_initial("tiny› ", (&buffer, "")) {
      Ok(text) => match indent(&text) {
        0 => {
          rl.add_history_entry(&text)?;
          buffer = text;
          if buffer.ends_with("  )") || buffer.ends_with("  ]") || buffer.ends_with("  }") {
            buffer.remove(buffer.len() - 2);
            buffer.remove(buffer.len() - 2);
          }
          stdout
            .execute(cursor::MoveUp(buffer.lines().count() as u16))?
            .execute(terminal::Clear(ClearType::FromCursorDown))?;
          println!("tiny› {buffer}");
          eval(&buffer, &mut cx).ok();
          buffer.clear();
          println!();
        }
        n => {
          buffer = text;
          if buffer.ends_with("  )") || buffer.ends_with("  ]") || buffer.ends_with("  }") {
            buffer.remove(buffer.len() - 2);
            buffer.remove(buffer.len() - 2);
          }
          buffer.push_str("\n");
          buffer.push_str(&"  ".repeat(n));
          stdout
            .execute(cursor::MoveUp(buffer.lines().count() as u16 - 1))?
            .execute(terminal::Clear(ClearType::FromCursorDown))?;
          // stdout.execute(cursor::MoveTo(pos.0, pos.1))?;
          continue;
        }
      },
      Err(ReadlineError::Interrupted) => {
        if last_ctrl_c {
          println!("Press Ctrl-D to exit");
          last_ctrl_c = false;
        } else {
          last_ctrl_c = true;
        }
        buffer.clear();
        continue;
      }
      Err(ReadlineError::Eof) => {
        println!("Bye!");
        break;
      }
      x => {
        println!("Error: {x:?}")
      }
    }
  }

  Ok(())
}

// ================================= Helpers / Utilities =================================

fn indent(code: &str) -> usize {
  code
    .chars()
    .fold(vec![], |mut acc, c| {
      if let Some(close) = [['{', '}'], ['[', ']'], ['(', ')']]
        .iter()
        .find_map(|s| (s[0] == c as _).then_some(s[1]))
      {
        acc.push(close);
        acc
      } else if acc.last() == Some(&c) {
        acc.pop();
        acc
      } else {
        acc
      }
    })
    .len()
}
