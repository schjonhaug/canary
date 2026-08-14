use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::net::lookup_host;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn validate_public_url(input: &str) -> Result<Url, String> {
    let url = Url::parse(input).map_err(|_| "URL must be an absolute URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("URL must use http or https and include a host".to_string());
    }
    resolve_public_addresses(&url).await?;
    Ok(url)
}

pub async fn client_for_public_url(url: &Url) -> Result<reqwest::Client, String> {
    let addresses = resolve_public_addresses(url).await?;
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        // Use only the addresses just validated, preventing DNS rebinding.
        .resolve_to_addrs(url.host_str().expect("validated URL host"), &addresses)
        .build()
        .map_err(|_| "Could not create outbound request client".to_string())
}

async fn resolve_public_addresses(url: &Url) -> Result<Vec<SocketAddr>, String> {
    let host = url.host_str().expect("validated URL host");
    let port = url
        .port_or_known_default()
        .expect("HTTP URL has a default port");
    let addresses: Vec<_> = lookup_host((host, port))
        .await
        .map_err(|_| "Could not resolve outbound host".to_string())?
        .collect();

    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("Outbound URL must resolve only to public addresses".to_string());
    }
    Ok(addresses)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.octets()[0] == 0
                || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])))
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            let mapped_ipv4 = octets[..10] == [0; 10] && octets[10] == 0xff && octets[11] == 0xff;
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || mapped_ipv4)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_public_url;

    #[tokio::test]
    async fn rejects_private_and_special_ranges() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            let url = if address.contains(':') {
                format!("http://[{address}]/")
            } else {
                format!("http://{address}/")
            };
            assert!(
                validate_public_url(&url).await.is_err(),
                "accepted {address}"
            );
        }
    }
}
