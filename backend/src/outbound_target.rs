use std::future::Future;
use std::io;
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
/// `SelfHostedNtfy` also permits the shared address space used by Tailscale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboundTargetPolicy {
    PublicOnly,
    SelfHostedWebhook,
    SelfHostedNtfy,
}

pub async fn validate_outbound_url(
    input: &str,
    policy: OutboundTargetPolicy,
) -> Result<Url, String> {
    validate_outbound_url_with_resolver(input, policy, system_resolver).await
}

async fn validate_outbound_url_with_resolver<F, Fut>(
    input: &str,
    policy: OutboundTargetPolicy,
    resolver: F,
) -> Result<Url, String>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = io::Result<Vec<SocketAddr>>>,
{
    let url = Url::parse(input).map_err(|_| "URL must be an absolute URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("URL must use http or https and include a host".to_string());
    }
    resolve_allowed_addresses(&url, policy, resolver).await?;
    Ok(url)
}

pub async fn client_for_outbound_url(
    url: &Url,
    policy: OutboundTargetPolicy,
) -> Result<reqwest::Client, String> {
    client_for_outbound_url_with_resolver(url, policy, system_resolver).await
}

async fn client_for_outbound_url_with_resolver<F, Fut>(
    url: &Url,
    policy: OutboundTargetPolicy,
    resolver: F,
) -> Result<reqwest::Client, String>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = io::Result<Vec<SocketAddr>>>,
{
    let addresses = resolve_allowed_addresses(url, policy, resolver).await?;
    let builder = reqwest::Client::builder();
    // Private ntfy connections must use our validated addresses, not proxy DNS.
    // Preserve the existing proxy behavior of the other outbound policies.
    let builder = if policy == OutboundTargetPolicy::SelfHostedNtfy {
        builder.no_proxy()
    } else {
        builder
    };
    builder
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        // Use only the addresses just validated, preventing DNS rebinding.
        .resolve_to_addrs(url.host_str().expect("validated URL host"), &addresses)
        .build()
        .map_err(|_| "Could not create outbound request client".to_string())
}

async fn system_resolver(host: String, port: u16) -> io::Result<Vec<SocketAddr>> {
    lookup_host((host.as_str(), port))
        .await
        .map(|addresses| addresses.collect())
}

