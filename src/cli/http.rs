//! A small HTTP client, for the commands that ask a server how it is. Only plain HTTP is spoken:
//! to reach a server behind HTTPS, use a port-forward.
use std::time::Duration;

/// GET `url`: the status and the body
pub fn get(url: &str, timeout: Duration) -> Result<(u16, String), String> {
    get_with_token(url, None, timeout)
}

/// GET `url`, with `Authorization: Bearer <token>` when there is a token
pub fn get_with_token(url: &str, token: Option<&str>, timeout: Duration) -> Result<(u16, String), String> {
    let mut request = agent(timeout).get(url);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let mut response = request.call().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok((status, body))
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into()
}

/// A request with a method other than GET: `POST` with `body`, or `DELETE`
pub fn send(method: &str, url: &str, token: Option<&str>, body: &str, timeout: Duration) -> Result<(u16, String), String> {
    let agent = agent(timeout);
    let authorization = token.map(|token| format!("Bearer {token}"));
    let mut response = if method == "POST" {
        let mut request = agent.post(url);
        if let Some(authorization) = &authorization {
            request = request.header("Authorization", authorization);
        }
        request.send(body.as_bytes()).map_err(|e| e.to_string())?
    } else {
        let mut request = agent.delete(url);
        if let Some(authorization) = &authorization {
            request = request.header("Authorization", authorization);
        }
        request.call().map_err(|e| e.to_string())?
    };
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// The address of `/info` on the server that `base` points to, whatever the path of `base`
pub fn info_url(base: &str) -> Result<String, String> {
    endpoint_url(base, "/info")
}

/// The address of `path` (which starts with a `/`) on the server that `base` points to
pub fn endpoint_url(base: &str, path: &str) -> Result<String, String> {
    let (scheme, rest) = base.split_once("://").ok_or("Invalid URL")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if scheme.is_empty() || authority.is_empty() {
        return Err("Invalid URL".into());
    }
    Ok(format!("{scheme}://{authority}{path}"))
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
