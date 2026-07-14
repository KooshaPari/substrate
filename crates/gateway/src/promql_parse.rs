// Minimal PromQL parser — supports the operators most commonly used in alert
// rules and recording rules, expressed as a small AST.
//
// Supported:
//   - metric identifiers (e.g. `up`, `http_requests_total`)
//   - label matchers: `metric{label="value", label!="other"}`
//   - selectors: instant and range vectors `metric[5m]`
//   - binary operators: + - * / % ^
//   - comparison operators: == != > >= < <= =~ !~
//   - aggregations: sum, avg, min, max, count (over `by (label)`)
//
// This is a single-pass recursive-descent parser. It deliberately rejects
// matrix selectors, subqueries, `@`-modifiers, and binary operator chaining on
// vectors — the goal is to lint the common rule shapes, not be a full PromQL
// implementation.

use std::fmt;

/// A single label matcher inside a selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelMatcher {
    pub name: String,
    pub op: MatchOp,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOp {
    Eq,  // =
    Ne,  // !=
    Re,  // =~
    Nre, // !~
}

impl fmt::Display for MatchOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatchOp::Eq => write!(f, "="),
            MatchOp::Ne => write!(f, "!="),
            MatchOp::Re => write!(f, "=~"),
            MatchOp::Nre => write!(f, "!~"),
        }
    }
}

/// AST node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A metric selector with optional label matchers and optional range vector.
    Metric(Metric),
    /// A function call (e.g. `rate(...)`).
    Call(Call),
    /// A binary expression between two operands.
    Binary(Binary),
    /// An aggregation expression (`sum by (label) (...)`).
    Aggregate(Aggregate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metric {
    pub name: String,
    pub matchers: Vec<LabelMatcher>,
    /// `Some(duration)` for range vectors (`metric[5m]`), `None` for instant.
    pub range: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub func: String,
    pub args: Vec<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Re,
    Nre,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Pow => "^",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Re => "=~",
            BinOp::Nre => "!~",
        };
        f.write_str(s)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Metric(m) => {
                write!(f, "{}", m.name)?;
                if !m.matchers.is_empty() {
                    write!(f, "{{")?;
                    for (i, m2) in m.matchers.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}{}{:?}", m2.name, m2.op, m2.value)?;
                    }
                    write!(f, "}}")?;
                }
                if let Some(d) = &m.range {
                    write!(f, "[{}]", d)?;
                }
                Ok(())
            }
            Expr::Call(c) => {
                write!(f, "{}(", c.func)?;
                for (i, a) in c.args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", a)?;
                }
                write!(f, ")")
            }
            Expr::Binary(b) => {
                write!(f, "({} {} {}", b.lhs, b.op, b.rhs)?;
                if b.bool_modifier {
                    write!(f, " bool")?;
                }
                write!(f, ")")
            }
            Expr::Aggregate(a) => {
                write!(f, "{}", a.op)?;
                if !a.by.is_empty() {
                    write!(f, " by (")?;
                    for (i, n) in a.by.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", n)?;
                    }
                    write!(f, ")")?;
                }
                write!(f, "({})", a.expr)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binary {
    pub op: BinOp,
    pub lhs: Box<Expr>,
    pub rhs: Box<Expr>,
    /// `true` when the operator has an explicit `bool` modifier (`== bool ...`).
    pub bool_modifier: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aggregate {
    pub op: AggOp,
    pub by: Vec<String>,
    pub expr: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggOp {
    Sum,
    Avg,
    Min,
    Max,
    Count,
}

impl fmt::Display for AggOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AggOp::Sum => "sum",
            AggOp::Avg => "avg",
            AggOp::Min => "min",
            AggOp::Max => "max",
            AggOp::Count => "count",
        };
        f.write_str(s)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    UnexpectedEof,
    UnexpectedChar(char),
    InvalidEscape,
    InvalidNumber,
    EmptyParens,
    EmptyAggregation,
    UnknownFunction(String),
    UnknownAggregation(String),
    Expected(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnexpectedEof => write!(f, "unexpected end of input"),
            Error::UnexpectedChar(c) => write!(f, "unexpected character '{}'", c),
            Error::InvalidEscape => write!(f, "invalid escape sequence"),
            Error::InvalidNumber => write!(f, "invalid number"),
            Error::EmptyParens => write!(f, "empty parentheses"),
            Error::EmptyAggregation => write!(f, "aggregation requires argument list"),
            Error::UnknownFunction(s) => write!(f, "unknown function: {}", s),
            Error::UnknownAggregation(s) => write!(f, "unknown aggregation: {}", s),
            Error::Expected(s) => write!(f, "expected {}", s),
        }
    }
}

