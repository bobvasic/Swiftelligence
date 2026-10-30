use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::header::LOCATION;
use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use url::Url;

pub const PAGE_LIMIT: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Serialize)]
pub struct ResponseData {
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub retrieved_at: DateTime<Utc>,
    #[serde(skip)]
    pub body: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct HttpClient {
    allow_private: bool,
}

impl HttpClient {
    pub fn new(allow_private: bool) -> Self {
        Self { allow_private }
    }

    pub async fn get(
        &self,
        input: &str,
        max_bytes: usize,
        origin: Option<&str>,
    ) -> Result<ResponseData> {
        let mut url = Url::parse(input).context("invalid URL")?;
        for step in 0..=MAX_REDIRECTS {
            validate_url(&url, self.allow_private)?;
            if let Some(scope) = origin
                && url.origin().ascii_serialization() != scope
            {
                bail!("cross-origin request refused: {}", url);
            }
            let host = url.host_str().context("URL lacks host")?;
            let port = url.port_or_known_default().context("URL lacks port")?;
            let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
                .await
                .with_context(|| format!("DNS lookup failed for {host}"))?
                .collect();
            if addresses.is_empty() {
                bail!("DNS returned no addresses for {host}");
            }
            // Reject mixed public/private answers rather than relying on resolver order.
            if !self.allow_private && addresses.iter().any(|addr| !public_ip(addr.ip())) {
                bail!("non-public DNS address refused for {host}");
            }
            // The resolved addresses are pinned into a fresh client for this hop. Redirects
            // get their own DNS validation; reqwest may not resolve a hostname again.
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .connect_timeout(Duration::from_secs(8))
                .user_agent("Swiftel/0.1 (public-source research)")
                .resolve_to_addrs(host, &addresses)
                .build()?;
            let response = client
                .get(url.clone())
                .header(
                    reqwest::header::ACCEPT,
                    "text/html,application/json,application/xml,text/xml,*/*;q=0.5",
                )
                .send()
                .await
                .with_context(|| format!("request failed for {url}"))?;
            if response.status().is_redirection() {
                if step == MAX_REDIRECTS {
                    bail!("too many redirects");
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .context("redirect without Location")?;
                let location = location.to_str().context("invalid redirect Location")?;
                url = url.join(location).context("invalid redirect target")?;
                continue;
            }
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            if response
                .content_length()
                .is_some_and(|len| len > max_bytes as u64)
            {
                bail!("response exceeds {max_bytes} byte limit");
            }
            let mut body = Vec::new();
            let mut response = response;
            while let Some(chunk) = response.chunk().await? {
                if body.len().saturating_add(chunk.len()) > max_bytes {
                    bail!("response exceeds {max_bytes} byte limit");
                }
                body.extend_from_slice(&chunk);
            }
            return Ok(ResponseData {
                url: url.to_string(),
                status,
                content_type,
                retrieved_at: Utc::now(),
                body,
            });
        }
        unreachable!()
    }
}

pub fn validate_url(url: &Url, allow_private: bool) -> Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        bail!("only HTTP(S) URLs are supported");
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        bail!("URL must have a host and no embedded credentials");
    }
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    if !allow_private {
        if let Some(port) = url.port()
            && ((url.scheme() == "https" && port != 443) || (url.scheme() == "http" && port != 80))
        {
            bail!("nonstandard port refused (use --allow-private for controlled local fixtures)");
        }
        if host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
        {
            bail!("local hostname refused");
        }
        if let Ok(ip) = host.parse::<IpAddr>()
            && !public_ip(ip)
        {
            bail!("non-public IP address refused");
        }
    }
    Ok(())
}

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => public_v4(a),
        IpAddr::V6(a) => public_v6(a),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    if a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
    {
        return false;
    }
    // Remaining RFC special-purpose space; conservative for SSRF safety.
    !(a == 192 && b == 0) && !(a == 192 && b == 31)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4() {
        return public_v4(v4);
    }
    // Only global unicast. Exclude 6to4/Teredo and special-purpose IPv6 ranges.
    let s = ip.segments();
    (s[0] & 0xe000) == 0x2000
        && s[0] != 0x2002
        && !(s[0] == 0x2001 && (s[1] <= 0x0020 || s[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssrf_addresses_rejected() {
        for ip in [
            "127.0.0.1",
            "169.254.169.254",
            "10.0.0.3",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::1",
            "2001:0::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn hostile_urls_rejected() {
        for url in [
            "file:///etc/passwd",
            "http://user:pass@example.org",
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://localhost:8080/",
            "http://0177.0.0.1/",
            "http://0x7f000001/",
            "http://2130706433/",
            "http://example.org:1234/",
        ] {
            assert!(
                validate_url(&Url::parse(url).unwrap(), false).is_err(),
                "{url}"
            );
        }
        assert!(validate_url(&Url::parse("http://127.0.0.1:8080/").unwrap(), true).is_ok());
    }

    #[tokio::test]
    async fn scoped_redirect_and_body_budget_are_enforced() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0u8; 2048];
                let read = socket.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..read]);
                let response = if request.starts_with("GET /redirect") {
                    "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/metadata\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{}",
                        "x".repeat(100)
                    )
                };
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let client = HttpClient::new(true);
        let scope = format!("http://127.0.0.1:{port}");
        let error = client
            .get(&format!("{scope}/redirect"), 1024, Some(&scope))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cross-origin"));
        let error = client
            .get(&format!("{scope}/large"), 10, Some(&scope))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("byte limit"));
        server.await.unwrap();
    }
}
