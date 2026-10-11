//! Opt-in adapter for bounded triangle-layout reuse and fresh coarse arcs.
//! No cached screen positions; refusals append nothing and use ordinary Lyon.
use super::StrokeWriter;
use super::mesh::{MAX_INDICES, MAX_VERTICES, Vertex};
use engine_common::RenderColor;
use lyon_tessellation::{
    BuffersBuilder, StrokeOptions, StrokeTessellator, StrokeTopologyOutput, StrokeTopologyReject,
    StrokeTopologySource, StrokeTopologyStats, StrokeTopologyTessellator, StrokeVertex,
    VertexBuffers, math::Point, path::Polygon,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stats {
    pub cached: u64,
    pub prepared: u64,
    /// Diagnostic emitter exclusions, not unsafe/unsupported geometry refusals.
    pub skipped: [u64; 2],
    // Unsupported, merged, fine/uncertain arc, nonfinite, budget.
    pub refused: [u64; 5],
    pub checked: u64,
    /// Only the checked prefix writer increments this cumulative diagnostic.
    /// Unchecked frames have zero deltas (unmeasured), even when reusing positions.
    pub reused_prefix_vertices: u64,
    pub layouts: StrokeTopologyStats,
}

impl Stats {
    pub fn delta(self, before: Self) -> Self {
        Self {
            cached: self.cached - before.cached,
            prepared: self.prepared - before.prepared,
            skipped: std::array::from_fn(|i| self.skipped[i] - before.skipped[i]),
            refused: std::array::from_fn(|i| self.refused[i] - before.refused[i]),
            checked: self.checked - before.checked,
            reused_prefix_vertices: self.reused_prefix_vertices - before.reused_prefix_vertices,
            layouts: StrokeTopologyStats {
                hits: self.layouts.hits - before.layouts.hits,
                misses: self.layouts.misses - before.layouts.misses,
                evictions: self.layouts.evictions - before.layouts.evictions,
                entries: self.layouts.entries,
                bytes: self.layouts.bytes,
            },
        }
    }

    pub fn bounded(self) -> bool {
        self.layouts.entries <= 128 && self.layouts.bytes <= 512 * 1024
    }
}

#[derive(Default)]
pub(crate) struct State {
    tessellator: StrokeTopologyTessellator,
    reference: Option<(StrokeTessellator, VertexBuffers<Point, u32>)>,
    counts: Stats,
}

impl State {
    pub fn stats(&self) -> Stats {
        Stats {
            layouts: self.tessellator.stats(),
            ..self.counts
        }
    }

    pub fn append(
        &mut self,
        points: &[Point],
        options: &StrokeOptions,
        color: RenderColor,
        writer: StrokeWriter,
        output: &mut VertexBuffers<Vertex, u32>,
    ) -> Result<bool, String> {
        let polygon = Polygon {
            points,
            closed: true,
        };
        let mut checked_reused_prefix_vertices = 0;
        let result = match writer {
            StrokeWriter::BulkSinglePassLayouts => self
                .tessellator
                .tessellate_single_pass_control::<true, false>(polygon.clone(), options),
            StrokeWriter::BulkSinglePassArcs => self
                .tessellator
                .tessellate_single_pass_control::<false, true>(polygon.clone(), options),
            StrokeWriter::BulkSinglePassClassify => self
                .tessellator
                .tessellate_single_pass_control::<false, false>(polygon.clone(), options),
            StrokeWriter::BulkSinglePassPrefix => self
                .tessellator
                .tessellate_single_pass_prefix(polygon.clone(), options)
                .map(StrokeTopologyOutput::Mesh),
            StrokeWriter::BulkSinglePassPrefixChecked => self
                .tessellator
                .tessellate_single_pass_prefix_counted(polygon.clone(), options)
                .map(|(mesh, reused)| {
                    checked_reused_prefix_vertices = reused;
                    StrokeTopologyOutput::Mesh(mesh)
                }),
            _ => self
                .tessellator
                .tessellate_single_pass(polygon.clone(), options)
                .map(StrokeTopologyOutput::Mesh),
        };
        let mesh = match result {
            Ok(StrokeTopologyOutput::Mesh(mesh)) => mesh,
            Ok(StrokeTopologyOutput::Skipped(source)) => {
                let index = match source {
                    StrokeTopologySource::Layout => 0,
                    StrokeTopologySource::Prepared => 1,
                };
                self.counts.skipped[index] += 1;
                return Ok(false);
            }
            Err(reason) => {
                let index = match reason {
                    StrokeTopologyReject::Unsupported => 0,
                    StrokeTopologyReject::Merge => 1,
                    StrokeTopologyReject::Arc => 2,
                    StrokeTopologyReject::NonFinite => 3,
                    StrokeTopologyReject::Budget => 4,
                };
                self.counts.refused[index] += 1;
                return Ok(false);
            }
        };
        if mesh.positions.len() > MAX_VERTICES.saturating_sub(output.vertices.len())
            || mesh.indices.len() > MAX_INDICES.saturating_sub(output.indices.len())
        {
            return Err("GPU scene exceeds prototype mesh budget".into());
        }
        if matches!(
            writer,
            StrokeWriter::BulkSinglePassChecked | StrokeWriter::BulkSinglePassPrefixChecked
        ) {
            let (tessellator, reference) = self.reference.get_or_insert_with(Default::default);
            reference.vertices.clear();
            reference.indices.clear();
            tessellator
                .tessellate_polygon(
                    polygon,
                    options,
                    &mut BuffersBuilder::new(reference, |v: StrokeVertex| v.position()),
                )
                .map_err(|e| format!("single-pass reference: {e:?}"))?;
            if mesh.indices != reference.indices
                || mesh
                    .positions
                    .iter()
                    .map(|p| [p.x.to_bits(), p.y.to_bits()])
                    .ne(reference
                        .vertices
                        .iter()
                        .map(|p| [p.x.to_bits(), p.y.to_bits()]))
            {
                return Err("single-pass stroke differs from ordinary tessellation".into());
            }
            self.counts.checked += 1;
            self.counts.reused_prefix_vertices += checked_reused_prefix_vertices as u64;
        }
        match mesh.source {
            StrokeTopologySource::Layout => self.counts.cached += 1,
            StrokeTopologySource::Prepared => self.counts.prepared += 1,
        }
        let base = output.vertices.len() as u32;
        let c = [
            color.r * color.a,
            color.g * color.a,
            color.b * color.a,
            color.a,
        ];
        output.vertices.extend(
            mesh.positions
                .iter()
                .map(|p| [p.x, p.y, c[0], c[1], c[2], c[3]]),
        );
        output
            .indices
            .extend(mesh.indices.iter().map(|&i| i + base));
        Ok(true)
    }
}
