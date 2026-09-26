//! Recover settings through their existing Serde defaults, and retain keys
//! owned by other app versions. This runs on load/save, never during input polls.

use engine_common::Settings;
use serde::{Serialize, Serializer, ser::SerializeMap, ser::SerializeSeq};
use serde_path_to_error::Segment;
use serde_spanned::Spanned;
use toml::de::{DeTable, DeValue};

// Each retry removes a bad field or collection record. Bound pathological files
// rather than spending unbounded time retrying or resetting the whole config.
const MAX_REPAIRS: usize = 128;

pub(super) struct Document<'a> {
    pub settings: Settings,
    pub repaired: Vec<String>,
    root: Spanned<DeTable<'a>>,
}

impl<'a> Document<'a> {
    pub fn decode(mut root: Spanned<DeTable<'a>>) -> Result<Self, toml::de::Error> {
        let mut repaired = Vec::new();
        loop {
            match serde_path_to_error::deserialize::<_, Settings>(toml::Deserializer::from(
                root.clone(),
            )) {
                Ok(mut settings) => {
                    settings.audio = settings.audio.normalized();
                    return Ok(Self {
                        settings,
                        repaired,
                        root,
                    });
                }
                Err(error) => {
                    let mut path = Vec::new();
                    for segment in error.path() {
                        match segment {
                            Segment::Map { key } => path.push(Key::Field(key.clone())),
                            Segment::Seq { index } => {
                                // A profile is an atomic record: do not salvage a
                                // half-configured mapping. Keep its valid siblings.
                                path.push(Key::Index(*index));
                                break;
                            }
                            _ => break,
                        }
                    }
                    if repaired.len() == MAX_REPAIRS || !remove_field(root.get_mut(), &path) {
                        return Err(error.into_inner());
                    }
                    repaired.push(display_path(&path));
                }
            }
        }
    }
}

enum Key {
    Field(String),
    Index(usize),
}

fn display_path(path: &[Key]) -> String {
    let mut result = String::new();
    for key in path {
        match key {
            Key::Field(name) => {
                if !result.is_empty() {
                    result.push('.');
                }
                result.push_str(name);
            }
            Key::Index(index) => result.push_str(&format!("[{index}]")),
        }
    }
    result
}

fn remove_field(table: &mut DeTable<'_>, path: &[Key]) -> bool {
    let Some((Key::Field(name), rest)) = path.split_first() else {
        return false;
    };
    if rest.is_empty() {
        return table.remove(name.as_str()).is_some();
    }
    match table.get_mut(name.as_str()).map(Spanned::get_mut) {
        Some(DeValue::Table(child)) => remove_field(child, rest),
        Some(DeValue::Array(items)) => {
            let [Key::Index(index)] = rest else {
                return false;
            };
            if *index >= items.len() {
                return false;
            }
            let old = std::mem::replace(items, toml::de::DeArray::new());
            *items = old
                .into_iter()
                .enumerate()
                .filter_map(|(i, item)| (i != *index).then_some(item))
                .collect();
            true
        }
        _ => false,
    }
}

/// Overlay only *unknown* keys onto the new typed snapshot. Merging the entire
/// old file would resurrect cleared optional values and deleted profiles.
pub(super) fn serialize(
    settings: &Settings,
    previous: Option<&Document<'_>>,
) -> Result<String, toml::ser::Error> {
    use serde::ser::Error as _;

    let current_text = toml::to_string_pretty(settings)?;
    let mut current = DeTable::parse(&current_text).map_err(toml::ser::Error::custom)?;
    if let Some(previous) = previous {
        // Compare with the previous *typed* snapshot, not Settings::default():
        // optional values and nonempty lists are absent from default TOML.
        let known_text = toml::to_string_pretty(&previous.settings)?;
        let known = DeTable::parse(&known_text).map_err(toml::ser::Error::custom)?;
        merge_unknown_tables(current.get_mut(), previous.root.get_ref(), known.get_ref());
    }
    toml::to_string_pretty(&Node(&DeValue::Table(current.into_inner())))
}

fn merge_unknown_tables<'a>(
    current: &mut DeTable<'a>,
    original: &DeTable<'a>,
    known: &DeTable<'_>,
) {
    for (key, old) in original {
        let name = key.get_ref().as_ref();
        match known.get(name) {
            None => {
                if !current.contains_key(name) {
                    current.insert(key.clone(), old.clone());
                }
            }
            Some(before) => {
                if let Some(after) = current.get_mut(name) {
                    merge_unknown_value(after.get_mut(), old.get_ref(), before.get_ref(), name);
                }
            }
        }
    }
}

