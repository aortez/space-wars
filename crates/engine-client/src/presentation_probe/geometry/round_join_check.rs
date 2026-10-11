//! Untimed exact-output oracle for every pass, including the checked original
//! subdivision calculation. Never replace vertex equality with an image tolerance.
use super::{Error, Plan};
use crate::gpu_scene::{
    StrokeWriter,
    mesh::MeshBuilder,
    retained::{Cache, Policy},
    stroke_culling::Control,
};
use lyon_tessellation::RoundJoinMode;

pub(super) struct Validation {
    caches: [Cache; 45],
    meshes: [MeshBuilder; 45],
}
impl Default for Validation {
    fn default() -> Self {
        Self {
            caches: std::array::from_fn(|_| Cache::default()),
            meshes: std::array::from_fn(|_| MeshBuilder::default()),
        }
    }
}

impl Validation {
    pub(super) fn reused_prefix_vertices(&self) -> u64 {
        self.meshes[42].single_pass_stats().reused_prefix_vertices
    }

    pub fn check(&mut self, plan: &Plan) -> Result<(), Error> {
        for i in 0..45 {
            let mode = [
                RoundJoinMode::Reference,
                RoundJoinMode::Exact,
                RoundJoinMode::Checked,
            ][i % 3];
            self.caches[i].begin_frame();
            self.meshes[i].set_round_join_mode(mode);
            self.meshes[i].stroke_writer = [
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
            ][i / 3];
            self.meshes[i].stroke_culling = Control::Conservative;
        }
        for pass in &plan.passes {
            for i in 0..45 {
                self.caches[i].rebuild_clipped(
                    pass.scene(),
                    &pass.scene().retained,
                    &mut self.meshes[i],
                    Policy::HybridCulled,
                    pass.clip(),
                )?;
                self.meshes[i].validate()?;
            }
            for i in 1..45 {
                let a = &self.meshes[0];
                let b = &self.meshes[i];
                if !a
                    .buffers
                    .vertices
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .eq(b.buffers.vertices.iter().flatten().map(|v| v.to_bits()))
                    || a.buffers.indices != b.buffers.indices
                    || a.batches != b.batches
                    || super::decisions(self.caches[0].stats)
                        != super::decisions(self.caches[i].stats)
                {
                    return Err(
                        "stroke writer/edge reuse/prepared guard/single-pass/round-join exact geometry differs"
                            .into(),
                    );
                }
            }
            if (2..24)
                .filter(|i| i % 3 != 0)
                .any(|i| self.meshes[1].round_join_stats() != self.meshes[i].round_join_stats())
            {
                return Err("checked round-join selection differs".into());
            }
            for i in 12..18 {
                if self.meshes[i].edge_stats() != self.meshes[12].edge_stats() {
                    return Err("checked edge reuse selection differs".into());
                }
            }
            // Single-pass skips ordinary join calls. Compare its own selections
            // and layout cache across policies, not the skipped join counters.
            let mut single_reference = self.meshes[24].single_pass_stats();
            single_reference.checked = 0;
            for i in 24..30 {
                let mut single = self.meshes[i].single_pass_stats();
                if i >= 27 && single.checked != single.cached + single.prepared {
                    return Err("single-pass checked mode missed an accepted polygon".into());
                }
                single.checked = 0;
                if single != single_reference || !single.bounded() {
                    return Err(
                        "single-pass selection/cache counters differ or exceed bounds".into(),
                    );
                }
            }
            for i in 30..39 {
                let single = self.meshes[i].single_pass_stats();
                if single.cached + single.skipped[0] != single_reference.cached
                    || single.prepared + single.skipped[1] != single_reference.prepared
                    || single.refused != single_reference.refused
                    || single != self.meshes[30 + (i - 30) / 3 * 3].single_pass_stats()
                    || !single.bounded()
                    || (i >= 33 && single.layouts != Default::default())
                {
                    return Err("selective single-pass work/caches differ".into());
                }
            }
            for i in 39..45 {
                let mut single = self.meshes[i].single_pass_stats();
                if (i >= 42 && single.checked != single.cached + single.prepared)
                    || single.reused_prefix_vertices
                        != if i >= 42 {
                            self.meshes[42].single_pass_stats().reused_prefix_vertices
                        } else {
                            0
                        }
                    || !single.bounded()
                {
                    return Err("prefix reuse checked coverage/counters differ".into());
                }
                single.checked = 0;
                single.reused_prefix_vertices = 0;
                if single != single_reference {
                    return Err("prefix reuse changed classification/cache decisions".into());
                }
            }
        }
        Ok(())
    }
}
