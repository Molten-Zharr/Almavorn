use almavorn::preferences::FontFace;
use ratatui::{buffer::Cell, text::Span};
use rustc_hash::FxHashSet;
use soft_ratatui::{EmbeddedTTF, RasterBackend, RgbPixmap, SoftBackend, rusttype::Font};

/// Keep font backgrounds and glyphs inside the terminal cells they occupy.
pub struct CellFont {
    inner: EmbeddedTTF,
    cell: RgbPixmap,
    blink_cells: FxHashSet<(u16, u16)>,
}

impl RasterBackend for CellFont {
    fn draw_cell(
        &mut self,
        x: u16,
        y: u16,
        cell: &Cell,
        always_redraw_list: &mut FxHashSet<(u16, u16)>,
        blinking_fast: bool,
        blinking_slow: bool,
        char_width: usize,
        char_height: usize,
        pixmap: &mut RgbPixmap,
    ) {
        let width = char_width * Span::raw(cell.symbol()).width().max(1);
        if self.cell.width < width || self.cell.height != char_height {
            self.cell = RgbPixmap::new(width, char_height);
        }
        self.blink_cells.clear();
        // EmbeddedTTF's background can exceed a cell by a few pixels. Render
        // locally and copy only the occupied cells, including wide characters.
        self.inner.draw_cell(
            0,
            0,
            cell,
            &mut self.blink_cells,
            blinking_fast,
            blinking_slow,
            width,
            char_height,
            &mut self.cell,
        );
        if self.blink_cells.is_empty() {
            always_redraw_list.remove(&(x, y));
        } else {
            always_redraw_list.insert((x, y));
        }
        let left = usize::from(x) * char_width;
        let top = usize::from(y) * char_height;
        let width = width.min(pixmap.width.saturating_sub(left));
        for row in 0..char_height.min(pixmap.height.saturating_sub(top)) {
            let source = row * self.cell.width * 3;
            let destination = ((top + row) * pixmap.width + left) * 3;
            pixmap.data[destination..destination + width * 3]
                .copy_from_slice(&self.cell.data[source..source + width * 3]);
        }
    }
}

pub fn backend(face: FontFace, size: u16, columns: u16, rows: u16) -> SoftBackend<CellFont> {
    let bytes: &'static [u8] = match face {
        FontFace::FiraCodeBold => include_bytes!("../assets/fonts/FiraCode-Bold.ttf"),
        FontFace::FiraCode => include_bytes!("../assets/fonts/FiraCode-Regular.ttf"),
        FontFace::DejaVuMono => include_bytes!("../assets/fonts/DejaVuSansMono.ttf"),
    };
    let font = Font::try_from_bytes(bytes).expect("Bundled font is valid");
    let bold = Font::try_from_bytes(include_bytes!("../assets/fonts/FiraCode-Bold.ttf"))
        .expect("Bundled bold font is valid");
    let backend = SoftBackend::<EmbeddedTTF>::new(
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
    );
    SoftBackend {
        raster_backend: CellFont {
            inner: backend.raster_backend,
            cell: RgbPixmap::new(backend.char_width, backend.char_height),
            blink_cells: FxHashSet::default(),
        },
        buffer: backend.buffer,
        cursor: backend.cursor,
        cursor_pos: backend.cursor_pos,
        cursor_config: backend.cursor_config,
        char_width: backend.char_width,
        char_height: backend.char_height,
        frame_count: backend.frame_count,
        blink_config: backend.blink_config,
        rgb_pixmap: backend.rgb_pixmap,
        always_redraw_list: backend.always_redraw_list,
        rendered_cursor: backend.rendered_cursor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        Terminal,
        layout::Rect,
        style::{Color, Modifier, Style},
        widgets::{Block, Paragraph},
    };

    #[test]
    fn clearing_a_selected_cell_removes_its_entire_background() {
        for face in FontFace::ALL {
            for size in [12, 16, 24] {
                for symbol in ["●", "A", "F", "[", "←", "╭"] {
                    let mut terminal = Terminal::new(backend(face, size, 5, 3)).unwrap();
                    terminal
                        .draw(|frame| {
                            frame.render_widget(
                                Block::default().style(Style::default().bg(Color::Black)),
                                frame.area(),
                            );
                        })
                        .unwrap();
                    terminal
                        .draw(|frame| {
                            frame.render_widget(
                                Block::default().style(Style::default().bg(Color::Black)),
                                frame.area(),
                            );
                            frame.render_widget(
                                Paragraph::new(symbol).style(
                                    Style::default()
                                        .fg(Color::Red)
                                        .bg(Color::Rgb(82, 69, 73))
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Rect::new(1, 1, 1, 1),
                            );
                        })
                        .unwrap();
                    terminal
                        .draw(|frame| {
                            frame.render_widget(
                                Block::default().style(Style::default().bg(Color::Black)),
                                frame.area(),
                            );
                        })
                        .unwrap();
                    let raster = terminal.backend();
                    let leftover = raster
                        .get_pixmap_data()
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .enumerate()
                        .find(|(_, rgb)| **rgb != [0, 0, 0]);
                    assert!(
                        leftover.is_none(),
                        "{} {size}px {symbol}: leftover pixel {leftover:?}, cell {}x{}",
                        face.name(),
                        raster.char_width,
                        raster.char_height,
                    );
                }
            }
        }
    }

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
                    small.raster_backend.inner.font_regular.glyph(c).id().0,
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
                small.raster_backend.inner.font_regular.glyph(repeat).id().0,
                0,
                "{}: {repeat}",
                face.name()
            );
        }
    }
}