fn merge_unknown_value<'a>(
    current: &mut DeValue<'a>,
    original: &DeValue<'a>,
    known: &DeValue<'_>,
    field: &str,
) {
    match (current, original, known) {
        (DeValue::Table(after), DeValue::Table(old), DeValue::Table(before)) => {
            // A tagged union that changes kind is a replacement, not an edit of
            // the old variant's (possibly version-specific) fields.
            if identity(after, "kind") == identity(before, "kind") {
                merge_unknown_tables(after, old, before);
            }
        }
        (DeValue::Array(after), DeValue::Array(old), DeValue::Array(before)) => {
            // These are the current settings' editable record collections.
            // Match by identity so removing/reordering a controller never moves
            // another controller's unknown properties onto it.
            let identity_key = match field {
                "controller_profiles" => Some("device_key"),
                "bindings" => Some("control"),
                _ => None,
            };
            let mut used = vec![false; before.len()];
            for item in after.iter_mut() {
                let index = before.iter().enumerate().position(|(index, prior)| {
                    !used[index]
                        && match identity_key {
                            Some(key) => match (item.get_ref(), prior.get_ref()) {
                                (DeValue::Table(a), DeValue::Table(b)) => {
                                    identity(a, key).is_some()
                                        && identity(a, key) == identity(b, key)
                                }
                                _ => false,
                            },
                            None => same_value(item.get_ref(), prior.get_ref()),
                        }
                });
                if let Some(index) = index {
                    used[index] = true;
                    if let Some(source) = old.get(index) {
                        merge_unknown_value(
                            item.get_mut(),
                            source.get_ref(),
                            before[index].get_ref(),
                            field,
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

fn identity<'a>(table: &'a DeTable<'_>, key: &str) -> Option<&'a str> {
    table.get(key)?.get_ref().as_str()
}

fn same_value(a: &DeValue<'_>, b: &DeValue<'_>) -> bool {
    match (a, b) {
        (DeValue::String(a), DeValue::String(b)) => a == b,
        (DeValue::Integer(a), DeValue::Integer(b)) => {
            a.radix() == b.radix() && a.as_str() == b.as_str()
        }
        (DeValue::Float(a), DeValue::Float(b)) => a.as_str() == b.as_str(),
        (DeValue::Boolean(a), DeValue::Boolean(b)) => a == b,
        (DeValue::Datetime(a), DeValue::Datetime(b)) => a == b,
        (DeValue::Array(a), DeValue::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| same_value(a.get_ref(), b.get_ref()))
        }
        (DeValue::Table(a), DeValue::Table(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, a)| {
                    b.get(key.get_ref().as_ref())
                        .is_some_and(|b| same_value(a.get_ref(), b.get_ref()))
                })
        }
        _ => false,
    }
}

/// `toml::Value` only holds signed i64 integers, but launch seeds use the full
/// u64 range. The parser's DeValue preserves that range, dates and non-finite
/// floats, without routing TOML through JSON or narrowing unknown values.
struct Node<'a, 'b>(&'a DeValue<'b>);

impl Serialize for Node<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error as _;
        match self.0 {
            DeValue::String(value) => serializer.serialize_str(value),
            DeValue::Integer(value) => {
                if let Ok(n) = i64::from_str_radix(value.as_str(), value.radix()) {
                    serializer.serialize_i64(n)
                } else if let Ok(n) = u64::from_str_radix(value.as_str(), value.radix()) {
                    serializer.serialize_u64(n)
                } else if let Ok(n) = i128::from_str_radix(value.as_str(), value.radix()) {
                    serializer.serialize_i128(n)
                } else {
                    serializer.serialize_u128(
                        u128::from_str_radix(value.as_str(), value.radix())
                            .map_err(S::Error::custom)?,
                    )
                }
            }
            DeValue::Float(value) => {
                serializer.serialize_f64(value.as_str().parse().map_err(S::Error::custom)?)
            }
            DeValue::Boolean(value) => serializer.serialize_bool(*value),
            DeValue::Datetime(value) => value.serialize(serializer),
            DeValue::Array(values) => {
                let mut array = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    array.serialize_element(&Node(value.get_ref()))?;
                }
                array.end()
            }
            DeValue::Table(values) => {
                let mut table = serializer.serialize_map(Some(values.len()))?;
                for (key, value) in values {
                    table.serialize_entry(key.get_ref().as_ref(), &Node(value.get_ref()))?;
                }
                table.end()
            }
        }
    }
}
