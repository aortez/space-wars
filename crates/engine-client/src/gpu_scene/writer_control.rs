//! Session-only stroke output selection. All paths emit identical geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StrokeWriter {
    #[default]
    Stock,
    Bulk,
    /// Bulk output with concrete-type dispatch for polygon/line strokes.
    /// Circle builders and retained templates keep the reference path.
    BulkStatic,
    /// Concrete bulk output with direct polygon/line input (no path-event loop).
    BulkPolygon,
    /// Concrete bulk output with bit-keyed adjacent-edge reuse (path-event input).
    BulkEdges,
    /// Recompute and check every reused edge; validation only, never timed.
    BulkEdgesChecked,
    /// Concrete path-event output with per-stroke fixed-width join bounds.
    BulkPrepared,
    /// Compare each prepared bound decision against the original predicate.
    BulkPreparedChecked,
    /// Bounded topology layouts with single-pass coarse-arc emission; opt-in.
    BulkSinglePass,
    /// Compare every accepted polygon against ordinary Lyon before appending.
    BulkSinglePassChecked,
    /// Diagnostic: classify all polygons, emit layouts, fall back for fresh arcs.
    BulkSinglePassLayouts,
    /// Diagnostic: classify all polygons, emit fresh arcs, fall back for layouts.
    BulkSinglePassArcs,
    /// Diagnostic classification overhead: ordinary output for every polygon.
    BulkSinglePassClassify,
    /// Reuse the current polygon's certified base-vertex prefix when emitting arcs.
    BulkSinglePassPrefix,
    /// Check prefix reuse against ordinary Lyon before appending.
    BulkSinglePassPrefixChecked,
}

impl StrokeWriter {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stock => "stock",
            Self::Bulk => "bulk",
            Self::BulkStatic => "bulk-static",
            Self::BulkPolygon => "bulk-polygon",
            Self::BulkEdges => "bulk-edges",
            Self::BulkEdgesChecked => "bulk-edges-checked",
            Self::BulkPrepared => "bulk-prepared",
            Self::BulkPreparedChecked => "bulk-prepared-checked",
            Self::BulkSinglePass => "bulk-single-pass",
            Self::BulkSinglePassChecked => "bulk-single-pass-checked",
            Self::BulkSinglePassLayouts => "bulk-single-pass-layouts",
            Self::BulkSinglePassArcs => "bulk-single-pass-arcs",
            Self::BulkSinglePassClassify => "bulk-single-pass-classify",
            Self::BulkSinglePassPrefix => "bulk-single-pass-prefix",
            Self::BulkSinglePassPrefixChecked => "bulk-single-pass-prefix-checked",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "stock" => Ok(Self::Stock),
            "bulk" => Ok(Self::Bulk),
            "bulk-static" => Ok(Self::BulkStatic),
            "bulk-polygon" => Ok(Self::BulkPolygon),
            "bulk-edges" => Ok(Self::BulkEdges),
            "bulk-edges-checked" => Ok(Self::BulkEdgesChecked),
            "bulk-prepared" => Ok(Self::BulkPrepared),
            "bulk-prepared-checked" => Ok(Self::BulkPreparedChecked),
            "bulk-single-pass" => Ok(Self::BulkSinglePass),
            "bulk-single-pass-checked" => Ok(Self::BulkSinglePassChecked),
            "bulk-single-pass-layouts" => Ok(Self::BulkSinglePassLayouts),
            "bulk-single-pass-arcs" => Ok(Self::BulkSinglePassArcs),
            "bulk-single-pass-classify" => Ok(Self::BulkSinglePassClassify),
            "bulk-single-pass-prefix" => Ok(Self::BulkSinglePassPrefix),
            "bulk-single-pass-prefix-checked" => Ok(Self::BulkSinglePassPrefixChecked),
            _ => Err(
                "gpu-stroke-writer requires stock, bulk, bulk-static, bulk-polygon, bulk-edges, bulk-edges-checked, bulk-prepared, bulk-prepared-checked, bulk-single-pass, bulk-single-pass-checked, bulk-single-pass-layouts, bulk-single-pass-arcs, bulk-single-pass-classify, bulk-single-pass-prefix, or bulk-single-pass-prefix-checked",
            ),
        }
    }

    pub fn single_pass(self) -> bool {
        matches!(
            self,
            Self::BulkSinglePass
                | Self::BulkSinglePassChecked
                | Self::BulkSinglePassLayouts
                | Self::BulkSinglePassArcs
                | Self::BulkSinglePassClassify
                | Self::BulkSinglePassPrefix
                | Self::BulkSinglePassPrefixChecked
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_is_default_and_selection_is_explicit() {
        assert_eq!(StrokeWriter::default(), StrokeWriter::Stock);
        for mode in [
            StrokeWriter::Stock,
            StrokeWriter::Bulk,
            StrokeWriter::BulkStatic,
            StrokeWriter::BulkPolygon,
            StrokeWriter::BulkEdges,
            StrokeWriter::BulkEdgesChecked,
            StrokeWriter::BulkPrepared,
            StrokeWriter::BulkPreparedChecked,
            StrokeWriter::BulkSinglePass,
            StrokeWriter::BulkSinglePassChecked,
            StrokeWriter::BulkSinglePassLayouts,
            StrokeWriter::BulkSinglePassArcs,
            StrokeWriter::BulkSinglePassClassify,
            StrokeWriter::BulkSinglePassPrefix,
            StrokeWriter::BulkSinglePassPrefixChecked,
        ] {
            assert_eq!(StrokeWriter::parse(mode.name()), Ok(mode));
        }
        for invalid in ["", "auto", "on", "off", "fast", "bulk\nstatus"] {
            assert!(StrokeWriter::parse(invalid).is_err());
        }
    }
}
