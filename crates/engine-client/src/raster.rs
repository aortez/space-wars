//! Experimental software raster presentation path for benchmark comparison.
//!
//! The vector path turns every scenario primitive into Slint UI items. This
//! renderer draws the same `RenderFrame` shape vocabulary into a small number
//! of reusable RGBA buffers, then uploads one image to Slint.

use std::time::{Duration, Instant};

use engine_common::{
    Camera2, RenderCircle, RenderColor, RenderFrame, RenderLayer, RenderLine, RenderPoint,
    RenderPolygon, RenderPrimitive, Stroke,
};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

use crate::render::{self, FrameLayout, Viewport};

pub const DEFAULT_OVERVIEW_CACHE_PERIOD: u64 = 6;

const BUFFER_COUNT: usize = 3;
const MAX_STACK_POINTS: usize = 32;
const SPACEWARS_STARFIELD_LAYER: i32 = -30;
const SPACEWARS_WORLD_LAYER: i32 = -20;
const SPACEWARS_SUN_LAYER: i32 = -15;
const SPACEWARS_PLANET_LAYER: i32 = -10;
const SPACEWARS_SPACEPORT_LAYER: i32 = -5;
const SPACEWARS_EXHAUST_LAYER: i32 = -1;
const SPACEWARS_SHIP_LAYER: i32 = 0;
const SPACEWARS_DEBRIS_LAYER: i32 = 1;
const SPACEWARS_LASER_LAYER: i32 = 2;
const SPACEWARS_PARTICLE_LAYER: i32 = 3;
const SPACEWARS_VIEW_RECTANGLE_LAYER: i32 = 8;
const STARFIELD_CACHE_MIN_PRIMITIVES: usize = 128;
const STARFIELD_CACHE_CELL_SIZE: f32 = 64.0;
const STARFIELD_CACHE_MAX_ENTRIES: usize = 64;
const BACKGROUND: Rgba8Pixel = Rgba8Pixel {
    r: 5,
    g: 5,
    b: 20,
    a: 255,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterOptions {
    pub overview_cache_period: u64,
    pub overview_minimum_object_diameter: f32,
}

impl Default for RasterOptions {
    fn default() -> Self {
        Self {
            overview_cache_period: DEFAULT_OVERVIEW_CACHE_PERIOD,
            overview_minimum_object_diameter: render::MIN_SPACEWARS_OVERVIEW_OBJECT_DIAMETER,
        }
    }
}

impl RasterOptions {
    pub fn for_scale(output_scale: f32) -> Self {
        let output_scale = if output_scale.is_finite() {
            output_scale.clamp(0.1, 3.0)
        } else {
            1.0
        };
        Self {
            overview_minimum_object_diameter: render::MIN_SPACEWARS_OVERVIEW_OBJECT_DIAMETER
                * output_scale,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RasterTimings {
    pub clear: Duration,
    pub player_views: Duration,
    pub player_starfield: Duration,
    pub player_bodies: Duration,
    pub player_world: Duration,
    pub player_sun_planets: Duration,
    pub player_spaceports: Duration,
    pub player_effects: Duration,
    pub player_ships: Duration,
    pub player_debris: Duration,
    pub player_particles: Duration,
    pub player_other: Duration,
    pub overview_refresh: Duration,
    pub overview_blit: Duration,
    pub overview_live: Duration,
    pub other_frames: Duration,
    pub image: Duration,
}

impl RasterTimings {
    fn add_player_layer(&mut self, z: i32, elapsed: Duration) {
        match z {
            SPACEWARS_STARFIELD_LAYER => self.player_starfield += elapsed,
            SPACEWARS_WORLD_LAYER => {
                self.player_bodies += elapsed;
                self.player_world += elapsed;
            }
            SPACEWARS_SUN_LAYER | SPACEWARS_PLANET_LAYER => {
                self.player_bodies += elapsed;
                self.player_sun_planets += elapsed;
            }
            SPACEWARS_SPACEPORT_LAYER => {
                self.player_bodies += elapsed;
                self.player_spaceports += elapsed;
            }
            SPACEWARS_EXHAUST_LAYER | SPACEWARS_LASER_LAYER => self.player_effects += elapsed,
            SPACEWARS_SHIP_LAYER => self.player_ships += elapsed,
            SPACEWARS_DEBRIS_LAYER => self.player_debris += elapsed,
            SPACEWARS_PARTICLE_LAYER => self.player_particles += elapsed,
            _ => self.player_other += elapsed,
        }
    }
}

impl std::ops::AddAssign for RasterTimings {
    fn add_assign(&mut self, rhs: Self) {
        self.clear += rhs.clear;
        self.player_views += rhs.player_views;
        self.player_starfield += rhs.player_starfield;
        self.player_bodies += rhs.player_bodies;
        self.player_world += rhs.player_world;
        self.player_sun_planets += rhs.player_sun_planets;
        self.player_spaceports += rhs.player_spaceports;
        self.player_effects += rhs.player_effects;
        self.player_ships += rhs.player_ships;
        self.player_debris += rhs.player_debris;
        self.player_particles += rhs.player_particles;
        self.player_other += rhs.player_other;
        self.overview_refresh += rhs.overview_refresh;
        self.overview_blit += rhs.overview_blit;
        self.overview_live += rhs.overview_live;
        self.other_frames += rhs.other_frames;
        self.image += rhs.image;
    }
}

pub struct RasterRenderResult {
    pub image: Image,
    pub timings: RasterTimings,
}

#[derive(Debug)]
pub struct RasterRenderer {
    buffers: Vec<SharedPixelBuffer<Rgba8Pixel>>,
    width: u32,
    height: u32,
    active_buffer: usize,
    frame_index: u64,
    overview_cache: [Option<CachedRaster>; 2],
    starfield_cache: StarfieldVisibilityCache,
}

impl RasterRenderer {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            width: 0,
            height: 0,
            active_buffer: 0,
            frame_index: 0,
            overview_cache: [None, None],
            starfield_cache: StarfieldVisibilityCache::default(),
        }
    }

    pub fn image_from_frames_with_layout(
        &mut self,
        frames: &[RenderFrame],
        viewport: Viewport,
        layout: FrameLayout,
        options: RasterOptions,
    ) -> Image {
        self.image_from_frames_with_layout_timed(frames, viewport, layout, options)
            .image
    }

    pub fn image_from_frames_with_layout_timed(
        &mut self,
        frames: &[RenderFrame],
        viewport: Viewport,
        layout: FrameLayout,
        options: RasterOptions,
    ) -> RasterRenderResult {
        let viewport = viewport.with_default_if_empty();
        let width = viewport.width.ceil().max(1.0) as u32;
        let height = viewport.height.ceil().max(1.0) as u32;
        self.ensure_size(width, height);

        let buffer_index = self.active_buffer;
        self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
        let mut timings = RasterTimings::default();

        {
            let frame_index = self.frame_index;
            let overview_cache = &mut self.overview_cache;
            let starfield_cache = &mut self.starfield_cache;
            let pixels = self.buffers[buffer_index].make_mut_slice();
            let started = Instant::now();
            clear_pixels(pixels);
            timings.clear += started.elapsed();
            let mut canvas = Canvas::new(width, height, pixels);

            if frames.len() == 1 {
                let started = Instant::now();
                canvas.draw_frame(&frames[0], Viewport::new(width as f32, height as f32));
                timings.other_frames += started.elapsed();
            } else if layout == FrameLayout::SpacewarsLocalPlay && frames.len() >= 4 {
                draw_spacewars_layout(
                    &mut canvas,
                    frames,
                    width,
                    height,
                    options,
                    frame_index,
                    overview_cache,
                    starfield_cache,
                    &mut timings,
                );
            } else if !frames.is_empty() {
                let viewports = render::frame_viewports(
                    Viewport::new(width as f32, height as f32),
                    frames.len(),
                    layout,
                );
                let started = Instant::now();
                for (frame, viewport) in frames.iter().zip(viewports) {
                    canvas.draw_frame(frame, viewport);
                }
                timings.other_frames += started.elapsed();
            }
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        let started = Instant::now();
        let image = Image::from_rgba8(self.buffers[buffer_index].clone());
        timings.image += started.elapsed();
        RasterRenderResult { image, timings }
    }

    fn ensure_size(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height && self.buffers.len() == BUFFER_COUNT {
            return;
        }

        self.width = width;
        self.height = height;
        self.active_buffer = 0;
        self.buffers = (0..BUFFER_COUNT)
            .map(|_| SharedPixelBuffer::new(width, height))
            .collect();
        self.overview_cache = [None, None];
        self.starfield_cache.clear();
    }
}

impl Default for RasterRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct CachedRaster {
    width: u32,
    height: u32,
    pixels: SharedPixelBuffer<Rgba8Pixel>,
    valid: bool,
}

impl CachedRaster {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: SharedPixelBuffer::new(width, height),
            valid: false,
        }
    }
}

fn draw_spacewars_layout(
    canvas: &mut Canvas<'_>,
    frames: &[RenderFrame],
    width: u32,
    height: u32,
    options: RasterOptions,
    frame_index: u64,
    overview_cache: &mut [Option<CachedRaster>; 2],
    starfield_cache: &mut StarfieldVisibilityCache,
    timings: &mut RasterTimings,
) {
    let viewports = render::frame_viewports(
        Viewport::new(width as f32, height as f32),
        frames.len(),
        FrameLayout::SpacewarsLocalPlay,
    );

    for index in 0..2 {
        *timings += canvas.draw_player_frame_timed(
            &frames[index],
            viewports[index],
            starfield_cache,
            frame_index,
        );
    }

    for overview in 0..2 {
        draw_cached_overview(
            canvas,
            &frames[2 + overview],
            viewports[2 + overview],
            overview,
            options,
            frame_index,
            overview_cache,
            timings,
        );
    }

    let started = Instant::now();
    for index in 4..frames.len() {
        canvas.draw_frame(&frames[index], viewports[index]);
    }
    timings.other_frames += started.elapsed();
}

fn draw_cached_overview(
    canvas: &mut Canvas<'_>,
    frame: &RenderFrame,
    viewport: Viewport,
    overview: usize,
    options: RasterOptions,
    frame_index: u64,
    overview_cache: &mut [Option<CachedRaster>; 2],
    timings: &mut RasterTimings,
) {
    let width = viewport.width.ceil().max(1.0) as u32;
    let height = viewport.height.ceil().max(1.0) as u32;
    let period = options.overview_cache_period.max(1);
    let should_refresh = frame_index % period == 0;

    let cache = overview_cache[overview].get_or_insert_with(|| CachedRaster::new(width, height));
    if cache.width != width || cache.height != height {
        *cache = CachedRaster::new(width, height);
    }

    if should_refresh || !cache.valid {
        let started = Instant::now();
        let pixels = cache.pixels.make_mut_slice();
        clear_pixels(pixels);
        let cache_viewport = Viewport::new(width as f32, height as f32);
        Canvas::new(width, height, pixels).draw_frame_filtered(
            frame,
            cache_viewport,
            |z, primitive| {
                is_cached_overview_primitive(z, primitive)
                    && render::spacewars_overview_primitive_visible(
                        frame.camera,
                        cache_viewport,
                        z,
                        primitive,
                        options.overview_minimum_object_diameter,
                    )
            },
        );
        cache.valid = true;
        timings.overview_refresh += started.elapsed();
    }

    let started = Instant::now();
    canvas.blit(
        cache.pixels.as_slice(),
        cache.width,
        cache.height,
        viewport.x.round() as i32,
        viewport.y.round() as i32,
    );
    timings.overview_blit += started.elapsed();

    let started = Instant::now();
    canvas.draw_frame_filtered(frame, viewport, |z, primitive| {
        is_live_overview_primitive(z, primitive)
            && render::spacewars_overview_primitive_visible(
                frame.camera,
                viewport,
                z,
                primitive,
                options.overview_minimum_object_diameter,
            )
    });
    timings.overview_live += started.elapsed();
}

fn is_cached_overview_primitive(z: i32, primitive: &RenderPrimitive) -> bool {
    z < SPACEWARS_SHIP_LAYER || matches!(primitive, RenderPrimitive::Circle(_))
}

fn is_live_overview_primitive(z: i32, primitive: &RenderPrimitive) -> bool {
    z >= SPACEWARS_LASER_LAYER
        || z == SPACEWARS_VIEW_RECTANGLE_LAYER
        || (z == SPACEWARS_SHIP_LAYER && !matches!(primitive, RenderPrimitive::Circle(_)))
}

#[cfg(test)]
fn image_from_frames_with_layout(
    frames: &[RenderFrame],
    viewport: Viewport,
    layout: FrameLayout,
) -> Image {
    RasterRenderer::new().image_from_frames_with_layout(
        frames,
        viewport,
        layout,
        RasterOptions::default(),
    )
}

pub fn primitive_count(frames: &[RenderFrame]) -> usize {
    frames
        .iter()
        .map(|frame| {
            frame
                .layers
                .iter()
                .map(|layer| layer.primitives.len())
                .sum::<usize>()
        })
        .sum()
}

struct Canvas<'a> {
    width: u32,
    height: u32,
    pixels: &'a mut [Rgba8Pixel],
}

