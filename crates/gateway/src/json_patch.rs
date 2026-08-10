//! RFC 6902 JSON Patch operations.
//!
//! Reference: <https://datatracker.ietf.org/doc/html/rfc6902>
//! JSON Pointers (RFC 6901): <https://datatracker.ietf.org/doc/html/rfc6901>
//!
//! Six operations are supported: add, remove, replace, move, copy, test.
//! Operations are applied in order to the document.

use std::fmt;

/// A JSON-like value tree used by the patcher.
///
/// Kept intentionally small so the patch engine stays dependency-free and the
/// type mirrors the shape of `serde_json::Value` closely enough for the
/// operations we care about.
#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsValue>),
    Object(Vec<(String, JsValue)>),
}

impl JsValue {
    pub fn null() -> JsValue {
        JsValue::Null
    }

    pub fn from_bool(b: bool) -> JsValue {
        JsValue::Bool(b)
    }

    pub fn from_number(n: f64) -> JsValue {
        JsValue::Number(n)
    }

    pub fn from_string(s: impl Into<String>) -> JsValue {
        JsValue::String(s.into())
    }

    pub fn from_array(items: Vec<JsValue>) -> JsValue {
        JsValue::Array(items)
    }

    pub fn from_object(entries: Vec<(String, JsValue)>) -> JsValue {
        JsValue::Object(entries)
    }
}

impl fmt::Display for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsValue::Null => write!(f, "null"),
            JsValue::Bool(b) => write!(f, "{}", b),
            JsValue::Number(n) => write!(f, "{}", n),
            JsValue::String(s) => write!(f, "\"{}\"", escape_str(s)),
            JsValue::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            JsValue::Object(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "\"{}\":{}", escape_str(k), v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            _ => out.push(c),
        }
    }
    out
}

/// A JSON Pointer reference (RFC 6901).
///
/// Stored as the raw pointer string, e.g. `"/foo/bar/0"`. The empty string
/// represents the document root.
pub type OpPointer = String;

/// One RFC 6902 patch operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Patch {
    Add(OpPointer, JsValue),
    Remove(OpPointer),
    Replace(OpPointer, JsValue),
    Move(OpPointer, OpPointer),
    Copy(OpPointer, OpPointer),
    Test(OpPointer, JsValue),
}

/// Apply a sequence of patch operations in place. Returns Err if any
/// operation fails (pointer resolution, type mismatch, test mismatch, etc.).
pub fn apply(doc: &mut JsValue, patches: &[Patch]) -> Result<(), String> {
    for patch in patches {
        match patch {
            Patch::Add(ptr, value) => add_at(doc, ptr, value.clone())?,
            Patch::Remove(ptr) => remove_at(doc, ptr)?,
            Patch::Replace(ptr, value) => replace_at(doc, ptr, value.clone())?,
            Patch::Move(from, to) => move_at(doc, from, to)?,
            Patch::Copy(from, to) => copy_at(doc, from, to)?,
            Patch::Test(ptr, value) => {
                let actual = resolve(doc, ptr)?;
                if actual != value {
                    return Err(format!(
                        "test failed at {}: expected {}, got {}",
                        ptr, value, actual
                    ));
                }
            }
        }
    }
    Ok(())
}

fn parse_pointer(ptr: &str) -> Result<Vec<String>, String> {
    if ptr.is_empty() {
        return Ok(Vec::new());
    }
    if !ptr.starts_with('/') {
        return Err(format!(
            "pointer must start with '/' or be empty: '{}'",
            ptr
        ));
    }
    let parts: Vec<&str> = ptr.split('/').collect();
    let mut tokens = Vec::with_capacity(parts.len() - 1);
    for raw in &parts[1..] {
        let unescaped = raw.replace("~1", "/").replace("~0", "~");
        tokens.push(unescaped);
    }
    Ok(tokens)
}

fn resolve<'a>(doc: &'a JsValue, ptr: &str) -> Result<&'a JsValue, String> {
    let tokens = parse_pointer(ptr)?;
    let mut current = doc;
    for token in &tokens {
        current = step(current, token)?;
    }
    Ok(current)
}

fn resolve_mut<'a>(doc: &'a mut JsValue, ptr: &str) -> Result<ResolvedMut<'a>, String> {
    let tokens = parse_pointer(ptr)?;
    if tokens.is_empty() {
        return Ok(ResolvedMut::Root(doc));
    }
    let (last, parents) = tokens.split_last().unwrap();
    let mut current = doc;
    for token in parents {
        current = step_mut(current, token)?;
    }
    match current {
        JsValue::Array(items) => {
            let idx = parse_index(last, items.len())?;
            Ok(ResolvedMut::ArrayIndex(items, idx))
        }
        JsValue::Object(entries) => {
            if let Some(pos) = entries.iter().position(|(k, _)| k == last) {
                Ok(ResolvedMut::ObjectKey(entries, pos))
            } else {
                Err(format!("object key '{}' not found", last))
            }
        }
        _ => Err(format!("cannot index into non-container at '{}'", last)),
    }
}

