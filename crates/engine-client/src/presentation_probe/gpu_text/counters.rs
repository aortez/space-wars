//! Count draw destinations in a text-only fixture. Nonzero FBOs belong to
//! FemtoVG's glyph atlas here, not arbitrary Slint layers in the full app.
//! No clocks, allocation, GL queries or waits inside the wrappers.
use std::{
    cell::Cell,
    ffi::{CStr, c_void},
};

#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Counts {
    pub screen_draws: u64,
    pub atlas_draws: u64,
    pub buffer_bytes: u64,
    pub texture_store_bytes: u64,
    pub texture_upload_bytes: u64,
}
thread_local! {
    static FRAMEBUFFER: Cell<u32> = const { Cell::new(0) };
    static COUNTS: Cell<Counts> = Cell::new(Counts::default());
}
pub(super) fn reset() {
    // Each measured render begins by binding the pbuffer's default framebuffer.
    FRAMEBUFFER.set(0);
    COUNTS.set(Counts::default());
}
pub(super) fn counts() -> Counts {
    COUNTS.get()
}
fn edit(f: impl FnOnce(&mut Counts)) {
    let mut counts = COUNTS.get();
    f(&mut counts);
    COUNTS.set(counts);
}
fn drew() {
    edit(|c| {
        if FRAMEBUFFER.get() == 0 {
            c.screen_draws += 1
        } else {
            c.atlas_draws += 1
        }
    });
}
fn bytes(w: i32, h: i32, format: u32, kind: u32) -> u64 {
    let channels = match format {
        0x1903 | 0x1909 => 1,
        0x1907 => 3,
        0x1908 => 4,
        _ => 0,
    };
    if w <= 0 || h <= 0 || kind != 0x1401 {
        0
    } else {
        w as u64 * h as u64 * channels
    }
}
macro_rules! entry {
    ($slot:ident, $ty:ident, $func:ident, ($($arg:ident: $arg_ty:ty),*), $after:block) => {
        type $ty = unsafe extern "system" fn($($arg_ty),*);
        thread_local! { static $slot: Cell<Option<$ty>> = const { Cell::new(None) }; }
        unsafe extern "system" fn $func($($arg: $arg_ty),*) {
            let real = $slot.get().expect("probe GL loader installed");
            // SAFETY: Exact GL signature from the current context; forward all
            // arguments unchanged and never dereference application buffers.
            unsafe { real($($arg),*); }
            $after
        }
    };
}
entry!(BIND, Bind, bind, (target: u32, framebuffer: u32), {
    if matches!(target, 0x8d40 | 0x8ca9) { FRAMEBUFFER.set(framebuffer); }
});
entry!(ARRAYS, Arrays, arrays, (mode: u32, first: i32, count: i32), { drew(); });
entry!(ELEMENTS, Elements, elements, (mode: u32, count: i32, kind: u32, indices: *const c_void), { drew(); });
entry!(BUFFER, Buffer, buffer, (target: u32, size: isize, data: *const c_void, usage: u32), {
    if size > 0 && !data.is_null() { edit(|c| c.buffer_bytes += size as u64); }
});
entry!(IMAGE, Image, image, (target: u32, level: i32, internal: i32, w: i32, h: i32, border: i32, format: u32, kind: u32, data: *const c_void), {
    edit(|c| {
        c.texture_store_bytes += bytes(w, h, format, kind);
        if !data.is_null() { c.texture_upload_bytes += bytes(w, h, format, kind); }
    });
});
entry!(SUB_IMAGE, SubImage, sub_image, (target: u32, level: i32, x: i32, y: i32, w: i32, h: i32, format: u32, kind: u32, data: *const c_void), {
    if !data.is_null() { edit(|c| c.texture_upload_bytes += bytes(w, h, format, kind)); }
});

pub(super) unsafe fn wrap(name: &CStr, address: *const c_void) -> *const c_void {
    if address.is_null() {
        return address;
    }
    macro_rules! install {
        ($slot:ident, $ty:ident, $func:ident) => {{
            // SAFETY: The entry-point name identifies this exact function ABI.
            $slot.set(Some(unsafe {
                std::mem::transmute::<*const c_void, $ty>(address)
            }));
            $func as *const c_void
        }};
    }
    match name.to_bytes() {
        b"glBindFramebuffer" => install!(BIND, Bind, bind),
        b"glDrawArrays" => install!(ARRAYS, Arrays, arrays),
        b"glDrawElements" => install!(ELEMENTS, Elements, elements),
        b"glBufferData" => install!(BUFFER, Buffer, buffer),
        b"glTexImage2D" => install!(IMAGE, Image, image),
        b"glTexSubImage2D" => install!(SUB_IMAGE, SubImage, sub_image),
        _ => address,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draw_destinations_ignore_read_framebuffer_and_preserve_arguments() {
        thread_local! { static LAST: Cell<usize> = const { Cell::new(0) }; }
        unsafe extern "system" fn real_bind(_: u32, _: u32) {}
        unsafe extern "system" fn real_draw(mode: u32, count: i32, kind: u32, p: *const c_void) {
            assert_eq!((mode, count, kind), (4, 6, 0x1403));
            LAST.set(p as usize);
        }
        let bind: Bind =
            unsafe { std::mem::transmute(wrap(c"glBindFramebuffer", real_bind as *const c_void)) };
        let draw: Elements =
            unsafe { std::mem::transmute(wrap(c"glDrawElements", real_draw as *const c_void)) };
        reset();
        unsafe {
            draw(4, 6, 0x1403, 12usize as *const c_void);
            bind(0x8d40, 7);
            draw(4, 6, 0x1403, 12usize as *const c_void);
            bind(0x8ca8, 0); // GL_READ_FRAMEBUFFER must not change draw destination.
            draw(4, 6, 0x1403, 12usize as *const c_void);
            bind(0x8ca9, 0);
            draw(4, 6, 0x1403, 12usize as *const c_void);
        }
        assert_eq!((counts().screen_draws, counts().atlas_draws), (2, 2));
        assert_eq!(LAST.get(), 12);
    }
    #[test]
    fn payload_counts_are_logical_known_formats_only() {
        assert_eq!(bytes(512, 512, 0x1908, 0x1401), 1_048_576);
        assert_eq!(bytes(-1, 512, 0x1908, 0x1401), 0);
        assert_eq!(bytes(512, 512, 0x1908, 0x1406), 0);
        assert_eq!(bytes(512, 512, 0, 0x1401), 0);
    }
}
