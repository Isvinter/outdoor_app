//! Sharing the installed regions and the saved routes in the home network:
//! the read-only server of `core-share`, switched on and off from the page.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use core_app::{App, SharedRoute, SharedRouteInfo};
use core_maps::Catalog;
use core_share::{
    LocalAddress, ShareContent, ShareOptions, ShareServer, firewall_hint, local_addresses, qr_svg,
    share_url,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::error::{CommandError, CommandResult};

/// Set to run the share self-test on start and exit (see README).
const SELFTEST_SHARE_ENV: &str = "OUTDOOR_SELFTEST_SHARE";

/// Hands out what the core of the app has; the core lives in the Tauri state.
struct Content(AppHandle);

impl ShareContent for Content {
    fn catalog(&self) -> Catalog {
        ShareContent::catalog(&*self.0.state::<App>())
    }

    fn region_file(&self, id: &str, name: &str) -> Option<(PathBuf, u64)> {
        ShareContent::region_file(&*self.0.state::<App>(), id, name)
    }

    fn routes(&self) -> Result<Vec<SharedRouteInfo>, String> {
        ShareContent::routes(&*self.0.state::<App>())
    }

    fn route(&self, id: Uuid) -> Option<SharedRoute> {
        ShareContent::route(&*self.0.state::<App>(), id)
    }
}

/// The running server and the address shown for it.
struct Running {
    server: ShareServer,
    selected: Option<Ipv4Addr>,
}

#[derive(Default)]
pub struct Share {
    running: Mutex<Option<Running>>,
}

impl Share {
    fn running(&self) -> MutexGuard<'_, Option<Running>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Serialize)]
pub struct ShareStatus {
    running: bool,
    port: u16,
    addresses: Vec<LocalAddress>,
    /// The address the URL, QR code and firewall hint are for.
    selected: Option<String>,
    url: Option<String>,
    qr_svg: Option<String>,
    firewall_hint: Option<String>,
}

fn status(port: u16, running: Option<&Running>) -> ShareStatus {
    let addresses = local_addresses();
    let shown = running.and_then(|running| {
        addresses
            .iter()
            .find(|a| Some(a.ip) == running.selected)
            .or_else(|| addresses.first())
    });
    let url = shown.map(|a| share_url(a.ip, port));
    ShareStatus {
        running: running.is_some(),
        port,
        selected: shown.map(|a| a.ip.to_string()),
        qr_svg: url.as_deref().map(qr_svg),
        firewall_hint: shown.map(|a| firewall_hint(a, port)),
        url,
        addresses,
    }
}

fn current(handle: &AppHandle) -> CommandResult<ShareStatus> {
    let share = handle.state::<Share>();
    let running = share.running();
    let port = match running.as_ref() {
        Some(running) => running.server.local_addr().port(),
        None => handle.state::<App>().share_port()?,
    };
    Ok(status(port, running.as_ref()))
}

async fn blocking<T, F>(handle: AppHandle, operation: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle) -> CommandResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || operation(&handle))
        .await
        .map_err(|e| CommandError::task(&e))?
}

#[tauri::command]
pub async fn share_status(handle: AppHandle) -> CommandResult<ShareStatus> {
    blocking(handle, current).await
}

/// Starts the share on `0.0.0.0:<port>` (stores a changed port), or, while
/// it runs, only shows it for the address `ip`.
#[tauri::command]
pub async fn share_start(
    handle: AppHandle,
    port: u16,
    ip: Option<String>,
) -> CommandResult<ShareStatus> {
    blocking(handle, move |handle| {
        let selected = match ip {
            Some(ip) => Some(ip.parse::<Ipv4Addr>().map_err(|e| CommandError {
                code: "share",
                message: format!("{ip}: {e}"),
            })?),
            None => None,
        };
        let share = handle.state::<Share>();
        let mut running = share.running();
        match running.as_mut() {
            Some(running) => running.selected = selected,
            None => {
                let app = handle.state::<App>();
                if app.share_port()? != port {
                    app.set_share_port(port)?;
                }
                let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
                let content = Arc::new(Content(handle.clone()));
                let server = ShareServer::start(addr, content, ShareOptions::default())
                    .map_err(|e| CommandError::share(port, &e))?;
                *running = Some(Running { server, selected });
            }
        }
        let port = running
            .as_ref()
            .map_or(port, |running| running.server.local_addr().port());
        Ok(status(port, running.as_ref()))
    })
    .await
}

/// Stops the share; running downloads end at once.
#[tauri::command]
pub async fn share_stop(handle: AppHandle) -> CommandResult<()> {
    blocking(handle, |handle| {
        stop(handle);
        Ok(())
    })
    .await
}

/// Stops the share if it runs; also when the app closes.
pub fn stop(handle: &AppHandle) {
    let Some(share) = handle.try_state::<Share>() else {
        return;
    };
    // Take it out first, so that stopping does not hold the lock.
    let running = share.running().take();
    if let Some(running) = running {
        running.server.stop();
    }
}

#[tauri::command]
pub fn selftest_share() -> bool {
    std::env::var_os(SELFTEST_SHARE_ENV).is_some()
}

/// Reads `url` as text, for the self-test only.
#[tauri::command]
pub async fn selftest_fetch(url: String) -> CommandResult<String> {
    let fail = |message: String| CommandError {
        code: "unknown",
        message,
    };
    if !selftest_share() {
        return Err(fail(format!("{SELFTEST_SHARE_ENV} is not set")));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        agent
            .get(&url)
            .call()
            .map_err(|e| fail(format!("{url}: {e}")))?
            .body_mut()
            .read_to_string()
            .map_err(|e| fail(format!("{url}: {e}")))
    })
    .await
    .map_err(|e| CommandError::task(&e))?
}
