// TRUST_PROXY and the addresses that follow from it, as Express 5 and proxy-addr work them out:
// the client is the first address, walking back from the socket through X-Forwarded-For, that
// does not belong to a trusted proxy.
use crate::config::TrustProxy;
use std::net::IpAddr;

const ALIASES: [(&str, &[&str]); 3] = [
    ("linklocal", &["169.254.0.0/16", "fe80::/10"]),
    ("loopback", &["127.0.0.1/8", "::1/128"]),
    ("uniquelocal", &["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fc00::/7"]),
];

#[derive(Debug, PartialEq)]
pub struct Range {
    ip: IpAddr,
    prefix: u32,
}

#[derive(Debug, PartialEq)]
pub enum Trust {
    // every proxy: TRUST_PROXY=true
    All,
    // the first n hops from the application
    Hops(i64),
    // addresses and subnets; none at all is the default
    Ranges(Vec<Range>),
}

fn is_mapped(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some())
}

fn same_prefix(a: u128, b: u128, width: u32, prefix: u32) -> bool {
    prefix == 0 || (a >> (width - prefix)) == (b >> (width - prefix))
}

// "255.255.0.0" as the 16 it stands for. None when the ones are not all at the start.
fn prefix_from_netmask(mask: &str) -> Option<u32> {
    let mask: u32 = mask.parse::<std::net::Ipv4Addr>().ok()?.into();
    (mask.leading_ones() + mask.trailing_zeros() == 32).then(|| mask.leading_ones())
}

// An address ("10.0.0.1") or a subnet ("10.0.0.0/8", "10.0.0.0/255.0.0.0")
fn parse_range(note: &str) -> Result<Range, String> {
    let (address, mask) = match note.rfind('/') {
        Some(at) => (&note[..at], Some(&note[at + 1..])),
        None => (note, None),
    };
    let mut ip: IpAddr = address.parse().map_err(|_| format!("invalid IP address: {address}"))?;
    if mask.is_none() {
        if let IpAddr::V6(v6) = ip {
            if let Some(v4) = v6.to_ipv4_mapped() {
                ip = IpAddr::V4(v4); // stored as IPv4
            }
        }
    }
    let max = if ip.is_ipv6() { 128 } else { 32 };
    let prefix = match mask {
        None => Some(max),
        Some(mask) if !mask.is_empty() && mask.bytes().all(|b| b.is_ascii_digit()) => mask.parse().ok(),
        Some(mask) if ip.is_ipv4() => prefix_from_netmask(mask),
        Some(_) => None,
    };
    match prefix {
        Some(prefix) if prefix > 0 && prefix <= max => Ok(Range { ip, prefix }),
        _ => Err(format!("invalid range on address: {note}")),
    }
}

impl Range {
    fn covers(&self, candidate: IpAddr) -> bool {
        // an IPv4-mapped address is the IPv4 one
        let candidate = match candidate {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(candidate, IpAddr::V4),
            v4 => v4,
        };
        match (candidate, self.ip) {
            (IpAddr::V4(a), IpAddr::V4(b)) => same_prefix(u32::from(a).into(), u32::from(b).into(), 32, self.prefix),
            // a native IPv6 address can't be in an IPv4-mapped subnet
            (IpAddr::V6(a), IpAddr::V6(b)) => !is_mapped(self.ip) && same_prefix(a.into(), b.into(), 128, self.prefix),
            // an IPv6 subnet spans IPv4 only when it is a mapped one that covers the ::ffff: marker
            (IpAddr::V4(a), IpAddr::V6(b)) => is_mapped(self.ip) && self.prefix >= 96 && same_prefix(a.to_ipv6_mapped().into(), b.into(), 128, self.prefix),
            (IpAddr::V6(_), IpAddr::V4(_)) => false,
        }
    }
}

impl Trust {
    pub fn none() -> Trust {
        Trust::Ranges(Vec::new())
    }

    pub fn from_setting(setting: &TrustProxy) -> Result<Trust, String> {
        match setting {
            TrustProxy::Bool(true) => Ok(Trust::All),
            TrustProxy::Bool(false) => Ok(Trust::none()),
            TrustProxy::Hops(hops) => Ok(Trust::Hops(*hops)),
            TrustProxy::List(list) => {
                let mut ranges = Vec::new();
                for note in list.split(',').map(str::trim) {
                    match ALIASES.iter().find(|(name, _)| *name == note) {
                        Some((_, subnets)) => {
                            for subnet in *subnets {
                                ranges.push(parse_range(subnet)?);
                            }
                        }
                        None => ranges.push(parse_range(note)?),
                    }
                }
                Ok(Trust::Ranges(ranges))
            }
        }
    }

    // Whether the proxy at `address`, `hop` steps away from the application, is trusted
    pub fn trusts(&self, address: &str, hop: usize) -> bool {
        match self {
            Trust::All => true,
            Trust::Hops(hops) => (hop as i64) < *hops,
            Trust::Ranges(ranges) => match address.parse::<IpAddr>() {
                Ok(ip) => ranges.iter().any(|range| range.covers(ip)),
                Err(_) => false,
            },
        }
    }

