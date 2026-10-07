use almavorn::preferences::FontFace;
use soft_ratatui::{EmbeddedTTF, SoftBackend, rusttype::Font};

pub fn backend(face: FontFace, size: u16, columns: u16, rows: u16) -> SoftBackend<EmbeddedTTF> {
    let bytes: &'static [u8] = match face {
        FontFace::FiraCodeBold => include_bytes!("../assets/fonts/FiraCode-Bold.ttf"),
        FontFace::FiraCode => include_bytes!("../assets/fonts/FiraCode-Regular.ttf"),
        FontFace::DejaVuMono => include_bytes!("../assets/fonts/DejaVuSansMono.ttf"),
    };
    let font = Font::try_from_bytes(bytes).expect("Bundled font is valid");
    let bold = Font::try_from_bytes(include_bytes!("../assets/fonts/FiraCode-Bold.ttf"))
        .expect("Bundled bold font is valid");
    SoftBackend::<EmbeddedTTF>::new(
        columns,
        rows,
        u32::from(size),
        font,
        if face == FontFace::DejaVuMono {
            None
        } else {
            Some(bold)
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_fonts_support_cyrillic_and_change_cell_size() {
        for face in FontFace::ALL {
            let small = backend(face, 12, 2, 2);
            let large = backend(face, 24, 2, 2);
            assert!(small.char_width > 0 && small.char_height > 0);
            assert!(large.char_height > small.char_height);
            for c in "Настройки ABC 0123 ↑↓ ←→ ↔∞ ─│┌┐╭╮ ▶■‹›●…"
                .chars()
                .filter(|c| !c.is_whitespace())
            {
                assert_ne!(
                    small.raster_backend.font_regular.glyph(c).id().0,
                    0,
                    "{}: {c}",
                    face.name()
                );
            }
            let repeat = if face == FontFace::DejaVuMono {
                '↻'
            } else {
                '⟳'
            };
            assert_ne!(
                small.raster_backend.font_regular.glyph(repeat).id().0,
                0,
                "{}: {repeat}",
                face.name()
            );
        }
    }
}
