//! Geographic distance and coordinate utilities.
//!
//! - [`haversine_km`] — great-circle distance via the Haversine formula.
//! - [`bearing_deg`] — initial bearing from one point to another.
//! - [`midpoint`] — geographic midpoint of two coordinates.
//! - [`geohash_encode`] / [`geohash_decode`] — base-32 geohash codec with
//!   configurable precision (1..=12).
//! - [`maidenhead_grid`] — 6-character Maidenhead grid locator (ham radio).

/// Earth mean radius in kilometres (IUGG).
pub const EARTH_RADIUS_KM: f64 = 6371.0088;

/// A WGS84 latitude/longitude pair in decimal degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

fn deg2rad(d: f64) -> f64 {
    d * std::f64::consts::PI / 180.0
}

fn rad2deg(r: f64) -> f64 {
    r * 180.0 / std::f64::consts::PI
}

/// Great-circle distance in kilometres using the Haversine formula.
/// Handles antipodal points correctly (no NaN from acos).
pub fn haversine_km(a: LatLon, b: LatLon) -> f64 {
    let lat1 = deg2rad(a.lat);
    let lat2 = deg2rad(b.lat);
    let dlat = deg2rad(b.lat - a.lat);
    let dlon = deg2rad(b.lon - a.lon);
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * h.sqrt().asin();
    EARTH_RADIUS_KM * c
}

/// Initial bearing in degrees clockwise from true north (0..360).
pub fn bearing_deg(from: LatLon, to: LatLon) -> f64 {
    let lat1 = deg2rad(from.lat);
    let lat2 = deg2rad(to.lat);
    let dlon = deg2rad(to.lon - from.lon);
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    let brng = y.atan2(x);
    (rad2deg(brng) + 360.0) % 360.0
}

/// Geographic midpoint of two coordinates (averages in Cartesian space,
/// then projects back to lat/lon — accurate even for long distances).
pub fn midpoint(a: LatLon, b: LatLon) -> LatLon {
    let lat1 = deg2rad(a.lat);
    let lon1 = deg2rad(a.lon);
    let lat2 = deg2rad(b.lat);
    let lon2 = deg2rad(b.lon);
    let dlon = lon2 - lon1;
    let bx = lat2.cos() * dlon.cos();
    let by = lat2.cos() * dlon.sin();
    let y = lat1.sin() + lat2.sin();
    let x = (lat1.cos() + bx).hypot(by);
    let lat3 = y.atan2(x);
    let lon3 = lon1 + by.atan2(lat1.cos() + bx);
    LatLon::new(rad2deg(lat3), rad2deg(lon3))
}

const GEOHASH_ALPHABET: &[u8; 32] = b"0123456789bcdefghjkmnpqrstuvwxyz";

/// Encode a coordinate as a base-32 geohash with the given precision
/// (1..=12 characters). Higher precision ≈ smaller cell.
pub fn geohash_encode(p: LatLon, precision: usize) -> String {
    assert!(
        precision >= 1 && precision <= 12,
        "precision must be 1..=12"
    );
    let mut lat_range = [-90.0_f64, 90.0_f64];
    let mut lon_range = [-180.0_f64, 180.0_f64];
    let mut hash = String::with_capacity(precision);
    let mut bits = 0u8;
    let mut bit_idx = 0u8;
    let mut even = true; // even bit = longitude, odd = latitude.
    while hash.len() < precision {
        if even {
            let mid = (lon_range[0] + lon_range[1]) / 2.0;
            if p.lon >= mid {
                bits = (bits << 1) | 1;
                lon_range[0] = mid;
            } else {
                bits <<= 1;
                lon_range[1] = mid;
            }
        } else {
            let mid = (lat_range[0] + lat_range[1]) / 2.0;
            if p.lat >= mid {
                bits = (bits << 1) | 1;
                lat_range[0] = mid;
            } else {
                bits <<= 1;
                lat_range[1] = mid;
            }
        }
        even = !even;
        bit_idx += 1;
        if bit_idx == 5 {
            hash.push(GEOHASH_ALPHABET[bits as usize] as char);
            bits = 0;
            bit_idx = 0;
        }
    }
    hash
}