enum ResolvedMut<'a> {
    Root(&'a mut JsValue),
    ArrayIndex(&'a mut Vec<JsValue>, usize),
    ObjectKey(&'a mut Vec<(String, JsValue)>, usize),
}

fn step<'a>(value: &'a JsValue, token: &str) -> Result<&'a JsValue, String> {
    match value {
        JsValue::Array(items) => {
            let idx = parse_index(token, items.len())?;
            items
                .get(idx)
                .ok_or_else(|| format!("array index {} out of bounds (len={})", idx, items.len()))
        }
        JsValue::Object(entries) => entries
            .iter()
            .find(|(k, _)| k == token)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("object key '{}' not found", token)),
        _ => Err(format!("cannot index into non-container with '{}'", token)),
    }
}

fn step_mut<'a>(value: &'a mut JsValue, token: &str) -> Result<&'a mut JsValue, String> {
    match value {
        JsValue::Array(items) => {
            let idx = parse_index(token, items.len())?;
            let len = items.len();
            items
                .get_mut(idx)
                .ok_or_else(|| format!("array index {} out of bounds (len={})", idx, len))
        }
        JsValue::Object(entries) => {
            let pos = entries
                .iter()
                .position(|(k, _)| k == token)
                .ok_or_else(|| format!("object key '{}' not found", token))?;
            Ok(&mut entries[pos].1)
        }
        _ => Err(format!("cannot index into non-container with '{}'", token)),
    }
}

fn parse_index(token: &str, len: usize) -> Result<usize, String> {
    if token == "-" {
        return Ok(len);
    }
    if token.starts_with('+') {
        return Err(format!("array index may not start with '+': '{}'", token));
    }
    let n: usize = token
        .parse()
        .map_err(|_| format!("invalid array index: '{}'", token))?;
    if n >= len && len > 0 && token != "-" {
        // Per RFC 6901, "01" is not equal to "1"; we just accept any parsed usize.
    }
    Ok(n)
}

fn add_at(doc: &mut JsValue, ptr: &str, value: JsValue) -> Result<(), String> {
    let tokens = parse_pointer(ptr)?;
    if tokens.is_empty() {
        *doc = value;
        return Ok(());
    }
    let (last, parents) = tokens.split_last().unwrap();
    let mut current = doc;
    for token in parents {
        current = step_mut(current, token)?;
    }
    match current {
        JsValue::Array(items) => {
            let idx = parse_index(last, items.len())?;
            if idx > items.len() {
                return Err(format!(
                    "array index {} out of bounds (len={})",
                    idx,
                    items.len()
                ));
            }
            items.insert(idx, value);
            Ok(())
        }
        JsValue::Object(entries) => {
            if let Some(pos) = entries.iter().position(|(k, _)| k == last) {
                entries[pos].1 = value;
            } else {
                entries.push((last.to_string(), value));
            }
            Ok(())
        }
        _ => Err(format!("cannot add into non-container at '{}'", last)),
    }
}

fn remove_at(doc: &mut JsValue, ptr: &str) -> Result<(), String> {
    let target = resolve_mut(doc, ptr)?;
    match target {
        ResolvedMut::Root(_) => Err("cannot remove document root".to_string()),
        ResolvedMut::ArrayIndex(items, idx) => {
            if idx >= items.len() {
                return Err(format!(
                    "array index {} out of bounds (len={})",
                    idx,
                    items.len()
                ));
            }
            items.remove(idx);
            Ok(())
        }
        ResolvedMut::ObjectKey(entries, pos) => {
            entries.remove(pos);
            Ok(())
        }
    }
}

fn replace_at(doc: &mut JsValue, ptr: &str, value: JsValue) -> Result<(), String> {
    let target = resolve_mut(doc, ptr)?;
    match target {
        ResolvedMut::Root(slot) => {
            *slot = value;
            Ok(())
        }
        ResolvedMut::ArrayIndex(items, idx) => {
            if idx >= items.len() {
                return Err(format!(
                    "array index {} out of bounds (len={})",
                    idx,
                    items.len()
                ));
            }
            items[idx] = value;
            Ok(())
        }
        ResolvedMut::ObjectKey(entries, pos) => {
            entries[pos].1 = value;
            Ok(())
        }
    }
}

fn move_at(doc: &mut JsValue, from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    let value = {
        let target = resolve_mut(doc, from)?;
        match target {
            ResolvedMut::Root(_) => return Err("cannot move document root".to_string()),
            ResolvedMut::ArrayIndex(items, idx) => {
                if idx >= items.len() {
                    return Err(format!(
                        "move source array index {} out of bounds (len={})",
                        idx,
                        items.len()
                    ));
                }
                items.remove(idx)
            }
            ResolvedMut::ObjectKey(entries, pos) => entries.remove(pos).1,
        }
    };
    add_at(doc, to, value)
}

