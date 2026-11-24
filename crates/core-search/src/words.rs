//! What the words of a query can mean: other spellings, and kinds of places.

use crate::{Category, normalize};

/// Words for a kind of place in German, English and Spanish, normalised.
const CATEGORY_WORDS: &[(Category, &[&str])] = &[
    (
        Category::Lodging,
        &[
            "hostel",
            "hostal",
            "hotel",
            "pension",
            "unterkunft",
            "hospedaje",
            "alojamiento",
            "lodging",
        ],
    ),
    (
        Category::Pharmacy,
        &["apotheke", "farmacia", "botica", "pharmacy"],
    ),
    (
        Category::Fuel,
        &["tankstelle", "grifo", "gasolinera", "fuel"],
    ),
    (
        Category::Groceries,
        &[
            "supermarkt",
            "lebensmittel",
            "supermercado",
            "mercado",
            "grocery",
            "supermarket",
        ],
    ),
    (
        Category::Food,
        &["restaurant", "essen", "restaurante", "cafe", "food"],
    ),
    (Category::Camping, &["camping", "campingplatz", "campsite"]),
    (
        Category::Parking,
        &["parkplatz", "parken", "parking", "estacionamiento"],
    ),
    (
        Category::Sights,
        &[
            "sehenswuerdigkeit",
            "sehenswurdigkeit",
            "sehenswert",
            "sights",
            "museum",
            "museo",
        ],
    ),
];

fn expand_umlauts(word: &str) -> String {
    word.replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue")
}

/// The words of a query, each with the spellings it may be stored under.
/// The first spelling is the plain normalised word.
#[must_use]
pub fn query_words(text: &str) -> Vec<Vec<String>> {
    let lower = text.to_lowercase();
    let mut words = Vec::new();
    for raw in lower.split(|c: char| !c.is_alphanumeric()) {
        let plain = normalize(raw);
        let mut tokens = plain.split_whitespace();
        let (Some(first), None) = (tokens.next(), tokens.next()) else {
            // Nothing left, or a character that folds into several words.
            words.extend(plain.split_whitespace().map(|t| vec![t.to_string()]));
            continue;
        };
        let mut variants = vec![first.to_string()];
        let expanded = normalize(&expand_umlauts(raw));
        if !variants.contains(&expanded) && !expanded.contains(' ') {
            variants.push(expanded);
        }
        if first == "strasse" {
            variants.push("str".to_string());
        }
        words.push(variants);
    }
    words
}

/// The category a query names, if all of it is one word for a kind of place.
#[must_use]
pub fn category_for_word(text: &str) -> Option<Category> {
    let words = query_words(text);
    let [variants] = words.as_slice() else {
        return None;
    };
    CATEGORY_WORDS
        .iter()
        .find(|(_, list)| variants.iter().any(|v| list.contains(&v.as_str())))
        .map(|(category, _)| *category)
}
