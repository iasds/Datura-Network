//! Pure address-classification and static hidden-service resolution logic,
//! with no I/O of its own.
//!
//! `classify_address` looks at a SOCKS5 CONNECT target's host string (which
//! may be a domain name or the textual form of an IP address, exactly as
//! `fast_socks5` hands it to us) and buckets it into one of a small set of
//! `AddressType`s, so that `exit_node` can log what kind of network the
//! traffic is destined for and (for `LocalIp`/`Unknown`) refuse to route it
//! at all.
//!
//! `build_hidden_service_map`/`resolve_hidden_service` implement a tiny
//! hardcoded `/etc/hosts`-style static map from `.dn` hidden-service names to
//! the loopback `SocketAddr` where a simulated hidden service is actually
//! listening, standing in for the eventual (out of scope for this PoC)
//! circuit-based hidden-service routing.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressType {
    DaturaHidden, // *.dn
    TorOnion,     // *.onion
    I2P,          // *.i2p
    Clearnet,     // *.com / *.net / *.org / any other recognized clearnet TLD
    PublicIp,     // routable IPv4/IPv6
    LocalIp,      // loopback/private/link-local/unspecified IPs, and localhost
    Unknown,      // anything else: no dot, unrecognized TLD, unparseable
}

impl AddressType {
    /// Whether this address type is a candidate for routing through some
    /// circuit at all. `LocalIp` and `Unknown` are the only two kinds this
    /// PoC always refuses outright.
    pub(crate) fn is_routable(self) -> bool {
        !matches!(self, AddressType::LocalIp | AddressType::Unknown)
    }
}

/// Common clearnet TLDs recognized for `AddressType::Clearnet`. This is
/// intentionally a small, illustrative allowlist (not the full IANA TLD
/// list): anything with a dot that isn't `.dn`/`.onion`/`.i2p` and isn't in
/// this list falls through to `Unknown` rather than being guessed at.
const CLEARNET_TLDS: &[&str] = &[
    "com", "net", "org", "io", "co", "info", "biz", "dev", "app", "xyz", "gov", "edu", "me",
];

/// Classifies a SOCKS5 CONNECT target host string into an `AddressType`.
///
/// `host` is normalized first (trailing FQDN dot stripped, lowercased) so
/// that `Example.COM.` and `example.com` classify identically. IP addresses
/// are classified using std's own loopback/private/link-local/unspecified
/// predicates rather than hand-rolled octet math, and IPv4-mapped IPv6
/// addresses are unwrapped and reclassified as their underlying IPv4
/// address rather than being treated as a distinct IPv6 case.
pub(crate) fn classify_address(host: &str) -> AddressType {
    let normalized = host.trim_end_matches('.').to_lowercase();

    if normalized == "localhost" || normalized.ends_with(".localhost") {
        return AddressType::LocalIp;
    }

    if let Ok(ip_address) = normalized.parse::<IpAddr>() {
        return classify_ip(ip_address);
    }

    let top_level_domain = normalized.rsplit('.').next().unwrap_or("");
    match top_level_domain {
        "dn" => AddressType::DaturaHidden,
        "onion" => AddressType::TorOnion,
        "i2p" => AddressType::I2P,
        tld if !tld.is_empty() && normalized.contains('.') && CLEARNET_TLDS.contains(&tld) => {
            AddressType::Clearnet
        }
        _ => AddressType::Unknown,
    }
}

fn classify_ip(ip_address: IpAddr) -> AddressType {
    match ip_address {
        IpAddr::V4(v4) => {
            if v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() {
                AddressType::LocalIp
            } else {
                AddressType::PublicIp
            }
        }
        IpAddr::V6(v6) => {
            // Unwrap IPv4-mapped IPv6 addresses (::ffff:a.b.c.d) and
            // reclassify on the underlying IPv4 address rather than treating
            // them as a distinct IPv6 case.
            if let Some(mapped_v4) = v6.to_ipv4_mapped() {
                return classify_ip(IpAddr::V4(mapped_v4));
            }

            if v6.is_loopback() || v6.is_unspecified() {
                return AddressType::LocalIp;
            }

            let segments = v6.segments();
            // Unique Local Address: fc00::/7 (segments[0] & 0xfe00 == 0xfc00).
            // Neither `Ipv6Addr::is_unique_local` nor `is_unicast_link_local`
            // is stable Rust as of this writing, hence the manual bitmask
            // checks instead of those (nightly-only) predicates.
            let is_unique_local = (segments[0] & 0xfe00) == 0xfc00;
            // Link-local: fe80::/10 (segments[0] & 0xffc0 == 0xfe80).
            let is_link_local = (segments[0] & 0xffc0) == 0xfe80;

            if is_unique_local || is_link_local {
                AddressType::LocalIp
            } else {
                AddressType::PublicIp
            }
        }
    }
}

pub(crate) type HiddenServiceMap = HashMap<String, SocketAddr>;

