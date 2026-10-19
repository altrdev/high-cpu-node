//! Content negotiation, for the one place that needs it: given the `Accept` header, does the client
//! prefer HTML to JSON? A browser sends `text/html` first, curl and most libraries send `*/*` or nothing.

struct Range<'a> {
    kind: &'a str,
    subtype: &'a str,
    quality: f64,
}

fn parse(header: &str) -> Vec<Range<'_>> {
    header
        .split(',')
        .filter_map(|part| {
            let mut pieces = part.split(';');
            let (kind, subtype) = pieces.next()?.trim().split_once('/')?;
            let quality = pieces.filter_map(|p| p.trim().strip_prefix("q=")).find_map(|q| q.trim().parse::<f64>().ok()).unwrap_or(1.0);
            Some(Range { kind: kind.trim(), subtype: subtype.trim(), quality })
        })
        .collect()
}

// (quality, specificity, earlier in the header is better) of the range that fits a media type best
fn score(ranges: &[Range], kind: &str, subtype: &str) -> Option<(f64, u8, isize)> {
    ranges
        .iter()
        .enumerate()
        .filter_map(|(index, range)| {
            let specificity = match (range.kind, range.subtype) {
                (k, s) if k.eq_ignore_ascii_case(kind) && s.eq_ignore_ascii_case(subtype) => 3,
                (k, "*") if k.eq_ignore_ascii_case(kind) => 2,
                ("*", "*") => 1,
                _ => return None,
            };
            (range.quality > 0.0).then_some((range.quality, specificity, -(index as isize)))
        })
        .max_by(|a, b| (a.1, a.0, a.2).partial_cmp(&(b.1, b.0, b.2)).unwrap())
}

/// JSON is what the API answers unless the client prefers HTML. With no preference (no header, or
/// `*/*`) it is JSON; between equals, the one listed first in the header wins.
pub fn prefers_html(header: Option<&str>) -> bool {
    let Some(header) = header else { return false };
    let ranges = parse(header);
    let (Some(html), json) = (score(&ranges, "text", "html"), score(&ranges, "application", "json")) else {
        return false;
    };
    match json {
        None => true,
        Some(json) => (html.0, html.1, html.2) > (json.0, json.1, json.2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_browser_gets_html() {
        assert!(prefers_html(Some("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")));
        assert!(prefers_html(Some("text/html")));
    }

    #[test]
    fn everything_else_gets_json() {
        assert!(!prefers_html(None));
        assert!(!prefers_html(Some("*/*")));
        assert!(!prefers_html(Some("application/json")));
        assert!(!prefers_html(Some("application/json, text/html;q=0.5")));
        assert!(!prefers_html(Some("text/html;q=0")));
        assert!(!prefers_html(Some("garbage")));
    }

    #[test]
    fn quality_decides_before_the_order() {
        assert!(prefers_html(Some("application/json;q=0.5, text/html")));
        assert!(!prefers_html(Some("text/html;q=0.5, application/json")));
        assert!(prefers_html(Some("text/html;q=0.9, */*;q=0.1")));
    }

    #[test]
    fn a_more_specific_range_wins_over_a_wildcard() {
        assert!(prefers_html(Some("text/html, application/*")), "text/html is exact and listed first");
        assert!(!prefers_html(Some("application/json, text/*")));
    }

    #[test]
    fn media_types_are_not_case_sensitive_and_spaces_do_not_matter() {
        assert!(prefers_html(Some(" Text/HTML ;  q=1 ")));
    }

    #[test]
    fn on_a_tie_the_one_listed_first_wins_and_json_wins_without_a_preference() {
        assert!(prefers_html(Some("text/*, application/*")));
        assert!(!prefers_html(Some("application/*, text/*")));
        assert!(!prefers_html(Some("*/*")));
    }
}
