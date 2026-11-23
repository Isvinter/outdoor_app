//! What a feature is, judged by its tags (the lists of the design).

use core_search::Category;
use serde_json::{Map, Value};

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
const PLACES: &[(&str, f64)] = &[
    ("city", 15_000.0),
    ("town", 8_000.0),
    ("suburb", 4_000.0),
    ("village", 4_000.0),
    ("quarter", 2_000.0),
    ("neighbourhood", 2_000.0),
    ("hamlet", 2_000.0),
];

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
        .or_else(|| tag(tags, "shop").map(|v| (Category::Retail, format!("shop={v}"))))
        .or_else(|| found(Category::Sights, SIGHTS))
}

/// The radius of a `place` tag, if it is one that is indexed.
pub fn place_radius(tags: &Tags) -> Option<f64> {
    let value = tag(tags, "place")?;
    PLACES.iter().find(|(v, _)| *v == value).map(|(_, r)| *r)
}

pub fn is_street(tags: &Tags) -> bool {
    tag(tags, "highway").is_some_and(|v| STREETS.contains(&v))
}

/// `name` if it is there and not empty.
pub fn name(tags: &Tags) -> Option<&str> {
    tag(tags, "name").filter(|n| !n.is_empty())
}

/// The other names a feature can be found under, without the display name.
pub fn alt_names(tags: &Tags, display: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut add = |n: &str| {
        let n = n.trim();
        if !n.is_empty() && n != display && !names.iter().any(|x| x == n) {
            names.push(n.to_string());
        }
    };
    for key in ["name:de", "name:en", "int_name", "short_name"] {
        if let Some(v) = tag(tags, key) {
            add(v);
        }
    }
    if let Some(v) = tag(tags, "alt_name") {
        v.split(';').for_each(&mut add);
    }
    names
}
