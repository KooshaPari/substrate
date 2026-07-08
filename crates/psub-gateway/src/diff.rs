pub enum DiffOp<'a> { Equal(&'a str), Delete(&'a str), Insert(&'a str) }
impl<'a> DiffOp<'a> {
    pub fn is_change(&self) -> bool { matches!(self, Self::Delete(_) | Self::Insert(_)) }
    pub fn as_str(&self) -> &'a str { match self { Self::Equal(s) | Self::Delete(s) | Self::Insert(s) => s } }
}

pub fn diff_lines<'a>(a: &'a str, b: &'a str) -> Vec<DiffOp<'a>> {
    let a_lines: Vec<&'a str> = a.lines().collect();
    let b_lines: Vec<&'a str> = b.lines().collect();
    let mut ops: Vec<DiffOp<'a>> = Vec::new();
    let mut i = 0; let mut j = 0;
    while i < a_lines.len() && j < b_lines.len() {
        if a_lines[i] == b_lines[j] { ops.push(DiffOp::Equal(a_lines[i])); i += 1; j += 1; }
        else { ops.push(DiffOp::Delete(a_lines[i])); ops.push(DiffOp::Insert(b_lines[j])); i += 1; j += 1; }
    }
    while i < a_lines.len() { ops.push(DiffOp::Delete(a_lines[i])); i += 1; }
    while j < b_lines.len() { ops.push(DiffOp::Insert(b_lines[j])); j += 1; }
    ops
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn equal_lines() { let ops = diff_lines("a\nb\nc", "a\nb\nc"); assert_eq!(ops.len(), 3); }
    #[test] fn one_change() { let ops = diff_lines("a\nb\nc", "a\nX\nc"); assert!(ops.iter().any(|o| o.is_change())); }
    #[test] fn additions() { let ops = diff_lines("a", "a\nb"); assert!(ops.iter().any(|o| matches!(o, DiffOp::Insert(_)))); }
    #[test] fn deletions() { let ops = diff_lines("a\nb", "a"); assert!(ops.iter().any(|o| matches!(o, DiffOp::Delete(_)))); }
    #[test] fn as_str() { let ops = diff_lines("a", "b"); let s: Vec<&str> = ops.iter().map(|o| o.as_str()).collect(); assert!(s.contains(&"a")); assert!(s.contains(&"b")); }
}
