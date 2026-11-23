//! Offline search for places, streets and POIs.
//!
//! A region package carries one read-only `search.sqlite`. [`SearchWriter`]
//! creates it at build time, [`SearchIndex`] reads it and never changes it.

mod normalize;
mod read;
mod write;

pub use normalize::{index_terms, normalize};
pub use read::SearchIndex;
pub use write::SearchWriter;

/// Version of the file layout, stored in the `meta` table.
pub const SEARCH_FORMAT: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Place,
    Street,
    Poi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Fuel,
    Groceries,
    Food,
    Pharmacy,
    Lodging,
    Camping,
    Parking,
    Retail,
    Sights,
}

impl Kind {
    pub(crate) fn code(self) -> i64 {
        match self {
            Kind::Place => 0,
            Kind::Street => 1,
            Kind::Poi => 2,
        }
    }

    pub(crate) fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Kind::Place,
            1 => Kind::Street,
            2 => Kind::Poi,
            _ => return None,
        })
    }
}

impl Category {
    pub(crate) fn code(self) -> i64 {
        match self {
            Category::Fuel => 0,
            Category::Groceries => 1,
            Category::Food => 2,
            Category::Pharmacy => 3,
            Category::Lodging => 4,
            Category::Camping => 5,
            Category::Parking => 6,
            Category::Retail => 7,
            Category::Sights => 8,
        }
    }

    pub(crate) fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Category::Fuel,
            1 => Category::Groceries,
            2 => Category::Food,
            3 => Category::Pharmacy,
            4 => Category::Lodging,
            5 => Category::Camping,
            6 => Category::Parking,
            7 => Category::Retail,
            8 => Category::Sights,
            _ => return None,
        })
    }
}

/// One thing to index. An empty `name` is allowed (an unnamed car park): such
/// an entry is found by category only.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: Kind,
    pub category: Option<Category>,
    pub subtype: Option<String>,
    pub name: String,
    pub alt_names: Vec<String>,
    /// The town shown with the entry.
    pub town: Option<String>,
    /// All names the entry is found under in the town column, for example the
    /// suburb and the city around it. Never shown, and never a match alone.
    pub town_names: Vec<String>,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub kind: Kind,
    pub category: Option<Category>,
    pub subtype: Option<String>,
    pub name: String,
    pub town: Option<String>,
    pub lat: f64,
    pub lon: f64,
    /// Metres from the `near` point, when one was given.
    pub distance_m: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("search index database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("search index has format {found}, this build reads format {SEARCH_FORMAT}")]
    Format { found: String },
    #[error("search index is damaged: {0}")]
    Corrupt(String),
}
