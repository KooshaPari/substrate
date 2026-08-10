// Minimal W3C Web App Manifest validator.
//
// References:
//   W3C Web App Manifest: https://www.w3.org/TR/appmanifest/
//   - Required: at least one of `name` or `short_name` (per spec section 6.1.2
//     "Application Identity" -- the user agent falls back to `short_name` when
//     `name` is absent, so both being missing is the only invalid case).
//   - Required: `start_url`.
//   - Optional `display` must be one of the enum values from the spec section
//     6.1.6 "Display Modes".
//   - `theme_color` / `background_color` are CSS <color> values; we accept the
//     hex forms required by most tooling (#RGB, #RRGGBB, #RRGGBBAA) plus named
//     colors via the small built-in table in `is_css_color`.
//   - `icons` must be an array; each entry requires `src` and `sizes`.
//   - `scope` (if present) must resolve to the same origin as `start_url`.

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct ManifestIcon {
    pub src: String,
    pub sizes: String,
    pub r#type: Option<String>,
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub name: Option<String>,
    pub short_name: Option<String>,
    pub start_url: String,
    pub display: Option<String>,
    pub orientation: Option<String>,
    pub theme_color: Option<String>,
    pub background_color: Option<String>,
    pub scope: Option<String>,
    pub icons: Vec<ManifestIcon>,
}

const VALID_DISPLAYS: &[&str] = &[
    "fullscreen",
    "standalone",
    "minimal-ui",
    "browser",
    "picture-in-picture",
    "window-controls-overlay",
];

const VALID_PURPOSES: &[&str] = &["monochrome", "maskable", "any"];

