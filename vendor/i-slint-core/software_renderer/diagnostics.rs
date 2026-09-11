// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Optional, clock-free observation points for local software-renderer diagnostics.

/// Nested CPU-rendering operations. Begin/end events do not imply visible pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum DrawDiagnostic {
    /// Complete render call.
    Render,
    /// Window/buffer setup before dirty-region calculation.
    Prepare,
    /// Compute dirty regions and update item caches.
    DirtyRegion,
    /// Traverse and render the component trees, including operations below.
    Items,
    /// Image preparation and drawing, including texture operations.
    Image,
    /// Target texture operation, including accelerated or fallback drawing.
    Texture,
    /// Software texture fallback, including sampling and pixel writes.
    TextureFallback,
    /// Rectangle operation, including borders and gradients.
    Rectangle,
    /// Text preparation and glyph drawing.
    Text,
    /// Path operation (possibly unsupported by this renderer).
    Path,
    /// Window background fill (accelerated or software fallback).
    Background,
}

/// Number of diagnostic operation kinds.
pub const DRAW_DIAGNOSTIC_COUNT: usize = 11;

/// Receives balanced begin (`true`) and end (`false`) events on the rendering
/// thread. Observers must not re-enter rendering or panic. No observer is installed
/// by default; the renderer itself does not read clocks or retain measurements.
pub type DrawDiagnosticObserver = fn(DrawDiagnostic, bool);

pub(super) struct Span {
    observer: Option<DrawDiagnosticObserver>,
    operation: DrawDiagnostic,
}

impl Span {
    pub(super) fn new(observer: Option<DrawDiagnosticObserver>, operation: DrawDiagnostic) -> Self {
        if let Some(observer) = observer {
            observer(operation, true);
        }
        Self { observer, operation }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(observer) = self.observer {
            observer(self.operation, false);
        }
    }
}
