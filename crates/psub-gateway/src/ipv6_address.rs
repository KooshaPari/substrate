//! IPv6 address representation + scope/category classification.
//!
//! A minimal IPv6 address type that stores the 16-byte address, plus
//! classification helpers for the standard IPv6 address categories
//! (loopback, unspecified, link-local, unique-local, multicast). Text
//! parsing/formatting covers the canonical `x:x:x:x:x:x:x:x` form and
//! the `::` shorthand for runs of zero groups.

use std::fmt;

/// 128-bit IPv6 address, in network byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ipv6Address(pub [u8; 16]);

/// IPv6 address category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ipv6Category {
    Unspecified,
    Loopback,
    LinkLocal,
    UniqueLocal,
    Multicast,
    GlobalUnicast,
}

impl Ipv6Address {
    /// All-zeros address `::` / `::0`.
    pub const UNSPECIFIED: Ipv6Address = Ipv6Address([0u8; 16]);
    /// Loopback `::1`.
    pub const LOOPBACK: Ipv6Address = Ipv6Address([
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    ]);

    /// Classify the address.
    pub fn category(&self) -> Ipv6Category {
        if self.0 == Self::UNSPECIFIED.0 {
            return Ipv6Category::Unspecified;
        }
        if self.0 == Self::LOOPBACK.0 {
            return Ipv6Category::Loopback;
        }
        if self.0[0] == 0xfe && (self.0[1] & 0xc0) == 0x80 {
            return Ipv6Category::LinkLocal;
        }
        if (self.0[0] & 0xfe) == 0xfc {
            return Ipv6Category::UniqueLocal;
        }
        if self.0[0] == 0xff {
            return Ipv6Category::Multicast;
        }
        Ipv6Category::GlobalUnicast
    }

    /// True if this is a link-local address (fe80::/10).
    pub fn is_link_local(&self) -> bool {
        self.category() == Ipv6Category::LinkLocal
    }

    /// True if this is a unique-local address (fc00::/7).
    pub fn is_unique_local(&self) -> bool {
        self.category() == Ipv6Category::UniqueLocal
    }

    /// True if this is any kind of multicast address (ff00::/8).
    pub fn is_multicast(&self) -> bool {
        self.category() == Ipv6Category::Multicast
    }

    /// Parse a canonical IPv6 address string. Supports the `::` shorthand
    /// for runs of zero groups, and the embedded IPv4 suffix form
    /// (`::ffff:192.0.2.1`) is intentionally NOT supported (callers
    /// should normalise that form before calling).
    pub fn parse(s: &str) -> Option<Ipv6Address> {
        let mut bytes = [0u8; 16];
        if s == "::" {
            return Some(Ipv6Address(bytes));
        }
        // Detect `::` (which may appear once, anywhere — start, end, or middle)
        let (left, right) = if let Some(pos) = s.find("::") {
            let (l, r) = s.split_at(pos);
            // Skip the `::` itself
            let r = &r[2..];
            (l, r)
        } else {
            (s, "")
        };
        let left_groups: Vec<&str> = if left.is_empty() {
            Vec::new()
        } else {
            left.split(':').collect()
        };
        let right_groups: Vec<&str> = if right.is_empty() {
            Vec::new()
        } else {
            right.split(':').collect()
        };
        if left_groups.len() + right_groups.len() > 8 {
            return None;
        }
        let mut cursor = 0;
        for g in &left_groups {
            if cursor + 2 > 16 {
                return None;
            }
            let parsed = u16::from_str_radix(g, 16).ok()?;
            bytes[cursor] = (parsed >> 8) as u8;
            bytes[cursor + 1] = parsed as u8;
            cursor += 2;
        }
        // Skip the `::` zero groups
        cursor += 2 * (8 - left_groups.len() - right_groups.len());
        for g in &right_groups {
            if cursor + 2 > 16 {
                return None;
            }
            let parsed = u16::from_str_radix(g, 16).ok()?;
            bytes[cursor] = (parsed >> 8) as u8;
            bytes[cursor + 1] = parsed as u8;
            cursor += 2;
        }
        Some(Ipv6Address(bytes))
    }

    /// Format in canonical `x:x:x:x:x:x:x:x` (no `::` shorthand).
    pub fn to_canonical(&self) -> String {
        let mut out = String::with_capacity(8 * 5 - 1);
        for i in 0..8 {
            if i > 0 {
                out.push(':');
            }
            let hi = self.0[i * 2] as u16;
            let lo = self.0[i * 2 + 1] as u16;
            out.push_str(&format!("{:x}", (hi << 8) | lo));
        }
        out
    }
}

impl fmt::Display for Ipv6Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_canonical())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_category() {
        assert_eq!(Ipv6Address::UNSPECIFIED.category(), Ipv6Category::Unspecified);
    }

    #[test]
    fn loopback_category() {
        assert_eq!(Ipv6Address::LOOPBACK.category(), Ipv6Category::Loopback);
        assert!(Ipv6Address::LOOPBACK.is_link_local() == false);
    }

    #[test]
    fn link_local_category() {
        let addr = Ipv6Address([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(addr.category(), Ipv6Category::LinkLocal);
        assert!(addr.is_link_local());
    }

    #[test]
    fn unique_local_category() {
        let addr = Ipv6Address([0xfd, 0x12, 0x34, 0x56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(addr.category(), Ipv6Category::UniqueLocal);
        assert!(addr.is_unique_local());
    }

    #[test]
    fn multicast_category() {
        let addr = Ipv6Address([0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(addr.category(), Ipv6Category::Multicast);
        assert!(addr.is_multicast());
    }

    #[test]
    fn global_unicast_category() {
        let addr = Ipv6Address([0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(addr.category(), Ipv6Category::GlobalUnicast);
    }

    #[test]
    fn parse_canonical_form() {
        let addr = Ipv6Address::parse("2001:db8::1").unwrap();
        assert_eq!(addr.0[0], 0x20);
        assert_eq!(addr.0[1], 0x01);
        assert_eq!(addr.0[2], 0x0d);
        assert_eq!(addr.0[3], 0xb8);
        // Trailing zero groups (from `::` shorthand) are NOT auto-filled;
        // bytes 4..14 stay 0, and byte 15 = 1
        assert_eq!(addr.0[15], 0x01);
    }

    #[test]
    fn parse_unspecified() {
        let addr = Ipv6Address::parse("::").unwrap();
        assert_eq!(addr, Ipv6Address::UNSPECIFIED);
    }

    #[test]
    fn parse_loopback() {
        let addr = Ipv6Address::parse("::1").unwrap();
        assert_eq!(addr, Ipv6Address::LOOPBACK);
    }

    #[test]
    fn parse_too_many_groups_errors() {
        // 9 groups is invalid
        assert!(Ipv6Address::parse("1:2:3:4:5:6:7:8:9").is_none());
    }

    #[test]
    fn parse_invalid_hex_errors() {
        assert!(Ipv6Address::parse("xyz").is_none());
    }

    #[test]
    fn canonical_format() {
        let addr = Ipv6Address::LOOPBACK;
        assert_eq!(addr.to_canonical(), "0:0:0:0:0:0:0:1");
    }
}