fn copy_at(doc: &mut JsValue, from: &str, to: &str) -> Result<(), String> {
    let value = resolve(doc, from)?.clone();
    add_at(doc, to, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(entries: Vec<(&str, JsValue)>) -> JsValue {
        JsValue::Object(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    fn arr(items: Vec<JsValue>) -> JsValue {
        JsValue::Array(items)
    }

    #[test]
    fn add_to_object() {
        let mut doc = obj(vec![("a", JsValue::from_number(1.0))]);
        apply(
            &mut doc,
            &[Patch::Add("/b".to_string(), JsValue::from_number(2.0))],
        )
        .unwrap();
        assert_eq!(
            doc,
            obj(vec![
                ("a", JsValue::from_number(1.0)),
                ("b", JsValue::from_number(2.0))
            ])
        );
    }

    #[test]
    fn add_to_array_at_index() {
        let mut doc = arr(vec![JsValue::from_number(1.0), JsValue::from_number(3.0)]);
        apply(
            &mut doc,
            &[Patch::Add("/1".to_string(), JsValue::from_number(2.0))],
        )
        .unwrap();
        assert_eq!(
            doc,
            arr(vec![
                JsValue::from_number(1.0),
                JsValue::from_number(2.0),
                JsValue::from_number(3.0)
            ])
        );
    }

    #[test]
    fn remove_path() {
        let mut doc = obj(vec![
            ("a", JsValue::from_number(1.0)),
            ("b", JsValue::from_number(2.0)),
        ]);
        apply(&mut doc, &[Patch::Remove("/a".to_string())]).unwrap();
        assert_eq!(doc, obj(vec![("b", JsValue::from_number(2.0))]));
    }

    #[test]
    fn replace_value() {
        let mut doc = obj(vec![
            ("name", JsValue::from_string("alice")),
            ("age", JsValue::from_number(30.0)),
        ]);
        apply(
            &mut doc,
            &[Patch::Replace(
                "/name".to_string(),
                JsValue::from_string("bob"),
            )],
        )
        .unwrap();
        assert_eq!(
            doc,
            obj(vec![
                ("name", JsValue::from_string("bob")),
                ("age", JsValue::from_number(30.0))
            ])
        );
    }

    #[test]
    fn test_pass() {
        let mut doc = obj(vec![("a", JsValue::from_number(1.0))]);
        apply(
            &mut doc,
            &[Patch::Test("/a".to_string(), JsValue::from_number(1.0))],
        )
        .unwrap();
    }

    #[test]
    fn test_fail() {
        let mut doc = obj(vec![("a", JsValue::from_number(1.0))]);
        let res = apply(
            &mut doc,
            &[Patch::Test("/a".to_string(), JsValue::from_number(2.0))],
        );
        assert!(res.is_err());
    }

    #[test]
    fn move_semantics() {
        let mut doc = obj(vec![
            ("a", JsValue::from_number(1.0)),
            ("b", JsValue::from_number(2.0)),
        ]);
        apply(&mut doc, &[Patch::Move("/a".to_string(), "/c".to_string())]).unwrap();
        assert_eq!(
            doc,
            obj(vec![
                ("b", JsValue::from_number(2.0)),
                ("c", JsValue::from_number(1.0))
            ])
        );
    }

    #[test]
    fn copy_semantics() {
        let mut doc = obj(vec![
            ("a", JsValue::from_number(1.0)),
            ("b", JsValue::from_number(2.0)),
        ]);
        apply(&mut doc, &[Patch::Copy("/a".to_string(), "/c".to_string())]).unwrap();
        assert_eq!(
            doc,
            obj(vec![
                ("a", JsValue::from_number(1.0)),
                ("b", JsValue::from_number(2.0)),
                ("c", JsValue::from_number(1.0))
            ])
        );
    }

    #[test]
    fn add_to_nested_path() {
        let mut doc = obj(vec![(
            "outer",
            obj(vec![("inner", JsValue::from_number(0.0))]),
        )]);
        apply(
            &mut doc,
            &[Patch::Add(
                "/outer/inner".to_string(),
                JsValue::from_number(42.0),
            )],
        )
        .unwrap();
        let expected = obj(vec![(
            "outer",
            obj(vec![("inner", JsValue::from_number(42.0))]),
        )]);
        assert_eq!(doc, expected);
    }

    #[test]
    fn pointer_with_escaped_slash() {
        let mut doc = obj(vec![("a/b", JsValue::from_number(1.0))]);
        apply(
            &mut doc,
            &[Patch::Replace(
                "/a~1b".to_string(),
                JsValue::from_number(2.0),
            )],
        )
        .unwrap();
        let expected = obj(vec![("a/b", JsValue::from_number(2.0))]);
        assert_eq!(doc, expected);
    }
}
