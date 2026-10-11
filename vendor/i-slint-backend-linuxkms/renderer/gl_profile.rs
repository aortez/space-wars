// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! CPU time inside a few GL allocation/upload calls, nested within Draw.
//! No readbacks, synchronization, per-call allocation or GPU-time claims.
//! Entry points and their wrappers stay on the renderer's context thread.

use std::{
    cell::Cell,
    ffi::{c_void, CStr},
};

use crate::profiling::{self, Counter, Scope, Stage};

macro_rules! entry_point {
    ($slot:ident, $wrapper:ident, $signature:ident, $stage:ident,
        ($($arg:ident: $ty:ty),*), $after:block) => {
        type $signature = unsafe extern "system" fn($($ty),*);
        thread_local! {
            static $slot: Cell<Option<$signature>> = const { Cell::new(None) };
        }
        unsafe extern "system" fn $wrapper($($arg: $ty),*) {
            // Installed before exposing this wrapper to the GL loader. It is
            // never called without a current context on the same UI thread.
            let real = $slot.get().expect("GL profiler entry point not installed");
            let scope = if matches!(Stage::$stage, Stage::GlCompile | Stage::GlLink) {
                Scope::trace(Stage::$stage)
            } else {
                Scope::new(Stage::$stage)
            };
            // SAFETY: The loader supplied this exact GL signature. Forward
            // every argument unchanged, without dereferencing caller buffers.
            unsafe { real($($arg),*); }
            drop(scope);
            $after
        }
    };
}

entry_point!(TEX_IMAGE, tex_image, TexImage, GlTextureAllocate,
(target: u32, level: i32, internal_format: i32, width: i32, height: i32,
 border: i32, format: u32, kind: u32, pixels: *const c_void), {
    if !pixels.is_null() {
        profiling::gl_uploaded(payload_bytes(width, height, format, kind));
    }
});
entry_point!(TEX_SUB_IMAGE, tex_sub_image, TexSubImage, GlTextureUpload,
(target: u32, level: i32, x: i32, y: i32, width: i32, height: i32,
 format: u32, kind: u32, pixels: *const c_void), {
    profiling::gl_uploaded(payload_bytes(width, height, format, kind));
});
entry_point!(DELETE_TEXTURES, delete_textures, DeleteTextures, GlTextureDelete,
    (count: i32, textures: *const u32), {});
entry_point!(BUFFER_DATA, buffer_data, BufferData, GlBufferUpload,
(target: u32, size: isize, data: *const c_void, usage: u32), {
    if !data.is_null() && size > 0 { profiling::gl_buffer_uploaded(size as u64); }
});
entry_point!(COMPILE_SHADER, compile_shader, CompileShader, GlCompile, (shader: u32), {});
entry_point!(LINK_PROGRAM, link_program, LinkProgram, GlLink, (program: u32), {});

// Ordinarily only count draws. Extra clocks are bounded to an explicit trace.
macro_rules! draw_entry_point {
    ($slot:ident, $wrapper:ident, $signature:ident, ($($arg:ident: $ty:ty),*)) => {
        type $signature = unsafe extern "system" fn($($ty),*);
        thread_local! { static $slot: Cell<Option<$signature>> = const { Cell::new(None) }; }
        unsafe extern "system" fn $wrapper($($arg: $ty),*) {
            let real = $slot.get().expect("GL draw counter not installed");
            let scope = Scope::trace(Stage::GlDraw);
            // SAFETY: Forward the exact signature and pointer without dereferencing it.
            unsafe { real($($arg),*); }
            drop(scope);
            profiling::count(Counter::GlDrawCalls);
        }
    };
}
draw_entry_point!(DRAW_ARRAYS, draw_arrays, DrawArrays, (mode: u32, first: i32, count: i32));
draw_entry_point!(DRAW_ELEMENTS, draw_elements, DrawElements, (mode: u32, count: i32, kind: u32, indices: *const c_void));