    // The socket address followed by X-Forwarded-For from the nearest to the farthest, up to the
    // first one that is not trusted. The last one is the client.
    pub fn addresses(&self, socket: &str, forwarded_for: Option<&str>) -> Vec<String> {
        let mut addresses = vec![socket.to_string()];
        let listed = forwarded_for.unwrap_or("").split(',').map(|part| part.trim_matches(' ')).filter(|part| !part.is_empty());
        addresses.extend(listed.rev().map(String::from));
        for hop in 0..addresses.len().saturating_sub(1) {
            if !self.trusts(&addresses[hop], hop) {
                addresses.truncate(hop + 1);
                break;
            }
        }
        addresses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trust(list: &str) -> Trust {
        Trust::from_setting(&TrustProxy::List(list.to_string())).unwrap()
    }

    #[test]
    fn nothing_is_trusted_by_default() {
        let none = Trust::none();
        assert!(!none.trusts("127.0.0.1", 0));
        assert_eq!(none.addresses("10.0.0.1", Some("203.0.113.7")), ["10.0.0.1"]);
    }

    #[test]
    fn true_trusts_every_hop() {
        let all = Trust::All;
        assert_eq!(all.addresses("10.0.0.1", Some("203.0.113.7, 198.51.100.2")), ["10.0.0.1", "198.51.100.2", "203.0.113.7"]);
    }

    #[test]
    fn a_number_trusts_that_many_hops() {
        let one = Trust::Hops(1);
        // only the socket is a trusted proxy, so the nearest address in the header is the client
        assert_eq!(one.addresses("10.0.0.1", Some("203.0.113.7, 198.51.100.2")), ["10.0.0.1", "198.51.100.2"]);
        let two = Trust::Hops(2);
        assert_eq!(two.addresses("10.0.0.1", Some("203.0.113.7, 198.51.100.2")), ["10.0.0.1", "198.51.100.2", "203.0.113.7"]);
    }

    #[test]
    fn subnets_and_names() {
        let t = trust("loopback, 10.0.0.0/8, 192.168.1.1");
        for trusted in ["127.0.0.1", "127.9.9.9", "::1", "10.200.0.1", "192.168.1.1", "::ffff:10.0.0.1"] {
            assert!(t.trusts(trusted, 0), "{trusted}");
        }
        for other in ["11.0.0.1", "192.168.1.2", "2001:db8::1", "not an address", ""] {
            assert!(!t.trusts(other, 0), "{other}");
        }
        assert!(trust("uniquelocal").trusts("172.31.0.1", 0));
        assert!(!trust("uniquelocal").trusts("172.32.0.1", 0));
        assert!(trust("linklocal").trusts("169.254.1.1", 0));
        assert!(trust("fc00::/7").trusts("fd12::1", 0));
    }

    #[test]
    fn the_client_is_the_first_untrusted_address_from_the_socket() {
        let t = trust("10.0.0.0/8");
        // socket and the nearest address are proxies, 203.0.113.7 is the client, 1.2.3.4 is something it claims
        let addresses = t.addresses("10.0.0.1", Some("1.2.3.4, 203.0.113.7, 10.0.0.2"));
        assert_eq!(addresses, ["10.0.0.1", "10.0.0.2", "203.0.113.7"]);
        // a socket that is not a proxy: the header is ignored
        assert_eq!(t.addresses("198.51.100.9", Some("1.2.3.4")), ["198.51.100.9"]);
    }

    #[test]
    fn the_header_is_read_like_forwarded_does() {
        let all = Trust::All;
        assert_eq!(all.addresses("::1", Some("  a ,, b ,")), ["::1", "b", "a"]);
        assert_eq!(all.addresses("::1", None), ["::1"]);
    }

    #[test]
    fn netmasks_and_mapped_ranges() {
        assert!(trust("10.0.0.0/255.0.0.0").trusts("10.1.2.3", 0));
        assert!(!trust("10.0.0.0/255.0.0.0").trusts("11.1.2.3", 0));
        assert!(trust("::ffff:10.0.0.0/104").trusts("10.1.2.3", 0), "a mapped subnet covers IPv4");
        assert!(!trust("::ffff:10.0.0.0/104").trusts("2001:db8::1", 0));
        assert!(!trust("10.0.0.0/8").trusts("2001:db8::1", 0), "IPv4 subnets don't cover IPv6");
    }

    #[test]
    fn invalid_settings_are_reported_like_proxy_addr_does() {
        let error = |list: &str| Trust::from_setting(&TrustProxy::List(list.to_string())).unwrap_err();
        assert_eq!(error("10.0.0.0/8, nope"), "invalid IP address: nope");
        assert_eq!(error("10.0.0.0/33"), "invalid range on address: 10.0.0.0/33");
        assert_eq!(error("10.0.0.0/0"), "invalid range on address: 10.0.0.0/0");
        assert_eq!(error("10.0.0.0/255.0.255.0"), "invalid range on address: 10.0.0.0/255.0.255.0");
        assert_eq!(error("a,,b"), "invalid IP address: a");
    }
}
