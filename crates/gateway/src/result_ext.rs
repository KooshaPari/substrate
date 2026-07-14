#[derive(Debug, PartialEq)]
pub enum ResultExt<T, E> {
    Ok(T),
    Err(E),
    Recovered(T, String),
}
impl<T, E> ResultExt<T, E> {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_) | Self::Recovered(_, _))
    }
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }
    pub fn is_recovered(&self) -> bool {
        matches!(self, Self::Recovered(_, _))
    }
    pub fn ok(self) -> Option<T> {
        match self {
            Self::Ok(v) | Self::Recovered(v, _) => Some(v),
            Self::Err(_) => None,
        }
    }
    pub fn err(self) -> Option<E> {
        match self {
            Self::Err(e) => Some(e),
            _ => None,
        }
    }
    pub fn recovery_reason(&self) -> Option<&str> {
        match self {
            Self::Recovered(_, r) => Some(r.as_str()),
            _ => None,
        }
    }
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> ResultExt<U, E> {
        match self {
            Self::Ok(v) => ResultExt::Ok(f(v)),
            Self::Recovered(v, r) => ResultExt::Recovered(f(v), r),
            Self::Err(e) => ResultExt::Err(e),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ok_is_ok() {
        let r: ResultExt<i32, &str> = ResultExt::Ok(1);
        assert!(r.is_ok());
    }
    #[test]
    fn err_is_err() {
        let r: ResultExt<i32, &str> = ResultExt::Err("fail");
        assert!(r.is_err());
    }
    #[test]
    fn recovered() {
        let r: ResultExt<i32, &str> = ResultExt::Recovered(42, "stale cache".into());
        assert!(r.is_recovered());
        assert_eq!(r.recovery_reason(), Some("stale cache"));
    }
    #[test]
    fn map() {
        let r: ResultExt<i32, &str> = ResultExt::Ok(5);
        assert_eq!(r.map(|x| x * 2), ResultExt::Ok(10));
    }
    #[test]
    fn err_returns_none() {
        let r: ResultExt<i32, &str> = ResultExt::Err("x");
        assert_eq!(r.ok(), None);
    }
}