/// Builds a hardcoded static map of `.dn` hidden-service names to the
/// loopback `SocketAddr` where a simulated hidden service is actually
/// listening, standing in for `/etc/hosts`-style static DNS for this PoC.
/// Keys are stored lowercase; look up through `resolve_hidden_service`,
/// which lowercases its query for you.
pub(crate) fn build_hidden_service_map() -> HiddenServiceMap {
    let mut map = HiddenServiceMap::new();
    map.insert(
        "hiddenserviceajshhsbdbdbdb.dn".to_string(),
        "127.0.0.1:5001"
            .parse()
            .expect("bad hidden-service map entry"),
    );
    map
}

/// Looks up `host` (case-insensitively) in `map`, returning the loopback
/// `SocketAddr` a simulated hidden service is actually listening on, if any.
/// The mapped port always wins over whatever port the client originally
/// asked for -- that is a deliberate design choice, since the mapped port is
/// where the simulated hidden service actually listens, not wherever the
/// client happened to ask for.
pub(crate) fn resolve_hidden_service(map: &HiddenServiceMap, host: &str) -> Option<SocketAddr> {
    map.get(&host.to_lowercase()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_datura_hidden() {
        assert_eq!(
            classify_address("hiddenserviceajshhsbdbdbdb.dn"),
            AddressType::DaturaHidden
        );
    }

    #[test]
    fn classifies_tor_onion() {
        assert_eq!(
            classify_address("exampleonionaddress.onion"),
            AddressType::TorOnion
        );
    }

    #[test]
    fn classifies_i2p() {
        assert_eq!(classify_address("exampleaddress.i2p"), AddressType::I2P);
    }

    #[test]
    fn classifies_clearnet() {
        assert_eq!(classify_address("example.com"), AddressType::Clearnet);
    }

    #[test]
    fn classifies_public_ip() {
        assert_eq!(classify_address("8.8.8.8"), AddressType::PublicIp);
    }

    #[test]
    fn classifies_local_ip() {
        assert_eq!(classify_address("127.0.0.1"), AddressType::LocalIp);
        assert_eq!(classify_address("192.168.1.1"), AddressType::LocalIp);
        assert_eq!(classify_address("10.0.0.1"), AddressType::LocalIp);
        assert_eq!(classify_address("172.16.0.1"), AddressType::LocalIp);
    }

    #[test]
    fn classifies_unknown() {
        assert_eq!(
            classify_address("not-a-real-tld-example"),
            AddressType::Unknown
        );
        assert_eq!(classify_address("example.zzqq"), AddressType::Unknown);
    }

    #[test]
    fn classifies_localhost_and_subdomain() {
        assert_eq!(classify_address("localhost"), AddressType::LocalIp);
        assert_eq!(classify_address("sub.localhost"), AddressType::LocalIp);
    }

    #[test]
    fn classifies_ipv4_mapped_ipv6_loopback() {
        assert_eq!(classify_address("::ffff:127.0.0.1"), AddressType::LocalIp);
    }

    #[test]
    fn classifies_link_local_ipv4() {
        assert_eq!(classify_address("169.254.1.1"), AddressType::LocalIp);
    }

    #[test]
    fn classifies_trailing_dot_fqdn() {
        assert_eq!(classify_address("example.com."), AddressType::Clearnet);
    }

    #[test]
    fn classifies_mixed_case() {
        assert_eq!(classify_address("Example.COM"), AddressType::Clearnet);
        assert_eq!(
            classify_address("HiddenServiceAjshhsbdbdbdb.DN"),
            AddressType::DaturaHidden
        );
    }

    #[test]
    fn resolves_hidden_service_case_insensitively() {
        let map = build_hidden_service_map();
        let resolved = resolve_hidden_service(&map, "HiddenServiceAjshhsbdbdbdb.DN");
        assert_eq!(resolved, Some("127.0.0.1:5001".parse().unwrap()));
    }

    #[test]
    fn resolves_unknown_hidden_service_to_none() {
        let map = build_hidden_service_map();
        assert_eq!(resolve_hidden_service(&map, "nonexistent.dn"), None);
    }

    /// Direct per-variant coverage of `AddressType::is_routable`: every
    /// variant is asserted individually (rather than relying on the
    /// indirect coverage `classify_address` tests above give it) so a
    /// future addition of a new `AddressType` variant is forced to make an
    /// explicit, considered decision about its routability here.
    #[test]
    fn is_routable_holds_for_every_variant() {
        assert!(AddressType::DaturaHidden.is_routable());
        assert!(AddressType::TorOnion.is_routable());
        assert!(AddressType::I2P.is_routable());
        assert!(AddressType::Clearnet.is_routable());
        assert!(AddressType::PublicIp.is_routable());
        assert!(!AddressType::LocalIp.is_routable());
        assert!(!AddressType::Unknown.is_routable());
    }
}
