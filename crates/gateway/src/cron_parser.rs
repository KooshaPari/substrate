#[derive(Debug, PartialEq, Clone)]
pub enum Field {
    All,
    Specific(u32),
    Range(u32, u32),
    Step(u32, u32),
    List(Vec<u32>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct Cron {
    pub minute: Field,
    pub hour: Field,
    pub day: Field,
    pub month: Field,
    pub weekday: Field,
}

pub fn parse(expr: &str) -> Result<Cron, String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(format!("expected 5 fields, got {}", parts.len()));
    }
    Ok(Cron {
        minute: parse_field(parts[0], 0, 59)?,
        hour: parse_field(parts[1], 0, 23)?,
        day: parse_field(parts[2], 1, 31)?,
        month: parse_field(parts[3], 1, 12)?,
        weekday: parse_field(parts[4], 0, 6)?,
    })
}

pub fn parse_field(s: &str, lo: u32, hi: u32) -> Result<Field, String> {
    if s == "*" {
        return Ok(Field::All);
    }
    if let Some(rest) = s.strip_prefix("*/") {
        let step: u32 = rest.parse().map_err(|_| format!("bad step: {}", s))?;
        if step == 0 {
            return Err("step must be > 0".into());
        }
        return Ok(Field::Step(lo, step));
    }
    if s.contains(',') {
        let mut out = Vec::new();
        for p in s.split(',') {
            match parse_field(p, lo, hi)? {
                Field::Specific(n) => out.push(n),
                Field::Range(a, b) => {
                    for v in a..=b {
                        out.push(v);
                    }
                }
                Field::Step(a, step) => {
                    let mut v = a;
                    while v <= hi {
                        out.push(v);
                        v += step;
                    }
                }
                _ => return Err(format!("unsupported list item: {}", p)),
            }
        }
        return Ok(Field::List(out));
    }
    if let Some(dash) = s.find('-') {
        let a: u32 = s[..dash]
            .parse()
            .map_err(|_| format!("bad range start: {}", s))?;
        let b: u32 = s[dash + 1..]
            .parse()
            .map_err(|_| format!("bad range end: {}", s))?;
        if a < lo || b > hi || a > b {
            return Err(format!("range out of bounds: {}", s));
        }
        return Ok(Field::Range(a, b));
    }
    let n: u32 = s.parse().map_err(|_| format!("bad number: {}", s))?;
    if n < lo || n > hi {
        return Err(format!("out of bounds: {}", n));
    }
    Ok(Field::Specific(n))
}

pub fn matches(c: &Cron, ts: (u32, u32, u32, u32, u32)) -> bool {
    let (m, h, d, mo, w) = ts;
    in_field(&c.minute, m)
        && in_field(&c.hour, h)
        && in_field(&c.day, d)
        && in_field(&c.month, mo)
        && in_field(&c.weekday, w)
}

fn in_field(f: &Field, v: u32) -> bool {
    match f {
        Field::All => true,
        Field::Specific(n) => *n == v,
        Field::Range(a, b) => v >= *a && v <= *b,
        Field::Step(lo, step) => v >= *lo && (v - lo) % step == 0,
        Field::List(l) => l.contains(&v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_5_fields() {
        assert!(parse("0 0 * * *").is_ok());
        assert!(parse("* * * * * *").is_err());
    }
    #[test]
    fn parse_specific() {
        let c = parse("5 12 1 6 0").unwrap();
        assert_eq!(c.minute, Field::Specific(5));
    }
    #[test]
    fn parse_range() {
        let c = parse("0 9-17 * * *").unwrap();
        assert_eq!(c.hour, Field::Range(9, 17));
    }
    #[test]
    fn parse_step() {
        let c = parse("*/15 * * * *").unwrap();
        assert_eq!(c.minute, Field::Step(0, 15));
    }
    #[test]
    fn parse_list() {
        let c = parse("0,15,30,45 * * * *").unwrap();
        assert_eq!(c.minute, Field::List(vec![0, 15, 30, 45]));
    }
    #[test]
    fn parse_out_of_bounds() {
        assert!(parse("60 * * * *").is_err());
        assert!(parse("* 25 * * *").is_err());
    }
    #[test]
    fn matches_all() {
        let c = parse("* * * * *").unwrap();
        assert!(matches(&c, (5, 5, 5, 5, 5)));
    }
    #[test]
    fn matches_specific() {
        let c = parse("0 12 * * *").unwrap();
        assert!(matches(&c, (0, 12, 15, 6, 1)));
        assert!(!matches(&c, (1, 12, 15, 6, 1)));
    }
    #[test]
    fn matches_step() {
        let c = parse("*/15 * * * *").unwrap();
        assert!(matches(&c, (0, 5, 1, 1, 0)));
        assert!(matches(&c, (15, 5, 1, 1, 0)));
        assert!(matches(&c, (30, 5, 1, 1, 0)));
        assert!(!matches(&c, (7, 5, 1, 1, 0)));
    }
}
