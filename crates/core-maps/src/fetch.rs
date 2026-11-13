//! Access to catalog and region files, over HTTP(S) or from a local directory.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use crate::MapsError;

/// An opened download.
pub struct Fetched {
    /// Position in the file at which `body` starts. Equals the requested
    /// offset unless the source cannot resume; then it is 0.
    pub offset: u64,
    pub body: Box<dyn Read>,
}

/// Opens files of a catalog. Implemented for HTTP and local directories by
/// [`DefaultFetcher`]; tests and other platforms may bring their own.
pub trait Fetcher: Send + Sync {
    /// Opens `location` for reading, if possible starting at `offset`.
    fn open(&self, location: &str, offset: u64) -> Result<Fetched, MapsError>;
}

/// Fetches `http://` and `https://` locations over the network and everything
/// else (plain paths and `file://` URLs) from the local file system.
pub struct DefaultFetcher {
    agent: ureq::Agent,
}

impl DefaultFetcher {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        DefaultFetcher { agent }
    }

    fn open_http(&self, location: &str, offset: u64) -> Result<Fetched, MapsError> {
        let fail = |reason: String| MapsError::Download {
            location: location.to_string(),
            reason,
        };
        let mut request = self.agent.get(location);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request.call().map_err(|e| fail(e.to_string()))?;
        let offset = match response.status().as_u16() {
            206 => offset,
            200 => 0,
            status => return Err(fail(format!("the server answered with status {status}"))),
        };
        Ok(Fetched {
            offset,
            body: Box::new(response.into_body().into_reader()),
        })
    }

    fn open_file(&self, location: &str, offset: u64) -> Result<Fetched, MapsError> {
        let fail = |e: std::io::Error| MapsError::Download {
            location: location.to_string(),
            reason: e.to_string(),
        };
        let path = Path::new(location.strip_prefix("file://").unwrap_or(location));
        let mut file = File::open(path).map_err(fail)?;
        let offset = if offset <= file.metadata().map_err(fail)?.len() {
            offset
        } else {
            0
        };
        file.seek(SeekFrom::Start(offset)).map_err(fail)?;
        Ok(Fetched {
            offset,
            body: Box::new(file),
        })
    }
}

impl Default for DefaultFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetcher for DefaultFetcher {
    fn open(&self, location: &str, offset: u64) -> Result<Fetched, MapsError> {
        if location.starts_with("http://") || location.starts_with("https://") {
            self.open_http(location, offset)
        } else {
            self.open_file(location, offset)
        }
    }
}
