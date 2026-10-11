//! Same Lyon stroke geometry, with a specialized premultiplied output writer.
//! The stock writer remains the default and the benchmark reference.
pub(crate) use super::StrokeWriter;
use super::mesh::Vertex;
use engine_common::RenderColor;
use lyon_tessellation::{
    GeometryBuilderError, StrokeVertex, VertexBuffers, VertexId,
    geometry_builder::{GeometryBuilder, MaxIndex, StrokeGeometryBuilder},
    math::Point,
};

pub(crate) struct Bulk<'a> {
    buffers: &'a mut VertexBuffers<Vertex, u32>,
    first_vertex: usize,
    first_index: usize,
    color: [f32; 4],
}

fn vertex_id(len: usize) -> Result<VertexId, GeometryBuilderError> {
    // Match Lyon's u32 index limit, including its reserved INVALID sentinel.
    if len > <u32 as MaxIndex>::MAX {
        return Err(GeometryBuilderError::TooManyVertices);
    }
    Ok(VertexId(len as u32))
}

impl<'a> Bulk<'a> {
    pub fn new(buffers: &'a mut VertexBuffers<Vertex, u32>, color: RenderColor) -> Self {
        Self {
            first_vertex: buffers.vertices.len(),
            first_index: buffers.indices.len(),
            buffers,
            color: [
                color.r * color.a,
                color.g * color.a,
                color.b * color.a,
                color.a,
            ],
        }
    }

    fn push_position(&mut self, p: Point) -> Result<VertexId, GeometryBuilderError> {
        let id = vertex_id(self.buffers.vertices.len())?;
        self.buffers.vertices.push([
            p.x,
            p.y,
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        ]);
        Ok(id)
    }
}

impl GeometryBuilder for Bulk<'_> {
    fn begin_geometry(&mut self) {
        self.first_vertex = self.buffers.vertices.len();
        self.first_index = self.buffers.indices.len();
    }

    fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
        // Preserve the pinned stock writer's diagnostic and winding/order.
        if a == b || a == c || b == c {
            println!("bad triangle {a:?} {b:?} {c:?}");
        }
        debug_assert!(a != b && a != c && b != c);
        debug_assert!(a != VertexId::INVALID && b != VertexId::INVALID && c != VertexId::INVALID);
        self.buffers.indices.extend_from_slice(&[a.0, b.0, c.0]);
    }

    fn abort_geometry(&mut self) {
        self.buffers.vertices.truncate(self.first_vertex);
        self.buffers.indices.truncate(self.first_index);
    }
}

impl StrokeGeometryBuilder for Bulk<'_> {
    fn add_stroke_vertex(&mut self, v: StrokeVertex) -> Result<VertexId, GeometryBuilderError> {
        // Keep Lyon's exact position arithmetic; only color work is hoisted.
        self.push_position(v.position())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyon_tessellation::math::point;

    #[test]
    fn index_limit_preserves_lyon_invalid_sentinel_without_large_allocations() {
        assert_eq!(vertex_id(0).unwrap(), VertexId(0));
        assert_eq!(
            vertex_id((u32::MAX - 1) as usize).unwrap(),
            VertexId(u32::MAX - 1)
        );
        assert!(matches!(
            vertex_id(u32::MAX as usize),
            Err(GeometryBuilderError::TooManyVertices)
        ));
        assert!(vertex_id(usize::MAX).is_err());
    }

    #[test]
    fn abort_restores_prefix_and_repeated_geometry_keeps_global_indices() {
        let mut buffers = VertexBuffers::new();
        buffers.vertices.push([1.; 6]);
        buffers.indices.extend([0, 0, 0]);
        let mut output = Bulk::new(&mut buffers, RenderColor::rgba(0.5, 0.2, 0.8, 0.25));
        for abort in [true, false, true] {
            let v = output.buffers.vertices.len();
            let i = output.buffers.indices.len();
            output.begin_geometry();
            let ids = [point(1., 2.), point(2., 3.), point(3., 1.)]
                .map(|p| output.push_position(p).unwrap());
            assert_eq!(
                ids,
                [
                    VertexId(v as u32),
                    VertexId(v as u32 + 1),
                    VertexId(v as u32 + 2)
                ]
            );
            output.add_triangle(ids[2], ids[0], ids[1]);
            assert_eq!(
                &output.buffers.indices[i..],
                &[ids[2].0, ids[0].0, ids[1].0]
            );
            assert_eq!(output.buffers.vertices[v], [1., 2., 0.125, 0.05, 0.2, 0.25]);
            if abort {
                output.abort_geometry();
                assert_eq!(output.buffers.vertices.len(), v);
                assert_eq!(output.buffers.indices.len(), i);
            } else {
                output.end_geometry();
            }
        }
        assert_eq!(output.buffers.vertices.len(), 4);
        assert_eq!(output.buffers.indices, [0, 0, 0, 3, 1, 2]);
    }
}