impl<'a> Canvas<'a> {
    fn new(width: u32, height: u32, pixels: &'a mut [Rgba8Pixel]) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    fn draw_frame(&mut self, frame: &RenderFrame, viewport: Viewport) {
        self.draw_frame_internal(frame, viewport, None, |_, _| true);
    }

    fn draw_player_frame_timed(
        &mut self,
        frame: &RenderFrame,
        viewport: Viewport,
        starfield_cache: &mut StarfieldVisibilityCache,
        frame_index: u64,
    ) -> RasterTimings {
        let mut timings = RasterTimings::default();
        let frame_started = Instant::now();
        let Some(clip) = PixelClip::from_viewport(viewport, self.width, self.height) else {
            return timings;
        };

        for layer in frame.ordered_layers() {
            let layer_started = Instant::now();
            if layer.z == SPACEWARS_STARFIELD_LAYER
                && layer.primitives.len() >= STARFIELD_CACHE_MIN_PRIMITIVES
            {
                self.draw_cached_starfield_layer(
                    frame.camera,
                    viewport,
                    layer,
                    starfield_cache,
                    frame_index,
                    clip,
                    &|_, _| true,
                );
            } else {
                for primitive in &layer.primitives {
                    self.draw_primitive(layer.z, frame.camera, viewport, clip, primitive);
                }
            }
            timings.add_player_layer(layer.z, layer_started.elapsed());
        }

        timings.player_views += frame_started.elapsed();
        timings
    }

