//! A small HTTP client, for the commands that ask a server how it is. Only plain HTTP is spoken:
//! to reach a server behind HTTPS, use a port-forward.
use std::time::Duration;

/// GET `url`: the status and the body
pub fn get(url: &str, timeout: Duration) -> Result<(u16, String), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into();
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// The address of `/info` on the server that `base` points to, whatever the path of `base`
pub fn info_url(base: &str) -> Result<String, String> {
    let (scheme, rest) = base.split_once("://").ok_or("Invalid URL")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if scheme.is_empty() || authority.is_empty() {
        return Err("Invalid URL".into());
    }
    Ok(format!("{scheme}://{authority}/info"))
}

/// The port of the local server: `PORT`, or 3000
pub fn port() -> String {
    std::env::var("PORT").ok().filter(|port| !port.is_empty()).unwrap_or_else(|| "3000".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_is_asked_to_the_server_whatever_the_path_of_the_url() {
        assert_eq!(info_url("http://127.0.0.1:3000").unwrap(), "http://127.0.0.1:3000/info");
        assert_eq!(info_url("http://host:3000/some/path?x=1").unwrap(), "http://host:3000/info");
        assert_eq!(info_url("http://host#top").unwrap(), "http://host/info");
    }

    #[test]
    fn an_address_without_a_scheme_or_a_host_is_invalid() {
        for url in ["127.0.0.1:3000", "http://", "://host", "", "http:///path"] {
            assert_eq!(info_url(url), Err("Invalid URL".to_string()), "{url:?}");
        }
    }

    #[test]
    fn a_server_that_is_not_there_is_an_error_and_not_a_hang() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert!(get(&format!("http://127.0.0.1:{port}/"), Duration::from_secs(2)).is_err());
    }
}
