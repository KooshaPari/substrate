//! Semantic Versioning 2.0.0 comparator and parser.
//!
//! Parses version strings of the form `MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`
//! and provides comparison via the [`Ord`] impl. Pre-release identifiers are
//! compared per SemVer §11: shorter wins when all preceding identifiers are
//! equal, numeric identifiers sort lower than non-numeric, and missing
//! pre-release sorts higher than present (1.0.0 > 1.0.0-alpha).
//!
//! Build metadata is ignored for ordering (SemVer §10).
//!
//! Reference: <https://semver.org/spec/v2.0.0.html>

use std::cmp::Ordering;

/// A single identifier in the pre-release or build section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ident {
    /// Numeric identifier (must be a non-empty sequence of ASCII digits with
    /// no leading zeros, except "0" itself).
    Number(u64),
    /// Alphanumeric identifier — letters, digits, hyphens; non-empty.
    Text(String),
}

impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Ident::Number(a), Ident::Number(b)) => a.cmp(b),
            (Ident::Number(_), Ident::Text(_)) => Ordering::Less,
            (Ident::Text(_), Ident::Number(_)) => Ordering::Greater,
            (Ident::Text(a), Ident::Text(b)) => a.cmp(b),
        }
    }
}

/// A parsed semantic version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Pre-release identifiers (without the leading `-`).
    pub pre: Vec<Ident>,
    /// Build metadata (without the leading `+`).
    pub build: Vec<String>,
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        // 1) Major/minor/patch numeric comparison.
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            // 2) Pre-release: shorter wins (when shared prefix is equal);
            //    fewer pre-release ids > more pre-release ids.
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater, // no pre > some pre
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
        // Build metadata is intentionally ignored (§10).
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemVerError {
    Empty,
    BadFormat(String),
    BadIdent(String),
    LeadingZero(String),
}

impl std::fmt::Display for SemVerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SemVerError::Empty => write!(f, "empty version"),
            SemVerError::BadFormat(s) => write!(f, "bad semver format: {s:?}"),
            SemVerError::BadIdent(s) => write!(f, "bad identifier: {s:?}"),
            SemVerError::LeadingZero(s) => write!(f, "leading zero in numeric id: {s:?}"),
        }
    }
}

impl std::str::FromStr for SemVer {
    type Err = SemVerError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse(s)
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            let parts: Vec<String> = self.pre.iter().map(|i| match i {
                Ident::Number(n) => n.to_string(),
                Ident::Text(t) => t.clone(),
            }).collect();
            write!(f, "-{}", parts.join("."))?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build.join("."))?;
        }
        Ok(())
    }
}