    fn draw_frame_filtered(
        &mut self,
        frame: &RenderFrame,
        viewport: Viewport,
        include: impl Fn(i32, &RenderPrimitive) -> bool,
    ) {
        self.draw_frame_internal(frame, viewport, None, include);
    }

    fn draw_frame_internal(
        &mut self,
        frame: &RenderFrame,
        viewport: Viewport,
        mut starfield_cache: Option<(&mut StarfieldVisibilityCache, u64)>,
        include: impl Fn(i32, &RenderPrimitive) -> bool,
    ) {
        let Some(clip) = PixelClip::from_viewport(viewport, self.width, self.height) else {
            return;
        };

        for layer in frame.ordered_layers() {
            if layer.z == SPACEWARS_STARFIELD_LAYER
                && layer.primitives.len() >= STARFIELD_CACHE_MIN_PRIMITIVES
            {
                if let Some((cache, frame_index)) = starfield_cache.as_mut() {
                    self.draw_cached_starfield_layer(
                        frame.camera,
                        viewport,
                        layer,
                        &mut **cache,
                        *frame_index,
                        clip,
                        &include,
                    );
                    continue;
                }
            }

            for primitive in &layer.primitives {
                if include(layer.z, primitive) {
                    self.draw_primitive(layer.z, frame.camera, viewport, clip, primitive);
                }
            }
        }
    }

    fn draw_cached_starfield_layer(
        &mut self,
        camera: Camera2,
        viewport: Viewport,
        layer: &RenderLayer,
        starfield_cache: &mut StarfieldVisibilityCache,
        frame_index: u64,
        clip: PixelClip,
        include: &impl Fn(i32, &RenderPrimitive) -> bool,
    ) {
        let indices =
            starfield_cache.visible_indices(camera, viewport, &layer.primitives, frame_index);
        for &index in indices {
            let Some(primitive) = layer.primitives.get(index) else {
                continue;
            };
            if include(layer.z, primitive) {
                self.draw_primitive(layer.z, camera, viewport, clip, primitive);
            }
        }
    }

    fn draw_primitive(
        &mut self,
        layer_z: i32,
        camera: Camera2,
        viewport: Viewport,
        clip: PixelClip,
        primitive: &RenderPrimitive,
    ) {
        if !primitive_visible(camera, viewport, primitive) {
            return;
        }

        match primitive {
            RenderPrimitive::Circle(circle) => {
                self.draw_circle(layer_z, camera, viewport, clip, circle)
            }
            RenderPrimitive::Line(line) => self.draw_line(camera, viewport, clip, line),
            RenderPrimitive::Polygon(polygon) => self.draw_polygon(camera, viewport, clip, polygon),
            RenderPrimitive::Text(_) => {}
        }
    }

    fn draw_circle(
        &mut self,
        layer_z: i32,
        camera: Camera2,
        viewport: Viewport,
        clip: PixelClip,
        circle: &RenderCircle,
    ) {
        let center = project(camera, circle.center, viewport);
        let radius = circle.radius / camera.height * viewport.height;

        if let Some(fill) = circle.fill {
            fill_circle_pixels(
                self.pixels,
                self.width,
                self.height,
                clip,
                center,
                radius,
                fill.color,
                span_fill_mode_for_layer(layer_z, fill.color),
            );
        }

        if let Some(stroke) = circle.stroke {
            stroke_circle_pixels(
                self.pixels,
                self.width,
                self.height,
                clip,
                center,
                radius,
                stroke,
            );
        }
    }

