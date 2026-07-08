//! HTTP status codes (RFC 7231 §6 + RFC 6585 + RFC 4918 + RFC 8470).
//!
//! Provides a [`StatusCode`] enum covering the codes you're likely
//! to encounter in real-world HTTP traffic, plus the [`reason_phrase`]
//! function returning the canonical reason phrase for each code.
//!
//! Construct codes via [`StatusCode::from_u16`]. Unknown numeric
//! codes are accepted as [`StatusCode::Unknown`] but `reason_phrase`
//! returns `Some("Unknown")` for them.

/// HTTP response status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusCode {
    // 1xx Informational
    Continue,
    SwitchingProtocols,
    Processing,
    EarlyHints,

    // 2xx Success
    Ok,
    Created,
    Accepted,
    NonAuthoritativeInformation,
    NoContent,
    ResetContent,
    PartialContent,
    MultiStatus,
    AlreadyReported,
    ImUsed,

    // 3xx Redirection
    MultipleChoices,
    MovedPermanently,
    Found,
    SeeOther,
    NotModified,
    UseProxy,
    TemporaryRedirect,
    PermanentRedirect,

    // 4xx Client Error
    BadRequest,
    Unauthorized,
    PaymentRequired,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    NotAcceptable,
    ProxyAuthenticationRequired,
    RequestTimeout,
    Conflict,
    Gone,
    LengthRequired,
    PreconditionFailed,
    PayloadTooLarge,
    UriTooLong,
    UnsupportedMediaType,
    RangeNotSatisfiable,
    ExpectationFailed,
    ImATeapot,
    MisdirectedRequest,
    UnprocessableEntity,
    Locked,
    FailedDependency,
    TooEarly,
    UpgradeRequired,
    PreconditionRequired,
    TooManyRequests,
    RequestHeaderFieldsTooLarge,
    UnavailableForLegalReasons,

    // 5xx Server Error
    InternalServerError,
    NotImplemented,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,
    HttpVersionNotSupported,
    VariantAlsoNegotiates,
    InsufficientStorage,
    LoopDetected,
    NotExtended,
    NetworkAuthenticationRequired,

    /// Any numeric code not listed above (RFC reserves the right to
    /// add new ones — see RFC 7231 §6 and IANA registry).
    Unknown(u16),
}

impl StatusCode {
    /// Parse a status code from its numeric value.
    pub fn from_u16(code: u16) -> Self {
        match code {
            100 => Self::Continue,
            101 => Self::SwitchingProtocols,
            102 => Self::Processing,
            103 => Self::EarlyHints,
            200 => Self::Ok,
            201 => Self::Created,
            202 => Self::Accepted,
            203 => Self::NonAuthoritativeInformation,
            204 => Self::NoContent,
            205 => Self::ResetContent,
            206 => Self::PartialContent,
            207 => Self::MultiStatus,
            208 => Self::AlreadyReported,
            226 => Self::ImUsed,
            300 => Self::MultipleChoices,
            301 => Self::MovedPermanently,
            302 => Self::Found,
            303 => Self::SeeOther,
            304 => Self::NotModified,
            305 => Self::UseProxy,
            307 => Self::TemporaryRedirect,
            308 => Self::PermanentRedirect,
            400 => Self::BadRequest,
            401 => Self::Unauthorized,
            402 => Self::PaymentRequired,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            405 => Self::MethodNotAllowed,
            406 => Self::NotAcceptable,
            407 => Self::ProxyAuthenticationRequired,
            408 => Self::RequestTimeout,
            409 => Self::Conflict,
            410 => Self::Gone,
            411 => Self::LengthRequired,
            412 => Self::PreconditionFailed,
            413 => Self::PayloadTooLarge,
            414 => Self::UriTooLong,
            415 => Self::UnsupportedMediaType,
            416 => Self::RangeNotSatisfiable,
            417 => Self::ExpectationFailed,
            418 => Self::ImATeapot,
            421 => Self::MisdirectedRequest,
            422 => Self::UnprocessableEntity,
            423 => Self::Locked,
            424 => Self::FailedDependency,
            425 => Self::TooEarly,
            426 => Self::UpgradeRequired,
            428 => Self::PreconditionRequired,
            429 => Self::TooManyRequests,
            431 => Self::RequestHeaderFieldsTooLarge,
            451 => Self::UnavailableForLegalReasons,
            500 => Self::InternalServerError,
            501 => Self::NotImplemented,
            502 => Self::BadGateway,
            503 => Self::ServiceUnavailable,
            504 => Self::GatewayTimeout,
            505 => Self::HttpVersionNotSupported,
            506 => Self::VariantAlsoNegotiates,
            507 => Self::InsufficientStorage,
            508 => Self::LoopDetected,
            510 => Self::NotExtended,
            511 => Self::NetworkAuthenticationRequired,
            c => Self::Unknown(c),
        }
    }