async fn resolve_allowed_addresses<F, Fut>(
    url: &Url,
    policy: OutboundTargetPolicy,
    resolver: F,
) -> Result<Vec<SocketAddr>, String>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = io::Result<Vec<SocketAddr>>>,
{
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("URL must use http or https and include a host".to_string());
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "URL scheme has no known default port".to_string())?;
    let addresses = match url.host() {
        Some(url::Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        Some(url::Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        Some(url::Host::Domain(host)) => {
            tokio::time::timeout(DNS_LOOKUP_TIMEOUT, resolver(host.to_string(), port))
                .await
                .map_err(|_| "Timed out resolving outbound host".to_string())?
                .map_err(|_| "Could not resolve outbound host".to_string())?
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
        OutboundTargetPolicy::SelfHostedWebhook | OutboundTargetPolicy::SelfHostedNtfy => {
            "Outbound URL must resolve only to public, private, or loopback addresses".to_string()
        }
    }
}

fn is_allowed_ip(ip: IpAddr, policy: OutboundTargetPolicy) -> bool {
    match policy {
        OutboundTargetPolicy::PublicOnly => is_public_ip(ip),
        OutboundTargetPolicy::SelfHostedNtfy => {
            let ip = match ip {
                IpAddr::V6(ip) => ip
                    .to_ipv4_mapped()
                    .map(IpAddr::V4)
                    .unwrap_or(IpAddr::V6(ip)),
                ip => ip,
            };
            is_self_hosted_webhook_ip(ip)
                || matches!(ip, IpAddr::V4(ip) if ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        }
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
    use super::{client_for_outbound_url, validate_outbound_url, OutboundTargetPolicy};
    use url::Url;

    #[tokio::test]
    async fn hostname_resolution_checks_every_answer_and_preserves_policy_boundaries() {
        use super::validate_outbound_url_with_resolver;
        for host in ["ntfy", "ntfy.embassy", "ntfy.tailnet.ts.net"] {
            for (policy, addresses, allowed) in [
                (
                    OutboundTargetPolicy::SelfHostedNtfy,
                    vec!["192.168.1.2", "100.64.0.1"],
                    true,
                ),
                (OutboundTargetPolicy::PublicOnly, vec!["192.168.1.2"], false),
                (
                    OutboundTargetPolicy::SelfHostedWebhook,
                    vec!["100.64.0.1"],
                    false,
                ),
                (
                    OutboundTargetPolicy::SelfHostedNtfy,
                    vec!["127.0.0.1", "169.254.169.254"],
                    false,
                ),
                (
                    OutboundTargetPolicy::SelfHostedNtfy,
                    vec!["169.254.169.254", "8.8.8.8"],
                    false,
                ),
                (OutboundTargetPolicy::SelfHostedNtfy, vec![], false),
            ] {
                let result = validate_outbound_url_with_resolver(
                    &format!("http://{host}:8080/topic"),
                    policy,
                    |name, port| async move {
                        assert_eq!(name, host);
                        assert_eq!(port, 8080);
                        Ok(addresses
                            .into_iter()
                            .map(|ip| std::net::SocketAddr::new(ip.parse().unwrap(), port))
                            .collect())
                    },
                )
                .await;
                assert_eq!(result.is_ok(), allowed, "{host}: {policy:?}");
            }
        }
    }

    #[tokio::test]
    async fn changed_dns_is_revalidated_before_sending() {
        use super::{client_for_outbound_url_with_resolver, validate_outbound_url_with_resolver};
        let policy = OutboundTargetPolicy::SelfHostedNtfy;
        let url = validate_outbound_url_with_resolver(
            "http://ntfy/topic",
            policy,
            |_, port| async move { Ok(vec![std::net::SocketAddr::from(([10, 0, 0, 2], port))]) },
        )
        .await
        .unwrap();
        let error = client_for_outbound_url_with_resolver(&url, policy, |_, port| async move {
            Ok(vec![std::net::SocketAddr::from((
                [169, 254, 169, 254],
                port,
            ))])
        })
        .await
        .unwrap_err();
        assert!(error.contains("Outbound URL must resolve only"));
    }

    #[tokio::test]
    async fn injected_dns_errors_are_useful() {
        let error = super::validate_outbound_url_with_resolver(
            "http://ntfy/topic",
            OutboundTargetPolicy::SelfHostedNtfy,
            |_, _| async { Err(std::io::Error::other("test DNS failure")) },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Could not resolve outbound host");
    }

    #[tokio::test]
    async fn injected_dns_timeout_is_bounded() {
        let error = super::validate_outbound_url_with_resolver(
            "http://ntfy/topic",
            OutboundTargetPolicy::SelfHostedNtfy,
            |_, _| std::future::pending(),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Timed out resolving outbound host");
    }

    #[tokio::test]
    async fn client_pins_hostname_to_validated_addresses_and_does_not_follow_redirects() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let redirected = Arc::new(AtomicUsize::new(0));
        let redirect_count = redirected.clone();
        let app = axum::Router::new()
            .route(
                "/topic",
                axum::routing::get(|| async {
                    (
                        axum::http::StatusCode::FOUND,
                        [("location", "/redirected")],
                        "ntfy receiver",
                    )
                }),
            )
            .route(
                "/redirected",
                axum::routing::get(move || async move {
                    redirect_count.fetch_add(1, Ordering::SeqCst);
                    "should not arrive"
                }),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        // This name never goes to system DNS; only our validated loopback address is used.
        let url = Url::parse(&format!("http://ntfy.invalid:{}/topic", address.port())).unwrap();
        let resolutions = AtomicUsize::new(0);
        let client = super::client_for_outbound_url_with_resolver(
            &url,
            OutboundTargetPolicy::SelfHostedNtfy,
            |name, port| {
                resolutions.fetch_add(1, Ordering::SeqCst);
                assert_eq!(name, "ntfy.invalid");
                assert_eq!(port, address.port());
                async move { Ok(vec![address]) }
            },
        )
        .await
        .unwrap();
        let response = client.get(url).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert_eq!(response.text().await.unwrap(), "ntfy receiver");
        assert_eq!(resolutions.load(Ordering::SeqCst), 1);
        assert_eq!(redirected.load(Ordering::SeqCst), 0);
        server.abort();
    }

    #[tokio::test]
    async fn self_hosted_ntfy_allows_private_loopback_and_cgnat_boundaries() {
        for address in [
            "8.8.8.8",
            "10.0.0.0",
            "10.255.255.255",
            "172.16.0.0",
            "172.31.255.255",
            "192.168.0.0",
            "192.168.255.255",
            "127.0.0.0",
            "127.255.255.255",
            "100.64.0.0",
            "100.127.255.255",
            "100.63.255.255",
            "100.128.0.0",
            "::1",
            "fc00::",
            "fdff:ffff::1",
            "2606:4700:4700::1111",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
            "::ffff:100.64.0.0",
            "::ffff:100.127.255.255",
            "::ffff:8.8.8.8",
        ] {
            let url = address_url(address);
            assert!(
                validate_outbound_url(&url, OutboundTargetPolicy::SelfHostedNtfy)
                    .await
                    .is_ok(),
                "rejected {address}"
            );
        }
    }

    fn address_url(address: &str) -> String {
        if address.contains(':') {
            format!("http://[{address}]/")
        } else {
            format!("http://{address}/")
        }
    }

    #[tokio::test]
    async fn self_hosted_ntfy_rejects_special_and_mapped_special_addresses() {
        for address in [
            "0.0.0.0",
            "0.1.2.3",
            "169.254.0.0",
            "169.254.169.254",
            "169.254.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "198.18.0.0",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.0",
            "239.255.255.255",
            "240.0.0.0",
            "255.255.255.255",
            "::",
            "fe80::1",
            "febf::1",
            "fec0::1",
            "ff00::1",
            "::127.0.0.1",
            "100::",
            "2001:db8::1",
            "2001:2::1",
            "2001:10::1",
            "2002::1",
            "64:ff9b::1",
            "::ffff:169.254.169.254",
            "::ffff:0.0.0.0",
            "::ffff:224.0.0.1",
            "::ffff:255.255.255.255",
            "::ffff:198.18.0.1",
        ] {
            assert!(
                validate_outbound_url(&address_url(address), OutboundTargetPolicy::SelfHostedNtfy)
                    .await
                    .is_err(),
                "accepted {address}"
            );
        }
    }

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
                validate_outbound_url(&url, OutboundTargetPolicy::PublicOnly)
                    .await
                    .is_err(),
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

    #[tokio::test]
    async fn outbound_client_rejects_non_http_schemes() {
        let url = Url::parse("ftp://127.0.0.1/hook").unwrap();
        assert!(
            client_for_outbound_url(&url, OutboundTargetPolicy::SelfHostedWebhook)
                .await
                .is_err()
        );
    }
}