    fn draw_line(
        &mut self,
        camera: Camera2,
        viewport: Viewport,
        clip: PixelClip,
        line: &RenderLine,
    ) {
        draw_line_pixels(
            self.pixels,
            self.width,
            self.height,
            clip,
            project(camera, line.start, viewport),
            project(camera, line.end, viewport),
            line.stroke.color,
            line.stroke.width,
        );
    }

    fn draw_polygon(
        &mut self,
        camera: Camera2,
        viewport: Viewport,
        clip: PixelClip,
        polygon: &RenderPolygon,
    ) {
        if polygon.points.len() < 2 {
            return;
        }

        if polygon.points.len() <= MAX_STACK_POINTS {
            let mut points = [PixelPoint::ZERO; MAX_STACK_POINTS];
            for (output, point) in points.iter_mut().zip(&polygon.points) {
                *output = project(camera, *point, viewport);
            }
            self.draw_projected_polygon(clip, &points[..polygon.points.len()], polygon);
        } else {
            let points = polygon
                .points
                .iter()
                .map(|point| project(camera, *point, viewport))
                .collect::<Vec<_>>();
            self.draw_projected_polygon(clip, &points, polygon);
        }
    }

    fn draw_projected_polygon(
        &mut self,
        clip: PixelClip,
        points: &[PixelPoint],
        polygon: &RenderPolygon,
    ) {
        if let Some(fill) = polygon.fill {
            fill_convex_polygon(
                self.pixels,
                self.width,
                self.height,
                clip,
                points,
                fill.color,
            );
        }

        if let Some(stroke) = polygon.stroke {
            for index in 0..points.len() {
                draw_line_pixels(
                    self.pixels,
                    self.width,
                    self.height,
                    clip,
                    points[index],
                    points[(index + 1) % points.len()],
                    stroke.color,
                    stroke.width,
                );
            }
        }
    }