    /// Numeric value of the status code.
    pub fn as_u16(self) -> u16 {
        match self {
            Self::Continue => 100,
            Self::SwitchingProtocols => 101,
            Self::Processing => 102,
            Self::EarlyHints => 103,
            Self::Ok => 200,
            Self::Created => 201,
            Self::Accepted => 202,
            Self::NonAuthoritativeInformation => 203,
            Self::NoContent => 204,
            Self::ResetContent => 205,
            Self::PartialContent => 206,
            Self::MultiStatus => 207,
            Self::AlreadyReported => 208,
            Self::ImUsed => 226,
            Self::MultipleChoices => 300,
            Self::MovedPermanently => 301,
            Self::Found => 302,
            Self::SeeOther => 303,
            Self::NotModified => 304,
            Self::UseProxy => 305,
            Self::TemporaryRedirect => 307,
            Self::PermanentRedirect => 308,
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::PaymentRequired => 402,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::NotAcceptable => 406,
            Self::ProxyAuthenticationRequired => 407,
            Self::RequestTimeout => 408,
            Self::Conflict => 409,
            Self::Gone => 410,
            Self::LengthRequired => 411,
            Self::PreconditionFailed => 412,
            Self::PayloadTooLarge => 413,
            Self::UriTooLong => 414,
            Self::UnsupportedMediaType => 415,
            Self::RangeNotSatisfiable => 416,
            Self::ExpectationFailed => 417,
            Self::ImATeapot => 418,
            Self::MisdirectedRequest => 421,
            Self::UnprocessableEntity => 422,
            Self::Locked => 423,
            Self::FailedDependency => 424,
            Self::TooEarly => 425,
            Self::UpgradeRequired => 426,
            Self::PreconditionRequired => 428,
            Self::TooManyRequests => 429,
            Self::RequestHeaderFieldsTooLarge => 431,
            Self::UnavailableForLegalReasons => 451,
            Self::NotImplemented => 501,
            Self::InternalServerError => 500,
            Self::BadGateway => 502,
            Self::ServiceUnavailable => 503,
            Self::GatewayTimeout => 504,
            Self::HttpVersionNotSupported => 505,
            Self::VariantAlsoNegotiates => 506,
            Self::InsufficientStorage => 507,
            Self::LoopDetected => 508,
            Self::NotExtended => 510,
            Self::NetworkAuthenticationRequired => 511,
            Self::Unknown(c) => c,
        }
    }

    /// Class of the code (1xx..=5xx).
    pub fn class(self) -> Class {
        match self.as_u16() {
            100..=199 => Class::Informational,
            200..=299 => Class::Success,
            300..=399 => Class::Redirection,
            400..=499 => Class::ClientError,
            500..=599 => Class::ServerError,
            _ => Class::Unknown,
        }
    }

    /// True for 2xx codes.
    pub fn is_success(self) -> bool {
        matches!(self.class(), Class::Success)
    }
}

/// Coarse class of an HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Informational, // 1xx
    Success,        // 2xx
    Redirection,    // 3xx
    ClientError,    // 4xx
    ServerError,    // 5xx
    Unknown,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Informational => "1xx",
            Class::Success => "2xx",
            Class::Redirection => "3xx",
            Class::ClientError => "4xx",
            Class::ServerError => "5xx",
            Class::Unknown => "?xx",
        }
    }
}

