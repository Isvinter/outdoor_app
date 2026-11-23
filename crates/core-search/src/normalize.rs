//! Folding of names and queries into the plain form the index stores.

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// The query form: lower case, `ß` as `ss`, accents removed, other
/// punctuation turned into single spaces.
pub fn normalize(text: &str) -> String {
    let folded: String = text
        .to_lowercase()
        .replace('ß', "ss")
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every form under which a name is indexed: its normalised tokens, the
/// `ae/oe/ue` spelling of umlauts and `…strasse` for a token ending in `str`.
pub fn index_terms(name: &str) -> Vec<String> {
    let expanded = name
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue");
    let mut terms: Vec<String> = Vec::new();
    for source in [normalize(name), normalize(&expanded)] {
        for token in source.split_whitespace() {
            push_unique(&mut terms, token.to_string());
            if let Some(stem) = token.strip_suffix("str").filter(|s| !s.is_empty()) {
                push_unique(&mut terms, format!("{stem}strasse"));
            }
        }
    }
    terms
}

fn push_unique(terms: &mut Vec<String>, term: String) {
    if !terms.contains(&term) {
        terms.push(term);
    }
}