    fn blit(
        &mut self,
        source: &[Rgba8Pixel],
        source_width: u32,
        source_height: u32,
        x: i32,
        y: i32,
    ) {
        if x >= self.width as i32 || y >= self.height as i32 {
            return;
        }

        let start_x = x.max(0) as u32;
        let start_y = y.max(0) as u32;
        let source_x = (start_x as i32 - x) as u32;
        let source_y = (start_y as i32 - y) as u32;
        let copy_width = source_width
            .saturating_sub(source_x)
            .min(self.width.saturating_sub(start_x));
        let copy_height = source_height
            .saturating_sub(source_y)
            .min(self.height.saturating_sub(start_y));

        for row in 0..copy_height {
            let src_start = ((source_y + row) * source_width + source_x) as usize;
            let dst_start = ((start_y + row) * self.width + start_x) as usize;
            let len = copy_width as usize;
            self.pixels[dst_start..dst_start + len]
                .copy_from_slice(&source[src_start..src_start + len]);
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PixelPoint {
    x: f32,
    y: f32,
}

impl PixelPoint {
    const ZERO: Self = Self { x: 0.0, y: 0.0 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelClip {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

impl PixelClip {
    fn from_viewport(viewport: Viewport, width: u32, height: u32) -> Option<Self> {
        let min_x = viewport.x.floor() as i32;
        let max_x = (viewport.x + viewport.width).ceil() as i32 - 1;
        let min_y = viewport.y.floor() as i32;
        let max_y = (viewport.y + viewport.height).ceil() as i32 - 1;
        Self::new(width, height, min_x, max_x, min_y, max_y)
    }

    fn new(
        width: u32,
        height: u32,
        min_x: i32,
        max_x: i32,
        min_y: i32,
        max_y: i32,
    ) -> Option<Self> {
        let min_x = min_x.max(0);
        let min_y = min_y.max(0);
        let max_x = max_x.min(width as i32 - 1);
        let max_y = max_y.min(height as i32 - 1);

        if min_x > max_x || min_y > max_y {
            None
        } else {
            Some(Self {
                min_x,
                max_x,
                min_y,
                max_y,
            })
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WorldBounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl WorldBounds {
    fn intersects(self, other: Self) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StarfieldCacheKey {
    center_x_cell: i32,
    center_y_cell: i32,
    viewport_width: u32,
    viewport_height: u32,
    camera_height_milli: u32,
    primitive_count: usize,
    first_x_bits: u32,
    first_y_bits: u32,
    last_x_bits: u32,
    last_y_bits: u32,
}

#[derive(Debug)]
struct StarfieldVisibilityEntry {
    key: StarfieldCacheKey,
    last_used: u64,
    indices: Vec<usize>,
}

#[derive(Debug, Default)]
struct StarfieldVisibilityCache {
    entries: Vec<StarfieldVisibilityEntry>,
}

impl StarfieldVisibilityCache {
    fn clear(&mut self) {
        self.entries.clear();
    }

    fn visible_indices(
        &mut self,
        camera: Camera2,
        viewport: Viewport,
        primitives: &[RenderPrimitive],
        frame_index: u64,
    ) -> &[usize] {
        let key = starfield_cache_key(camera, viewport, primitives);
        if let Some(position) = self.entries.iter().position(|entry| entry.key == key) {
            self.entries[position].last_used = frame_index;
            return &self.entries[position].indices;
        }

        let bounds = camera_world_bounds(
            camera,
            viewport,
            STARFIELD_CACHE_CELL_SIZE + world_units_per_pixel(camera, viewport) * 4.0,
        );
        let indices = primitives
            .iter()
            .enumerate()
            .filter_map(|(index, primitive)| {
                primitive_world_bounds(primitive, 0.0)
                    .filter(|primitive_bounds| primitive_bounds.intersects(bounds))
                    .map(|_| index)
            })
            .collect();

        if self.entries.len() >= STARFIELD_CACHE_MAX_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.entries.swap_remove(oldest);
        }

        self.entries.push(StarfieldVisibilityEntry {
            key,
            last_used: frame_index,
            indices,
        });
        &self.entries.last().expect("entry was just pushed").indices
    }
}

#[derive(Debug, Clone, Copy)]
struct RasterColor {
    pixel: Rgba8Pixel,
    opaque: bool,
}

#[derive(Debug, Clone, Copy)]
enum SpanFillMode {
    Normal,
    BackgroundFastPath { blended_background: Rgba8Pixel },
}

fn project(camera: Camera2, point: RenderPoint, viewport: Viewport) -> PixelPoint {
    let normalized = camera.world_to_viewport(point, viewport.aspect_ratio());
    PixelPoint {
        x: viewport.x + normalized.x * viewport.width,
        y: viewport.y + normalized.y * viewport.height,
    }
}

fn primitive_visible(camera: Camera2, viewport: Viewport, primitive: &RenderPrimitive) -> bool {
    let padding = world_units_per_pixel(camera, viewport) * 4.0;
    let Some(primitive_bounds) = primitive_world_bounds(primitive, padding) else {
        return false;
    };
    primitive_bounds.intersects(camera_world_bounds(camera, viewport, padding))
}

fn primitive_world_bounds(primitive: &RenderPrimitive, padding: f32) -> Option<WorldBounds> {
    match primitive {
        RenderPrimitive::Circle(circle) => {
            let radius = circle.radius + padding;
            Some(WorldBounds {
                min_x: circle.center.x - radius,
                max_x: circle.center.x + radius,
                min_y: circle.center.y - radius,
                max_y: circle.center.y + radius,
            })
        }
        RenderPrimitive::Line(line) => {
            let padding = padding + line.stroke.width.max(1.0);
            Some(WorldBounds {
                min_x: line.start.x.min(line.end.x) - padding,
                max_x: line.start.x.max(line.end.x) + padding,
                min_y: line.start.y.min(line.end.y) - padding,
                max_y: line.start.y.max(line.end.y) + padding,
            })
        }
        RenderPrimitive::Polygon(polygon) => {
            let first = polygon.points.first()?;
            let mut bounds = WorldBounds {
                min_x: first.x,
                max_x: first.x,
                min_y: first.y,
                max_y: first.y,
            };
            for point in &polygon.points[1..] {
                bounds.min_x = bounds.min_x.min(point.x);
                bounds.max_x = bounds.max_x.max(point.x);
                bounds.min_y = bounds.min_y.min(point.y);
                bounds.max_y = bounds.max_y.max(point.y);
            }
            bounds.min_x -= padding;
            bounds.max_x += padding;
            bounds.min_y -= padding;
            bounds.max_y += padding;
            Some(bounds)
        }
        RenderPrimitive::Text(_) => None,
    }
}

fn camera_world_bounds(camera: Camera2, viewport: Viewport, padding: f32) -> WorldBounds {
    let bounds = camera.world_bounds(viewport.aspect_ratio());
    WorldBounds {
        min_x: bounds.min.x - padding,
        max_x: bounds.max.x + padding,
        min_y: bounds.min.y - padding,
        max_y: bounds.max.y + padding,
    }
}

fn world_units_per_pixel(camera: Camera2, viewport: Viewport) -> f32 {
    camera.height / viewport.height.max(1.0)
}

fn starfield_cache_key(
    camera: Camera2,
    viewport: Viewport,
    primitives: &[RenderPrimitive],
) -> StarfieldCacheKey {
    let (first_x_bits, first_y_bits) = primitive_first_point_bits(primitives.first());
    let (last_x_bits, last_y_bits) = primitive_first_point_bits(primitives.last());
    StarfieldCacheKey {
        center_x_cell: quantized_cell(camera.center.x),
        center_y_cell: quantized_cell(camera.center.y),
        viewport_width: viewport.width.round().max(0.0) as u32,
        viewport_height: viewport.height.round().max(0.0) as u32,
        camera_height_milli: (camera.height * 1000.0).round().max(0.0) as u32,
        primitive_count: primitives.len(),
        first_x_bits,
        first_y_bits,
        last_x_bits,
        last_y_bits,
    }
}

fn primitive_first_point_bits(primitive: Option<&RenderPrimitive>) -> (u32, u32) {
    match primitive {
        Some(RenderPrimitive::Polygon(polygon)) => polygon
            .points
            .first()
            .map(|point| (point.x.to_bits(), point.y.to_bits()))
            .unwrap_or_default(),
        Some(RenderPrimitive::Circle(circle)) => {
            (circle.center.x.to_bits(), circle.center.y.to_bits())
        }
        Some(RenderPrimitive::Line(line)) => (line.start.x.to_bits(), line.start.y.to_bits()),
        Some(RenderPrimitive::Text(text)) => (text.position.x.to_bits(), text.position.y.to_bits()),
        None => (0, 0),
    }
}

fn quantized_cell(value: f32) -> i32 {
    (value / STARFIELD_CACHE_CELL_SIZE).floor() as i32
}

fn span_fill_mode_for_layer(layer_z: i32, color: RenderColor) -> SpanFillMode {
    if !matches!(layer_z, SPACEWARS_SUN_LAYER | SPACEWARS_PLANET_LAYER) {
        return SpanFillMode::Normal;
    }

    let raster_color = raster_color(color);
    if raster_color.opaque {
        return SpanFillMode::Normal;
    }

    let mut blended_background = BACKGROUND;
    blend_pixel(&mut blended_background, raster_color);
    SpanFillMode::BackgroundFastPath { blended_background }
}

fn clear_pixels(pixels: &mut [Rgba8Pixel]) {
    pixels.fill(BACKGROUND);
}

fn fill_circle_pixels(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    height: u32,
    clip: PixelClip,
    center: PixelPoint,
    radius: f32,
    color: RenderColor,
    fill_mode: SpanFillMode,
) {
    if radius <= 0.0 {
        return;
    }

    let radius_sq = radius * radius;
    let raster_color = raster_color(color);
    if circle_covers_clip(center, radius_sq, clip) {
        fill_rect(pixels, width, clip, raster_color, fill_mode);
        return;
    }

    let min_y = (center.y - radius).floor() as i32;
    let max_y = (center.y + radius).ceil() as i32;
    let Some(draw_clip) = PixelClip::new(width, height, clip.min_x, clip.max_x, min_y, max_y)
    else {
        return;
    };

    for y in draw_clip.min_y..=draw_clip.max_y {
        let dy = y as f32 + 0.5 - center.y;
        let span = (radius_sq - dy * dy).max(0.0).sqrt();
        fill_span(
            pixels,
            width,
            height,
            draw_clip,
            y,
            (center.x - span).floor() as i32,
            (center.x + span).ceil() as i32,
            raster_color,
            fill_mode,
        );
    }
}

fn stroke_circle_pixels(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    height: u32,
    clip: PixelClip,
    center: PixelPoint,
    radius: f32,
    stroke: Stroke,
) {
    if radius <= 0.0 {
        return;
    }

    let line_width = stroke.width.max(1.0);
    let outer = radius + line_width * 0.5;
    let inner = (radius - line_width * 0.5).max(0.0);
    let outer_sq = outer * outer;
    let inner_sq = inner * inner;
    let raster_color = raster_color(stroke.color);
    let Some(draw_clip) = PixelClip::new(
        width,
        height,
        clip.min_x.max((center.x - outer).floor() as i32),
        clip.max_x.min((center.x + outer).ceil() as i32),
        (center.y - outer).floor() as i32,
        (center.y + outer).ceil() as i32,
    ) else {
        return;
    };

    for y in draw_clip.min_y..=draw_clip.max_y {
        let dy = y as f32 + 0.5 - center.y;
        let outer_span = (outer_sq - dy * dy).max(0.0).sqrt();
        let left_outer = (center.x - outer_span).floor() as i32;
        let right_outer = (center.x + outer_span).ceil() as i32;

        if inner <= 0.0 || dy * dy >= inner_sq {
            fill_span(
                pixels,
                width,
                height,
                draw_clip,
                y,
                left_outer,
                right_outer,
                raster_color,
                SpanFillMode::Normal,
            );
            continue;
        }

        let inner_span = (inner_sq - dy * dy).sqrt();
        let left_inner = (center.x - inner_span).ceil() as i32;
        let right_inner = (center.x + inner_span).floor() as i32;
        fill_span(
            pixels,
            width,
            height,
            draw_clip,
            y,
            left_outer,
            left_inner,
            raster_color,
            SpanFillMode::Normal,
        );
        fill_span(
            pixels,
            width,
            height,
            draw_clip,
            y,
            right_inner,
            right_outer,
            raster_color,
            SpanFillMode::Normal,
        );
    }
}

fn fill_convex_polygon(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    height: u32,
    clip: PixelClip,
    points: &[PixelPoint],
    color: RenderColor,
) {
    if points.len() < 3 {
        return;
    }

    for index in 1..points.len() - 1 {
        fill_triangle(
            pixels,
            width,
            height,
            clip,
            points[0],
            points[index],
            points[index + 1],
            color,
        );
    }
}

fn fill_triangle(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    height: u32,
    clip: PixelClip,
    a: PixelPoint,
    b: PixelPoint,
    c: PixelPoint,
    color: RenderColor,
) {
    let raster_color = raster_color(color);
    let min_y = a.y.min(b.y).min(c.y).floor() as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil() as i32;
    let Some(draw_clip) = PixelClip::new(width, height, clip.min_x, clip.max_x, min_y, max_y)
    else {
        return;
    };
    let edges = [(a, b), (b, c), (c, a)];

    for y in draw_clip.min_y..=draw_clip.max_y {
        let scan_y = y as f32 + 0.5;
        let mut intersections = [0.0_f32; 3];
        let mut count = 0;

        for (start, end) in edges {
            if (start.y <= scan_y && end.y > scan_y) || (end.y <= scan_y && start.y > scan_y) {
                let t = (scan_y - start.y) / (end.y - start.y);
                intersections[count] = start.x + (end.x - start.x) * t;
                count += 1;
            }
        }

        if count >= 2 {
            let left = intersections[0].min(intersections[1]).floor() as i32;
            let right = intersections[0].max(intersections[1]).ceil() as i32;
            fill_span(
                pixels,
                width,
                height,
                draw_clip,
                y,
                left,
                right,
                raster_color,
                SpanFillMode::Normal,
            );
        }
    }
}

fn draw_line_pixels(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    height: u32,
    clip: PixelClip,
    start: PixelPoint,
    end: PixelPoint,
    color: RenderColor,
    line_width: f32,
) {
    if line_width <= 1.25 {
        draw_thin_line(pixels, width, clip, start, end, color);
        return;
    }

    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        fill_circle_pixels(
            pixels,
            width,
            height,
            clip,
            start,
            line_width * 0.5,
            color,
            SpanFillMode::Normal,
        );
        return;
    }

    let nx = -dy / len * line_width * 0.5;
    let ny = dx / len * line_width * 0.5;
    fill_convex_polygon(
        pixels,
        width,
        height,
        clip,
        &[
            PixelPoint {
                x: start.x + nx,
                y: start.y + ny,
            },
            PixelPoint {
                x: end.x + nx,
                y: end.y + ny,
            },
            PixelPoint {
                x: end.x - nx,
                y: end.y - ny,
            },
            PixelPoint {
                x: start.x - nx,
                y: start.y - ny,
            },
        ],
        color,
    );
}

fn draw_thin_line(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    clip: PixelClip,
    start: PixelPoint,
    end: PixelPoint,
    color: RenderColor,
) {
    let raster_color = raster_color(color);
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i32;

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = (start.x + dx * t).round() as i32;
        let y = (start.y + dy * t).round() as i32;
        if x >= clip.min_x && y >= clip.min_y && x <= clip.max_x && y <= clip.max_y {
            paint_pixel(pixels, width, x, y, raster_color);
        }
    }
}

fn circle_covers_clip(center: PixelPoint, radius_sq: f32, clip: PixelClip) -> bool {
    [
        (clip.min_x, clip.min_y),
        (clip.max_x, clip.min_y),
        (clip.min_x, clip.max_y),
        (clip.max_x, clip.max_y),
    ]
    .into_iter()
    .all(|(x, y)| {
        let dx = x as f32 + 0.5 - center.x;
        let dy = y as f32 + 0.5 - center.y;
        dx * dx + dy * dy <= radius_sq
    })
}

fn fill_rect(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    clip: PixelClip,
    color: RasterColor,
    fill_mode: SpanFillMode,
) {
    for y in clip.min_y..=clip.max_y {
        let start = y as usize * width as usize + clip.min_x as usize;
        let end = y as usize * width as usize + clip.max_x as usize + 1;
        if color.opaque {
            pixels[start..end].fill(color.pixel);
        } else {
            match fill_mode {
                SpanFillMode::BackgroundFastPath { blended_background }
                    if pixels[start..end].iter().all(|pixel| *pixel == BACKGROUND) =>
                {
                    pixels[start..end].fill(blended_background);
                }
                _ => {
                    for pixel in &mut pixels[start..end] {
                        blend_pixel(pixel, color);
                    }
                }
            }
        }
    }
}

fn fill_span(
    pixels: &mut [Rgba8Pixel],
    width: u32,
    height: u32,
    clip: PixelClip,
    y: i32,
    left: i32,
    right: i32,
    color: RasterColor,
    fill_mode: SpanFillMode,
) {
    if y < clip.min_y || y > clip.max_y || y < 0 || y >= height as i32 {
        return;
    }

    let left = left.max(clip.min_x).max(0);
    let right = right.min(clip.max_x).min(width as i32 - 1);
    if left > right {
        return;
    }

    let start = y as usize * width as usize + left as usize;
    let end = y as usize * width as usize + right as usize + 1;
    if color.opaque {
        pixels[start..end].fill(color.pixel);
    } else {
        match fill_mode {
            SpanFillMode::BackgroundFastPath { blended_background }
                if pixels[start..end].iter().all(|pixel| *pixel == BACKGROUND) =>
            {
                pixels[start..end].fill(blended_background);
            }
            _ => {
                for pixel in &mut pixels[start..end] {
                    blend_pixel(pixel, color);
                }
            }
        }
    }
}

fn paint_pixel(pixels: &mut [Rgba8Pixel], width: u32, x: i32, y: i32, color: RasterColor) {
    let index = y as usize * width as usize + x as usize;
    if color.opaque {
        pixels[index] = color.pixel;
    } else {
        blend_pixel(&mut pixels[index], color);
    }
}

fn blend_pixel(destination: &mut Rgba8Pixel, source: RasterColor) {
    let alpha = source.pixel.a as f32 / 255.0;
    let inverse = 1.0 - alpha;
    destination.r = blend_channel(source.pixel.r, destination.r, alpha, inverse);
    destination.g = blend_channel(source.pixel.g, destination.g, alpha, inverse);
    destination.b = blend_channel(source.pixel.b, destination.b, alpha, inverse);
    destination.a = 255;
}

fn raster_color(color: RenderColor) -> RasterColor {
    let pixel = Rgba8Pixel {
        r: color_channel(color.r),
        g: color_channel(color.g),
        b: color_channel(color.b),
        a: color_channel(color.a),
    };
    RasterColor {
        pixel,
        opaque: pixel.a >= 250,
    }
}

fn color_channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn blend_channel(source: u8, destination: u8, alpha: f32, inverse_alpha: f32) -> u8 {
    (source as f32 * alpha + destination as f32 * inverse_alpha).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{
        Camera2, Fill, RenderCircle, RenderFrame, RenderLine, RenderPrimitive, Stroke,
    };

    #[test]
    fn rasterizes_filled_circle_into_image() {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        frame.push_primitive(
            0,
            RenderPrimitive::Circle(RenderCircle {
                center: RenderPoint::ZERO,
                radius: 20.0,
                fill: Some(Fill::new(RenderColor::RED)),
                stroke: None,
            }),
        );

        let image = image_from_frames_with_layout(
            &[frame],
            Viewport::new(100.0, 100.0),
            FrameLayout::EqualHorizontal,
        );
        let pixels = image.to_rgba8().expect("raster image should be rgba8");

        assert!(
            pixels
                .as_slice()
                .iter()
                .any(|pixel| pixel.r > BACKGROUND.r && pixel.g < BACKGROUND.g)
        );
    }

    #[test]
    fn rasterizes_filled_triangle_into_image() {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        frame.push_primitive(
            0,
            RenderPrimitive::Polygon(RenderPolygon {
                points: vec![
                    RenderPoint::new(-25.0, -25.0),
                    RenderPoint::new(25.0, -25.0),
                    RenderPoint::new(0.0, 25.0),
                ],
                fill: Some(Fill::new(RenderColor::GREEN)),
                stroke: None,
            }),
        );

        let image = image_from_frames_with_layout(
            &[frame],
            Viewport::new(100.0, 100.0),
            FrameLayout::EqualHorizontal,
        );
        let pixels = image.to_rgba8().expect("raster image should be rgba8");

        assert!(pixels.as_slice().iter().any(|pixel| pixel.g > BACKGROUND.g));
    }

    #[test]
    fn rasterizes_line_into_image() {
        let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        frame.push_primitive(
            0,
            RenderPrimitive::Line(RenderLine::new(
                RenderPoint::new(-40.0, -40.0),
                RenderPoint::new(40.0, 40.0),
                Stroke::new(RenderColor::WHITE, 1.0),
            )),
        );

        let image = image_from_frames_with_layout(
            &[frame],
            Viewport::new(100.0, 100.0),
            FrameLayout::EqualHorizontal,
        );
        let pixels = image.to_rgba8().expect("raster image should be rgba8");

        assert!(pixels.as_slice().iter().any(|pixel| pixel.r > 200));
    }

    #[test]
    fn alpha_blend_changes_destination_without_clearing_alpha() {
        let mut pixel = BACKGROUND;
        blend_pixel(
            &mut pixel,
            raster_color(RenderColor::rgba(1.0, 0.0, 0.0, 0.5)),
        );

        assert!(pixel.r > BACKGROUND.r);
        assert_eq!(pixel.a, 255);
    }

    #[test]
    fn background_fast_path_matches_normal_blend_on_background_span() {
        let color = RenderColor::rgba(1.0, 0.9, 0.1, 0.85);
        let raster_color = raster_color(color);
        let mut expected = vec![BACKGROUND; 8];
        let mut actual = vec![BACKGROUND; 8];
        let clip = PixelClip::new(8, 1, 0, 7, 0, 0).expect("clip should be valid");
        let mode = span_fill_mode_for_layer(SPACEWARS_SUN_LAYER, color);

        fill_span(
            &mut expected,
            8,
            1,
            clip,
            0,
            0,
            7,
            raster_color,
            SpanFillMode::Normal,
        );
        fill_span(&mut actual, 8, 1, clip, 0, 0, 7, raster_color, mode);

        assert_eq!(actual, expected);
    }

    #[test]
    fn background_fast_path_falls_back_on_non_background_span() {
        let color = RenderColor::rgba(1.0, 0.9, 0.1, 0.85);
        let raster_color = raster_color(color);
        let mut expected = vec![BACKGROUND; 8];
        let mut actual = vec![BACKGROUND; 8];
        expected[3] = Rgba8Pixel {
            r: 20,
            g: 30,
            b: 40,
            a: 255,
        };
        actual[3] = expected[3];
        let clip = PixelClip::new(8, 1, 0, 7, 0, 0).expect("clip should be valid");
        let mode = span_fill_mode_for_layer(SPACEWARS_PLANET_LAYER, color);

        fill_span(
            &mut expected,
            8,
            1,
            clip,
            0,
            0,
            7,
            raster_color,
            SpanFillMode::Normal,
        );
        fill_span(&mut actual, 8, 1, clip, 0, 0, 7, raster_color, mode);

        assert_eq!(actual, expected);
        assert_ne!(actual[3], BACKGROUND);
    }

    #[test]
    fn large_circle_is_clipped_to_its_frame_viewport() {
        let mut left = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        left.push_primitive(
            0,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                80.0,
                RenderColor::RED,
            )),
        );
        let right = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));

        let image = image_from_frames_with_layout(
            &[left, right],
            Viewport::new(100.0, 50.0),
            FrameLayout::EqualHorizontal,
        );
        let pixels = image.to_rgba8().expect("raster image should be rgba8");
        let right_pane_pixel = pixels.as_slice()[25 * 100 + 75];

        assert_eq!(right_pane_pixel, BACKGROUND);
    }

    #[test]
    fn primitive_visibility_rejects_distant_offscreen_circle() {
        let camera = Camera2::new(RenderPoint::ZERO, 100.0);
        let viewport = Viewport::new(100.0, 100.0);
        let circle = RenderPrimitive::Circle(RenderCircle::filled(
            RenderPoint::new(1_000.0, 1_000.0),
            5.0,
            RenderColor::WHITE,
        ));

        assert!(!primitive_visible(camera, viewport, &circle));
    }

    #[test]
    fn starfield_visibility_cache_reuses_quantized_camera_entry() {
        let camera = Camera2::new(RenderPoint::ZERO, 100.0);
        let viewport = Viewport::new(100.0, 100.0);
        let primitives = vec![
            RenderPrimitive::Polygon(RenderPolygon::filled(
                vec![
                    RenderPoint::new(-1.0, -1.0),
                    RenderPoint::new(1.0, -1.0),
                    RenderPoint::new(0.0, 1.0),
                ],
                RenderColor::WHITE,
            )),
            RenderPrimitive::Polygon(RenderPolygon::filled(
                vec![
                    RenderPoint::new(500.0, 500.0),
                    RenderPoint::new(501.0, 500.0),
                    RenderPoint::new(500.0, 501.0),
                ],
                RenderColor::WHITE,
            )),
        ];
        let mut cache = StarfieldVisibilityCache::default();

        assert_eq!(
            cache.visible_indices(camera, viewport, &primitives, 1),
            &[0]
        );
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(
            cache.visible_indices(
                Camera2::new(RenderPoint::new(10.0, 10.0), 100.0),
                viewport,
                &primitives,
                2
            ),
            &[0]
        );
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn primitive_count_counts_all_layers() {
        let mut frame = RenderFrame::new(Camera2::default());
        frame.push_primitive(
            0,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                1.0,
                RenderColor::WHITE,
            )),
        );
        frame.push_primitive(
            1,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                2.0,
                RenderColor::WHITE,
            )),
        );

        assert_eq!(primitive_count(&[frame]), 2);
    }