/// Returns `true` if `s` is a CSS hex color in one of the canonical forms:
/// `#RGB`, `#RRGGBB`, or `#RRGGBBAA` (case-insensitive). Also accepts a small
/// set of named CSS colors (`red`, `blue`, etc.) for tolerance with real-world
/// manifests.
pub fn is_css_color(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if let Some(hex) = s.strip_prefix('#') {
        let bytes = hex.len();
        return matches!(bytes, 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    matches!(
        s.to_ascii_lowercase().as_str(),
        "black"
            | "silver"
            | "gray"
            | "grey"
            | "white"
            | "maroon"
            | "red"
            | "purple"
            | "fuchsia"
            | "magenta"
            | "green"
            | "lime"
            | "olive"
            | "yellow"
            | "navy"
            | "blue"
            | "teal"
            | "aqua"
            | "cyan"
            | "orange"
            | "pink"
            | "brown"
            | "transparent"
            | "aliceblue"
            | "antiquewhite"
            | "aquamarine"
            | "azure"
            | "beige"
            | "bisque"
            | "blanchedalmond"
            | "blueviolet"
            | "burlywood"
            | "cadetblue"
            | "chartreuse"
            | "chocolate"
            | "coral"
            | "cornflowerblue"
            | "cornsilk"
            | "crimson"
            | "darkblue"
            | "darkcyan"
            | "darkgoldenrod"
            | "darkgray"
            | "darkgreen"
            | "darkgrey"
            | "darkkhaki"
            | "darkmagenta"
            | "darkolivegreen"
            | "darkorange"
            | "darkorchid"
            | "darkred"
            | "darksalmon"
            | "darkseagreen"
            | "darkslateblue"
            | "darkslategray"
            | "darkslategrey"
            | "darkturquoise"
            | "darkviolet"
            | "deeppink"
            | "deepskyblue"
            | "dimgray"
            | "dimgrey"
            | "dodgerblue"
            | "firebrick"
            | "floralwhite"
            | "forestgreen"
            | "gainsboro"
            | "ghostwhite"
            | "gold"
            | "goldenrod"
            | "greenyellow"
            | "honeydew"
            | "hotpink"
            | "indianred"
            | "indigo"
            | "ivory"
            | "khaki"
            | "lavender"
            | "lavenderblush"
            | "lawngreen"
            | "lemonchiffon"
            | "lightblue"
            | "lightcoral"
            | "lightcyan"
            | "lightgoldenrodyellow"
            | "lightgray"
            | "lightgreen"
            | "lightgrey"
            | "lightpink"
            | "lightsalmon"
            | "lightseagreen"
            | "lightskyblue"
            | "lightslategray"
            | "lightslategrey"
            | "lightsteelblue"
            | "lightyellow"
            | "limegreen"
            | "linen"
            | "mediumaquamarine"
            | "mediumblue"
            | "mediumorchid"
            | "mediumpurple"
            | "mediumseagreen"
            | "mediumslateblue"
            | "mediumspringgreen"
            | "mediumturquoise"
            | "mediumvioletred"
            | "midnightblue"
            | "mintcream"
            | "mistyrose"
            | "moccasin"
            | "navajowhite"
            | "oldlace"
            | "olivedrab"
            | "orangered"
            | "orchid"
            | "palegoldenrod"
            | "palegreen"
            | "paleturquoise"
            | "palevioletred"
            | "papayawhip"
            | "peachpuff"
            | "peru"
            | "plum"
            | "powderblue"
            | "rosybrown"
            | "royalblue"
            | "saddlebrown"
            | "salmon"
            | "sandybrown"
            | "seagreen"
            | "seashell"
            | "sienna"
            | "skyblue"
            | "slateblue"
            | "slategray"
            | "slategrey"
            | "snow"
            | "springgreen"
            | "steelblue"
            | "tan"
            | "thistle"
            | "tomato"
            | "turquoise"
            | "violet"
            | "wheat"
            | "whitesmoke"
            | "yellowgreen"
            | "rebeccapurple"
            | "darkseagreen"
            | "lightseagreen"
    )
}

/// Parse and validate a Web App Manifest JSON document. Returns the parsed
/// `Manifest` on success or a descriptive error string on the first violation.
pub fn validate(json: &str) -> Result<Manifest, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "manifest root must be a JSON object".to_string())?;

    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let short_name = obj
        .get("short_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if name.is_none() && short_name.is_none() {
        return Err("manifest must have at least one of `name` or `short_name`".into());
    }

    let start_url = obj
        .get("start_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "manifest must have a string `start_url`".to_string())?
        .to_string();
    if start_url.is_empty() {
        return Err("`start_url` must not be empty".into());
    }

    let display = obj
        .get("display")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(d) = &display {
        if !VALID_DISPLAYS.contains(&d.as_str()) {
            return Err(format!(
                "invalid `display` {d:?}; expected one of {VALID_DISPLAYS:?}"
            ));
        }
    }

    let theme_color = obj
        .get("theme_color")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(c) = &theme_color {
        if !is_css_color(c) {
            return Err(format!("invalid `theme_color` {c:?}"));
        }
    }
    let background_color = obj
        .get("background_color")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(c) = &background_color {
        if !is_css_color(c) {
            return Err(format!("invalid `background_color` {c:?}"));
        }
    }

    let orientation = obj
        .get("orientation")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let scope = obj
        .get("scope")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let (Some(sc), st) = (&scope, &start_url) {
        if !same_origin(sc, st) {
            return Err(format!(
                "`scope` ({sc:?}) and `start_url` ({st:?}) must share the same origin"
            ));
        }
    }

    let mut icons = Vec::new();
    if let Some(arr) = obj.get("icons") {
        let items = arr
            .as_array()
            .ok_or_else(|| "`icons` must be an array".to_string())?;
        let mut seen_sizes: BTreeSet<(String, String)> = BTreeSet::new();
        for (idx, entry) in items.iter().enumerate() {
            let e = entry
                .as_object()
                .ok_or_else(|| format!("icons[{idx}] must be an object"))?;
            let src = e
                .get("src")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("icons[{idx}].src must be a string"))?
                .to_string();
            if src.is_empty() {
                return Err(format!("icons[{idx}].src must not be empty"));
            }
            let sizes = e
                .get("sizes")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("icons[{idx}].sizes must be a string"))?
                .to_string();
            if !seen_sizes.insert((src.clone(), sizes.clone())) {
                return Err(format!("duplicate icon entry (src={src}, sizes={sizes})"));
            }
            let r#type = e.get("type").and_then(|v| v.as_str()).map(str::to_string);
            let purpose = e
                .get("purpose")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(p) = &purpose {
                // Spec allows a space-separated list; we accept any combination of
                // VALID_PURPOSES tokens (case-insensitive).
                for tok in p.split_whitespace() {
                    let lc = tok.to_ascii_lowercase();
                    if !VALID_PURPOSES.iter().any(|v| *v == lc) {
                        return Err(format!("icons[{idx}].purpose has invalid token {tok:?}"));
                    }
                }
            }
            icons.push(ManifestIcon {
                src,
                sizes,
                r#type,
                purpose,
            });
        }
    }

    Ok(Manifest {
        name,
        short_name,
        start_url,
        display,
        orientation,
        theme_color,
        background_color,
        scope,
        icons,
    })
}

