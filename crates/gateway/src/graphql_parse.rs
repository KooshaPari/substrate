// Minimal GraphQL query AST parser. Supports queries and mutations with field
// selections, nested selection sets, arguments, aliases, variables, fragments, and
// inline values (ints, floats, strings, booleans, null, enums, lists, objects).
//
// Tokenizer is hand-rolled (no logos/crane/etc.) to keep the dep tree minimal.

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Enum(String),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct Field {
    pub alias: Option<String>,
    pub name: String,
    pub args: Vec<(String, Value)>,
    pub selection: Option<Vec<Selection>>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Selection {
    Field(Field),
    FragmentSpread(String),
    InlineFragment {
        on_type: Option<String>,
        selection: Vec<Selection>,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub struct VarDef {
    pub name: String,
    pub ty: String,
    pub default: Option<Value>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Operation {
    pub kind: OperationKind,
    pub name: Option<String>,
    pub vars: Vec<VarDef>,
    pub selection: Vec<Selection>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Fragment {
    pub name: String,
    pub on_type: String,
    pub selection: Vec<Selection>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Definition {
    Operation(Operation),
    Fragment(Fragment),
}

pub fn parse(input: &str) -> Result<Vec<Definition>, String> {
    let mut p = Parser::new(input);
    let mut defs = Vec::new();
    p.skip_ws_and_comments();
    while !p.eof() {
        defs.push(p.parse_definition()?);
        p.skip_ws_and_comments();
    }
    Ok(defs)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}
impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self { src: s, pos: 0 }
    }
    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }
    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn skip_ws_and_comments(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else if c == '#' {
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.advance();
                }
            } else {
                break;
            }
        }
    }
    fn expect(&mut self, ch: char) -> Result<(), String> {
        self.skip_ws_and_comments();
        if self.peek() == Some(ch) {
            self.advance();
            Ok(())
        } else {
            Err(format!("expected '{}', got {:?}", ch, self.peek()))
        }
    }
    fn expect_keyword(&mut self, kw: &str) -> Result<(), String> {
        self.skip_ws_and_comments();
        if self.src[self.pos..].starts_with(kw) {
            let after = self.pos + kw.len();
            if after >= self.src.len() || !self.src.as_bytes()[after].is_ascii_alphanumeric() {
                self.pos = after;
                return Ok(());
            }
        }
        Err(format!("expected keyword '{}'", kw))
    }
    fn parse_identifier(&mut self) -> Result<String, String> {
        self.skip_ws_and_comments();
        let before_loop = self.pos;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(format!(
                "expected identifier pos={} (before_loop={}) rest={:?}",
                self.pos,
                before_loop,
                &self.src[before_loop..self.src.len().min(before_loop + 10)]
            ));
        }
        Ok(self.src[start..self.pos].to_string())
    }
    fn parse_definition(&mut self) -> Result<Definition, String> {
        self.skip_ws_and_comments();
        if self.try_keyword("fragment") {
            let name = self.parse_identifier()?;
            self.expect_keyword("on")?;
            let on_type = self.parse_identifier()?;
            self.expect('{')?;
            let selection = self.parse_selection_set()?;
            self.expect('}')?;
            Ok(Definition::Fragment(Fragment {
                name,
                on_type,
                selection,
            }))
        } else if self.peek_keyword("query")
            || self.peek_keyword("mutation")
            || self.peek_keyword("subscription")
        {
            self.parse_operation().map(Definition::Operation)
        } else if self.peek() == Some('{') {
            self.advance();
            let selection = self.parse_selection_set()?;
            self.expect('}')?;
            Ok(Definition::Operation(Operation {
                kind: OperationKind::Query,
                name: None,
                vars: Vec::new(),
                selection,
            }))
        } else {
            Err(format!("expected operation type at pos={}", self.pos))
        }
    }
    fn peek_keyword(&self, kw: &str) -> bool {
        if !self.src[self.pos..].starts_with(kw) {
            return false;
        }
        let after = self.pos + kw.len();
        after >= self.src.len()
            || matches!(
                self.src.as_bytes()[after],
                b' ' | b'\t' | b'\n' | b'\r' | b'(' | b'{' | b',' | b'}'
            )
    }
    fn parse_operation(&mut self) -> Result<Operation, String> {
        let kind = if self.try_keyword("query") {
            OperationKind::Query
        } else if self.try_keyword("mutation") {
            OperationKind::Mutation
        } else if self.try_keyword("subscription") {
            OperationKind::Subscription
        } else {
            return Err("expected operation type".into());
        };
        self.skip_ws_and_comments();
        let name = if self.peek() == Some('{') || self.peek() == Some('(') {
            None
        } else {
            Some(self.parse_identifier()?)
        };
        let vars = if self.peek() == Some('(') {
            self.parse_var_defs()?
        } else {
            Vec::new()
        };
        self.expect('{')?;
        let selection = self.parse_selection_set()?;
        self.expect('}')?;
        Ok(Operation {
            kind,
            name,
            vars,
            selection,
        })
    }
    fn try_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws_and_comments();
        if !self.src[self.pos..].starts_with(kw) {
            return false;
        }
        let after = self.pos + kw.len();
        let boundary_ok = after >= self.src.len()
            || matches!(
                self.src.as_bytes()[after],
                b' ' | b'\t' | b'\n' | b'\r' | b'(' | b'{' | b',' | b'}'
            );
        if boundary_ok {
            self.pos = after;
            true
        } else {
            false
        }
    }
    fn parse_var_defs(&mut self) -> Result<Vec<VarDef>, String> {
        self.expect('(')?;
        let mut out = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek() == Some(')') {
                self.advance();
                return Ok(out);
            }
            if self.advance() != Some('$') {
                return Err(format!("expected $ at pos={}", self.pos));
            }
            let name = self.parse_identifier()?;
            self.expect(':')?;
            let ty = self.parse_type_ref()?;
            let default = if self.peek() == Some('=') {
                self.advance();
                Some(self.parse_value()?)
            } else {
                None
            };
            out.push(VarDef { name, ty, default });
        }
    }
    fn parse_type_ref(&mut self) -> Result<String, String> {
        let name = self.parse_identifier()?;
        if self.peek() == Some('!') {
            self.advance();
            Ok(name + "!")
        } else {
            Ok(name)
        }
    }
    fn parse_selection_set(&mut self) -> Result<Vec<Selection>, String> {
        let mut out = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.peek() {
                Some('}') | None => return Ok(out),
                _ => out.push(self.parse_selection()?),
            }
        }
    }
    fn parse_selection(&mut self) -> Result<Selection, String> {
        self.skip_ws_and_comments();
        if self.src[self.pos..].starts_with("...") {
            self.pos += 3;
            self.skip_ws_and_comments();
            if self.peek() == Some('o') && self.src[self.pos..].starts_with("on ") {
                self.pos += 3;
                let on_type = self.parse_identifier()?;
                self.expect('{')?;
                let sel = self.parse_selection_set()?;
                self.expect('}')?;
                return Ok(Selection::InlineFragment {
                    on_type: Some(on_type),
                    selection: sel,
                });
            }
            let name = self.parse_identifier()?;
            return Ok(Selection::FragmentSpread(name));
        }
        let mut alias: Option<String> = None;
        let mut first = self.parse_identifier()?;
        self.skip_ws_and_comments();
        if self.peek() == Some(':') {
            self.advance();
            alias = Some(first);
            first = self.parse_identifier()?;
            self.skip_ws_and_comments();
        }
        let args = if self.peek() == Some('(') {
            self.parse_args()?
        } else {
            Vec::new()
        };
        self.skip_ws_and_comments();
        let selection = if self.peek() == Some('{') {
            self.advance();
            let s = self.parse_selection_set()?;
            self.expect('}')?;
            Some(s)
        } else {
            None
        };
        Ok(Selection::Field(Field {
            alias,
            name: first,
            args,
            selection,
        }))
    }
    fn parse_args(&mut self) -> Result<Vec<(String, Value)>, String> {
        self.expect('(')?;
        let mut out = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek() == Some(')') {
                self.advance();
                return Ok(out);
            }
            let name = self.parse_identifier()?;
            self.expect(':')?;
            let v = self.parse_value()?;
            out.push((name, v));
        }
    }
    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_ws_and_comments();
        let c = self.peek().ok_or("expected value")?;
        match c {
            '$' => {
                self.advance();
                let name = self.parse_identifier()?;
                Ok(Value::String(format!("${}", name)))
            }
            '[' => {
                self.advance();
                let mut items = Vec::new();
                loop {
                    self.skip_ws_and_comments();
                    if self.peek() == Some(']') {
                        self.advance();
                        return Ok(Value::List(items));
                    }
                    items.push(self.parse_value()?);
                    self.skip_ws_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                    }
                }
            }
            '{' => {
                self.advance();
                let mut entries = Vec::new();
                loop {
                    self.skip_ws_and_comments();
                    if self.peek() == Some('}') {
                        self.advance();
                        return Ok(Value::Object(entries));
                    }
                    let k = self.parse_identifier()?;
                    self.expect(':');
                    let v = self.parse_value()?;
                    entries.push((k, v));
                    self.skip_ws_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                    }
                }
            }
            '"' => Ok(Value::String(self.parse_string()?)),
            '-' | '0'..='9' => self.parse_number(),
            't' | 'f' => {
                if self.src[self.pos..].starts_with("true") {
                    self.pos += 4;
                    Ok(Value::Bool(true))
                } else if self.src[self.pos..].starts_with("false") {
                    self.pos += 5;
                    Ok(Value::Bool(false))
                } else {
                    Err("expected true/false".into())
                }
            }
            'n' => {
                if self.src[self.pos..].starts_with("null") {
                    self.pos += 4;
                    Ok(Value::Null)
                } else {
                    Err("expected null".into())
                }
            }
            _ => {
                let name = self.parse_identifier()?;
                Ok(Value::Enum(name))
            }
        }
    }
    fn parse_string(&mut self) -> Result<String, String> {
        if self.advance() != Some('"') {
            return Err("expected '\"'".into());
        }
        let mut out = String::new();
        loop {
            let c = self.advance().ok_or("unterminated string")?;
            if c == '"' {
                return Ok(out);
            }
            if c == '\\' {
                let esc = self.advance().ok_or("bad escape")?;
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'u' => {
                        let hex: String = (0..4).map(|_| self.advance().unwrap_or('\0')).collect();
                        let cp = u32::from_str_radix(&hex, 16).map_err(|_| "bad unicode")?;
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                    }
                    _ => return Err(format!("bad escape \\{}", esc)),
                }
            } else {
                out.push(c);
            }
        }
    }
    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.advance();
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        let mut is_float = false;
        if self.peek() == Some('.') {
            is_float = true;
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            is_float = true;
            self.advance();
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.advance();
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let s = &self.src[start..self.pos];
        if is_float {
            s.parse::<f64>()
                .map(Value::Float)
                .map_err(|e| e.to_string())
        } else {
            s.parse::<i64>().map(Value::Int).map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simple_query() {
        let defs = parse("query { user { name } }").unwrap();
        assert_eq!(defs.len(), 1);
        if let Definition::Operation(op) = &defs[0] {
            assert_eq!(op.kind, OperationKind::Query);
            assert_eq!(op.selection.len(), 1);
        } else {
            panic!();
        }
    }
    #[test]
    fn mutation_with_vars() {
        let defs =
            parse("mutation AddPost($title: String!) { addPost(title: $title) { id } }").unwrap();
        if let Definition::Operation(op) = &defs[0] {
            assert_eq!(op.kind, OperationKind::Mutation);
            assert_eq!(op.vars.len(), 1);
            assert_eq!(op.vars[0].name, "title");
            assert_eq!(op.vars[0].ty, "String!");
        } else {
            panic!();
        }
    }
    #[test]
    fn aliased_field() {
        let defs = parse("{ a: user { name } }").unwrap();
        if let Definition::Operation(op) = &defs[0] {
            if let Selection::Field(f) = &op.selection[0] {
                assert_eq!(f.alias.as_deref(), Some("a"));
                assert_eq!(f.name, "user");
            } else {
                panic!();
            }
        } else {
            panic!();
        }
    }
    #[test]
    fn args_object_value() {
        let defs =
            parse("{ user(filter: { active: true, tags: [\"a\", \"b\"] }) { id } }").unwrap();
        if let Definition::Operation(op) = &defs[0] {
            if let Selection::Field(f) = &op.selection[0] {
                assert_eq!(f.args[0].0, "filter");
                if let Value::Object(entries) = &f.args[0].1 {
                    assert_eq!(entries.len(), 2);
                } else {
                    panic!();
                }
            } else {
                panic!();
            }
        } else {
            panic!();
        }
    }
    #[test]
    fn fragment_spread() {
        let defs =
            parse("query { user { ...UserFields } } fragment UserFields on User { id name }")
                .unwrap();
        assert_eq!(defs.len(), 2);
        if let Definition::Operation(op) = &defs[0] {
            if let Selection::Field(f) = &op.selection[0] {
                if let Selection::FragmentSpread(name) = &f.selection.as_ref().unwrap()[0] {
                    assert_eq!(name, "UserFields");
                } else {
                    panic!();
                }
            } else {
                panic!();
            }
        }
    }
    #[test]
    fn inline_fragment() {
        let defs = parse("query { ... on User { id } }").unwrap();
        if let Definition::Operation(op) = &defs[0] {
            if let Selection::InlineFragment { on_type, .. } = &op.selection[0] {
                assert_eq!(on_type.as_deref(), Some("User"));
            } else {
                panic!();
            }
        } else {
            panic!();
        }
    }
    #[test]
    fn values_string_escapes() {
        let v = Parser::new(r#""a\nbA""#).parse_value().unwrap();
        assert_eq!(v, Value::String("a\nbA".into()));
    }
    #[test]
    fn negative_number() {
        let v = Parser::new("-42").parse_value().unwrap();
        assert_eq!(v, Value::Int(-42));
    }
    #[test]
    fn comment_skipped() {
        let defs = parse("# comment\nquery { x }").unwrap();
        assert_eq!(defs.len(), 1);
    }
    #[test]
    fn multi_def() {
        let defs = parse("query A { x } query B { y }").unwrap();
        assert_eq!(defs.len(), 2);
    }
}
