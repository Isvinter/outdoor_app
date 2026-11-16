//! The addresses under which this machine is reachable in the home network,
//! and how they are shown: URL, QR code and firewall hint.

use std::net::{Ipv4Addr, UdpSocket};

use qrcode::QrCode;
use qrcode::render::svg;
use serde::Serialize;

/// A private IPv4 address of a network interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalAddress {
    pub ip: Ipv4Addr,
    pub interface: String,
    /// Length of the network prefix (24 for 255.255.255.0).
    pub prefix: u8,
    /// The address the default route leaves from.
    pub default_route: bool,
}

/// Interfaces of VPNs, containers, virtual machines and the loopback, which
/// a phone in the home network cannot reach.
const SKIPPED_PREFIXES: [&str; 6] = ["wg", "docker", "br-", "veth", "virbr", "tun"];

/// Picks the private IPv4 addresses (10/8, 172.16/12, 192.168/16) from
/// `(interface, address, netmask)` triples, skipping virtual interfaces. The
/// address of the default route comes first, the others follow by interface.
pub fn pick_addresses(
    all: &[(String, Ipv4Addr, Ipv4Addr)],
    default_ip: Option<Ipv4Addr>,
) -> Vec<LocalAddress> {
    let mut picked: Vec<LocalAddress> = all
        .iter()
        .filter(|(name, ip, _)| {
            ip.is_private()
                && name != "lo"
                && !SKIPPED_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
        })
        .map(|(name, ip, netmask)| LocalAddress {
            ip: *ip,
            interface: name.clone(),
            prefix: netmask.to_bits().count_ones() as u8,
            default_route: default_ip == Some(*ip),
        })
        .collect();
    picked.sort_by(|a, b| {
        b.default_route
            .cmp(&a.default_route)
            .then_with(|| a.interface.cmp(&b.interface))
            .then_with(|| a.ip.cmp(&b.ip))
    });
    picked
}

/// The private addresses of this machine, the one of the default route first.
pub fn local_addresses() -> Vec<LocalAddress> {
    let all: Vec<(String, Ipv4Addr, Ipv4Addr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(v4) => Some((interface.name, v4.ip, v4.netmask)),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    pick_addresses(&all, default_route_ip())
}

/// The local address the default route leaves from. Connecting a UDP socket
/// sends nothing; it only makes the system pick the route to the (reserved,
/// documentation-only) address 192.0.2.1.
fn default_route_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// The address a phone enters (or scans) to reach the share.
pub fn share_url(ip: Ipv4Addr, port: u16) -> String {
    format!("http://{ip}:{port}/")
}

/// `text` as a QR code: an SVG image with black modules on white.
pub fn qr_svg(text: &str) -> String {
    match QrCode::new(text.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color<'_>>()
            .min_dimensions(240, 240)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        // Only for texts far too long for a QR code: an empty white image.
        Err(_) => concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1">"#,
            r##"<rect width="1" height="1" fill="#ffffff"/></svg>"##
        )
        .to_string(),
    }
}

/// The command that opens the port of the share for the network of `address`.
pub fn firewall_hint(address: &LocalAddress, port: u16) -> String {
    let prefix = address.prefix.min(32);
    let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
    let network = Ipv4Addr::from_bits(address.ip.to_bits() & mask);
    format!("sudo ufw allow from {network}/{prefix} to any port {port} proto tcp")
}
