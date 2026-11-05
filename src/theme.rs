//! Shared visual vocabulary: Ubuntu-native work surfaces and a single small-stroke icon set.
use gpui::{prelude::*, *};
use std::sync::atomic::{AtomicBool, Ordering};
/// Every color the interface paints, resolved for the active appearance.
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub raised: u32,
    pub control_hover: u32,
    pub line: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub selected: u32,
    pub row_alternate: u32,
    pub row_hover: u32,
    pub row_line: u32,
    pub chart_grid: u32,
    pub chart_grid_minor: u32,
    /// Text-weight series colors.
    pub receive: u32,
    pub transmit: u32,
    pub gpu: u32,
    pub vram: u32,
    /// Graph strokes and fills.
    pub receive_graph: u32,
    pub transmit_graph: u32,
    pub gpu_graph: u32,
    pub vram_graph: u32,
    pub memory_used: u32,
    pub memory_cache: u32,
    pub memory_buffer: u32,
    pub memory_free: u32,
    pub swap: u32,
    pub temperature: [(f32, u32); 5],
    pub cores: [u32; 24],
}
pub const LIGHT: Palette = Palette {
    bg: 0xf7f7f7,
    panel: 0xffffff,
    raised: 0xf1f1f1,
    control_hover: 0xe7e7e7,
    line: 0xd9d9d9,
    text: 0x3d3846,
    muted: 0x6f6b72,
    accent: 0x3584e4,
    selected: 0xe7e5ff,
    row_alternate: 0xfafafa,
    row_hover: 0xececec,
    row_line: 0xeeeeee,
    chart_grid: 0xd8dadd,
    chart_grid_minor: 0xd8daddb0,
    receive: 0x1c71d8,
    transmit: 0xc64600,
    gpu: 0x2e7d4f,
    vram: 0x7b3f94,
    receive_graph: 0x3584e4,
    transmit_graph: 0xff7800,
    gpu_graph: 0x2ec27e,
    vram_graph: 0x9141ac,
    memory_used: 0x2ec27e,
    memory_cache: 0x9141ac,
    memory_buffer: 0x3584e4,
    memory_free: 0xd5d5d5,
    swap: 0xc061cb,
    temperature: [
        (30., 0x3584e4),
        (55., 0x2ec27e),
        (70., 0xe5a50a),
        (85., 0xff7800),
        (100., 0xe01b24),
    ],
    cores: [
        0xe01b24, 0xff7800, 0xf6d32d, 0x33d17a, 0x26a269, 0x3584e4, 0x1c71d8, 0x9141ac, 0xc061cb,
        0xe66100, 0x2ec27e, 0x62a0ea, 0xa51d2d, 0xc64600, 0xe5a50a, 0x2d7f5e, 0x1a5fb4, 0x613583,
        0x813d9c, 0xff9b5e, 0x57e389, 0x99c1f1, 0xed333b, 0xf8e45c,
    ],
};
/// Adwaita dark, as GNOME System Monitor draws it: neutral grays, white text, and the
/// lighter accent and series tints that keep small labels and thin strokes readable.
pub const DARK: Palette = Palette {
    bg: 0x242424,
    panel: 0x2e2e2e,
    raised: 0x383838,
    control_hover: 0x454545,
    line: 0x484848,
    text: 0xf6f5f4,
    muted: 0xbcbbc0,
    accent: 0x78aeed,
    selected: 0x283c57,
    row_alternate: 0x323232,
    row_hover: 0x3c3c3c,
    row_line: 0x383838,
    chart_grid: 0x4a4a4a,
    chart_grid_minor: 0x4a4a4ab0,
    receive: 0x78aeed,
    transmit: 0xffa348,
    gpu: 0x57e389,
    vram: 0xdc8add,
    receive_graph: 0x62a0ea,
    transmit_graph: 0xff7800,
    gpu_graph: 0x33d17a,
    vram_graph: 0xc061cb,
    memory_used: 0x33d17a,
    memory_cache: 0xb36cd0,
    memory_buffer: 0x62a0ea,
    memory_free: 0x6a6a6a,
    swap: 0xdc8add,
    temperature: [
        (30., 0x78aeed),
        (55., 0x57e389),
        (70., 0xf8e45c),
        (85., 0xffa348),
        (100., 0xff7b63),
    ],
    cores: [
        0xed333b, 0xff7800, 0xf6d32d, 0x33d17a, 0x26a269, 0x3584e4, 0x62a0ea, 0xc061cb, 0xdc8add,
        0xe66100, 0x2ec27e, 0x99c1f1, 0xf66151, 0xffa348, 0xe5a50a, 0x5bc8af, 0xcdab8f, 0xb5835a,
        0x9141ac, 0xff9b5e, 0x57e389, 0x1c71d8, 0xc01c28, 0xf8e45c,
    ],
};
static DARK_MODE: AtomicBool = AtomicBool::new(true);
pub fn set_dark(dark: bool) {
    DARK_MODE.store(dark, Ordering::Relaxed);
}
pub fn is_dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}
pub fn colors() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}
#[derive(Clone, Copy)]
pub enum Icon {
    Close,
    Minimize,
    Maximize,
    Grip,
    Pause,
    Play,
    Copy,
    Resize,
    Columns,
    Search,
    Check,
    Plus,
}
pub fn icon(kind: Icon) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let lines: &[&[(f32, f32)]] = match kind {
                Icon::Check => &[&[(3., 8.), (6., 11.), (13., 4.)]],
                Icon::Plus => &[&[(3., 8.), (13., 8.)], &[(8., 3.), (8., 13.)]],
                Icon::Close => &[&[(4., 4.), (12., 12.)], &[(12., 4.), (4., 12.)]],
                Icon::Minimize => &[&[(3., 10.), (13., 10.)]],
                Icon::Maximize => &[&[(3., 3.), (13., 3.), (13., 13.), (3., 13.), (3., 3.)]],
                Icon::Grip => &[
                    &[(5., 3.), (5., 4.)],
                    &[(10., 3.), (10., 4.)],
                    &[(5., 7.), (5., 8.)],
                    &[(10., 7.), (10., 8.)],
                    &[(5., 11.), (5., 12.)],
                    &[(10., 11.), (10., 12.)],
                ],
                Icon::Pause => &[&[(5., 3.), (5., 13.)], &[(11., 3.), (11., 13.)]],
                Icon::Play => &[&[(5., 3.), (12., 8.), (5., 13.), (5., 3.)]],
                Icon::Copy => &[
                    &[(6., 5.), (13., 5.), (13., 13.), (6., 13.), (6., 5.)],
                    &[(10., 3.), (3., 3.), (3., 10.)],
                ],
                Icon::Resize => &[&[(7., 13.), (13., 7.)], &[(11., 13.), (13., 11.)]],
                Icon::Columns => &[
                    &[(2., 3.), (14., 3.), (14., 13.), (2., 13.), (2., 3.)],
                    &[(8., 3.), (8., 13.)],
                ],
                Icon::Search => &[
                    &[
                        (6., 2.),
                        (9., 3.),
                        (11., 6.),
                        (10., 9.),
                        (7., 11.),
                        (4., 10.),
                        (2., 7.),
                        (3., 4.),
                        (6., 2.),
                    ],
                    &[(10., 10.), (14., 14.)],
                ],
            };
            let mut path = PathBuilder::stroke(px(1.15));
            for line in lines {
                for (i, (x, y)) in line.iter().enumerate() {
                    let p = point(
                        bounds.origin.x + bounds.size.width * (*x / 16.),
                        bounds.origin.y + bounds.size.height * (*y / 16.),
                    );
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(colors().muted));
            }
        },
    )
    .size(px(14.))
}
pub fn icon_button(id: impl Into<ElementId>, kind: Icon) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(28.))
        .h(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|d| d.bg(rgb(colors().control_hover)))
        .child(icon(kind))
}
