//! The integer query parameters of the endpoints: a default, a range, and a `400` outside it.
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// A parameter with its default and limits. Without a default it is required.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Param {
    pub fallback: Option<i64>,
    pub min: i64,
    pub max: i64,
}

impl Param {
    pub const fn new(fallback: i64, min: i64, max: i64) -> Param {
        Param { fallback: Some(fallback), min, max }
    }
}

/// The integer values of a request, in the order of its specs
pub struct Params(Vec<(&'static str, i64)>);

impl Params {
    pub fn get(&self, name: &str) -> i64 {
        self.0.iter().find(|(key, _)| *key == name).map(|(_, value)| *value).expect("a parameter that is in the specs")
    }
}

/// A request that is answered with a `400` and says why
#[derive(Debug, PartialEq)]
pub struct Rejection(pub String);

impl IntoResponse for Rejection {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.0).into_response()
    }
}

/// The integer at the start of a string: leading spaces, a sign, then digits (hexadecimal after
/// `0x`) up to the first character that is not one. `None` when there is none.
pub fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim_start();
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (radix, digits) = match rest.get(..2) {
        Some("0x") | Some("0X") => (16, &rest[2..]),
        _ => (10, rest),
    };
    let mut value: Option<i64> = None;
    for digit in digits.chars().map_while(|c| c.to_digit(radix)) {
        value = Some(value.unwrap_or(0).saturating_mul(radix as i64).saturating_add(digit as i64));
    }
    value.map(|n| if negative { -n } else { n })
}

/// Reads the parameters of `specs` from a query string. A value that is not a number becomes the
/// fallback; a repeated one counts once, the first time; one outside min..max is rejected.
pub fn read(query: Option<&str>, specs: &[(&'static str, Param)]) -> Result<Params, Rejection> {
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query.unwrap_or("")).unwrap_or_default();
    let mut values = Vec::with_capacity(specs.len());
    for (name, Param { fallback, min, max }) in specs {
        let parsed = pairs.iter().find(|(key, _)| key == name).and_then(|(_, value)| parse_int(value));
        let Some(value) = parsed.or(*fallback) else {
            return Err(Rejection(format!("{name} is required")));
        };
        if value < *min || value > *max {
            return Err(Rejection(format!("{name} must be between {min} and {max}")));
        }
        values.push((*name, value));
    }
    Ok(Params(values))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: [(&str, Param); 1] = [("ms", Param::new(1000, 0, 5000))];

    #[test]
    fn it_reads_the_integer_at_the_start() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("  7"), Some(7));
        assert_eq!(parse_int("12abc"), Some(12));
        assert_eq!(parse_int("-5"), Some(-5));
        assert_eq!(parse_int("+5"), Some(5));
        assert_eq!(parse_int("0x10"), Some(16));
        assert_eq!(parse_int("0XfF"), Some(255));
        assert_eq!(parse_int("2.9"), Some(2));
        assert_eq!(parse_int("007"), Some(7));
    }

    #[test]
    fn it_finds_no_integer_where_there_is_none() {
        for text in ["abc", "", "-", "+", " ", "0x", ".5", "e3"] {
            assert_eq!(parse_int(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_number_too_big_stops_at_the_largest_one() {
        assert_eq!(parse_int("99999999999999999999999"), Some(i64::MAX));
        assert_eq!(parse_int("-99999999999999999999999"), Some(-i64::MAX));
    }

    #[test]
    fn missing_or_unreadable_values_use_the_fallback() {
        assert_eq!(read(None, &MS).unwrap().get("ms"), 1000);
        assert_eq!(read(Some(""), &MS).unwrap().get("ms"), 1000);
        assert_eq!(read(Some("ms=abc"), &MS).unwrap().get("ms"), 1000);
        assert_eq!(read(Some("ms="), &MS).unwrap().get("ms"), 1000);
        assert_eq!(read(Some("other=3"), &MS).unwrap().get("ms"), 1000);
    }

    #[test]
    fn the_first_of_a_repeated_value_counts() {
        assert_eq!(read(Some("ms=300&ms=9"), &MS).unwrap().get("ms"), 300);
    }

    #[test]
    fn values_are_url_decoded() {
        assert_eq!(read(Some("ms=%32%30"), &MS).unwrap().get("ms"), 20);
        assert_eq!(read(Some("ms=+7"), &MS).unwrap().get("ms"), 7, "a plus is a space, and spaces are skipped");
    }

    #[test]
    fn the_limits_are_included() {
        assert_eq!(read(Some("ms=0"), &MS).unwrap().get("ms"), 0);
        assert_eq!(read(Some("ms=5000"), &MS).unwrap().get("ms"), 5000);
    }

    #[test]
    fn values_out_of_range_are_rejected_with_the_range() {
        assert_eq!(read(Some("ms=5001"), &MS).err(), Some(Rejection("ms must be between 0 and 5000".into())));
        assert_eq!(read(Some("ms=-1"), &MS).err(), Some(Rejection("ms must be between 0 and 5000".into())));
    }

    #[test]
    fn a_fallback_out_of_range_is_rejected_too() {
        let specs = [("n", Param::new(45, 0, 30))];
        assert_eq!(read(None, &specs).err(), Some(Rejection("n must be between 0 and 30".into())));
    }

    #[test]
    fn a_parameter_without_a_default_is_required() {
        let specs = [("code", Param { fallback: None, min: 200, max: 599 })];
        assert_eq!(read(None, &specs).err(), Some(Rejection("code is required".into())));
        assert_eq!(read(Some("code=204"), &specs).unwrap().get("code"), 204);
    }

    #[test]
    fn several_parameters_are_read_together_and_the_first_bad_one_is_reported() {
        let specs = [("a", Param::new(1, 0, 10)), ("b", Param::new(2, 0, 10))];
        let params = read(Some("b=7"), &specs).unwrap();
        assert_eq!((params.get("a"), params.get("b")), (1, 7));
        assert_eq!(read(Some("a=11&b=11"), &specs).err(), Some(Rejection("a must be between 0 and 10".into())));
    }

    #[test]
    fn a_rejection_is_a_400() {
        assert_eq!(Rejection("no".into()).into_response().status(), StatusCode::BAD_REQUEST);
    }
}
