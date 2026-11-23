//! What a feature is, judged by its tags (the lists of the design).

use core_search::{Category, normalize};
use serde_json::{Map, Value};

use crate::towns::Rank;

pub type Tags = Map<String, Value>;

/// The string value of a tag.
pub fn tag<'a>(tags: &'a Tags, key: &str) -> Option<&'a str> {
    tags.get(key).and_then(Value::as_str)
}

const FUEL: &[(&str, &[&str])] = &[("amenity", &["fuel"])];
const GROCERIES: &[(&str, &[&str])] = &[(
    "shop",
    &[
        "supermarket",
        "convenience",
        "discount",
        "greengrocer",
        "grocery",
    ],
)];
const FOOD: &[(&str, &[&str])] = &[(
    "amenity",
    &[
        "restaurant",
        "cafe",
        "fast_food",
        "bar",
        "pub",
        "biergarten",
    ],
)];
const PHARMACY: &[(&str, &[&str])] = &[("amenity", &["pharmacy"])];
const LODGING: &[(&str, &[&str])] = &[(
    "tourism",
    &[
        "hotel",
        "hostel",
        "guest_house",
        "motel",
        "apartment",
        "chalet",
        "alpine_hut",
    ],
)];
const CAMPING: &[(&str, &[&str])] = &[("tourism", &["camp_site", "caravan_site"])];
const SIGHTS: &[(&str, &[&str])] = &[
    (
        "tourism",
        &[
            "attraction",
            "museum",
            "viewpoint",
            "gallery",
            "zoo",
            "theme_park",
        ],
    ),
    (
        "historic",
        &[
            "castle",
            "monument",
            "ruins",
            "archaeological_site",
            "fort",
            "city_gate",
        ],
    ),
];

/// Place values and the radius in metres in which a place names its surroundings.
const PLACES: &[(&str, f64, Rank)] = &[
    ("city", 15_000.0, Rank::City),
    ("town", 8_000.0, Rank::Town),
    ("suburb", 4_000.0, Rank::Part),
    ("village", 4_000.0, Rank::Village),
    ("quarter", 2_000.0, Rank::Part),
    ("neighbourhood", 2_000.0, Rank::Part),
    ("hamlet", 2_000.0, Rank::Part),
];

/// Values of `shop` that say there is no shop (any more).
const NOT_A_SHOP: &[&str] = &["vacant", "no", "disused"];

const STREETS: &[&str] = &[
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "unclassified",
    "residential",
    "living_street",
    "pedestrian",
    "service",
    "road",
];

fn listed(tags: &Tags, list: &[(&str, &[&str])]) -> Option<String> {
    list.iter().find_map(|(key, values)| {
        let value = tag(tags, key)?;
        values.contains(&value).then(|| format!("{key}={value}"))
    })
}

/// The POI category and the matched `key=value`, in the order of the design's table.
pub fn poi(tags: &Tags) -> Option<(Category, String)> {
    let found = |category, list| listed(tags, list).map(|subtype| (category, subtype));
    found(Category::Fuel, FUEL)
        .or_else(|| found(Category::Groceries, GROCERIES))
        .or_else(|| found(Category::Food, FOOD))
        .or_else(|| found(Category::Pharmacy, PHARMACY))
        .or_else(|| found(Category::Lodging, LODGING))
        .or_else(|| found(Category::Camping, CAMPING))
        .or_else(|| {
            (tag(tags, "amenity") == Some("parking") && tag(tags, "access") != Some("private"))
                .then(|| (Category::Parking, "amenity=parking".to_string()))
        })
        .or_else(|| {
            tag(tags, "shop")
                .filter(|v| !NOT_A_SHOP.contains(v))
                .map(|v| (Category::Retail, format!("shop={v}")))
        })
        .or_else(|| found(Category::Sights, SIGHTS))
}

/// Radius and rank of a `place` tag, if it is one that is indexed.
pub fn place(tags: &Tags) -> Option<(f64, Rank)> {
    let value = tag(tags, "place")?;
    PLACES
        .iter()
        .find(|(v, _, _)| *v == value)
        .map(|(_, radius, rank)| (*radius, *rank))
}

/// The radius of a `place` tag, if it is one that is indexed.
pub fn place_radius(tags: &Tags) -> Option<f64> {
    place(tags).map(|(radius, _)| radius)
}

pub fn is_street(tags: &Tags) -> bool {
    tag(tags, "highway").is_some_and(|v| STREETS.contains(&v))
}

/// `name` if it is there and not empty.
pub fn name(tags: &Tags) -> Option<&str> {
    tag(tags, "name").filter(|n| !n.is_empty())
}

/// Collects names, leaving out what folds to a name it already has.
struct Names {
    seen: Vec<String>,
    names: Vec<String>,
}

impl Names {
    fn new(display: &str) -> Self {
        Self {
            seen: vec![normalize(display)],
            names: Vec::new(),
        }
    }

    fn add(&mut self, name: &str) {
        let name = name.trim();
        let folded = normalize(name);
        if !folded.is_empty() && !self.seen.contains(&folded) {
            self.seen.push(folded);
            self.names.push(name.to_string());
        }
    }

    /// Adds the value of `key`, which may list several names.
    fn add_list(&mut self, tags: &Tags, key: &str) {
        for name in tag(tags, key).into_iter().flat_map(|v| v.split(';')) {
            self.add(name);
        }
    }

    fn short_list(&mut self, tags: &Tags) {
        for key in ["name:de", "name:en", "int_name", "short_name"] {
            if let Some(v) = tag(tags, key) {
                self.add(v);
            }
        }
        self.add_list(tags, "alt_name");
        self.add_list(tags, "official_name");
    }
}

/// The other names a street or POI is found under, without the display name.
/// A place gives the same list to the town column of what lies around it.
pub fn alt_names(tags: &Tags, display: &str) -> Vec<String> {
    let mut names = Names::new(display);
    names.short_list(tags);
    names.names
}

/// Every other name a place itself is found under: the short list, all
/// languages and its local and old names.
pub fn place_names(tags: &Tags, display: &str) -> Vec<String> {
    let mut names = Names::new(display);
    names.short_list(tags);
    for key in ["loc_name", "old_name"] {
        names.add_list(tags, key);
    }
    for (key, value) in tags {
        if let (Some(language), Some(value)) = (key.strip_prefix("name:"), value.as_str())
            && is_language(language)
        {
            names.add(value);
        }
    }
    names.names
}

/// A language code such as `de`, `gsw` or `zh-Hans`; not `left` or `etymology`.
fn is_language(code: &str) -> bool {
    let mut parts = code.split('-');
    let primary = parts.next().unwrap_or("");
    (2..=3).contains(&primary.len())
        && primary.chars().all(|c| c.is_ascii_lowercase())
        && parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    fn tags(value: Value) -> Tags {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn names_are_deduplicated_after_folding() {
        let tags = tags(json!({
            "name": "Café Süd",
            "official_name": "CAFE SUD",
            "name:en": "Cafe Sud",
            "name:fr": "cafe  sud",
            "name:it": "Caffè Sud",
        }));
        assert!(alt_names(&tags, "Café Süd").is_empty());
        assert_eq!(place_names(&tags, "Café Süd"), ["Caffè Sud"]);
    }

    #[test]
    fn language_codes_are_told_from_other_suffixes() {
        for code in ["de", "gsw", "zh-Hans", "sr-Latn"] {
            assert!(is_language(code), "{code}");
        }
        for code in ["left", "etymology", "etymology:wikidata", "DE", "", "de-"] {
            assert!(!is_language(code), "{code}");
        }
    }
}
