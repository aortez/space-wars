//! Optional elapsed GPU queries, collected after a completion fence. Unsupported
//! or disjoint timers are unavailable, never zero. No per-frame glFinish.
use glow::HasContext;
use std::{
    ffi::{CStr, c_void},
    rc::Rc,
};

type QueryBits = unsafe extern "system" fn(u32, u32, *mut i32);
type QueryResult = unsafe extern "system" fn(u32, u32, *mut u64);
const GPU_DISJOINT: u32 = 0x8FBB;
const QUERY_COUNTER_BITS: u32 = 0x8864;

pub(super) struct Timer {
    gl: Rc<glow::Context>,
    queries: Vec<glow::NativeQuery>,
    result: QueryResult,
    disjoint: bool,
}

impl Timer {
    pub fn new(
        gl: Rc<glow::Context>,
        loader: impl Fn(&CStr) -> *const c_void,
        slots: usize,
        enabled: bool,
    ) -> Result<Option<Self>, String> {
        if !enabled {
            eprintln!("geometry_gpu_timer=disabled");
            return Ok(None);
        }
        let ext = gl.supported_extensions();
        let disjoint = gl.version().is_embedded;
        if disjoint && !ext.contains("GL_EXT_disjoint_timer_query") {
            eprintln!("geometry_gpu_timer=unavailable; reason=EXT_disjoint_timer_query absent");
            return Ok(None);
        }
        let bits = loader(if disjoint {
            c"glGetQueryivEXT"
        } else {
            c"glGetQueryiv"
        });
        let result = loader(if disjoint {
            c"glGetQueryObjectui64vEXT"
        } else {
            c"glGetQueryObjectui64v"
        });
        if bits.is_null() || result.is_null() {
            eprintln!("geometry_gpu_timer=unavailable; reason=query entry points absent");
            return Ok(None);
        }
        // SAFETY: Exact GL signatures loaded from this current context.
        let bits: QueryBits = unsafe { std::mem::transmute(bits) };
        let result: QueryResult = unsafe { std::mem::transmute(result) };
        let mut count = 0;
        unsafe {
            bits(glow::TIME_ELAPSED, QUERY_COUNTER_BITS, &mut count);
        }
        if count == 0 {
            eprintln!("geometry_gpu_timer=unavailable; reason=zero counter bits");
            return Ok(None);
        }
        let mut timer = Self {
            gl,
            queries: Vec::new(),
            result,
            disjoint,
        };
        for _ in 0..slots {
            timer.queries.push(unsafe { timer.gl.create_query()? });
        }
        eprintln!(
            "geometry_gpu_timer=elapsed_query; counter_bits={count}; slots={slots}; disjoint_checked={disjoint}"
        );
        Ok(Some(timer))
    }
    pub fn begin(&self, slot: usize) {
        unsafe {
            self.gl.begin_query(glow::TIME_ELAPSED, self.queries[slot]);
        }
    }
    pub fn end(&self) {
        unsafe {
            self.gl.end_query(glow::TIME_ELAPSED);
        }
    }
    pub fn disjoint(&self) -> bool {
        self.disjoint && unsafe { self.gl.get_parameter_i32(GPU_DISJOINT) != 0 }
    }
    /// Caller has waited for the fence following this query. Full u64 result,
    /// not a truncated u32 nanosecond value or CPU-side enqueue duration.
    pub fn milliseconds(&self, slot: usize) -> f64 {
        let mut ns = 0;
        unsafe {
            (self.result)(self.queries[slot].0.get(), glow::QUERY_RESULT, &mut ns);
        }
        ns as f64 / 1_000_000.0
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        for q in self.queries.drain(..) {
            unsafe {
                self.gl.delete_query(q);
            }
        }
    }
}
