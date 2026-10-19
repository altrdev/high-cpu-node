use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

// A parameter with its default and limits. Without a default it is required.
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

// The integer values of a request, in the order of its specs
pub struct Params(Vec<(&'static str, i64)>);

impl Params {
    pub fn get(&self, name: &str) -> i64 {
        self.0.iter().find(|(key, _)| *key == name).map(|(_, value)| *value).expect("a parameter that is in the specs")
    }
}

// What JavaScript's parseInt makes of a string: leading spaces, a sign, then digits (hexadecimal
// after 0x), up to the first character that is not one. None is NaN.
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

// Reads integer query parameters. A value that is not a number becomes the fallback, one
// outside min..max is answered with a 400, so the caller only has to propagate the Err.
pub fn read(query: Option<&str>, specs: &[(&'static str, Param)]) -> Result<Params, Response> {
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query.unwrap_or("")).unwrap_or_default();
    let mut values = Vec::with_capacity(specs.len());
    for (name, Param { fallback, min, max }) in specs {
        let parsed = pairs.iter().find(|(key, _)| key == name).and_then(|(_, value)| parse_int(value));
        let Some(value) = parsed.or(*fallback) else {
            return Err((StatusCode::BAD_REQUEST, format!("{name} is required")).into_response());
        };
        if value < *min || value > *max {
            return Err((StatusCode::BAD_REQUEST, format!("{name} must be between {min} and {max}")).into_response());
        }
        values.push((*name, value));
    }
    Ok(Params(values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_int_like_javascript() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("  7"), Some(7));
        assert_eq!(parse_int("12abc"), Some(12));
        assert_eq!(parse_int("-5"), Some(-5));
        assert_eq!(parse_int("+5"), Some(5));
        assert_eq!(parse_int("0x10"), Some(16));
        assert_eq!(parse_int("2.9"), Some(2));
        assert_eq!(parse_int("abc"), None);
        assert_eq!(parse_int(""), None);
        assert_eq!(parse_int("-"), None);
        assert_eq!(parse_int("99999999999999999999999"), Some(i64::MAX));
    }

    #[test]
    fn missing_or_unreadable_values_use_the_fallback() {
        let specs = [("ms", Param::new(1000, 0, 5000))];
        assert_eq!(read(None, &specs).ok().unwrap().get("ms"), 1000);
        assert_eq!(read(Some("ms=abc"), &specs).ok().unwrap().get("ms"), 1000);
        assert_eq!(read(Some("ms=300&ms=9"), &specs).ok().unwrap().get("ms"), 300);
    }

    #[test]
    fn values_out_of_range_are_rejected() {
        let specs = [("ms", Param::new(1000, 0, 5000))];
        for query in ["ms=6000", "ms=-1"] {
            let response = read(Some(query), &specs).err().expect("rejected");
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }
}
