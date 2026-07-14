#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub fn parse(s: &str) -> Option<SemVer> {
    let v = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].split('-').next().unwrap().parse().ok()?;
    Some(SemVer {
        major,
        minor,
        patch,
    })
}

pub fn is_compatible(a: &SemVer, b: &SemVer) -> bool {
    a.major == b.major
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_basic() {
        assert_eq!(
            parse("1.2.3"),
            Some(SemVer {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
    }
    #[test]
    fn parse_v_prefix() {
        assert_eq!(
            parse("v2.0.1"),
            Some(SemVer {
                major: 2,
                minor: 0,
                patch: 1
            })
        );
    }
    #[test]
    fn parse_invalid() {
        assert_eq!(parse("1.2"), None);
        assert_eq!(parse("abc"), None);
    }
    #[test]
    fn order() {
        assert!(
            SemVer {
                major: 1,
                minor: 0,
                patch: 0
            } < SemVer {
                major: 1,
                minor: 1,
                patch: 0
            }
        );
    }
    #[test]
    fn compat() {
        assert!(is_compatible(
            &SemVer {
                major: 1,
                minor: 2,
                patch: 3
            },
            &SemVer {
                major: 1,
                minor: 5,
                patch: 0
            }
        ));
        assert!(!is_compatible(
            &SemVer {
                major: 2,
                minor: 0,
                patch: 0
            },
            &SemVer {
                major: 1,
                minor: 9,
                patch: 9
            }
        ));
    }
}
