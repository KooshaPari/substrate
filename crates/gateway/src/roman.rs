pub fn to_roman(mut n: u32) -> Option<String> {
    if n == 0 || n >= 4000 { return None; }
    let pairs = [(1000,"M"),(900,"CM"),(500,"D"),(400,"CD"),(100,"C"),(90,"XC"),(50,"L"),(40,"XL"),(10,"X"),(9,"IX"),(5,"V"),(4,"IV"),(1,"I")];
    let mut out = String::new();
    for (val, sym) in pairs.iter() { while n >= *val { out.push_str(sym); n -= *val; } }
    Some(out)
}
pub fn from_roman(s: &str) -> Option<u32> {
    let map = |c: char| -> Option<u32> { match c { 'I'=>Some(1),'V'=>Some(5),'X'=>Some(10),'L'=>Some(50),'C'=>Some(100),'D'=>Some(500),'M'=>Some(1000), _ => None } };
    let chars: Vec<char> = s.chars().collect();
    let mut total: i32 = 0;
    for i in 0..chars.len() {
        let v = map(chars[i])? as i32;
        let next = if i + 1 < chars.len() { map(chars[i+1])? as i32 } else { 0 };
        if next > v { total -= v; } else { total += v; }
    }
    if total < 0 { None } else { Some(total as u32) }
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
