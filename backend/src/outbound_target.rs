use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::lookup_host;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Address policy for operator-controlled outbound HTTP clients.
///
/// `PublicOnly` is the default SSRF posture for untrusted destinations.
/// `SelfHostedWebhook` additionally allows private/LAN and loopback targets
/// because webhooks exist only in self-hosted mode, where the operator
/// controls both the Canary host and the destination network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboundTargetPolicy {
    PublicOnly,
    SelfHostedWebhook,
}

pub async fn validate_public_url(input: &str) -> Result<Url, String> {
    validate_outbound_url(input, OutboundTargetPolicy::PublicOnly).await
}

pub async fn client_for_public_url(url: &Url) -> Result<reqwest::Client, String> {
    client_for_outbound_url(url, OutboundTargetPolicy::PublicOnly).await
}

pub async fn validate_outbound_url(
    input: &str,
    policy: OutboundTargetPolicy,
) -> Result<Url, String> {
    let url = Url::parse(input).map_err(|_| "URL must be an absolute URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("URL must use http or https and include a host".to_string());
    }
    resolve_allowed_addresses(&url, policy).await?;
    Ok(url)
}

pub async fn client_for_outbound_url(
    url: &Url,
    policy: OutboundTargetPolicy,
) -> Result<reqwest::Client, String> {
    let addresses = resolve_allowed_addresses(url, policy).await?;
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        // Use only the addresses just validated, preventing DNS rebinding.
        .resolve_to_addrs(url.host_str().expect("validated URL host"), &addresses)
        .build()
        .map_err(|_| "Could not create outbound request client".to_string())
}

async fn resolve_allowed_addresses(
    url: &Url,
    policy: OutboundTargetPolicy,
) -> Result<Vec<SocketAddr>, String> {
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "URL scheme has no known default port".to_string())?;
    let addresses = match url.host() {
        Some(url::Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        Some(url::Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        Some(url::Host::Domain(host)) => {
            tokio::time::timeout(DNS_LOOKUP_TIMEOUT, lookup_host((host, port)))
                .await
                .map_err(|_| "Timed out resolving outbound host".to_string())?
                .map_err(|_| "Could not resolve outbound host".to_string())?
                .collect()
        }
        None => return Err("URL must use http or https and include a host".to_string()),
    };

    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !is_allowed_ip(address.ip(), policy))
    {
        return Err(disallowed_address_error(policy));
    }
    Ok(addresses)
}

fn disallowed_address_error(policy: OutboundTargetPolicy) -> String {
    match policy {
        OutboundTargetPolicy::PublicOnly => {
            "Outbound URL must resolve only to public addresses".to_string()
        }
        OutboundTargetPolicy::SelfHostedWebhook => {
            "Outbound URL must resolve only to public, private, or loopback addresses".to_string()
        }
    }
}

fn is_allowed_ip(ip: IpAddr, policy: OutboundTargetPolicy) -> bool {
    match policy {
        OutboundTargetPolicy::PublicOnly => is_public_ip(ip),
        OutboundTargetPolicy::SelfHostedWebhook => is_self_hosted_webhook_ip(ip),
    }
}

fn is_self_hosted_webhook_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_self_hosted_webhook_ipv4(ip),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_self_hosted_webhook_ipv4(mapped);
            }
            ip.is_loopback() || ip.is_unique_local() || is_public_ip(IpAddr::V6(ip))
        }
    }
}

fn is_self_hosted_webhook_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || is_public_ip(IpAddr::V4(ip))
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
                || ip.octets()[0] >= 224
                || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
                || (ip.octets()[0] == 192 && matches!(ip.octets()[1], 0 | 168))
                || (ip.octets()[0] == 198 && matches!(ip.octets()[1], 18 | 19))
                || (ip.octets()[0] == 198 && ip.octets()[1] == 51 && ip.octets()[2] == 100)
                || (ip.octets()[0] == 203 && ip.octets()[1] == 0 && ip.octets()[2] == 113))
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0xc0)
                || octets[..12] == [0; 12]
                || (octets[0] == 0x01 && octets[1] == 0x00 && octets[2..] == [0; 14])
                || (octets[0] == 0x20
                    && octets[1] == 0x01
                    && matches!(octets[2], 0x00 | 0x02 | 0x0d | 0x10))
                || (octets[0] == 0x20 && octets[1] == 0x02)
                || (octets[0] == 0x00
                    && octets[1] == 0x64
                    && octets[2] == 0xff
                    && octets[3] == 0x9b)
                || ip.to_ipv4_mapped().is_some())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_outbound_url, validate_public_url, OutboundTargetPolicy};

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
            "192.0.0.1",
            "198.18.0.1",
            "240.0.0.1",
            "fec0::1",
            "::127.0.0.1",
            "100::",
            "2001:db8::1",
            "2001:2::1",
            "2001:10::1",
            "2002::1",
            "64:ff9b::1",
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

    #[tokio::test]
    async fn self_hosted_webhooks_allow_private_and_loopback_but_not_special_ranges() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.0.1",
            "::1",
            "fc00::1",
            "::ffff:192.168.1.10",
            "::ffff:127.0.0.1",
        ] {
            let url = if address.contains(':') {
                format!("http://[{address}]/")
            } else {
                format!("http://{address}/")
            };
            assert!(
                validate_outbound_url(&url, OutboundTargetPolicy::SelfHostedWebhook)
                    .await
                    .is_ok(),
                "rejected {address}"
            );
        }

        for address in [
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "fe80::1",
            "::ffff:169.254.169.254",
            "192.0.0.1",
            "198.18.0.1",
            "240.0.0.1",
            "fec0::1",
            "100::",
            "2001:db8::1",
        ] {
            let url = if address.contains(':') {
                format!("http://[{address}]/")
            } else {
                format!("http://{address}/")
            };
            assert!(
                validate_outbound_url(&url, OutboundTargetPolicy::SelfHostedWebhook)
                    .await
                    .is_err(),
                "accepted {address}"
            );
        }
    }
}
