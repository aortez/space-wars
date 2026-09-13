//! Preserve the version-1 binary layout, including when nested in another record.
use super::*;
use serde::de::{self, SeqAccess, Visitor};

pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<TerrainState, D::Error> {
    if d.is_human_readable() {
        return TerrainState::deserialize(d);
    }
    struct StateVisitor;
    impl<'de> Visitor<'de> for StateVisitor {
        type Value = TerrainState;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a version 1 or 2 terrain field")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            macro_rules! next {
                ($index:expr) => {
                    seq.next_element()?
                        .ok_or_else(|| de::Error::invalid_length($index, &self))?
                };
            }
            let version = next!(0);
            if !matches!(version, 1 | 2) {
                return Err(de::Error::custom("unsupported terrain version"));
            }
            Ok(TerrainState {
                version,
                width: next!(1),
                height: next!(2),
                cell_size: next!(3),
                materials: next!(4),
                cells: next!(5),
                revision: next!(6),
                chunk_revisions: next!(7),
                distances: if version == 2 { next!(8) } else { None },
            })
        }
    }
    d.deserialize_tuple(9, StateVisitor)
}
