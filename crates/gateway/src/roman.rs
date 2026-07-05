pub fn to_roman(mut n: u32) -> Option<String> {
    if n == 0 || n >= 4000 { return None; }
    let pairs = [(1000,"M"),(900,"CM"),(500,"D"),(400,"CD"),(100,"C"),(90,"XC"),(50,"L"),(40,"XL"),(10,"X"),(9,"IX"),(5,"V"),(4,"IV"),(1,"I")];
    let mut out = String::new();
    for (val, sym) in pairs.iter() { while n >= *val { out.push_str(sym); n -= *val; } }
    Some(out)
}
pub fn from_roman(s: &str) -> Option<u32> {
    let map = |c: char| match c { 'I'=>1,'V'=>5,'X'=>10,'L'=>50,'C'=>100,'D'=>500,'M'=>1000, _ => return None };
    let chars: Vec<char> = s.chars().collect();
    let mut total = 0u32;
    for i in 0..chars.len() {
        let v = map(chars[i])?;
        if i + 1 < chars.len() { let n = map(chars[i+1])?; if n > v { total -= v; continue; } }
        total += v;
    }
    Some(total)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn to_roman_basic() { assert_eq!(to_roman(1), Some("I".into())); assert_eq!(to_roman(4), Some("IV".into())); assert_eq!(to_roman(9), Some("IX".into())); }
    #[test] fn to_roman_complex() { assert_eq!(to_roman(1994), Some("MCMXCIV".into())); assert_eq!(to_roman(3888), Some("MMMDCCCLXXXVIII".into())); }
    #[test] fn to_roman_invalid() { assert_eq!(to_roman(0), None); assert_eq!(to_roman(4000), None); }
    #[test] fn from_roman_basic() { assert_eq!(from_roman("I"), Some(1)); assert_eq!(from_roman("IV"), Some(4)); assert_eq!(from_roman("IX"), Some(9)); }
    #[test] fn from_roman_complex() { assert_eq!(from_roman("MCMXCIV"), Some(1994)); }
    #[test] fn from_roman_invalid() { assert_eq!(from_roman("Z"), None); }
}
