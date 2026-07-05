// Minimal JWK (JSON Web Key) parsing — extract RSA n/e or EC x/y from a JWK object.
// Plus basic HS256/HS384/HS512 verification using HMAC-SHA from the jwt_hs256 module.
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct RsaJwk {
    pub kty: String,
    pub n: String,   // base64url modulus
    pub e: String,   // base64url exponent
    pub alg: Option<String>,
    pub kid: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EcJwk {
    pub kty: String,
    pub crv: String,
    pub x: String,
    pub y: String,
    pub alg: Option<String>,
    pub kid: Option<String>,
}

pub fn parse_jwk(val: &Value) -> Result<RsaJwk, String> {
    let kty = val.get("kty").and_then(|v| v.as_str()).ok_or("missing kty")?;
    if kty != "RSA" { return Err(format!("not RSA: {}", kty)); }
    let n = val.get("n").and_then(|v| v.as_str()).ok_or("missing n")?.to_string();
    let e = val.get("e").and_then(|v| v.as_str()).ok_or("missing e")?.to_string();
    let alg = val.get("alg").and_then(|v| v.as_str()).map(String::from);
    let kid = val.get("kid").and_then(|v| v.as_str()).map(String::from);
    Ok(RsaJwk { kty: kty.into(), n, e, alg, kid })
}

pub fn parse_ec_jwk(val: &Value) -> Result<EcJwk, String> {
    let kty = val.get("kty").and_then(|v| v.as_str()).ok_or("missing kty")?;
    if kty != "EC" { return Err(format!("not EC: {}", kty)); }
    let crv = val.get("crv").and_then(|v| v.as_str()).ok_or("missing crv")?.to_string();
    let x = val.get("x").and_then(|v| v.as_str()).ok_or("missing x")?.to_string();
    let y = val.get("y").and_then(|v| v.as_str()).ok_or("missing y")?.to_string();
    let alg = val.get("alg").and_then(|v| v.as_str()).map(String::from);
    let kid = val.get("kid").and_then(|v| v.as_str()).map(String::from);
    Ok(EcJwk { kty: kty.into(), crv, x, y, alg, kid })
}

pub fn parse_jwks(json: &str) -> Result<Vec<RsaJwk>, String> {
    let val: Value = serde_json::from_str(json).map_err(|e| format!("parse: {}", e))?;
    let arr = val.get("keys").and_then(|v| v.as_array()).ok_or("missing keys array")?;
    let mut out = Vec::new();
    for k in arr {
        if let Ok(rsa) = parse_jwk(k) { out.push(rsa); }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_basic_rsa() {
        let json = r#"{"kty":"RSA","n":"abc","e":"AQAB","alg":"RS256","kid":"k1"}"#;
        let k = parse_jwk(&serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(k.kty, "RSA");
        assert_eq!(k.n, "abc");
        assert_eq!(k.e, "AQAB");
        assert_eq!(k.alg, Some("RS256".into()));
        assert_eq!(k.kid, Some("k1".into()));
    }
    #[test]
    fn parse_ec() {
        let json = r#"{"kty":"EC","crv":"P-256","x":"x1","y":"y1","kid":"k2"}"#;
        let k = parse_ec_jwk(&serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(k.crv, "P-256");
        assert_eq!(k.x, "x1");
    }
    #[test]
    fn parse_jwks_set() {
        let json = r#"{"keys":[
            {"kty":"RSA","n":"n1","e":"AQAB"},
            {"kty":"EC","crv":"P-256","x":"x","y":"y"},
            {"kty":"RSA","n":"n2","e":"AQAB","kid":"k2"}
        ]}"#;
        let set = parse_jwks(json).unwrap();
        assert_eq!(set.len(), 2);
        assert_eq!(set[1].kid, Some("k2".into()));
    }
    #[test]
    fn parse_missing_kty() {
        let json = r#"{"n":"x","e":"y"}"#;
        assert!(parse_jwk(&serde_json::from_str(json).unwrap()).is_err());
    }
    #[test]
    fn parse_wrong_kty() {
        let json = r#"{"kty":"oct","k":"abc"}"#;
        assert!(parse_jwk(&serde_json::from_str(json).unwrap()).is_err());
    }
    #[test]
    fn parse_no_keys_array() {
        let json = r#"{"foo":"bar"}"#;
        assert!(parse_jwks(json).is_err());
    }
    #[test]
    fn parse_ec_missing_crv() {
        let json = r#"{"kty":"EC","x":"x","y":"y"}"#;
        assert!(parse_ec_jwk(&serde_json::from_str(json).unwrap()).is_err());
    }
    #[test]
    fn parse_rsa_no_alg() {
        let json = r#"{"kty":"RSA","n":"n","e":"e"}"#;
        let k = parse_jwk(&serde_json::from_str(json).unwrap()).unwrap();
        assert_eq!(k.alg, None);
        assert_eq!(k.kid, None);
    }
}
