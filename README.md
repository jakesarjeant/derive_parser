<div align="center">
  <h1>derive_parser</h1>
  <p>
    <a href="https://docs.rs/derive_parser">docs.rs</a> |
    <a href="https://crates.io/crates/derive_parser">crates.io</a> |
    <a href="https://github.com/jakesarjeant/derive_parser">github.com</a>
  </p>
  <p>
  <img alt="License: BSD 3-clause" src="https://img.shields.io/github/license/jakesarjeant/derive_parser?color=orange&style=for-the-badge" />
  <img alt="Latest release" src="https://img.shields.io/crates/v/derive_parser?color=yellow&style=for-the-badge" />
  <img alt="Github issue counter" src="https://img.shields.io/github/issues/jakesarjeant/derive_parser?style=for-the-badge" />
  </p>
</div>

<p align="center">
  <code>&lt;&gt;Derive parsers from CST representations&lt;/&gt;</code>
</p>

---

This crate provides a derive macro, `#[derive(Parse)]` that derives a recursive descent parser for a syntax tree node based on fields' `Parse` implementations as well as derive macros.

> [!IMPORTANT]
> Disclaimer: I had an interesting idea and sketched out a proof-of-concept. As of right now, that's all this is. It works, mostly, but is in no way feature-complete or efficient. If people end up showing interest, I'll rewrite it from scratch with better design choices. For now, feel free to experiment, but don't expect feature or performance parity with existing parser generators. It's worth noting that the nature of derive macros imposes a limit on how efficient an approach like this can be, since it's not possible to globally collect definitions across structs and build e.g. an LR transition table.

# Example

```rust
use derive_parser::{Parse, Token};

mod lexer;
use lexer::TokenKind::*; // Implemented e.g. with Logos

#[derive(Parse, Spanned)]
#[input(Token)]
struct FunctionCall {
    #[token(Ident)]
    name: Token,
    #[token(LParen)]
    _lparen: Token,
    args: Delimited<Expression, Delim>,
    #[token(RParen)]
    _rparen: Token
}

#[derive(Parse, Spanned)]
#[input(Token)]
enum Expression {
    Call(FunctionCall),
    #[token(Bool)]
    #[token(Int)]
    #[token(String(_))]
    Literal(Token)
}

#[derive(Parse, Spanned)]
#[input(Token)]
struct Delim(#[token(Comma)])

// Support stuff; we could've just implemented `Token` for `TokenKind`:

#[derive(Clone, Debug)]
struct Token {
    pub inner: lexer::TokenKind,
    pub span: Span,
    pub trailing_trivia: Option<String>,
    pub string: String,
}

impl derive_parser::Token for Token {
    type Kind = TokenKind;
    fn kind(&self) -> Self::Kind {
        self.inner
    }
}
impl Spanned for Token {
    type Span = Span;
    fn span(&self) -> Self::Span {
        self.span
    }
}
```

By only annotating what can't be inferred from the syntax tree structs and deriving the rest of the parser implementation, `derive_parser` minimizes the amount of code needed to express your parser. In addition, this keeps your CST and parser implementations in sync, which should help you avoid bugs when updating your parser. The whole thing can be also be made zero-copy by just adding a lifetime paramter to `Token` and the node structs.

<details>
<summary>For comparison, here's the same parser written with chumsky:</summary>

```rust
use chumsky::{Parser, recursive::Recursive};

mod lexer;
use lexer::TokenKind; // Implemented e.g. with logos

struct FunctionCall {
    name: Token,
    _lparen: Token,
    args: Vec<Expression>,
    _rparen: Token
}

enum Expression {
    Call(FunctionCall),
    Literal(Token)
}

fn parser<I>() -> impl Parser<I, FunctionCall, Simple<I>>
where
    I: Input<Token = Token, Span = SimpleSpan<usize, ()> + ValueInput
{
    use TokenKind::*;

    let mut expression = Recursive::declare();

    let function_call = just!(Ident)
        .then(just!(LParen))
        .then(expression.separated_by(just!(Comma)).allow_trailing)
        .then(just!(RParen))
        .map(|(((name, _lparen), args), _rparen)| {
            FunctionCall { name, _lparen, args, _rparen }
        });

    expression.define(
        just!(Bool)
            .map(Expression::Literal)
            .or(just!(Int).map(Expression::Literal))
            .or(just!(String(_)).map(Expression::Literal))
            .or(function_call.clone())
    );

    function_call
}

#[derive(Clone)]
struct Token {
    pub inner: TokenKind,
    pub span: Span,
    pub trailing_trivia: Option<String>,
    pub string: String,
}

macro_rules! just {
    ($kind:pat) => {
        select! {
            t @ Token { inner: $kind, .. } => t
        }
    };
}
```

Even with all that, many of the convenience methods (like `.span()` on nodes) that `derive_parser` provides are still not implemented here, and I didn't even try to make this zero-copy due to the sheer amount of lifetime-juggling.
</summary>
</details>

# Pratt Parsing

Currently, `derive_parser` generates recursive descent parsers. This makes inherently left-recursive grammars like arithmetic expressions hard to represent. To solve this, a built-in pratt parser is provided:

```rust
use derive_parser::{Pratt, Precedence, Parse, Spanned};

#[derive(Parse, Spanned)]
#[input(Token)]
enum Atom {
  Paren(
    #[token(LParen)] Token,
    Box<Expression>
    #[token(RParen)] Token,
  ),
  Number(#[token(Number)] Token)
}

#[derive(Parse, Spanned)]
#[input(Token)]
struct Expression(Pratt<Operator, Atom>);

#[derive(Parse, Spanned, Precedence)]
#[input(Token)]
enum Operator {
  // Infix Operators
  #[pratt(1)]
  Add(#[token(Plus)] Token),
  #[pratt(2)]
  Mul(#[token(Times)] Token),
  
  // Prefix Operators
  #[pratt(prefix(4))]
  Sub(#[token(Minus)] Token),
  
  // Postfix operators
  #[pratt(postfix(3))]
  Fac(#[token(Bang)] Token)
}
```

This will parse an operator-precedence expression like `1 + 2 * 3 + 4 * -5!` as `(1 + (2 * 3)) + (4 * (-5)!)`.

# Attributes

If the right-hand side of a field implements `Parse`, you don't need any attributes — the parser will automatically try to parse the field with its own `Parse` implementation.

<!-- Otherwise, you can use attributes to explain how to parse the field. You may use any number of the following attributes (you can have many of the same, too), so long as they all return the correct type for the field. -->
Otherwise, you can use attributes to explain how to parse the field. Currently, the only attribute for this case is `#[token(...)]`.

## `#[token(PATTERN)]`

By far the most common attribute is `#[token(...)]`. It applies a pattern to the next input token, consuming it if it matches:

```rust
enum TokenKind {
  LParen,
  RParen
}

#[derive(Parse)]
#[input(Token)]
struct Parens {
  // Match tokens containing a `TokenKind::LParen`
  #[token(TokenKind::LParen)]
  lparen: Token,
  inner: Option<Parens>,
  #[token(TokenKind::RParen)]
  rparen: Token
}
```

It is common to `use TokenKind::*` in your parser module to avoid repetition. You may use `#[token]` multiple times to allow different tokens on the same field:

```rust
// -- snip --
use TokenKind::*;

#[derive(Parse)]
#[input(Token)]
struct Literal(
  #[token(Bool)]
  #[token(Int)]
  #[token(Float)]
  Token
);
```

# Planned Features

- [ ] Error recovery (`#[required]`, `#[recover]`)
- [ ] Better `Delimited` API (`#[delimited]`)

<!--
## `#[delimited(PATTERN[, allow_trailing = true])]`

Parses into `Delimited<T, Token>`, capturing a sequence of `T` separated by tokens matching `PATTERN`:

```rust
#[derive(Parse)]
#[input(Token)] 
struct Args {
  #[token(LParen)]
  _lparen: Token,
  #[delimited(Comma, allow_trailing = true)]
  args: Delimited<Expr, Token>
  #[token(RParen)]
  _rparen: Token
}
```

You can combine `#[delimited]` and `#[token]`, which will parse a sequence of tokens matched by token, delimited by the given delimiter. For example:

```rust
#[derive(Syntax)]
struct IntBoolList {
  #[token(Bool)]
  #[token(Int)]
  #[delimited(Comma)]
  values: Delimited<Token, Token>
}

// This will parse something like:
//   1,2,false,3,true
```

> [!NOTE]
> `#[delimited]` may only be used once per field.

# Utility Types and Implementations

A number of utility types are provided, as well as implementations on utility types from the standard library. For example, `Option<T>`'s implementation will attempt to parse `T` and simply return `None` if it fails, and `Vec<T>` parses `T` as often as possible in sequence.

By the same token as using `#[token]` with `#[delimited]`, `#[token]` will also automatically nest into `Vec` and `Option`:

```rust
#[derive(Syntax)]
struct MaybeBool {
  #[token(Bool)]
  value: Option<Token>
}
```
-->
<!--
# Error recovery

When possible, the parser will try to recover from syntax errors. In many cases, this will require a bit of guidance. For example, consider the following invalid JavaScript expression:

```js
while a > 5 {
  console.log("Hello World!"
}
```

A naive parser would simply break early with an `Expected '('`, but we can do better. Our principal tool for this is the `#[required]` attribute, which can be attached to any optional field. When this attribute is present, the parser will continue to parse if the item isn't present, but will still emit an error. The specific error that is emitted can be overwritten with `#[required(error = ...)]`.

```rust
#[derive(Syntax)]
struct WhileStatement {
  #[token(While)]
  kw_while: Token,
  #[token(LParen)]
  #[required]
  _lparen: Option<Token>,
  cond: Expression,
  #[token(RParen)]
  #[required(error = "Missing ')' in while statement")]
  _rparen: Option<Token>,
  #[token(LBrace)]
  _lbrace: Token,
  body: Vec<Statement>
  #[token(RBrace)]
  _rbrace: Token
}
```

Another case we have to consider is that in JS, braces aren't necessary for single expressions, so more complex recovery behavior is required for:

```js
while (a > 5]
  console.log("Hello World");
```

For example, here there is an incorrect parenthesis closing the conditional, which, again, could naively emit `Unexpected ']'` and parse the `console.log` as a separate statement. Again, we can do better, but this time with `#[recover_skip(STOP,..)]`, which will keep skipping tokens until the given field matches or one of the given stop tokens is reached (at which point it gives up).

```rust
#[derive(Syntax)]
struct WhileStatement {
  #[token(While)]
  kw_while: Token,
  #[token(LParen)]
  #[required]
  _lparen: Option<Token>,
  cond: Expression,
  #[token(RParen)]

  #[required(error = "Missing ')' in while statement")]
  _rparen: Option<Token>,

  #[recover_skip(RBrace, Semi)]
  body: StatementOrBlock
}
```
-->