    #[test]
    fn raster_scale_preserves_logical_overview_cutoff() {
        assert_eq!(
            RasterOptions::for_scale(2.0).overview_minimum_object_diameter,
            render::MIN_SPACEWARS_OVERVIEW_OBJECT_DIAMETER * 2.0
        );
        assert_eq!(
            RasterOptions::for_scale(f32::NAN).overview_minimum_object_diameter,
            render::MIN_SPACEWARS_OVERVIEW_OBJECT_DIAMETER
        );
    }

    #[test]
    fn spacewars_raster_overview_skips_tiny_debris() {
        let player_frames = [
            RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0)),
            RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0)),
        ];
        let mut tiny_overview = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        tiny_overview.push_primitive(
            SPACEWARS_DEBRIS_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                0.1,
                RenderColor::RED,
            )),
        );
        let mut visible_overview = RenderFrame::new(Camera2::new(RenderPoint::ZERO, 100.0));
        visible_overview.push_primitive(
            SPACEWARS_DEBRIS_LAYER,
            RenderPrimitive::Circle(RenderCircle::filled(
                RenderPoint::ZERO,
                10.0,
                RenderColor::BLUE,
            )),
        );
        let frames = [
            player_frames[0].clone(),
            player_frames[1].clone(),
            tiny_overview,
            visible_overview,
        ];

        let image = image_from_frames_with_layout(
            &frames,
            Viewport::new(100.0, 100.0),
            FrameLayout::SpacewarsLocalPlay,
        );
        let pixels = image.to_rgba8().expect("raster image should be rgba8");
        let left_overview_center = pixels.as_slice()[79 * 100 + 18];
        let right_overview_center = pixels.as_slice()[79 * 100 + 82];

        assert_eq!(left_overview_center, BACKGROUND);
        assert!(right_overview_center.b > BACKGROUND.b);
    }
}