fn same_origin(a: &str, b: &str) -> bool {
    // Very small URL origin check: scheme + host (with port) must match.
    fn origin(u: &str) -> Option<(String, String)> {
        let scheme_end = u.find("://")?;
        let scheme = u[..scheme_end].to_ascii_lowercase();
        let after = &u[scheme_end + 3..];
        let host_end = after
            .find(|c: char| c == '/' || c == '?' || c == '#')
            .unwrap_or(after.len());
        let host_part = &after[..host_end];
        // Strip userinfo if present.
        let host = host_part.rsplit('@').next().unwrap_or(host_part);
        let host_lc = host.to_ascii_lowercase();
        Some((scheme, host_lc))
    }
    match (origin(a), origin(b)) {
        (Some(o1), Some(o2)) => o1 == o2,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_forms() {
        assert!(is_css_color("#fff"));
        assert!(is_css_color("#FFF"));
        assert!(is_css_color("#ff00aa"));
        assert!(is_css_color("#FF00AA"));
        assert!(is_css_color("#11223344")); // RGBA hex
        assert!(!is_css_color("#ff")); // 2 hex digits invalid
        assert!(!is_css_color("#fffff")); // 5 hex digits invalid
        assert!(!is_css_color("#zzzzzz"));
        assert!(!is_css_color(""));
    }

    #[test]
    fn named_color_and_invalid() {
        assert!(is_css_color("red"));
        assert!(is_css_color("rebeccapurple"));
        assert!(!is_css_color("notacolor"));
    }

    #[test]
    fn minimal_valid_manifest() {
        let json = r#"{"name": "Demo", "start_url": "/"}"#;
        let m = validate(json).unwrap();
        assert_eq!(m.name.as_deref(), Some("Demo"));
        assert_eq!(m.start_url, "/");
        assert!(m.icons.is_empty());
    }

    #[test]
    fn short_name_alone_is_valid() {
        let json = r#"{"short_name": "Demo", "start_url": "/"}"#;
        let m = validate(json).unwrap();
        assert_eq!(m.short_name.as_deref(), Some("Demo"));
        assert_eq!(m.start_url, "/");
    }

    #[test]
    fn missing_name_and_short_name_errors() {
        let json = r#"{"start_url": "/"}"#;
        let err = validate(json).unwrap_err();
        assert!(err.contains("name"));
    }

    #[test]
    fn missing_start_url_errors() {
        let json = r#"{"name": "Demo"}"#;
        let err = validate(json).unwrap_err();
        assert!(err.contains("start_url"));
    }

    #[test]
    fn invalid_display_errors() {
        let json = r#"{"name": "Demo", "start_url": "/", "display": "wide"}"#;
        let err = validate(json).unwrap_err();
        assert!(err.contains("display"));
    }

    #[test]
    fn valid_display_values() {
        for d in VALID_DISPLAYS {
            let json = format!(r#"{{"name":"x","start_url":"/","display":"{d}"}}"#);
            assert!(validate(&json).is_ok(), "display {d} should validate");
        }
    }

    #[test]
    fn invalid_theme_color_errors() {
        let json = r#"{"name":"x","start_url":"/","theme_color":"not-a-color"}"#;
        assert!(validate(json).is_err());
    }

    #[test]
    fn valid_hex_theme_color() {
        let json = r##"{"name":"x","start_url":"/","theme_color":"#336699","background_color":"#f0f8ff"}"##;
        let m = validate(json).unwrap();
        assert_eq!(m.theme_color.as_deref(), Some("#336699"));
        assert_eq!(m.background_color.as_deref(), Some("#f0f8ff"));
    }

    #[test]
    fn icons_require_src_and_sizes() {
        let json = r#"{"name":"x","start_url":"/","icons":[{"src":"/a.png"}]}"#;
        let err = validate(json).unwrap_err();
        assert!(err.contains("sizes"));
    }

    #[test]
    fn icons_parse_full_entry() {
        let json = r#"{"name":"x","start_url":"/","icons":[
            {"src":"/icon-192.png","sizes":"192x192","type":"image/png","purpose":"any maskable"}
        ]}"#;
        let m = validate(json).unwrap();
        assert_eq!(m.icons.len(), 1);
        assert_eq!(m.icons[0].src, "/icon-192.png");
        assert_eq!(m.icons[0].sizes, "192x192");
        assert_eq!(m.icons[0].r#type.as_deref(), Some("image/png"));
        assert_eq!(m.icons[0].purpose.as_deref(), Some("any maskable"));
    }

    #[test]
    fn duplicate_icons_rejected() {
        let json = r#"{"name":"x","start_url":"/","icons":[
            {"src":"/a.png","sizes":"192x192"},
            {"src":"/a.png","sizes":"192x192"}
        ]}"#;
        let err = validate(json).unwrap_err();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn scope_must_match_origin() {
        let json =
            r#"{"name":"x","start_url":"https://a.example.com/","scope":"https://b.example.com/"}"#;
        let err = validate(json).unwrap_err();
        assert!(err.contains("origin"));
    }

    #[test]
    fn scope_same_origin_ok() {
        let json = r#"{"name":"x","start_url":"https://a.example.com/","scope":"https://a.example.com/app/"}"#;
        let m = validate(json).unwrap();
        assert_eq!(m.scope.as_deref(), Some("https://a.example.com/app/"));
    }

    #[test]
    fn malformed_json_errors() {
        let err = validate("{not json").unwrap_err();
        assert!(err.contains("JSON"));
    }
}
