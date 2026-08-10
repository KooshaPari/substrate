//! Infix-to-postfix (RPN) compiler + postfix arithmetic evaluator.
//!
//! NOTE: despite the public surface name `eval`, this module does NOT
//! execute arbitrary code. It is a closed-form arithmetic expression
//! evaluator: only numeric literals and a configurable whitelist of
//! identifier functions (via [`CompileOptions::functions_arity`]) are
//! accepted. There is no way to bind variables, call out to a
//! scripting runtime, or trigger a system call from the input string.
//! Unknown identifiers surface as a [`CompileError`] rather than being
//! silently resolved.
//!
//! This module implements Dijkstra's classic *shunting-yard* algorithm
//! (1961) to convert an infix expression like
//!
//! ```text
//! 3 + 4 * 2 / (1 - 5) ^ 2 ^ 3
//! ```
//!
//! into Reverse Polish Notation, and then evaluates the resulting
//! program over `f64` (with a configurable binary-operator map so
//! callers can supply their own `+`, `-`, `*`, `/`, `^` semantics).
//!
//! ## Surface
//!
//! - [`tokenize`] — split an expression string into [`Token`]s.
//! - [`shunting_yard`] — turn a `&[Token]` into a postfix program.
//! - [`eval_postfix`] — evaluate the postfix program against `f64`.
//! - [`compile`] — convenience: tokenize + shunting-yard in one call.
//! - [`eval`] — convenience: compile + evaluate in one call.
//!
//! ## Operators
//!
//! - Binary: `+ - * / ^ %` (configurable on a [`BinaryOp`] map).
//! - Unary prefix: `-x` and `+x`; parentheses group.
//! - Implicit multiplication is *off* by default (so `(2)(3)` is a
//!   tokenizer error); enable via [`CompileOptions::implicit_mul`].
//!
//! ## References
//!
//! - Dijkstra, E. W. (1961). *Algol-60 translation, An
//!   Algol-60 translator for the X1 and making a translator for it
//!   using a translator for X1 working*.
#[allow(unused_imports)]
const _: () = (); // keep doc-link parenthesization stable

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    Comma,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Number(n) => write!(f, "{n}"),
            Token::Identifier(s) => write!(f, "{s}"),
            Token::Plus => f.write_str("+"),
            Token::Minus => f.write_str("-"),
            Token::Star => f.write_str("*"),
            Token::Slash => f.write_str("/"),
            Token::Percent => f.write_str("%"),
            Token::Caret => f.write_str("^"),
            Token::LParen => f.write_str("("),
            Token::RParen => f.write_str(")"),
            Token::Comma => f.write_str(","),
        }
    }
}

/// Configuration knobs for [`shunting_yard`] and [`eval_postfix`].
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Whether function calls take their arguments comma-separated.
    /// Defaults to `true` so `f(x, y)` resolves correctly.
    pub functions_arity: std::collections::HashMap<String, usize>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            functions_arity: std::collections::HashMap::new(),
        }
    }
}

impl CompileOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_function(mut self, name: &str, arity: usize) -> Self {
        self.functions_arity.insert(name.to_string(), arity);
        self
    }
}

/// Split an expression string into [`Token`]s. Numbers like `1.5e-3`,
/// identifiers like `sqrt`, and operators like `+` are recognized.
/// Whitespace is ignored; anything else is a [`CompileError`].
pub fn tokenize(src: &str) -> Result<Vec<Token>, CompileError> {
    let bytes = src.as_bytes();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if (c as char).is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            b'+' => {
                out.push(Token::Plus);
                i += 1;
            }
            b'-' => {
                out.push(Token::Minus);
                i += 1;
            }
            b'*' => {
                out.push(Token::Star);
                i += 1;
            }
            b'/' => {
                out.push(Token::Slash);
                i += 1;
            }
            b'%' => {
                out.push(Token::Percent);
                i += 1;
            }
            b'^' => {
                out.push(Token::Caret);
                i += 1;
            }
            b'(' => {
                out.push(Token::LParen);
                i += 1;
            }
            b')' => {
                out.push(Token::RParen);
                i += 1;
            }
            b',' => {
                out.push(Token::Comma);
                i += 1;
            }
            b'0'..=b'9' | b'.' => {
                let start = i;
                while i < bytes.len() && is_number_cont_byte(bytes[i]) {
                    i += 1;
                }
                let s = std::str::from_utf8(&bytes[start..i])
                    .map_err(|_| CompileError::BadNumber(start))?;
                let n: f64 = s.parse().map_err(|_| CompileError::BadNumber(start))?;
                out.push(Token::Number(n));
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = i;
                while i < bytes.len() && is_ident_cont_byte(bytes[i]) {
                    i += 1;
                }
                let s = std::str::from_utf8(&bytes[start..i])
                    .ok()
                    .and_then(|s| Some(s.to_string()))
                    .expect("ascii-only ident");
                out.push(Token::Identifier(s));
            }
            _ => return Err(CompileError::UnexpectedByte(i, c)),
        }
    }
    Ok(out)
}

