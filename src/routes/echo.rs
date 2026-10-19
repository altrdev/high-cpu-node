use crate::app::AppState;
use crate::trust::Trust;
use axum::extract::{ConnectInfo, OriginalUri, State};
use axum::http::{HeaderMap, Method, Version};
use axum::Json;
use serde_json::{Map, Value, json};
use std::net::SocketAddr;
use std::sync::Arc;

// Headers that carry credentials are never echoed back
const MASKED: [&str; 4] = ["authorization", "proxy-authorization", "cookie", "set-cookie"];

fn http_version(version: Version) -> &'static str {
    match version {
        Version::HTTP_09 => "0.9",
        Version::HTTP_10 => "1.0",
        Version::HTTP_2 => "2.0",
        Version::HTTP_3 => "3.0",
        _ => "1.1",
    }
}

// A header as Node gives it: repeated ones joined with a comma
fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let values: Vec<String> = headers.get_all(name).iter().map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned()).collect();
    (!values.is_empty()).then(|| values.join(", "))
}

// req.protocol of Express: the forwarded one when the socket is a trusted proxy
fn protocol(trust: &Trust, remote: &str, headers: &HeaderMap) -> String {
    if !trust.trusts(remote, 0) {
        return "http".into();
    }
    let forwarded = header_value(headers, "x-forwarded-proto").filter(|v| !v.is_empty()).unwrap_or_else(|| "http".into());
    forwarded.split(',').next().unwrap_or("").trim().to_string()
}

// req.hostname of Express: the forwarded host when the socket is a trusted proxy, without the port
fn hostname(trust: &Trust, remote: &str, headers: &HeaderMap) -> Option<String> {
    let forwarded = header_value(headers, "x-forwarded-host").filter(|v| !v.is_empty());
    let host = match forwarded {
        Some(host) if trust.trusts(remote, 0) => match host.find(',') {
            Some(comma) => host[..comma].trim_end().to_string(),
            None => host,
        },
        _ => header_value(headers, "host").unwrap_or_default(),
    };
    if host.is_empty() {
        return None;
    }
    let offset = if host.starts_with('[') { host.find(']').map_or(0, |i| i + 1) } else { 0 };
    Some(match host[offset..].find(':') {
        Some(colon) => host[..offset + colon].to_string(),
        None => host,
    })
}

pub fn describe(trust: &Trust, method: &Method, url: &str, version: Version, headers: &HeaderMap, remote: &str) -> Value {
    let addresses = trust.addresses(remote, header_value(headers, "x-forwarded-for").as_deref());
    let ips: Vec<&String> = addresses.iter().skip(1).rev().collect();
    let protocol = protocol(trust, remote, headers);

    let mut echo = Map::new();
    echo.insert("method".into(), json!(method.as_str()));
    echo.insert("url".into(), json!(url));
    echo.insert("secure".into(), json!(protocol == "https"));
    echo.insert("protocol".into(), json!(protocol));
    echo.insert("httpVersion".into(), json!(http_version(version)));
    if let Some(hostname) = hostname(trust, remote, headers) {
        echo.insert("hostname".into(), json!(hostname));
    }
    echo.insert("ip".into(), json!(addresses.last()));
    echo.insert("ips".into(), json!(ips));
    echo.insert("remoteAddress".into(), json!(remote));

    let mut shown = Map::new();
    for name in headers.keys() {
        let value = if MASKED.contains(&name.as_str()) { Some("[masked]".to_string()) } else { header_value(headers, name.as_str()) };
        shown.insert(name.as_str().to_string(), json!(value));
    }
    echo.insert("headers".into(), Value::Object(shown));
    // Express lists protocol before secure
    let mut ordered = Map::new();
    for key in ["method", "url", "protocol", "secure", "httpVersion", "hostname", "ip", "ips", "remoteAddress", "headers"] {
        if let Some(value) = echo.remove(key) {
            ordered.insert(key.into(), value);
        }
    }
    Value::Object(ordered)
}

