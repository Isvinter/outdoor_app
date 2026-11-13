//! The `tiles` URI scheme: map tiles from the installed regions and the glyphs
//! of the map fonts, so that the map works without a network connection.
//!
//! ```text
//! tiles://localhost/<layer>/<z>/<x>/<y>          tile, or 204 if no region has it
//! tiles://localhost/fonts/<font>/<range>.pbf     glyph range bundled with the app
//! ```

use core_app::App;
use core_maps::TilePath;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager};

pub const SCHEME: &str = "tiles";
/// Where the glyph files live among the bundled frontend assets.
const FONT_ASSETS: &str = "map/fonts/";

fn response(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    // The page lives on another origin (tauri://localhost).
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        header::HeaderValue::from_static("*"),
    );
    if let Ok(value) = header::HeaderValue::from_str(mime) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    response
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    response(status, "application/octet-stream", Vec::new())
}

/// Answers one request to the scheme. Blocking: reads from disk.
pub fn respond(handle: &AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path();
    if let Some(font) = path.strip_prefix("/fonts/") {
        // Font names contain spaces. A range the app does not bundle is answered
        // with an empty glyph set, so labels render with what is there.
        let asset = format!("{FONT_ASSETS}{}", font.replace("%20", " "));
        let glyphs = handle
            .asset_resolver()
            .get(asset)
            .map(|asset| asset.bytes)
            .unwrap_or_default();
        return response(StatusCode::OK, "application/x-protobuf", glyphs);
    }
    let Ok(tile) = path.parse::<TilePath>() else {
        return empty(StatusCode::BAD_REQUEST);
    };
    match handle
        .state::<App>()
        .map_tile(tile.layer, tile.z, tile.x, tile.y)
    {
        Ok(Some(tile)) => response(StatusCode::OK, tile.mime, tile.data),
        Ok(None) => empty(StatusCode::NO_CONTENT),
        Err(e) => {
            eprintln!("error: {path}: {e}");
            empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
