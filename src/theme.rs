//! Shared visual vocabulary: Ubuntu-native work surfaces and a single small-stroke icon set.
use gpui::{prelude::*, *};
pub const BG: u32 = 0xf7f7f7;
pub const PANEL: u32 = 0xffffff;
pub const RAISED: u32 = 0xf1f1f1;
pub const LINE: u32 = 0xd9d9d9;
pub const TEXT: u32 = 0x3d3846;
pub const MUTED: u32 = 0x6f6b72;
pub const ACCENT: u32 = 0x3584e4;
pub const SELECTED: u32 = 0xe7e5ff;
pub const CHART_GRID: u32 = 0xd8dadd;
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
                window.paint_path(path, rgb(MUTED));
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
        .hover(|d| d.bg(rgb(0xe7e7e7)))
        .child(icon(kind))
}
