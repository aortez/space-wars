//! Compatibility for the V3D discard/stencil regression fixed in Mesa 25.2.3.
//! Keep overlap suppression intact; do not disable stencil strokes globally.
use glow::HasContext;

use crate::ErrorKind;

fn affected(renderer: &str, version: &str) -> bool {
    if !renderer.starts_with("V3D ") {
        return false;
    }
    let Some(version) = version.split_once("Mesa ").map(|(_, v)| v) else {
        return false;
    };
    let mut parts = version.split('.');
    let parsed = (|| {
        Some((
            parts.next()?.parse::<u32>().ok()?,
            parts.next()?.parse::<u32>().ok()?,
            parts
                .next()?
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse::<u32>()
                .ok()?,
        ))
    })();
    parsed.is_some_and(|version| (24, 2, 7) <= version && version < (25, 2, 3))
}

pub(super) fn fragment_prefix(context: &glow::Context) -> Result<&'static str, ErrorKind> {
    // Only queried during shader creation, never while drawing a frame.
    let (renderer, version) = unsafe {
        (
            context.get_parameter_string(glow::RENDERER),
            context.get_parameter_string(glow::VERSION),
        )
    };
    if !affected(&renderer, &version) {
        return Ok("");
    }
    if !context.supported_extensions().contains("GL_EXT_frag_depth") {
        return Err(ErrorKind::GeneralError(
            "V3D stencil compatibility requires GL_EXT_frag_depth or Mesa >= 25.2.3".into(),
        ));
    }
    Ok("#extension GL_EXT_frag_depth : require\n#define V3D_LATE_STENCIL 1\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workaround_is_limited_to_affected_v3d_mesa_releases() {
        for version in ["24.2.7", "24.3.0", "25.0.7", "25.1.6", "25.2.2-devel"] {
            assert!(affected("V3D 4.2.14.0", &format!("OpenGL ES 3.1 Mesa {version}")));
        }
        for version in ["24.2.6", "25.2.3", "25.3.0", "26.0.0", "garbled", "25.1."] {
            assert!(!affected("V3D 4.2.14.0", &format!("OpenGL ES 3.1 Mesa {version}")));
        }
        assert!(!affected("AMD Radeon", "OpenGL ES 3.2 Mesa 25.1.6"));
        assert!(!affected("V3D 4.2.14.0", "OpenGL ES 3.1 other driver"));
    }
}