/// Parse a SemVer string.
pub fn parse(input: &str) -> Result<SemVer, SemVerError> {
    if input.is_empty() {
        return Err(SemVerError::Empty);
    }
    let (version_part, build) = match input.split_once('+') {
        Some((v, b)) => (v, b),
        None => (input, ""),
    };
    // Treat a trailing "-" with nothing after as an error.
    if version_part.ends_with('-') {
        return Err(SemVerError::BadIdent(String::new()));
    }
    let (core, pre_str) = match version_part.split_once('-') {
        Some((c, p)) => (c, p),
        None => (version_part, ""),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(SemVerError::BadFormat(input.into()));
    }
    let major = parse_u64(parts[0])?;
    let minor = parse_u64(parts[1])?;
    let patch = parse_u64(parts[2])?;
    let pre = parse_idents(pre_str)?;
    let build = if build.is_empty() {
        Vec::new()
    } else {
        build
            .split('.')
            .map(|s| s.to_string())
            .collect()
    };
    Ok(SemVer {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn parse_u64(s: &str) -> Result<u64, SemVerError> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
        return Err(SemVerError::BadFormat(s.into()));
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(SemVerError::LeadingZero(s.into()));
    }
    s.parse::<u64>().map_err(|_| SemVerError::BadFormat(s.into()))
}

fn parse_idents(s: &str) -> Result<Vec<Ident>, SemVerError> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split('.')
        .map(|part| {
            if part.is_empty() {
                return Err(SemVerError::BadIdent(part.into()));
            }
            if !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err(SemVerError::BadIdent(part.into()));
            }
            if part.chars().all(|c| c.is_ascii_digit()) {
                // Numeric id: must be non-empty digits with no leading zero (unless "0").
                if part.len() > 1 && part.starts_with('0') {
                    return Err(SemVerError::LeadingZero(part.into()));
                }
                let n: u64 = part.parse().map_err(|_| SemVerError::BadIdent(part.into()))?;
                Ok(Ident::Number(n))
            } else {
                Ok(Ident::Text(part.to_string()))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> SemVer {
        s.parse().unwrap()
    }

    #[test]
    fn parse_basic() {
        let v = p("1.2.3");
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_empty());
    }

    #[test]
    fn parse_with_prerelease() {
        let v = p("1.0.0-alpha");
        assert_eq!(v.pre, vec![Ident::Text("alpha".into())]);
    }

    #[test]
    fn parse_with_numeric_prerelease() {
        let v = p("1.0.0-0.3.7");
        assert_eq!(v.pre.len(), 3);
        assert_eq!(v.pre[0], Ident::Number(0));
        assert_eq!(v.pre[1], Ident::Number(3));
        assert_eq!(v.pre[2], Ident::Number(7));
    }

    #[test]
    fn parse_with_build() {
        let v = p("1.0.0+20130313144700");
        assert_eq!(v.build, vec!["20130313144700".to_string()]);
    }

    #[test]
    fn parse_prerelease_and_build() {
        let v = p("1.0.0-beta+exp.sha.5114f85");
        assert_eq!(v.pre, vec![Ident::Text("beta".into())]);
        assert_eq!(
            v.build,
            vec!["exp".to_string(), "sha".to_string(), "5114f85".to_string()]
        );
    }

    #[test]
    fn order_basic() {
        assert!(p("1.0.0") < p("2.0.0"));
        assert!(p("2.0.0") < p("2.1.0"));
        assert!(p("2.1.0") < p("2.1.1"));
        assert!(p("1.0.0") > p("1.0.0-alpha"));
    }

    #[test]
    fn prerelease_per_semver() {
        // Per SemVer §11: 1.0.0-alpha < 1.0.0-alpha.1 < 1.0.0-alpha.beta < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0-beta.11 < 1.0.0-rc.1 < 1.0.0
        let v = vec![
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        let parsed: Vec<SemVer> = v.iter().map(|s| s.parse().unwrap()).collect();
        let mut sorted = parsed.clone();
        sorted.sort();
        assert_eq!(parsed, sorted);
    }

    #[test]
    fn numeric_less_than_alphanumeric() {
        // Per SemVer: numeric identifiers always have lower precedence than
        // alphanumeric identifiers. So 1.0.0-1 < 1.0.0-alpha.
        assert!(p("1.0.0-1") < p("1.0.0-alpha"));
    }

    #[test]
    fn equal_versions() {
        assert_eq!(p("1.2.3").cmp(&p("1.2.3")), Ordering::Equal);
    }

    #[test]
    fn build_metadata_ignored() {
        // Per SemVer §10: 1.0.0+abc == 1.0.0+def for ordering.
        assert_eq!(p("1.0.0+abc").cmp(&p("1.0.0+def")), Ordering::Equal);
    }

    #[test]
    fn rejects_empty() {
        assert!(parse("").is_err());
    }

    #[test]
    fn rejects_two_segments() {
        assert!(parse("1.2").is_err());
    }

    #[test]
    fn rejects_leading_zero() {
        assert!(parse("01.0.0").is_err());
        assert!(parse("1.0.0-01").is_err());
    }

    #[test]
    fn rejects_empty_identifier() {
        assert!(parse("1.0.0-").is_err());
        assert!(parse("1.0.0-a..b").is_err());
    }

    #[test]
    fn rejects_bad_chars() {
        assert!(parse("1.0.0-α").is_err()); // non-ASCII
    }

    #[test]
    fn display_round_trip() {
        let v = p("1.0.0-rc.1+build.42");
        assert_eq!(v.to_string(), "1.0.0-rc.1+build.42");
    }
}