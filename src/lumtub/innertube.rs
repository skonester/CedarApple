//! Helpers copied from Lumtub's innertube.rs (GPL-3.0-or-later).
use serde_json::{Map, Value};

pub(super) fn at<'a>(node: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(node, |current, key| current.as_object()?.get(*key))
}

pub(super) fn seconds(text: &str) -> Option<u32> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let digits = |s: &str, max: usize| {
        (!s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (hours, minutes, secs) = match parts.as_slice() {
        [m, s] => (0, digits(m, 2)?, *s),
        [h, m, s] => (digits(h, 9)?, digits(m, 2)?, *s),
        _ => return None,
    };
    if secs.len() != 2 {
        return None;
    }
    hours
        .checked_mul(3600)?
        .checked_add(minutes * 60)?
        .checked_add(digits(secs, 2)?)
}

pub(super) fn members_only(node: &Value) -> bool {
    match node {
        Value::String(text) => text.contains("MEMBERS_ONLY"),
        Value::Object(map) => members_only_in(map),
        Value::Array(list) => list.iter().any(members_only),
        _ => false,
    }
}

fn members_only_in(map: &Map<String, Value>) -> bool {
    map.iter().any(|(key, value)| match key.as_str() {
        "title"
        | "headline"
        | "descriptionSnippet"
        | "detailedMetadataSnippets"
        | "navigationEndpoint" => false,
        _ => members_only(value),
    })
}