pub fn parse(input: &str) -> Result<Expr, Error> {
    let mut p = Parser::new(input);
    let e = p.parse_expr_bp(0)?;
    p.skip_ws();
    if p.pos < p.src.len() {
        return Err(Error::UnexpectedChar(p.src.as_bytes()[p.pos] as char));
    }
    Ok(e)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s)
    }

    fn eat_str(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn eat_char(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    /// Pratt-style expression parser. `min_bp` is the minimum binding power
    /// required to keep consuming.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, Error> {
        let mut lhs = self.parse_atom()?;
        loop {
            self.skip_ws();
            let Some(c) = self.peek() else { break };
            // Range vector suffix attaches to atoms only.
            if let Expr::Metric(m) = &mut lhs {
                if m.range.is_none() && c == '[' {
                    self.eat_char('[');
                    self.skip_ws();
                    let start = self.pos;
                    while let Some(ch) = self.peek() {
                        if ch.is_alphanumeric() || ch == '_' {
                            self.pos += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                    if start == self.pos {
                        return Err(Error::Expected("duration"));
                    }
                    let dur = self.src[start..self.pos].to_string();
                    self.skip_ws();
                    if !self.eat_char(']') {
                        return Err(Error::Expected("]"));
                    }
                    m.range = Some(dur);
                    continue;
                }
            }
            // Binary operators.
            let Some((op, bp, postfix)) = self.try_infix_op(c) else {
                break;
            };
            if bp < min_bp {
                break;
            }
            // Consume operator.
            let op_str_len = match op {
                BinOp::Add => {
                    self.pos += 1;
                    1
                }
                BinOp::Sub => {
                    self.pos += 1;
                    1
                }
                BinOp::Mul => {
                    self.pos += 1;
                    1
                }
                BinOp::Div => {
                    self.pos += 1;
                    1
                }
                BinOp::Mod => {
                    self.pos += 1;
                    1
                }
                BinOp::Pow => {
                    self.pos += 1;
                    1
                }
                BinOp::Eq => {
                    self.pos += 2;
                    2
                }
                BinOp::Ne => {
                    self.pos += 2;
                    2
                }
                BinOp::Gt => {
                    self.pos += 1;
                    1
                }
                BinOp::Ge => {
                    self.pos += 2;
                    2
                }
                BinOp::Lt => {
                    self.pos += 1;
                    1
                }
                BinOp::Le => {
                    self.pos += 2;
                    2
                }
                BinOp::Re => {
                    self.pos += 2;
                    2
                }
                BinOp::Nre => {
                    self.pos += 2;
                    2
                }
            };
            // `== bool` / `!= bool` / `> bool` modifier.
            self.skip_ws();
            let mut bool_modifier = false;
            if matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Gt | BinOp::Ge | BinOp::Lt | BinOp::Le
            ) && self.eat_str("bool")
            {
                self.skip_ws();
                bool_modifier = true;
            }
            let next_bp = if postfix { bp } else { bp + 1 };
            let rhs = self.parse_expr_bp(next_bp)?;
            lhs = Expr::Binary(Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                bool_modifier,
            });
            // Suppress the unused assignment.
            let _ = op_str_len;
        }
        Ok(lhs)
    }

    /// Return (op, binding power, postfix) if `c` starts an operator here.
    fn try_infix_op(&self, c: char) -> Option<(BinOp, u8, bool)> {
        // Lower bp → looser binding. Comparison loosest; power tightest.
        match c {
            '+' | '-' => Some((if c == '+' { BinOp::Add } else { BinOp::Sub }, 3, false)),
            '*' | '/' | '%' => Some((
                match c {
                    '*' => BinOp::Mul,
                    '/' => BinOp::Div,
                    _ => BinOp::Mod,
                },
                4,
                false,
            )),
            '^' => Some((BinOp::Pow, 6, false)),
            '=' => {
                if self.src[self.pos..].starts_with("==") {
                    Some((BinOp::Eq, 1, false))
                } else {
                    None
                }
            }
            '!' => {
                if self.src[self.pos..].starts_with("!=") {
                    Some((BinOp::Ne, 1, false))
                } else if self.src[self.pos..].starts_with("!~") {
                    Some((BinOp::Nre, 1, false))
                } else {
                    None
                }
            }
            '>' => {
                if self.src[self.pos..].starts_with(">=") {
                    Some((BinOp::Ge, 1, false))
                } else {
                    Some((BinOp::Gt, 1, false))
                }
            }
            '<' => {
                if self.src[self.pos..].starts_with("<=") {
                    Some((BinOp::Le, 1, false))
                } else {
                    Some((BinOp::Lt, 1, false))
                }
            }
            '~' => {
                // =~ only valid in label matchers; binary comparison reuses it
                // (PromQL has no `~` binary op, so refuse).
                if self.src[self.pos..].starts_with("=~") {
                    Some((BinOp::Re, 1, false))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, Error> {
        self.skip_ws();
        let Some(c) = self.peek() else {
            return Err(Error::UnexpectedEof);
        };
        // Parenthesised expression.
        if c == '(' {
            self.pos += 1;
            self.skip_ws();
            // Empty parens are illegal.
            if self.eat_char(')') {
                return Err(Error::EmptyParens);
            }
            let inner = self.parse_expr_bp(0)?;
            self.skip_ws();
            if !self.eat_char(')') {
                return Err(Error::Expected(")"));
            }
            return Ok(inner);
        }
        // Identifier: metric or aggregation or function call.
        if is_ident_start(c) {
            let name = self.read_ident()?;
            self.skip_ws();
            // Aggregation without leading paren: `agg by (label) (...)`.
            if AggOp::from_str(&name).is_some()
                && (self.starts_with("by") || self.starts_with("without"))
            {
                return self.parse_aggregation(name);
            }
            // Function call: `name(args)` where args is comma-separated exprs.
            if self.eat_char('(') {
                return self.parse_call_or_aggregate(name);
            }
            // Otherwise: a bare metric, possibly followed by `{...}` and `[...]`.
            let matchers = self.try_parse_matchers()?;
            let range = None; // range is filled by the outer loop
            return Ok(Expr::Metric(Metric {
                name,
                matchers,
                range,
            }));
        }
        // Numeric scalar literal — pass through as Metric with name `_scalar`? No,
        // a real AST would have a `Number` node; for our minimal parser we just
        // surface a synthetic metric that the caller can recognise. Simpler: accept
        // digits/sign and fold them into a Metric with the original literal text.
        if c.is_ascii_digit() || c == '-' || c == '+' {
            // Roll forward past the scalar; treat it as a Metric with no matchers
            // so the parser accepts `rate(metric[5m]) > 0.5`.
            let start = self.pos;
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit()
                    || ch == '.'
                    || ch == 'e'
                    || ch == 'E'
                    || ch == '-'
                    || ch == '+'
                {
                    self.pos += ch.len_utf8();
                } else {
                    break;
                }
            }
            if start == self.pos {
                return Err(Error::InvalidNumber);
            }
            return Ok(Expr::Metric(Metric {
                name: self.src[start..self.pos].to_string(),
                matchers: vec![],
                range: None,
            }));
        }
        // String literal as scalar.
        if c == '"' || c == '\'' {
            let s = self.read_string()?;
            return Ok(Expr::Metric(Metric {
                name: format!("{:?}", s),
                matchers: vec![],
                range: None,
            }));
        }
        Err(Error::UnexpectedChar(c))
    }

    /// Parses `agg by (...) (...)` form (no leading paren).
    fn parse_aggregation(&mut self, name: String) -> Result<Expr, Error> {
        let agg = AggOp::from_str(&name).ok_or_else(|| Error::UnknownAggregation(name.clone()))?;
        // Consume `by` or `without`.
        let _is_by = self.eat_str("by") || self.eat_str("without");
        self.skip_ws();
        if !self.eat_char('(') {
            return Err(Error::Expected("("));
        }
        let by = self.read_label_list()?;
        self.skip_ws();
        if !self.eat_char(')') {
            return Err(Error::Expected(")"));
        }
        self.skip_ws();
        if !self.eat_char('(') {
            return Err(Error::Expected("("));
        }
        self.skip_ws();
        if self.eat_char(')') {
            return Err(Error::EmptyAggregation);
        }
        let expr = self.parse_expr_bp(0)?;
        self.skip_ws();
        if !self.eat_char(')') {
            return Err(Error::Expected(")"));
        }
        Ok(Expr::Aggregate(Aggregate {
            op: agg,
            by,
            expr: Box::new(expr),
        }))
    }

    /// Parses a call or aggregation: after `name(`, decides between
    /// `name(args)` (regular function) and `agg by (labels) (expr)`.
    fn parse_call_or_aggregate(&mut self, name: String) -> Result<Expr, Error> {
        self.skip_ws();
        if self.eat_char(')') {
            return Err(Error::EmptyParens);
        }
        // Detect aggregation: `agg by (label)` or `agg without (label)`.
        if let Some(agg) = AggOp::from_str(&name) {
            // Optional `by (labels)` immediately after the opening `(`.
            let mut by = Vec::new();
            if self.eat_str("by") || self.eat_str("without") {
                self.skip_ws();
                if !self.eat_char('(') {
                    return Err(Error::Expected("("));
                }
                by = self.read_label_list()?;
                self.skip_ws();
                if !self.eat_char(')') {
                    return Err(Error::Expected(")"));
                }
                self.skip_ws();
                // After `by (label)` we expect another `(expr)` paren.
                if !self.eat_char('(') {
                    return Err(Error::Expected("("));
                }
            }
            // We already consumed the outer `(`. Inner expression starts at
            // current pos; we close on the matching `)`.
            self.skip_ws();
            if self.eat_char(')') {
                return Err(Error::EmptyAggregation);
            }
            let expr = self.parse_expr_bp(0)?;
            self.skip_ws();
            if !self.eat_char(')') {
                return Err(Error::Expected(")"));
            }
            return Ok(Expr::Aggregate(Aggregate {
                op: agg,
                by,
                expr: Box::new(expr),
            }));
        }
        // Regular function call: comma-separated exprs.
        let mut args = Vec::new();
        loop {
            self.skip_ws();
            if self.eat_char(')') {
                break;
            }
            let e = self.parse_expr_bp(0)?;
            args.push(e);
            self.skip_ws();
            if !self.eat_char(',') {
                self.skip_ws();
                if !self.eat_char(')') {
                    return Err(Error::Expected(", or )"));
                }
                break;
            }
        }
        if args.is_empty() {
            // Allowed for some functions, but not common — accept silently.
        }
        Ok(Expr::Call(Call { func: name, args }))
    }

    fn read_ident(&mut self) -> Result<String, Error> {
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Err(Error::UnexpectedEof);
        };
        if !is_ident_start(c) {
            return Err(Error::UnexpectedChar(c));
        }
        self.pos += c.len_utf8();
        while let Some(c) = self.peek() {
            if is_ident_cont(c) {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        Ok(self.src[start..self.pos].to_string())
    }

    fn read_string(&mut self) -> Result<String, Error> {
        let quote = self.peek().ok_or(Error::UnexpectedEof)?;
        if quote != '"' && quote != '\'' {
            return Err(Error::UnexpectedChar(quote));
        }
        self.pos += quote.len_utf8();
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == quote {
                self.pos += c.len_utf8();
                return Ok(out);
            }
            if c == '\\' {
                self.pos += 1;
                let Some(n) = self.peek() else {
                    return Err(Error::InvalidEscape);
                };
                match n {
                    '"' | '\'' | '\\' => {
                        out.push(n);
                        self.pos += n.len_utf8();
                    }
                    'n' => {
                        out.push('\n');
                        self.pos += 1;
                    }
                    't' => {
                        out.push('\t');
                        self.pos += 1;
                    }
                    _ => return Err(Error::InvalidEscape),
                }
            } else {
                out.push(c);
                self.pos += c.len_utf8();
            }
        }
        Err(Error::UnexpectedEof)
    }

    fn try_parse_matchers(&mut self) -> Result<Vec<LabelMatcher>, Error> {
        self.skip_ws();
        if !self.eat_char('{') {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.eat_char('}') {
                break;
            }
            let name = self.read_ident()?;
            self.skip_ws();
            let op = if self.eat_str("=~") {
                MatchOp::Re
            } else if self.eat_str("!~") {
                MatchOp::Nre
            } else if self.eat_str("!=") {
                MatchOp::Ne
            } else if self.eat_char('=') {
                MatchOp::Eq
            } else {
                return Err(Error::Expected("matcher operator"));
            };
            self.skip_ws();
            let value = self.read_string()?;
            out.push(LabelMatcher { name, op, value });
            self.skip_ws();
            if self.eat_char(',') {
                continue;
            }
            if self.eat_char('}') {
                break;
            }
            return Err(Error::Expected(", or }"));
        }
        Ok(out)
    }

    fn read_label_list(&mut self) -> Result<Vec<String>, Error> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.peek().is_none() {
                return Err(Error::UnexpectedEof);
            }
            let name = self.read_ident()?;
            out.push(name);
            self.skip_ws();
            if self.eat_char(',') {
                continue;
            }
            break;
        }
        Ok(out)
    }
}

impl AggOp {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "sum" => Some(AggOp::Sum),
            "avg" => Some(AggOp::Avg),
            "min" => Some(AggOp::Min),
            "max" => Some(AggOp::Max),
            "count" => Some(AggOp::Count),
            _ => None,
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c == ':' || c.is_ascii_alphabetic()
}

fn is_ident_cont(c: char) -> bool {
    c == '_' || c == ':' || c.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(s: &str) -> Expr {
        parse(s).unwrap_or_else(|e| panic!("parse({:?}) failed: {}", s, e))
    }

    #[test]
    fn simple_metric() {
        let e = parse_ok("up");
        match e {
            Expr::Metric(m) => {
                assert_eq!(m.name, "up");
                assert!(m.matchers.is_empty());
                assert!(m.range.is_none());
            }
            other => panic!("expected Metric got {:?}", other),
        }
    }

    #[test]
    fn label_matchers() {
        let e = parse_ok(r#"http_requests_total{job="api", code!="500"}"#);
        match e {
            Expr::Metric(m) => {
                assert_eq!(m.name, "http_requests_total");
                assert_eq!(m.matchers.len(), 2);
                assert_eq!(m.matchers[0].name, "job");
                assert_eq!(m.matchers[0].op, MatchOp::Eq);
                assert_eq!(m.matchers[0].value, "api");
                assert_eq!(m.matchers[1].op, MatchOp::Ne);
                assert_eq!(m.matchers[1].value, "500");
            }
            other => panic!("expected Metric got {:?}", other),
        }
    }

    #[test]
    fn regex_matcher() {
        let e = parse_ok(r#"x{env=~"prod.*"}"#);
        match e {
            Expr::Metric(m) => {
                assert_eq!(m.matchers[0].op, MatchOp::Re);
                assert_eq!(m.matchers[0].value, "prod.*");
            }
            other => panic!("expected Metric got {:?}", other),
        }
    }

    #[test]
    fn range_vector_syntax() {
        let e = parse_ok("rate(http_requests_total[5m])");
        match e {
            Expr::Call(c) => {
                assert_eq!(c.func, "rate");
                assert_eq!(c.args.len(), 1);
                match &c.args[0] {
                    Expr::Metric(m) => {
                        assert_eq!(m.name, "http_requests_total");
                        assert_eq!(m.range.as_deref(), Some("5m"));
                    }
                    other => panic!("expected Metric got {:?}", other),
                }
            }
            other => panic!("expected Call got {:?}", other),
        }
    }

    #[test]
    fn binary_comparison_with_scalar() {
        let e = parse_ok("up == 1");
        match e {
            Expr::Binary(b) => {
                assert_eq!(b.op, BinOp::Eq);
                assert!(!b.bool_modifier);
                assert!(matches!(*b.lhs, Expr::Metric(_)));
                assert!(matches!(*b.rhs, Expr::Metric(_)));
            }
            other => panic!("expected Binary got {:?}", other),
        }
    }

    #[test]
    fn binary_arithmetic_precedence() {
        let e = parse_ok("a + b * c");
        // `*` binds tighter → (a + (b * c))
        match e {
            Expr::Binary(b) => {
                assert_eq!(b.op, BinOp::Add);
                assert!(matches!(*b.lhs, Expr::Metric(ref m) if m.name == "a"));
                match *b.rhs {
                    Expr::Binary(b2) => {
                        assert_eq!(b2.op, BinOp::Mul);
                    }
                    other => panic!("expected nested Binary got {:?}", other),
                }
            }
            other => panic!("expected Binary got {:?}", other),
        }
    }

    #[test]
    fn aggregation_by_label() {
        let e = parse_ok("sum by (job) (rate(http_requests_total[5m]))");
        match e {
            Expr::Aggregate(a) => {
                assert_eq!(a.op, AggOp::Sum);
                assert_eq!(a.by, vec!["job".to_string()]);
                match *a.expr {
                    Expr::Call(ref c) => {
                        assert_eq!(c.func, "rate");
                    }
                    other => panic!("expected Call got {:?}", other),
                }
            }
            other => panic!("expected Aggregate got {:?}", other),
        }
    }

    #[test]
    fn aggregation_min_max_count_avg() {
        for (s, expected) in [
            ("avg(up)", AggOp::Avg),
            ("min(up)", AggOp::Min),
            ("max(up)", AggOp::Max),
            ("count(up)", AggOp::Count),
        ] {
            let e = parse_ok(s);
            match e {
                Expr::Aggregate(a) => assert_eq!(a.op, expected),
                other => panic!("expected Aggregate for {} got {:?}", s, other),
            }
        }
    }

    #[test]
    fn display_does_not_panic() {
        let e = parse_ok(r#"sum by (job) (rate(http_requests_total{code=~"5.."}[5m])) > 100"#);
        let _ = format!("{}", e);
    }
}