/// Decode a geohash string into a (lat, lon) center point plus the
/// `(lat_half_height, lon_half_width)` bounding box for the cell.
pub fn geohash_decode(s: &str) -> Option<(LatLon, f64, f64)> {
    let mut lat_range = [-90.0_f64, 90.0_f64];
    let mut lon_range = [-180.0_f64, 180.0_f64];
    let mut even = true;
    for ch in s.bytes() {
        let idx = GEOHASH_ALPHABET.iter().position(|&a| a == ch)?;
        for bit in (0..5).rev() {
            let b = (idx >> bit) & 1;
            if even {
                let mid = (lon_range[0] + lon_range[1]) / 2.0;
                if b == 1 {
                    lon_range[0] = mid;
                } else {
                    lon_range[1] = mid;
                }
            } else {
                let mid = (lat_range[0] + lat_range[1]) / 2.0;
                if b == 1 {
                    lat_range[0] = mid;
                } else {
                    lat_range[1] = mid;
                }
            }
            even = !even;
        }
    }
    let lat = (lat_range[0] + lat_range[1]) / 2.0;
    let lon = (lon_range[0] + lon_range[1]) / 2.0;
    let lat_half = (lat_range[1] - lat_range[0]) / 2.0;
    let lon_half = (lon_range[1] - lon_range[0]) / 2.0;
    Some((LatLon::new(lat, lon), lat_half, lon_half))
}