/// Return the canonical reason phrase for `code`. Returns
/// `Some("Unknown")` for unrecognized numeric codes.
pub fn reason_phrase(code: StatusCode) -> &'static str {
    match code {
        StatusCode::Continue => "Continue",
        StatusCode::SwitchingProtocols => "Switching Protocols",
        StatusCode::Processing => "Processing",
        StatusCode::EarlyHints => "Early Hints",
        StatusCode::Ok => "OK",
        StatusCode::Created => "Created",
        StatusCode::Accepted => "Accepted",
        StatusCode::NonAuthoritativeInformation => "Non-Authoritative Information",
        StatusCode::NoContent => "No Content",
        StatusCode::ResetContent => "Reset Content",
        StatusCode::PartialContent => "Partial Content",
        StatusCode::MultiStatus => "Multi-Status",
        StatusCode::AlreadyReported => "Already Reported",
        StatusCode::ImUsed => "IM Used",
        StatusCode::MultipleChoices => "Multiple Choices",
        StatusCode::MovedPermanently => "Moved Permanently",
        StatusCode::Found => "Found",
        StatusCode::SeeOther => "See Other",
        StatusCode::NotModified => "Not Modified",
        StatusCode::UseProxy => "Use Proxy",
        StatusCode::TemporaryRedirect => "Temporary Redirect",
        StatusCode::PermanentRedirect => "Permanent Redirect",
        StatusCode::BadRequest => "Bad Request",
        StatusCode::Unauthorized => "Unauthorized",
        StatusCode::PaymentRequired => "Payment Required",
        StatusCode::Forbidden => "Forbidden",
        StatusCode::NotFound => "Not Found",
        StatusCode::MethodNotAllowed => "Method Not Allowed",
        StatusCode::NotAcceptable => "Not Acceptable",
        StatusCode::ProxyAuthenticationRequired => "Proxy Authentication Required",
        StatusCode::RequestTimeout => "Request Timeout",
        StatusCode::Conflict => "Conflict",
        StatusCode::Gone => "Gone",
        StatusCode::LengthRequired => "Length Required",
        StatusCode::PreconditionFailed => "Precondition Failed",
        StatusCode::PayloadTooLarge => "Payload Too Large",
        StatusCode::UriTooLong => "URI Too Long",
        StatusCode::UnsupportedMediaType => "Unsupported Media Type",
        StatusCode::RangeNotSatisfiable => "Range Not Satisfiable",
        StatusCode::ExpectationFailed => "Expectation Failed",
        StatusCode::ImATeapot => "I'm a teapot",
        StatusCode::MisdirectedRequest => "Misdirected Request",
        StatusCode::UnprocessableEntity => "Unprocessable Entity",
        StatusCode::Locked => "Locked",
        StatusCode::FailedDependency => "Failed Dependency",
        StatusCode::TooEarly => "Too Early",
        StatusCode::UpgradeRequired => "Upgrade Required",
        StatusCode::PreconditionRequired => "Precondition Required",
        StatusCode::TooManyRequests => "Too Many Requests",
        StatusCode::RequestHeaderFieldsTooLarge => "Request Header Fields Too Large",
        StatusCode::UnavailableForLegalReasons => "Unavailable For Legal Reasons",
        StatusCode::NotImplemented => "Not Implemented",
        StatusCode::InternalServerError => "Internal Server Error",
        StatusCode::BadGateway => "Bad Gateway",
        StatusCode::ServiceUnavailable => "Service Unavailable",
        StatusCode::GatewayTimeout => "Gateway Timeout",
        StatusCode::HttpVersionNotSupported => "HTTP Version Not Supported",
        StatusCode::VariantAlsoNegotiates => "Variant Also Negotiates",
        StatusCode::InsufficientStorage => "Insufficient Storage",
        StatusCode::LoopDetected => "Loop Detected",
        StatusCode::NotExtended => "Not Extended",
        StatusCode::NetworkAuthenticationRequired => "Network Authentication Required",
        StatusCode::Unknown(_) => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_codes_round_trip() {
        for &(code, expected) in &[
            (200u16, StatusCode::Ok),
            (404, StatusCode::NotFound),
            (500, StatusCode::InternalServerError),
            (418, StatusCode::ImATeapot),
            (451, StatusCode::UnavailableForLegalReasons),
            (508, StatusCode::LoopDetected),
        ] {
            let c = StatusCode::from_u16(code);
            assert_eq!(c, expected);
            assert_eq!(c.as_u16(), code);
        }
    }

    #[test]
    fn unknown_code_preserves_value() {
        let c = StatusCode::from_u16(999);
        assert_eq!(c, StatusCode::Unknown(999));
        assert_eq!(c.as_u16(), 999);
        assert_eq!(reason_phrase(c), "Unknown");
    }

    #[test]
    fn reason_phrase_common() {
        assert_eq!(reason_phrase(StatusCode::Ok), "OK");
        assert_eq!(reason_phrase(StatusCode::NotFound), "Not Found");
        assert_eq!(reason_phrase(StatusCode::InternalServerError), "Internal Server Error");
        assert_eq!(reason_phrase(StatusCode::Continue), "Continue");
        assert_eq!(reason_phrase(StatusCode::SwitchingProtocols), "Switching Protocols");
    }

    #[test]
    fn class_correct() {
        assert_eq!(StatusCode::from_u16(100).class(), Class::Informational);
        assert_eq!(StatusCode::from_u16(200).class(), Class::Success);
        assert_eq!(StatusCode::from_u16(301).class(), Class::Redirection);
        assert_eq!(StatusCode::from_u16(404).class(), Class::ClientError);
        assert_eq!(StatusCode::from_u16(500).class(), Class::ServerError);
        assert_eq!(StatusCode::from_u16(999).class(), Class::Unknown);
    }

    #[test]
    fn is_success() {
        assert!(StatusCode::from_u16(200).is_success());
        assert!(StatusCode::from_u16(201).is_success());
        assert!(StatusCode::from_u16(299).is_success());
        assert!(!StatusCode::from_u16(300).is_success());
        assert!(!StatusCode::from_u16(404).is_success());
    }

    #[test]
    fn all_listed_codes_categorize_correctly() {
        for code in 100..600u16 {
            let class = StatusCode::from_u16(code).class();
            if code < 200 {
                assert_eq!(class, Class::Informational, "{}", code);
            } else if code < 300 {
                assert_eq!(class, Class::Success, "{}", code);
            } else if code < 400 {
                assert_eq!(class, Class::Redirection, "{}", code);
            } else if code < 500 {
                assert_eq!(class, Class::ClientError, "{}", code);
            } else if code < 600 {
                assert_eq!(class, Class::ServerError, "{}", code);
            }
        }
    }
}