// Returns the request as the server sees it, to show what a proxy, an ingress controller or a
// load balancer adds, removes or rewrites (Host, X-Forwarded-*, the client address).
// `ip` follows TRUST_PROXY, so it shows the address a proxy would pass on.
pub async fn handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    version: Version,
    headers: HeaderMap,
) -> Json<Value> {
    let url = uri.path_and_query().map_or("/", |p| p.as_str());
    Json(describe(&state.trust, &method, url, version, &headers, &peer.ip().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TrustProxy;
    use axum::http::header;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(header::HeaderName::from_bytes(name.as_bytes()).unwrap(), value.parse().unwrap());
        }
        map
    }

    fn echo(trust: &Trust, pairs: &[(&str, &str)]) -> Value {
        describe(trust, &Method::GET, "/echo?a=1", Version::HTTP_11, &headers(pairs), "127.0.0.1")
    }

    #[test]
    fn what_the_server_sees_by_default() {
        let value = echo(&Trust::none(), &[("host", "127.0.0.1:3000"), ("authorization", "Bearer s3cret"), ("cookie", "sid=abc"), ("x-forwarded-for", "203.0.113.7"), ("x-custom", "yes")]);
        assert_eq!(value["method"], "GET");
        assert_eq!(value["url"], "/echo?a=1");
        assert_eq!(value["protocol"], "http");
        assert_eq!(value["secure"], false);
        assert_eq!(value["hostname"], "127.0.0.1");
        assert_eq!(value["ip"], "127.0.0.1", "X-Forwarded-For is not trusted by default");
        assert_eq!(value["ips"], json!([]));
        assert_eq!(value["headers"]["x-custom"], "yes");
        assert_eq!(value["headers"]["authorization"], "[masked]");
        assert_eq!(value["headers"]["cookie"], "[masked]");
        assert!(!value.to_string().contains("s3cret"));
    }

    #[test]
    fn with_trust_the_forwarded_headers_decide() {
        let trust = Trust::from_setting(&TrustProxy::Bool(true)).unwrap();
        let value = echo(&trust, &[("host", "10.0.0.5:8080"), ("x-forwarded-for", "203.0.113.7"), ("x-forwarded-proto", "https"), ("x-forwarded-host", "app.example.com")]);
        assert_eq!(value["ip"], "203.0.113.7");
        assert_eq!(value["ips"], json!(["203.0.113.7"]));
        assert_eq!(value["protocol"], "https");
        assert_eq!(value["secure"], true);
        assert_eq!(value["hostname"], "app.example.com");
    }

    #[test]
    fn forwarded_headers_from_a_socket_that_is_not_trusted_are_ignored() {
        let trust = Trust::from_setting(&TrustProxy::List("10.0.0.0/8".into())).unwrap();
        let value = echo(&trust, &[("host", "real.test"), ("x-forwarded-proto", "https"), ("x-forwarded-host", "fake.test")]);
        assert_eq!((value["protocol"].as_str(), value["hostname"].as_str()), (Some("http"), Some("real.test")));
    }

    #[test]
    fn hostnames_lose_the_port_and_keep_ipv6_brackets() {
        let none = Trust::none();
        assert_eq!(hostname(&none, "1.1.1.1", &headers(&[("host", "example.com:8080")])).as_deref(), Some("example.com"));
        assert_eq!(hostname(&none, "1.1.1.1", &headers(&[("host", "[::1]:3000")])).as_deref(), Some("[::1]"));
        assert_eq!(hostname(&none, "1.1.1.1", &headers(&[])), None);
        let all = Trust::All;
        assert_eq!(hostname(&all, "1.1.1.1", &headers(&[("x-forwarded-host", "a.test , b.test")])).as_deref(), Some("a.test"));
    }

    #[test]
    fn only_the_first_forwarded_protocol_counts() {
        let all = Trust::All;
        assert_eq!(protocol(&all, "1.1.1.1", &headers(&[("x-forwarded-proto", "https, http")])), "https");
        assert_eq!(protocol(&all, "1.1.1.1", &headers(&[])), "http");
    }
}
