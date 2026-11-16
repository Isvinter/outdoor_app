#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::Ipv4Addr;

use core_share::{LocalAddress, firewall_hint, pick_addresses, qr_svg, share_url};

fn ip(s: &str) -> Ipv4Addr {
    s.parse().unwrap()
}

fn interfaces() -> Vec<(String, Ipv4Addr, Ipv4Addr)> {
    [
        ("eth0", "192.168.178.20", "255.255.255.0"),
        ("wg0", "10.8.0.2", "255.255.255.0"),
        ("docker0", "172.17.0.1", "255.255.0.0"),
        ("lo", "127.0.0.1", "255.0.0.0"),
        ("wlan0", "10.0.0.5", "255.0.0.0"),
        ("eth1", "8.8.8.8", "255.255.255.0"),
    ]
    .into_iter()
    .map(|(name, addr, mask)| (name.to_string(), ip(addr), ip(mask)))
    .collect()
}

#[test]
fn picks_private_ipv4_only() {
    let picked = pick_addresses(&interfaces(), None);
    assert_eq!(
        picked,
        vec![
            LocalAddress {
                ip: ip("192.168.178.20"),
                interface: "eth0".into(),
                prefix: 24,
                default_route: false,
            },
            LocalAddress {
                ip: ip("10.0.0.5"),
                interface: "wlan0".into(),
                prefix: 8,
                default_route: false,
            },
        ]
    );
}

#[test]
fn skips_virtual_interfaces() {
    let all: Vec<_> = ["br-1a2b", "veth0", "virbr0", "tun0", "lo"]
        .into_iter()
        .map(|name| (name.to_string(), ip("192.168.1.2"), ip("255.255.255.0")))
        .collect();
    assert!(pick_addresses(&all, None).is_empty());
}

#[test]
fn default_route_first() {
    let picked = pick_addresses(&interfaces(), Some(ip("10.0.0.5")));
    assert_eq!(picked.len(), 2);
    assert_eq!(picked[0].interface, "wlan0");
    assert!(picked[0].default_route);
    assert_eq!(picked[1].interface, "eth0");
    assert!(!picked[1].default_route);
}

#[test]
fn firewall_hint_uses_network() {
    let address = LocalAddress {
        ip: ip("192.168.178.20"),
        interface: "eth0".into(),
        prefix: 24,
        default_route: true,
    };
    assert_eq!(
        firewall_hint(&address, 8765),
        "sudo ufw allow from 192.168.178.0/24 to any port 8765 proto tcp"
    );
    let wide = LocalAddress {
        ip: ip("172.20.5.9"),
        prefix: 12,
        ..address
    };
    assert_eq!(
        firewall_hint(&wide, 9000),
        "sudo ufw allow from 172.16.0.0/12 to any port 9000 proto tcp"
    );
}

#[test]
fn share_url_has_trailing_slash() {
    assert_eq!(
        share_url(ip("192.168.178.20"), 8765),
        "http://192.168.178.20:8765/"
    );
}

#[test]
fn qr_svg_is_svg() {
    let svg = qr_svg("http://192.168.178.20:8765/");
    assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"), "{svg}");
    assert!(svg.contains("</svg>"));
    assert!(svg.contains("#000000"));
    assert!(svg.contains("#ffffff"));
}

#[test]
fn local_addresses_are_private() {
    for address in core_share::local_addresses() {
        assert!(address.ip.is_private(), "{address:?}");
    }
}
