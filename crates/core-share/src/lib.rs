//! Read-only sharing in the home network: a small HTTP server handing out the
//! catalog of the installed regions, their files and the saved routes, plus
//! what the desktop shows to connect to it (addresses, QR code, firewall hint).

mod address;
mod server;

pub use address::{
    LocalAddress, firewall_hint, local_addresses, pick_addresses, qr_svg, share_url,
};
pub use server::{ShareContent, ShareOptions, ShareServer};