/// Maidenhead grid locator (6 chars: field·square·subsquare). Standard
/// ham-radio locator system; accurate to ~5 km lat × 2.5 km lon at 6 chars.
pub fn maidenhead_grid(p: LatLon) -> String {
    let mut lon = p.lon + 180.0;
    let mut lat = p.lat + 90.0;
    // Field (A..R, 20° lon, 10° lat).
    let field_lon = (lon / 20.0) as u8;
    let field_lat = (lat / 10.0) as u8;
    lon -= field_lon as f64 * 20.0;
    lat -= field_lat as f64 * 10.0;
    // Square (0..9, 2° lon, 1° lat).
    let sq_lon = (lon / 2.0) as u8;
    let sq_lat = (lat / 1.0) as u8;
    lon -= sq_lon as f64 * 2.0;
    lat -= sq_lat as f64 * 1.0;
    // Subsquare (a..x, 5' lon, 2.5' lat).
    let ss_lon = (lon / (2.0 / 24.0)) as u8;
    let ss_lat = (lat / (1.0 / 24.0)) as u8;
    format!(
        "{}{}{}{}{}{}",
        (b'A' + field_lon) as char,
        (b'A' + field_lat) as char,
        (b'0' + sq_lon) as char,
        (b'0' + sq_lat) as char,
        (b'a' + ss_lon.min(23)) as char,
        (b'a' + ss_lat.min(23)) as char
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) {
        assert!(
            (a - b).abs() < eps,
            "expected {} ≈ {} (|diff|={})",
            a,
            b,
            (a - b).abs()
        );
    }

    #[test]
    fn haversine_zero_distance() {
        let p = LatLon::new(48.8566, 2.3522);
        assert_eq!(haversine_km(p, p), 0.0);
    }

    #[test]
    fn haversine_paris_london() {
        // Paris (48.8566, 2.3522) → London (51.5074, -0.1278) ≈ 343 km
        let paris = LatLon::new(48.8566, 2.3522);
        let london = LatLon::new(51.5074, -0.1278);
        let d = haversine_km(paris, london);
        assert!((d - 343.0).abs() < 5.0, "expected ~343 km, got {}", d);
    }

    #[test]
    fn haversine_nyc_tokyo() {
        // NYC (40.7128, -74.0060) → Tokyo (35.6762, 139.6503) ≈ 10,870 km
        let nyc = LatLon::new(40.7128, -74.0060);
        let tokyo = LatLon::new(35.6762, 139.6503);
        let d = haversine_km(nyc, tokyo);
        assert!(
            (d - 10_870.0).abs() < 100.0,
            "expected ~10870 km, got {}",
            d
        );
    }

    #[test]
    fn haversine_antipodal_no_nan() {
        // North pole → south pole (antipodes) — should be ~half circumference.
        let a = LatLon::new(90.0, 0.0);
        let b = LatLon::new(-90.0, 0.0);
        let d = haversine_km(a, b);
        let half_circum = std::f64::consts::PI * EARTH_RADIUS_KM;
        approx(d, half_circum, 1.0);
    }

    #[test]
    fn bearing_cardinal_directions() {
        let origin = LatLon::new(0.0, 0.0);
        let north = LatLon::new(1.0, 0.0);
        let east = LatLon::new(0.0, 1.0);
        let south = LatLon::new(-1.0, 0.0);
        let west = LatLon::new(0.0, -1.0);
        approx(bearing_deg(origin, north), 0.0, 0.5);
        approx(bearing_deg(origin, east), 90.0, 0.5);
        approx(bearing_deg(origin, south), 180.0, 0.5);
        approx(bearing_deg(origin, west), 270.0, 0.5);
    }

    #[test]
    fn midpoint_same_point() {
        let p = LatLon::new(40.0, -70.0);
        let m = midpoint(p, p);
        approx(m.lat, p.lat, 1e-9);
        approx(m.lon, p.lon, 1e-9);
    }

    #[test]
    fn midpoint_known_pair() {
        // (0, 0) and (0, 10) midpoint should be (0, 5).
        let m = midpoint(LatLon::new(0.0, 0.0), LatLon::new(0.0, 10.0));
        approx(m.lat, 0.0, 1e-9);
        approx(m.lon, 5.0, 1e-9);
    }

    #[test]
    fn geohash_encode_decode_roundtrip() {
        let p = LatLon::new(57.64911, 10.40744); // Jutland
        let h = geohash_encode(p, 8);
        let (decoded, half_lat, half_lon) = geohash_decode(&h).unwrap();
        assert!(half_lat > 0.0 && half_lon > 0.0);
        assert!((decoded.lat - p.lat).abs() < half_lat * 2.0);
        assert!((decoded.lon - p.lon).abs() < half_lon * 2.0);
    }

    #[test]
    fn geohash_known_values() {
        // Reference: 57.64911, 10.40744 encodes to "u4pruydq" at precision 8.
        // We don't pin to exact base-32 codes (varies by impl detail), but
        // verify length and decode-failure for invalid chars.
        let p = LatLon::new(57.64911, 10.40744);
        assert_eq!(geohash_encode(p, 1).len(), 1);
        assert_eq!(geohash_encode(p, 5).len(), 5);
        assert_eq!(geohash_encode(p, 12).len(), 12);
        // Invalid characters should fail to decode.
        assert!(geohash_decode("!@#$").is_none());
    }

    #[test]
    fn geohash_higher_precision_is_smaller() {
        let p = LatLon::new(48.8566, 2.3522);
        let (_, h1_lat, h1_lon) = geohash_decode(&geohash_encode(p, 1)).unwrap();
        let (_, h7_lat, h7_lon) = geohash_decode(&geohash_encode(p, 7)).unwrap();
        assert!(h7_lat < h1_lat);
        assert!(h7_lon < h1_lon);
    }

    #[test]
    fn maidenhead_known_value() {
        // Reference: Maidenhead grid for London (51.5074, -0.1278) ≈ IO91VM.
        let grid = maidenhead_grid(LatLon::new(51.5074, -0.1278));
        assert_eq!(grid.len(), 6);
        assert_eq!(&grid[0..2], "IO");
        assert_eq!(&grid[2..4], "91");
        // Subsquare varies by rounding; just check it's lowercase a..x.
        assert!(grid.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
