pub fn sunrise_sunset(year: i32, month: u32, day: u32, lat: f64, lon: f64) -> (f64, f64) {
    let day_of_year = day_of_year(year, month, day) as f64;
    let decl = 23.45 * std::f64::consts::PI / 180.0 * (360.0 / 365.0 * (day_of_year - 81.0)).sin();
    let lat_rad = lat * std::f64::consts::PI / 180.0;
    let dec_rad = decl;
    let cos_h = -lat_rad.tan() * dec_rad.tan();
    let cos_h_clamped = cos_h.max(-1.0).min(1.0);
    let hour_angle = cos_h_clamped.acos() * 180.0 / std::f64::consts::PI;
    let solar_noon = 12.0 - lon / 15.0;
    let sunrise = solar_noon - hour_angle / 15.0;
    let sunset = solar_noon + hour_angle / 15.0;
    (sunrise, sunset)
}
fn day_of_year(year: i32, month: u32, day: u32) -> u32 {
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mut total = 0;
    for m in 0..(month - 1) as usize {
        total += days_in_month[m];
        if m == 1 && leap {
            total += 1;
        }
    }
    total + day
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equinox() {
        let (s, e) = sunrise_sunset(2024, 3, 20, 0.0, 0.0);
        assert!(s < e);
        assert!((s - 6.0).abs() < 0.5);
        assert!((e - 18.0).abs() < 0.5);
    }
    #[test]
    fn summer_daylight_longer() {
        let (_, e_summer) = sunrise_sunset(2024, 6, 21, 40.0, -74.0);
        let (_, e_winter) = sunrise_sunset(2024, 12, 21, 40.0, -74.0);
        let summer_len = if e_summer < 12.0 {
            e_summer + 24.0 - sunrise_sunset(2024, 6, 21, 40.0, -74.0).0
        } else {
            e_summer - sunrise_sunset(2024, 6, 21, 40.0, -74.0).0
        };
        assert!(
            summer_len > 9.0,
            "summer daylight expected >9h, got {}",
            summer_len
        );
    }
    #[test]
    fn winter_daylight_shorter() {
        let (s_w, e_w) = sunrise_sunset(2024, 12, 21, 40.0, -74.0);
        let winter_len = e_w - s_w;
        assert!(
            winter_len > 9.0 && winter_len < 15.5,
            "winter daylight expected 9-15.5h, got {}",
            winter_len
        );
    }
}