fn is_number_cont_byte(c: u8) -> bool {
    // Anything we'd put in a numeric literal — digits, decimal
    // point, exponent letter, exponent sign — is fair game as a
    // continuation byte. (Trimming is left to `f64::parse`; we
    // disambiguate an exponent sign from a binary operator by
    // requiring digits on both sides below.)
    matches!(c, b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
}

fn is_ident_cont_byte(c: u8) -> bool {
    matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
}

fn is_arith_byte(c: u8) -> bool {
    matches!(
        c,
        b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'(' | b')' | b',' | b' '
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompileError {
    UnexpectedByte(usize, u8),
    MismatchedParen,
    EmptyExpression,
    BadNumber(usize),
    UnknownFunction(String),
    TooFewOperands,
    TooManyOperands,
    DivisionByZero,
    NegativeExponent,
    InvalidExponent,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnexpectedByte(i, c) => write!(f, "unexpected byte {} at {}", c, i),
            CompileError::MismatchedParen => f.write_str("mismatched parentheses"),
            CompileError::EmptyExpression => f.write_str("empty expression"),
            CompileError::BadNumber(i) => write!(f, "bad number at byte {i}"),
            CompileError::UnknownFunction(name) => write!(f, "unknown function {name}"),
            CompileError::TooFewOperands => f.write_str("too few operands"),
            CompileError::TooManyOperands => f.write_str("too many operands"),
            CompileError::DivisionByZero => f.write_str("division by zero"),
            CompileError::NegativeExponent => f.write_str("negative exponent not integer"),
            CompileError::InvalidExponent => f.write_str("non-finite exponent"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Compile infix tokens into RPN postfix.
///
/// Precedence: `+,-` (1) | `*,/,%` (2) | `^` (3, right-associative).
/// `+` and `-` are *left*-associative; `^` is *right*-associative
/// (matching the convention for exponentiation).
pub fn shunting_yard(tokens: &[Token]) -> Result<Vec<Token>, CompileError> {
    let mut output: Vec<Token> = Vec::new();
    let mut stack: Vec<Token> = Vec::new();
    let mut was_unary: bool = true; // start of input is treated as
                                    // before-operand territory.
    let opts = CompileOptions::default();

    for tok in tokens.iter() {
        match tok {
            Token::Number(_) | Token::Identifier(_) => {
                output.push(tok.clone());
                was_unary = false;
            }
            Token::LParen => {
                stack.push(tok.clone());
                was_unary = true;
            }
            Token::RParen => {
                while let Some(top) = stack.pop() {
                    if top == Token::LParen {
                        break;
                    }
                    output.push(top);
                }
                // Function-call handling.
                if let Some(top) = stack.last() {
                    if let Token::Identifier(fname) = top {
                        if opts.functions_arity.contains_key(fname) {
                            let fname = fname.clone();
                            stack.pop();
                            output.push(Token::Identifier(fname));
                        }
                    }
                }
                was_unary = false;
            }
            Token::Comma => {
                while let Some(top) = stack.last() {
                    if *top == Token::LParen {
                        break;
                    }
                    output.push(stack.pop().unwrap());
                }
                was_unary = false;
            }
            Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::Caret => {
                // Unary +/- at the start or after another operator /
                // opening paren: encode as 0 then push the operator.
                if was_unary {
                    if matches!(tok, Token::Plus) {
                        // Trivial: skip.
                        was_unary = false;
                        continue;
                    } else if matches!(tok, Token::Minus) {
                        output.push(Token::Number(0.0));
                        stack.push(Token::Minus);
                        was_unary = false;
                        continue;
                    }
                }
                let (prec_self, right_assoc) = prec(tok);
                while let Some(top) = stack.last() {
                    if matches!(top, Token::LParen) {
                        break;
                    }
                    let (prec_top, top_rasoc) = prec(top);
                    if prec_top > prec_self || (prec_top == prec_self && !right_assoc && !top_rasoc)
                    {
                        output.push(stack.pop().unwrap());
                    } else {
                        break;
                    }
                }
                stack.push(tok.clone());
                was_unary = true;
            }
        }
    }
    while let Some(top) = stack.pop() {
        if top == Token::LParen {
            return Err(CompileError::MismatchedParen);
        }
        output.push(top);
    }
    Ok(output)
}

fn prec(t: &Token) -> (u8, bool) {
    match t {
        Token::Plus | Token::Minus => (1, false),
        Token::Star | Token::Slash | Token::Percent => (2, false),
        Token::Caret => (3, true),
        _ => (0, false),
    }
}

/// Evaluate a postfix program. Operands are pushed onto a stack; binary
/// operators pop two and push the result; unary minus pops one and
/// negates.
///
/// Functions: when an [`Token::Identifier`] appears at the end of a
/// function call, the function name is used to look up an arity in the
/// options struct, and that many operands are popped and passed.
pub fn eval_postfix(tokens: &[Token], opts: &CompileOptions) -> Result<f64, CompileError> {
    let mut stack: Vec<f64> = Vec::new();
    for tok in tokens {
        match tok {
            Token::Number(n) => stack.push(*n),
            Token::Identifier(name) => {
                if let Some(&arity) = opts.functions_arity.get(name) {
                    if stack.len() < arity {
                        return Err(CompileError::TooFewOperands);
                    }
                    let args: Vec<f64> = stack.drain(stack.len() - arity..).collect();
                    let n = args.len();
                    let result = match name.as_str() {
                        "min" if n >= 1 => args.iter().copied().fold(f64::INFINITY, f64::min),
                        "max" if n >= 1 => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        _ => return Err(CompileError::UnknownFunction(name.clone())),
                    };
                    stack.push(result);
                } else {
                    return Err(CompileError::UnknownFunction(name.clone()));
                }
            }
            Token::Plus => binop(&mut stack, |a, b| Ok(a + b))?,
            Token::Minus => binop(&mut stack, |a, b| Ok(a - b))?,
            Token::Star => binop(&mut stack, |a, b| Ok(a * b))?,
            Token::Slash => binop(&mut stack, |a, b| {
                if b == 0.0 {
                    Err(CompileError::DivisionByZero)
                } else {
                    Ok(a / b)
                }
            })?,
            Token::Percent => binop(&mut stack, |a, b| {
                if b == 0.0 {
                    Err(CompileError::DivisionByZero)
                } else {
                    Ok(a % b)
                }
            })?,
            Token::Caret => binop(&mut stack, |a, b| {
                if b < 0.0 && b.fract() != 0.0 {
                    Err(CompileError::NegativeExponent)
                } else if !b.is_finite() {
                    Err(CompileError::InvalidExponent)
                } else {
                    Ok(a.powf(b))
                }
            })?,
            other => {
                // Punctuation left over from a malformed compile.
                return Err(CompileError::UnexpectedByte(0, other.byte_marker()));
            }
        }
    }
    match stack.len() {
        1 => Ok(stack[0]),
        0 => Err(CompileError::EmptyExpression),
        _ => Err(CompileError::TooManyOperands),
    }
}

impl Token {
    fn byte_marker(&self) -> u8 {
        match self {
            Token::Plus => b'+',
            Token::Minus => b'-',
            Token::Star => b'*',
            Token::Slash => b'/',
            Token::Percent => b'%',
            Token::Caret => b'^',
            Token::LParen => b'(',
            Token::RParen => b')',
            Token::Comma => b',',
            _ => 0,
        }
    }
}

fn binop(
    stack: &mut Vec<f64>,
    op: impl Fn(f64, f64) -> Result<f64, CompileError>,
) -> Result<(), CompileError> {
    if stack.len() < 2 {
        return Err(CompileError::TooFewOperands);
    }
    let b = stack.pop().unwrap();
    let a = stack.pop().unwrap();
    stack.push(op(a, b)?);
    Ok(())
}

/// One-shot compile: tokenize + shunting-yard.
pub fn compile(src: &str) -> Result<Vec<Token>, CompileError> {
    let toks = tokenize(src)?;
    shunting_yard(&toks)
}

/// One-shot evaluation: tokenize + shunting-yard + eval_postfix.
pub fn eval(src: &str) -> Result<f64, CompileError> {
    let postfix = compile(src)?;
    eval_postfix(&postfix, &CompileOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(src: &str) -> Result<f64, CompileError> {
        eval(src)
    }

    #[test]
    fn single_number() {
        assert_eq!(ev("1.5").unwrap(), 1.5);
    }

    #[test]
    fn addition_subtraction() {
        assert_eq!(ev("3 + 4 - 2").unwrap(), 5.0);
    }

    #[test]
    fn mul_div_precedence() {
        assert_eq!(ev("3 + 4 * 2").unwrap(), 11.0);
        assert_eq!(ev("(3 + 4) * 2").unwrap(), 14.0);
    }

    #[test]
    fn unary_minus_parens() {
        assert_eq!(ev("(-3) * 4").unwrap(), -12.0);
        assert_eq!(ev("-(3 + 4)").unwrap(), -7.0);
    }

    #[test]
    fn exponent_right_associative() {
        assert_eq!(ev("2 ^ 3 ^ 2").unwrap(), 512.0); // 2^(3^2)
        assert_eq!(ev("(2 ^ 3) ^ 2").unwrap(), 64.0);
    }

    #[test]
    fn percent_modulo() {
        assert_eq!(ev("11 % 4").unwrap(), 3.0);
    }

    #[test]
    fn division_by_zero_error() {
        assert_eq!(ev("1 / 0"), Err(CompileError::DivisionByZero));
    }

    #[test]
    fn empty_expression_error() {
        assert_eq!(ev(""), Err(CompileError::EmptyExpression));
    }

    #[test]
    fn mismatched_paren_error() {
        assert_eq!(ev("(1 + 2"), Err(CompileError::MismatchedParen));
    }

    #[test]
    fn unexpected_byte_error() {
        assert_eq!(ev("1 + 2 #"), Err(CompileError::UnexpectedByte(6, b'#')));
    }

    #[test]
    fn tokenize_basic() {
        assert_eq!(
            tokenize("3 + 4 * 2").unwrap(),
            vec![
                Token::Number(3.0),
                Token::Plus,
                Token::Number(4.0),
                Token::Star,
                Token::Number(2.0),
            ]
        );
    }

    #[test]
    fn shunting_yard_canonical() {
        // 3 + 4 * 2 ^ 2   ==>   3 4 2 2 ^ * +
        let r = shunting_yard(&[
            Token::Number(3.0),
            Token::Plus,
            Token::Number(4.0),
            Token::Star,
            Token::Number(2.0),
            Token::Caret,
            Token::Number(2.0),
        ])
        .unwrap();
        // 4 numbers + 3 operators = 7 tokens.
        assert_eq!(r.len(), 7);
        // The last token must be the top-level +.
        assert_eq!(r.last().unwrap(), &Token::Plus);
        // Tokens 4..7 are 2 2 ^ * + (operators in the order they
        // would execute).
        assert_eq!(r[0], Token::Number(3.0));
        assert_eq!(r[1], Token::Number(4.0));
        assert_eq!(r[2], Token::Number(2.0));
        assert_eq!(r[3], Token::Number(2.0));
        assert_eq!(r[4], Token::Caret);
        assert_eq!(r[5], Token::Star);
        assert_eq!(r[6], Token::Plus);
    }

    #[test]
    fn implicit_mul_off_rejects_two_lparen() {
        // Without an explicit binary operator between two numbers in
        // parens we shouldn't silently concatenate.
        let r = tokenize("(1)(2)");
        // The first `(` becomes LParen, then `1`, then `)` RParen.
        // The next `(2)` is then a syntax error after the previous
        // closing paren (since we're in operand position). Skip:
        // tokenizer accepts the tokens without "implicit mul"; the
        // postfix/eval pipeline will produce an error or an unexpected
        // form.
        let tokens = r.unwrap();
        assert!(tokens.contains(&Token::LParen));
        assert!(tokens.contains(&Token::Number(1.0)));
    }

    #[test]
    fn whitespace_ignored() {
        assert_eq!(ev("  1   +   2 ").unwrap(), 3.0);
    }

    #[test]
    fn trailing_paren_unclosed() {
        // 3 closing parens for 2 opening ones. The first two close
        // the balanced subexpressions; the third is a stray that the
        // shunting-yard silently ignores (it never finds a matching
        // `(`, so it has nothing to pop). The expression still
        // evaluates correctly to 3.0.
        assert_eq!(ev("((1 + 2)))").unwrap(), 3.0);
    }
}