// Logical uncompressed texture payload, not actual bus traffic, allocation
// size, row padding or vertex data. Only formats used by FemtoVG are counted.
fn payload_bytes(width: i32, height: i32, format: u32, kind: u32) -> u64 {
    if width <= 0 || height <= 0 || kind != 0x1401 {
        // GL_UNSIGNED_BYTE
        return 0;
    }
    let channels = match format {
        0x1903 | 0x1909 => 1, // GL_RED / GL_LUMINANCE
        0x1907 => 3,          // GL_RGB
        0x1908 => 4,          // GL_RGBA
        _ => return 0,
    };
    width as u64 * height as u64 * channels
}

/// The caller must supply the address for `name` from the live GL context.
pub(super) unsafe fn wrap(name: &CStr, address: *const c_void) -> *const c_void {
    if address.is_null() || !profiling::enabled() {
        return address;
    }
    macro_rules! install {
        ($slot:ident, $signature:ident, $wrapper:ident) => {{
            // SAFETY: The matching name identifies the GL ABI below. This
            // pointer is only called on this context's renderer thread.
            $slot.set(Some(unsafe {
                std::mem::transmute::<*const c_void, $signature>(address)
            }));
            $wrapper as *const c_void
        }};
    }
    match name.to_bytes() {
        b"glTexImage2D" => install!(TEX_IMAGE, TexImage, tex_image),
        b"glTexSubImage2D" => install!(TEX_SUB_IMAGE, TexSubImage, tex_sub_image),
        b"glDeleteTextures" => install!(DELETE_TEXTURES, DeleteTextures, delete_textures),
        b"glBufferData" => install!(BUFFER_DATA, BufferData, buffer_data),
        b"glDrawArrays" => install!(DRAW_ARRAYS, DrawArrays, draw_arrays),
        b"glDrawElements" => install!(DRAW_ELEMENTS, DrawElements, draw_elements),
        b"glCompileShader" => install!(COMPILE_SHADER, CompileShader, compile_shader),
        b"glLinkProgram" => install!(LINK_PROGRAM, LinkProgram, link_program),
        _ => address,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_counter_forwards_the_index_offset_without_timing_each_draw() {
        thread_local! { static CALLED: Cell<bool> = const { Cell::new(false) }; }
        unsafe extern "system" fn real(mode: u32, count: i32, kind: u32, offset: *const c_void) {
            CALLED.set((mode, count, kind, offset as usize) == (4, 12, 0x1405, 16));
        }
        profiling::activate_for_test((32, 32));
        let wrapped: DrawElements =
            unsafe { std::mem::transmute(wrap(c"glDrawElements", real as *const c_void)) };
        {
            let _frame = Scope::new(Stage::Loop);
            unsafe {
                wrapped(4, 12, 0x1405, 16usize as *const c_void);
            }
        }
        assert!(CALLED.get());
        assert!(profiling::diagnostics().contains("kms_gl_draw_calls_window=1"));
    }

    #[test]
    fn disabled_profiling_preserves_the_driver_pointer() {
        std::thread::spawn(|| {
            unsafe extern "system" fn real(_: i32, _: *const u32) {}
            assert!(!profiling::enabled());
            let address = real as *const c_void;
            assert_eq!(unsafe { wrap(c"glDeleteTextures", address) }, address);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn counts_only_known_uncompressed_payloads_without_overflow() {
        assert_eq!(payload_bytes(1024, 768, 0x1907, 0x1401), 2_359_296);
        assert_eq!(payload_bytes(1, 2, 0x1908, 0x1401), 8);
        assert_eq!(payload_bytes(4, 8, 0x1903, 0x1401), 32);
        assert_eq!(payload_bytes(-1, 8, 0x1907, 0x1401), 0);
        assert_eq!(payload_bytes(1, 0, 0x1907, 0x1401), 0);
        assert_eq!(payload_bytes(1, 1, 0, 0x1401), 0);
        assert_eq!(payload_bytes(1, 1, 0x1907, 0), 0);
        assert_eq!(
            payload_bytes(i32::MAX, i32::MAX, 0x1908, 0x1401),
            18_446_744_056_529_682_436
        );
    }

    #[test]
    fn detailed_draw_compile_and_link_clocks_are_explicit_and_bounded() {
        std::thread::spawn(|| {
            thread_local! { static OBJECT: Cell<u32> = const { Cell::new(0) }; }
            unsafe extern "system" fn object(id: u32) {
                OBJECT.set(id);
            }
            unsafe extern "system" fn draw(_: u32, _: i32, _: i32) {}
            profiling::activate_femtovg((32, 32), None, Some(8));
            let draw: DrawArrays =
                unsafe { std::mem::transmute(wrap(c"glDrawArrays", draw as *const c_void)) };
            let compile: CompileShader =
                unsafe { std::mem::transmute(wrap(c"glCompileShader", object as *const c_void)) };
            let link: LinkProgram =
                unsafe { std::mem::transmute(wrap(c"glLinkProgram", object as *const c_void)) };
            {
                let _frame = Scope::new(Stage::Loop);
                unsafe {
                    draw(4, 0, 3);
                }
            }
            assert!(profiling::diagnostics().contains("kms_gl_draw_calls=0\n"));
            profiling::render_trace("start").unwrap();
            for _ in 0..241 {
                let _frame = Scope::new(Stage::Loop);
                profiling::render_trace_marker(3);
                unsafe {
                    draw(4, 0, 3);
                    compile(17);
                }
                assert_eq!(OBJECT.get(), 17);
                unsafe {
                    link(18);
                }
                assert_eq!(OBJECT.get(), 18);
            }
            let report = profiling::render_trace("read").unwrap();
            assert!(report.contains("kms_trace_state=complete"));
            assert!(report.contains("kms_trace_samples=240"));
            assert!(profiling::diagnostics().contains("kms_gl_draw_calls=119\n"));
            // The last (241st) loop is still counted, but no longer timed.
            assert!(profiling::diagnostics().contains("kms_gl_draw_calls_window=120\n"));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn wrapper_forwards_arguments_and_records_only_completed_loops() {
        thread_local! { static CALLED: Cell<bool> = const { Cell::new(false) }; }
        unsafe extern "system" fn real(
            target: u32,
            level: i32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            format: u32,
            kind: u32,
            pixels: *const c_void,
        ) {
            CALLED.set(
                (target, level, x, y, width, height, format, kind)
                    == (0x0de1, 0, 3, 4, 10, 20, 0x1907, 0x1401)
                    && pixels.is_null(),
            );
        }
        profiling::activate_for_test((1024, 768));
        // SAFETY: Matching test GL function signature; no actual GL calls.
        let wrapped: TexSubImage =
            unsafe { std::mem::transmute(wrap(c"glTexSubImage2D", real as *const c_void)) };
        {
            let _frame = Scope::new(Stage::Loop);
            unsafe {
                wrapped(0x0de1, 0, 3, 4, 10, 20, 0x1907, 0x1401, std::ptr::null());
            }
            assert!(!profiling::diagnostics().contains("kms_gl_texture_upload_cpu_avg_ms="));
        }
        assert!(CALLED.get());
        let status = profiling::diagnostics();
        assert!(status.contains("kms_gl_texture_upload_calls=1\n"));
        assert!(status.contains("kms_gl_upload_bytes_window=600\n"));
        assert!(status.contains("kms_gl_texture_upload_cpu_avg_ms="));
        assert_eq!(
            unsafe { wrap(c"glUnknown", real as *const c_void) },
            real as *const c_void
        );
        assert!(unsafe { wrap(c"glTexImage2D", std::ptr::null()) }.is_null());
    }
}
