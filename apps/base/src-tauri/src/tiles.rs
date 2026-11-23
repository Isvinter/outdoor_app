//! The `tiles` URI scheme: map tiles from the installed regions and the glyphs
//! of the map fonts, so that the map works without a network connection.
//!
//! ```text
//! tiles://localhost/<layer>/<z>/<x>/<y>          tile, or 204 if no region has it
//! tiles://localhost/contour-dem/<z>/<x>/<y>      elevation for contour lines
//! tiles://localhost/fonts/<font>/<range>.pbf     glyph range bundled with the app
//! ```

use core_app::App;
use core_maps::TilePath;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager};

pub const SCHEME: &str = "tiles";
/// Where the glyph files live among the bundled frontend assets.
const FONT_ASSETS: &str = "map/fonts/";
/// Elevation tiles for the contour lines computed in the page. That code cannot
/// handle a missing tile, so here a missing tile is answered with [`NO_DATA_DEM`].
const CONTOUR_DEM: &str = "/contour-dem/";
/// A black 512 x 512 tile. In Terrarium encoding black is -32768 m, which the
/// contour code treats as "no value": it draws no line there, not even at the
/// edge towards a tile with real heights.
const NO_DATA_DEM: &[u8] = include_bytes!("no-data-dem.png");

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
    let contour_dem = path.strip_prefix(CONTOUR_DEM);
    let parsed = match contour_dem {
        Some(rest) => format!("/dem/{rest}").parse::<TilePath>(),
        None => path.parse::<TilePath>(),
    };
    let Ok(tile) = parsed else {
        return empty(StatusCode::BAD_REQUEST);
    };
    match handle
        .state::<App>()
        .map_tile(tile.layer, tile.z, tile.x, tile.y)
    {
        Ok(Some(tile)) => response(StatusCode::OK, tile.mime, tile.data),
        Ok(None) if contour_dem.is_some() => {
            response(StatusCode::OK, "image/png", NO_DATA_DEM.to_vec())
        }
        Ok(None) => empty(StatusCode::NO_CONTENT),
        Err(e) => {
            eprintln!("error: {path}: {e}");
            empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NO_DATA_DEM;

    #[test]
    fn no_data_tile_is_a_png_of_the_size_of_the_elevation_tiles() {
        assert_eq!(&NO_DATA_DEM[1..4], b"PNG");
        assert_eq!(&NO_DATA_DEM[12..16], b"IHDR");
        assert_eq!(&NO_DATA_DEM[16..20], &512_u32.to_be_bytes());
        assert_eq!(&NO_DATA_DEM[20..24], &512_u32.to_be_bytes());
    }
}
