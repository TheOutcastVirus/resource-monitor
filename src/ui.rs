use crate::theme::{ACCENT, BG, Icon, LINE, MUTED, PANEL, RAISED, TEXT, icon, icon_button};
use crate::{
    chart::{self, COLORS, History},
    layout::{Layout, Panel, Settings},
    metrics::{self, Feed, Process, Snapshot},
};
use gpui::{prelude::*, *};
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
#[derive(Clone)]
struct PanelDrag(Panel);
impl Render for PanelDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_4()
            .py_2()
            .bg(rgb(LINE))
            .text_color(rgb(ACCENT))
            .rounded_md()
            .child(self.0.title())
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Entry {
    None,
    Search,
    Preset,
}
#[derive(Clone, Copy)]
enum Resize {
    Height(Panel, f32, f32),
}
pub struct Monitor {
    feed: Feed,
    sample: Arc<Snapshot>,
    history: History,
    cpu_chart: Entity<chart::AnimatedChart>,
    gpu_chart: Entity<chart::AnimatedChart>,
    network_chart: Entity<chart::AnimatedChart>,
    process_table: Entity<crate::table::ProcessTable>,
    gpu_table: Entity<crate::table::ProcessTable>,
    effective_columns: usize,
    settings: Settings,
    focus: FocusHandle,
    menu: bool,
    paused: bool,
    pause_at: Instant,
    paused_duration: Duration,
    entry: Entry,
    query: String,
    preset_name: String,
    notice: String,
    selected: Option<Process>,
    rows: Vec<usize>,
    gpu_rows: Vec<usize>,
    gpu_index: usize,
    resize: Option<Resize>,
    fitted: Vec<(Panel, f32)>,
    _timer: Task<()>,
}
fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(10.))
        .py(px(5.))
        .rounded(px(6.))
        .bg(rgb(RAISED))
        .border_1()
        .border_color(rgb(LINE))
        .cursor_pointer()
        .text_color(rgb(TEXT))
        .hover(|s| s.bg(rgb(0xe7e7e7)))
        .child(label.into())
}
fn muted(label: impl Into<SharedString>) -> Div {
    div().text_color(rgb(MUTED)).child(label.into())
}
fn inline_metric(label: &'static str, value: String, label_color: u32, value_color: u32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(label_color))
                .child(label),
        )
        .child(div().text_color(rgb(value_color)).child(value))
}
fn opt(v: Option<u32>, unit: &str) -> String {
    v.map(|v| format!("{v}{unit}")).unwrap_or("—".into())
}

fn mix_color(from: u32, to: u32, amount: f32) -> u32 {
    let channel = |shift| {
        let a = ((from >> shift) & 0xffu32) as f32;
        let b = ((to >> shift) & 0xffu32) as f32;
        (a + (b - a) * amount).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn temperature_color(temperature: f32) -> u32 {
    const STOPS: [(f32, u32); 5] = [
        (30., 0x3584e4),
        (55., 0x2ec27e),
        (70., 0xe5a50a),
        (85., 0xff7800),
        (100., 0xe01b24),
    ];
    if !temperature.is_finite() {
        return MUTED;
    }
    for pair in STOPS.windows(2) {
        if temperature <= pair[1].0 {
            let amount = ((temperature - pair[0].0) / (pair[1].0 - pair[0].0)).clamp(0., 1.);
            return mix_color(pair[0].1, pair[1].1, amount);
        }
    }
    STOPS.last().unwrap().1
}

fn gpu_meter(label: &'static str, value: String, ratio: f32, color: u32) -> Div {
    div()
        .flex()
        .items_center()
        .w(px(320.))
        .min_w_0()
        .gap_2()
        .child(
            div()
                .w(px(36.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(color))
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .h(px(8.))
                .rounded(px(2.))
                .overflow_hidden()
                .bg(rgb(LINE))
                .child(
                    div()
                        .h_full()
                        .w(relative(ratio.clamp(0., 1.)))
                        .bg(rgb(color)),
                ),
        )
        .child(
            div()
                .w(px(112.))
                .text_right()
                .text_size(px(10.))
                .child(value),
        )
}
impl Monitor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (settings, error) = Settings::load();
        let focus = cx.focus_handle();
        window.focus(&focus);
        let cpu_chart = cx.new(|cx| chart::AnimatedChart::new(window, cx));
        let gpu_chart = cx.new(|cx| chart::AnimatedChart::new(window, cx));
        let network_chart = cx.new(|cx| chart::AnimatedChart::new(window, cx));
        let process_table = cx.new(|_| crate::table::ProcessTable::new(false));
        let gpu_table = cx.new(|_| crate::table::ProcessTable::new(true));
        for table in [&process_table, &gpu_table] {
            cx.subscribe(table, |this, _, event, cx| {
                match event {
                    crate::table::TableEvent::Selected(p) => this.selected = Some(p.clone()),
                    crate::table::TableEvent::Copied(pid) => {
                        this.notice = format!("Copied PID {pid}")
                    }
                };
                cx.notify();
            })
            .detach();
        }
        let timer = cx.spawn_in(window, async move |weak, cx| {
            loop {
                let delay = weak.update_in(cx, |this, _window, cx| {
                    let next = this.feed.latest.lock().ok().and_then(|mut s| s.take());
                    if !this.paused
                        && let Some(s) = next
                    {
                        this.history.push(s.clone(), this.paused_duration);
                        this.sample = s;
                        this.reindex();
                        cx.notify();
                    }
                    if !this.paused {
                        for chart in [&this.cpu_chart, &this.gpu_chart, &this.network_chart] {
                            chart.update(cx, |chart, cx| {
                                if chart.enabled {
                                    cx.notify();
                                }
                            });
                        }
                    }
                    if this.paused {
                        250
                    } else {
                        1000 / this.settings.layout.fps
                    }
                });
                let Ok(ms) = delay else {
                    break;
                };
                cx.background_executor()
                    .timer(Duration::from_millis(ms))
                    .await;
            }
        });
        Self {
            feed: Feed::start(settings.layout.poll_ms),
            sample: Arc::new(Snapshot::default()),
            history: History::new(),
            cpu_chart,
            gpu_chart,
            network_chart,
            process_table,
            gpu_table,
            effective_columns: 1,
            settings,
            focus,
            menu: false,
            paused: false,
            pause_at: Instant::now(),
            paused_duration: Duration::ZERO,
            entry: Entry::None,
            query: String::new(),
            preset_name: String::new(),
            notice: error.unwrap_or_default(),
            selected: None,
            rows: vec![],
            gpu_rows: vec![],
            gpu_index: 0,
            resize: None,
            fitted: vec![],
            _timer: timer,
        }
    }
    fn save(&mut self) {
        self.feed
            .interval
            .store(self.settings.layout.poll_ms, Ordering::Relaxed);
        if let Err(e) = self.settings.save() {
            self.notice = format!("Save failed: {e}");
        }
    }
    fn reindex(&mut self) {
        let query = self.query.to_lowercase();
        let matches = |p: &Process| {
            query.is_empty()
                || p.name.to_lowercase().contains(&query)
                || p.command.to_lowercase().contains(&query)
                || p.user.to_lowercase().contains(&query)
                || p.pid.to_string().contains(&query)
        };
        self.rows = self
            .sample
            .processes
            .iter()
            .enumerate()
            .filter(|(_, p)| matches(p))
            .map(|(i, _)| i)
            .collect();
        self.gpu_index = self.gpu_index.min(self.sample.gpus.len().saturating_sub(1));
        if let Some(g) = self.sample.gpus.get(self.gpu_index) {
            self.gpu_rows = g
                .processes
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(p))
                .map(|(i, _)| i)
                .collect();
        } else {
            self.gpu_rows.clear();
        }
        if let Some(p) = &mut self.selected {
            let next = if p.kind.is_empty() {
                self.sample.processes.iter().find(|r| r.pid == p.pid)
            } else {
                self.sample
                    .gpus
                    .get(self.gpu_index)
                    .and_then(|g| g.processes.iter().find(|r| r.pid == p.pid))
            };
            if let Some(next) = next {
                *p = next.clone();
            }
        }
    }
    fn key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if key == "escape" {
            self.entry = Entry::None;
            self.selected = None;
            self.menu = false;
            self.query.clear();
            self.reindex();
            cx.notify();
            return;
        }
        if self.entry != Entry::None {
            if key == "enter" {
                if self.entry == Entry::Preset {
                    let name = self.preset_name.trim().to_string();
                    if !name.is_empty() {
                        self.settings
                            .presets
                            .insert(name.clone(), self.settings.layout.clone());
                        self.save();
                        self.notice = format!("Saved preset · {name}");
                    }
                }
                self.entry = Entry::None;
            } else {
                let value = if self.entry == Entry::Search {
                    &mut self.query
                } else {
                    &mut self.preset_name
                };
                if key == "backspace" {
                    value.pop();
                } else if !e.keystroke.modifiers.control
                    && !e.keystroke.modifiers.alt
                    && !e.keystroke.modifiers.platform
                    && let Some(s) = &e.keystroke.key_char
                    && value.len() < 128
                {
                    value.push_str(s);
                }
                self.reindex();
            }
        } else if key == "/" {
            self.entry = Entry::Search;
        } else if key == "space" {
            self.toggle_pause();
        }
        cx.notify();
    }
    fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if self.paused {
            self.pause_at = Instant::now();
        } else {
            self.paused_duration += self.pause_at.elapsed();
        }
    }
    fn layout_menu(&self, cx: &mut Context<Self>) -> Div {
        div()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .bg(rgb(RAISED))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(muted("Presets").w(px(64.)))
                    .child(button("portrait", "Portrait").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.settings.layout = Layout::portrait();
                            this.save();
                            cx.notify();
                        },
                    )))
                    .child(button("landscape", "Landscape").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.settings.layout = Layout::landscape();
                            this.save();
                            cx.notify();
                        },
                    )))
                    .child(button("new-preset", "Save preset…").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.entry = Entry::Preset;
                            this.preset_name.clear();
                            window.focus(&this.focus);
                            cx.notify();
                        },
                    )))
                    .children(self.settings.presets.keys().map(|name| {
                        let load = name.clone();
                        let delete = name.clone();
                        div()
                            .flex()
                            .items_center()
                            .rounded(px(4.))
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(LINE))
                            .child(
                                button(
                                    SharedString::from(format!("load-{name}")),
                                    SharedString::from(name.clone()),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if let Some(l) = this.settings.presets.get(&load) {
                                            this.settings.layout = l.clone();
                                            this.save();
                                            this.notice = format!("Loaded preset · {load}");
                                        }
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                icon_button(
                                    SharedString::from(format!("delete-{name}")),
                                    Icon::Close,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.settings.presets.remove(&delete);
                                        this.save();
                                        cx.notify();
                                    },
                                )),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(muted("Columns").w(px(64.)))
                    .children((1..=3).map(|n| {
                        button(("columns", n), SharedString::from(n.to_string()))
                            .when(self.settings.layout.columns == n, |d| {
                                d.text_color(rgb(ACCENT))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings.layout.columns = n;
                                this.save();
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(muted("Panels").w(px(64.)))
                    .children(Panel::ALL.into_iter().map(|p| {
                        let visible = self
                            .settings
                            .layout
                            .tiles
                            .iter()
                            .any(|t| t.panel == p && t.visible);
                        button(
                            SharedString::from(format!("show-{p:?}")),
                            SharedString::from(p.title()),
                        )
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(icon(if visible { Icon::Check } else { Icon::Plus }))
                        .when(visible, |s| s.text_color(rgb(ACCENT)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(t) =
                                this.settings.layout.tiles.iter_mut().find(|t| t.panel == p)
                            {
                                t.visible = !t.visible;
                            }
                            this.save();
                            cx.notify();
                        }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(muted("Polling").w(px(64.)))
                    .children([500, 1000, 2000].into_iter().map(|ms| {
                        button(
                            ("poll", ms as usize),
                            SharedString::from(format!("{}s", ms as f32 / 1000.)),
                        )
                        .when(self.settings.layout.poll_ms == ms, |s| {
                            s.text_color(rgb(ACCENT))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.settings.layout.poll_ms = ms;
                            this.save();
                            cx.notify();
                        }))
                    }))
                    .child(muted("Charts").ml_3())
                    .children([15, 30, 60].into_iter().map(|fps| {
                        button(
                            ("fps", fps as usize),
                            SharedString::from(format!("{fps} fps")),
                        )
                        .when(self.settings.layout.fps == fps, |s| {
                            s.text_color(rgb(ACCENT))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.settings.layout.fps = fps;
                            this.save();
                            cx.notify();
                        }))
                    }))
                    .child(
                        button("smaller", "A−").on_click(cx.listener(|this, _, _, cx| {
                            this.settings.layout.font_size =
                                (this.settings.layout.font_size - 1.).max(10.);
                            this.save();
                            cx.notify();
                        })),
                    )
                    .child(
                        button("larger", "A+").on_click(cx.listener(|this, _, _, cx| {
                            this.settings.layout.font_size =
                                (this.settings.layout.font_size + 1.).min(16.);
                            this.save();
                            cx.notify();
                        })),
                    ),
            )
    }
    fn cpu(&self) -> Div {
        let s = &self.sample;
        let core_columns = match s.cores.len() {
            0..=6 => 1,
            7..=12 => 2,
            13..=18 => 3,
            _ => 4,
        };
        let rows_per_column = s.cores.len().div_ceil(core_columns);
        div()
            .flex()
            .flex_col()
            .size_full()
            .px_4()
            .pb_3()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().overflow_hidden().text_ellipsis().child(
                        if s.cpu_name.is_empty() {
                            "Reading CPU…".into()
                        } else {
                            s.cpu_name.clone()
                        },
                    ))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(ACCENT))
                            .child(format!("{:.1}%", s.cpu)),
                    )
                    .child(
                        div()
                            .text_color(rgb(s.temperature.map(temperature_color).unwrap_or(MUTED)))
                            .child(
                                s.temperature
                                    .map(|t| format!("{t:.0} °C"))
                                    .unwrap_or("— °C".into()),
                            ),
                    ),
            )
            .child(self.chart(None))
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap_5()
                    .children((0..core_columns).map(|column| {
                        let start = column * rows_per_column;
                        let end = (start + rows_per_column).min(s.cores.len());
                        div().flex().flex_col().flex_1().min_w_0().gap_1().children(
                            (start..end).map(|i| {
                                div()
                                    .flex()
                                    .items_center()
                                    .text_size(px(10.))
                                    .child(
                                        div()
                                            .size(px(10.))
                                            .mr_2()
                                            .bg(rgb(COLORS[i % COLORS.len()])),
                                    )
                                    .child(muted(format!("CPU{}", i + 1)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_right()
                                            .child(format!("{:.1}%", s.cores[i])),
                                    )
                            }),
                        )
                    })),
            )
    }
    fn chart(&self, gpu: Option<String>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(35.))
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .border_1()
                            .border_color(rgb(LINE))
                            .child(if gpu.is_some() {
                                self.gpu_chart.clone()
                            } else {
                                self.cpu_chart.clone()
                            }),
                    )
                    .child(
                        div()
                            .w(px(32.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .justify_between()
                            .text_size(px(9.))
                            .text_color(rgb(MUTED))
                            .child("100%")
                            .child("80%")
                            .child("60%")
                            .child("40%")
                            .child("20%")
                            .child("0%"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .pr(px(40.))
                    .text_size(px(9.))
                    .text_color(rgb(MUTED))
                    .child("1 min")
                    .child("50 sec")
                    .child("40 sec")
                    .child("30 sec")
                    .child("20 sec")
                    .child("10 sec")
                    .child("now"),
            )
    }
    fn memory(&self) -> Div {
        let s = &self.sample;
        let total = s.memory_total;
        let ratio = |v: u64| {
            if total > 0 {
                v as f32 / total as f32
            } else {
                0.
            }
        };
        let segments = [
            ("Used", s.memory_active, 0x2ec27e),
            ("Cache", s.memory_cached, 0x9141ac),
            ("Buffers", s.memory_buffers, 0x3584e4),
            ("Free", s.memory_free, 0xd5d5d5),
        ];
        let swap = if s.swap_total > 0 {
            s.swap_used as f32 / s.swap_total as f32
        } else {
            0.
        };
        div()
            .px_4()
            .pb_2()
            .flex()
            .flex_col()
            .flex_1()
            .pt_2()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(muted("RAM").w(px(36.)))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .h(px(12.))
                            .rounded(px(3.))
                            .overflow_hidden()
                            .bg(rgb(LINE))
                            .children(segments.iter().map(|(_, v, c)| {
                                div().h_full().w(relative(ratio(*v))).bg(rgb(*c))
                            })),
                    )
                    .child(
                        div()
                            .w(px(130.))
                            .text_right()
                            .text_size(px(10.))
                            .child(format!(
                                "{} / {}",
                                metrics::bytes(s.memory_active),
                                metrics::bytes(total)
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x_3()
                    .gap_y_1()
                    .text_size(px(9.))
                    .children(segments.into_iter().map(|(label, v, c)| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().size(px(5.)).bg(rgb(c)).rounded(px(1.)))
                            .child(muted(format!("{label} {}", metrics::bytes(v))))
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(muted("SWAP").w(px(36.)))
                    .child(
                        div()
                            .flex_1()
                            .h(px(12.))
                            .rounded(px(3.))
                            .bg(rgb(LINE))
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(swap.clamp(0., 1.)))
                                    .bg(rgb(0xc061cb)),
                            ),
                    )
                    .child(div().w(px(130.)).text_right().text_size(px(10.)).child(
                        if s.swap_total == 0 {
                            "Not configured".into()
                        } else {
                            format!(
                                "{} / {}",
                                metrics::bytes(s.swap_used),
                                metrics::bytes(s.swap_total)
                            )
                        },
                    )),
            )
    }
    fn gpu(&self, cx: &mut Context<Self>) -> Div {
        let Some(g) = self.sample.gpus.get(self.gpu_index) else {
            return self.no_gpu();
        };
        let gpu_ratio = g.utilization.unwrap_or(0) as f32 / 100.;
        let vram_ratio = if g.memory_total > 0 {
            g.memory_used as f32 / g.memory_total as f32
        } else {
            0.
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .px_4()
            .pb_3()
            .gap_2()
            .child(self.gpu_details(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_5()
                    .child(gpu_meter(
                        "GPU",
                        opt(g.utilization, "%"),
                        gpu_ratio,
                        0x2e7d4f,
                    ))
                    .child(gpu_meter(
                        "VRAM",
                        format!(
                            "{} / {}",
                            metrics::bytes(g.memory_used),
                            metrics::bytes(g.memory_total)
                        ),
                        vram_ratio,
                        0x7b3f94,
                    )),
            )
            .child(self.chart(Some(g.uuid.clone())))
    }
    fn no_gpu(&self) -> Div {
        div().p_3().text_color(rgb(MUTED)).child(
            self.sample
                .gpu_error
                .clone()
                .unwrap_or("Waiting for a supported NVIDIA GPU…".into()),
        )
    }
    fn gpu_details(&self, cx: &mut Context<Self>) -> Div {
        let Some(g) = self.sample.gpus.get(self.gpu_index) else {
            return self.no_gpu();
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .pt_1()
            .text_size(px(10.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x_3()
                    .gap_y_1()
                    .child(inline_metric(
                        "Device",
                        format!("{}  [{}]", self.gpu_index, g.name),
                        0x2e7d4f,
                        TEXT,
                    ))
                    .child(inline_metric(
                        "PCIe",
                        format!("Gen {} ×{}", opt(g.generation, ""), opt(g.width, "")),
                        MUTED,
                        TEXT,
                    ))
                    .child(inline_metric(
                        "RX",
                        g.rx.map(|value| format!("{:.1} MiB/s", value as f32 / 1024.))
                            .unwrap_or("—".into()),
                        MUTED,
                        TEXT,
                    ))
                    .child(inline_metric(
                        "TX",
                        g.tx.map(|value| format!("{:.1} MiB/s", value as f32 / 1024.))
                            .unwrap_or("—".into()),
                        MUTED,
                        TEXT,
                    ))
                    .child(div().flex_1())
                    .when(self.sample.gpus.len() > 1, |row| {
                        row.child(
                            button(
                                "next-gpu",
                                SharedString::from(format!(
                                    "Show GPU {}",
                                    (self.gpu_index + 1) % self.sample.gpus.len()
                                )),
                            )
                            .text_size(px(10.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.gpu_index = (this.gpu_index + 1) % this.sample.gpus.len();
                                this.reindex();
                                cx.notify();
                            })),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x_3()
                    .gap_y_1()
                    .child(inline_metric(
                        "GPU",
                        format!("{} MHz", opt(g.clock, "")),
                        0x2e7d4f,
                        TEXT,
                    ))
                    .child(inline_metric(
                        "MEM",
                        format!("{} MHz", opt(g.memory_clock, "")),
                        0x7b3f94,
                        TEXT,
                    ))
                    .child(inline_metric(
                        "TEMP",
                        opt(g.temperature, " °C"),
                        MUTED,
                        g.temperature
                            .map(|temperature| temperature_color(temperature as f32))
                            .unwrap_or(MUTED),
                    ))
                    .child(inline_metric("FAN", opt(g.fan, "%"), MUTED, TEXT))
                    .child(inline_metric(
                        "POW",
                        format!(
                            "{} / {} W",
                            g.power
                                .map(|power| (power / 1000).to_string())
                                .unwrap_or("—".into()),
                            g.power_limit
                                .map(|power| (power / 1000).to_string())
                                .unwrap_or("—".into())
                        ),
                        MUTED,
                        TEXT,
                    ))
                    .child(inline_metric("ENC", opt(g.encoder, "%"), MUTED, TEXT))
                    .child(inline_metric("DEC", opt(g.decoder, "%"), MUTED, TEXT)),
            )
    }
    fn processes(&self, gpu: bool, _: &mut Context<Self>) -> Div {
        let table = if gpu {
            self.gpu_table.clone()
        } else {
            self.process_table.clone()
        };
        let style = StyleRefinement::default().size_full();
        div()
            .flex_1()
            .min_h_0()
            .child(AnyView::from(table).cached(style))
    }
    fn network(&self) -> Div {
        let n = &self.sample.network;
        let scale = chart::network_scale(&self.history);
        let interface_count = n.interfaces.len();
        div()
            .px_4()
            .pb_3()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .text_size(px(11.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x1c71d8))
                            .child(format!("↓ {}", crate::network::rate(n.rx_rate))),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xc64600))
                            .child(format!("↑ {}", crate::network::rate(n.tx_rate))),
                    )
                    .child(muted(format!(
                        "Total ↓ {}  ↑ {}",
                        metrics::bytes(n.rx),
                        metrics::bytes(n.tx)
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(40.))
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .border_1()
                            .border_color(rgb(LINE))
                            .child(self.network_chart.clone()),
                    )
                    .child(
                        div()
                            .w(px(64.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .justify_between()
                            .text_right()
                            .text_size(px(9.))
                            .text_color(rgb(MUTED))
                            .children(
                                [1., 0.8, 0.6, 0.4, 0.2, 0.]
                                    .into_iter()
                                    .map(|fraction| crate::network::rate(scale * fraction)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .pr(px(72.))
                    .text_size(px(10.))
                    .text_color(rgb(MUTED))
                    .child("1 min · auto scale")
                    .child("30 sec")
                    .child("now"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .pt_2()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .text_size(px(10.))
                    .children(n.interfaces.iter().enumerate().map(|(index, interface)| {
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(180.))
                            .gap_1()
                            .px_2()
                            .when(index + 1 < interface_count, |d| {
                                d.border_r_1().border_color(rgb(LINE))
                            })
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(interface.name.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_3()
                                    .child(div().text_color(rgb(0x1c71d8)).child(format!(
                                        "↓ {}",
                                        crate::network::rate(interface.rx_rate)
                                    )))
                                    .child(div().text_color(rgb(0xc64600)).child(format!(
                                        "↑ {}",
                                        crate::network::rate(interface.tx_rate)
                                    ))),
                            )
                            .child(muted(format!(
                                "Totals  ↓ {}   ↑ {}",
                                metrics::bytes(interface.rx),
                                metrics::bytes(interface.tx)
                            )))
                    })),
            )
    }
    fn panel(&self, p: Panel, height: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let summary = match p {
            Panel::Cpu => {
                if self.sample.cores.is_empty() {
                    String::new()
                } else {
                    format!("{} logical cores", self.sample.cores.len())
                }
            }
            Panel::Processes => format!("{}", self.rows.len()),
            Panel::GpuProcesses => format!("{}", self.gpu_rows.len()),
            Panel::Memory => String::new(),
            Panel::Network => String::new(),
            Panel::Gpu => String::new(),
            Panel::GpuDetails => String::new(),
        };
        let body = match p {
            Panel::Cpu => self.cpu(),
            Panel::Memory => self.memory(),
            Panel::Network => self.network(),
            Panel::Processes => self.processes(false, cx),
            Panel::Gpu => self.gpu(cx),
            Panel::GpuDetails => self.gpu_details(cx),
            Panel::GpuProcesses => self.processes(true, cx),
        };
        div()
            .id(SharedString::from(format!("panel-{p:?}")))
            .flex()
            .flex_col()
            .h(px(height))
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .rounded(px(8.))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .on_drop(cx.listener(move |this, drag: &PanelDrag, _, cx| {
                this.settings.layout.move_before(drag.0, p);
                this.save();
                cx.notify();
            }))
            .drag_over::<PanelDrag>(|s, _, _, _| s.border_color(rgb(ACCENT)))
            .child(
                div()
                    .id(SharedString::from(format!("header-{p:?}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_3()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .when(true, |d| {
                        d.cursor_move()
                            .on_drag(PanelDrag(p), |drag, _, _, cx| cx.new(|_| drag.clone()))
                            .child(icon(Icon::Grip))
                    })
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(12.))
                            .child(p.title()),
                    )
                    .child(muted(summary).text_size(px(10.)).ml_1())
                    .child(div().flex_1())
                    .when(self.settings.layout.columns > 1, |d| {
                        d.child(
                            button(
                                SharedString::from(format!("span-{p:?}")),
                                SharedString::from({
                                    let span = self
                                        .settings
                                        .layout
                                        .tiles
                                        .iter()
                                        .find(|t| t.panel == p)
                                        .map(|t| t.span.min(self.settings.layout.columns))
                                        .unwrap_or(1);
                                    if span == self.settings.layout.columns {
                                        "Full width".into()
                                    } else {
                                        format!("{span} col{}", if span > 1 { "s" } else { "" })
                                    }
                                }),
                            )
                            .text_size(px(10.))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(t) =
                                        this.settings.layout.tiles.iter_mut().find(|t| t.panel == p)
                                    {
                                        t.span = t.span % this.settings.layout.columns + 1;
                                    }
                                    this.save();
                                    cx.notify();
                                },
                            )),
                        )
                    })
                    .when(true, |d| {
                        d.child(
                            icon_button(SharedString::from(format!("hide-{p:?}")), Icon::Close)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(t) =
                                        this.settings.layout.tiles.iter_mut().find(|t| t.panel == p)
                                    {
                                        t.visible = false;
                                    }
                                    this.save();
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(body)
            .when(true, |d| {
                d.child(
                    div()
                        .id(SharedString::from(format!("resize-{p:?}")))
                        .h(px(9.))
                        .flex_shrink_0()
                        .w_full()
                        .cursor_row_resize()
                        .hover(|s| s.bg(rgb(LINE)))
                        .flex()
                        .justify_center()
                        .items_center()
                        .child(div().w(px(22.)).h(px(1.)).bg(rgb(LINE)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                for (panel, h) in &this.fitted {
                                    if let Some(t) = this
                                        .settings
                                        .layout
                                        .tiles
                                        .iter_mut()
                                        .find(|t| t.panel == *panel)
                                    {
                                        t.height = *h;
                                    }
                                }
                                let _ = window;
                                this.resize =
                                    Some(Resize::Height(p, f32::from(e.position.y), height));
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
    }
    fn workspace(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let columns = self.effective_columns;
        div()
            .id("workspace")
            .flex()
            .flex_col()
            .gap(px(10.))
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .children(
                self.settings
                    .layout
                    .rows(columns)
                    .into_iter()
                    .enumerate()
                    .map(|(i, row)| {
                        let empty =
                            columns - row.iter().map(|t| t.span.min(columns)).sum::<usize>();
                        let height = self
                            .fitted
                            .iter()
                            .find(|(p, _)| *p == row[0].panel)
                            .map(|(_, h)| *h)
                            .unwrap_or(200.);
                        div()
                            .id(("grid-row", i))
                            .flex()
                            .gap(px(10.))
                            .h(px(height))
                            .flex_shrink_0()
                            .w_full()
                            .children(row.into_iter().map(|tile| {
                                let weight = tile.span.min(columns) as f32;
                                div()
                                    .flex_basis(px(10. * (weight - 1.)))
                                    .flex_grow()
                                    .min_w_0()
                                    .h_full()
                                    .map(|mut d| {
                                        d.style().flex_grow = Some(weight);
                                        d
                                    })
                                    .child(self.panel(tile.panel, height, cx))
                            }))
                            .when(empty > 0, |d| {
                                d.child(div().flex_basis(px(10. * (empty as f32 - 1.))).map(
                                    |mut d| {
                                        d.style().flex_grow = Some(empty as f32);
                                        d
                                    },
                                ))
                            })
                    }),
            )
            .when(cx.has_active_drag(), |d| {
                d.child(
                    div()
                        .id("drop-end")
                        .h(px(40.))
                        .flex_shrink_0()
                        .border_1()
                        .border_dashed()
                        .border_color(rgb(LINE))
                        .text_color(rgb(MUTED))
                        .child("Drop panel here")
                        .on_drop(cx.listener(|this, drag: &PanelDrag, _, cx| {
                            if let Some(i) = this
                                .settings
                                .layout
                                .tiles
                                .iter()
                                .position(|t| t.panel == drag.0)
                            {
                                let tile = this.settings.layout.tiles.remove(i);
                                this.settings.layout.tiles.push(tile);
                                this.save();
                                cx.notify();
                            }
                        })),
                )
            })
    }
}
impl Render for Monitor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.effective_columns = self.settings.layout.columns.min(
            ((f32::from(window.viewport_size().width) - 24.) / 480.)
                .floor()
                .max(1.) as usize,
        );
        let available = f32::from(window.viewport_size().height)
            - 90.
            - if self.menu { 210. } else { 0. }
            - if self.selected.is_some() { 140. } else { 0. }
            - if self.entry == Entry::Preset { 36. } else { 0. };
        self.fitted = self
            .settings
            .layout
            .fitted_heights(self.effective_columns, available);
        for table in [&self.process_table, &self.gpu_table] {
            table.update(cx, |table, cx| {
                table.set(
                    self.sample.clone(),
                    self.gpu_index,
                    self.query.clone(),
                    self.settings.layout.font_size,
                    self.selected.as_ref().map(|process| process.pid),
                    cx,
                )
            });
        }
        for (entity, panel) in [
            (&self.cpu_chart, Panel::Cpu),
            (&self.gpu_chart, Panel::Gpu),
            (&self.network_chart, Panel::Network),
        ] {
            entity.update(cx, |chart, _| {
                chart.history = self.history.clone();
                chart.network = panel == Panel::Network;
                chart.fps = self.settings.layout.fps;
                chart.poll_ms = self.settings.layout.poll_ms;
                chart.clock_offset = self.paused_duration;
                chart.frozen = if self.paused {
                    Some(self.pause_at)
                } else {
                    None
                };
                chart.enabled = self
                    .settings
                    .layout
                    .tiles
                    .iter()
                    .any(|t| t.panel == panel && t.visible);
                chart.gpu = if panel == Panel::Gpu {
                    self.sample.gpus.get(self.gpu_index).map(|g| g.uuid.clone())
                } else {
                    None
                };
            });
        }
        div()
            .id("monitor")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Noto Sans")
            .text_size(px(self.settings.layout.font_size))
            .on_key_down(cx.listener(Self::key))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _window, cx| {
                if let Some(resize) = this.resize {
                    if e.pressed_button != Some(MouseButton::Left) {
                        this.resize = None;
                        this.save();
                        return;
                    }
                    match resize {
                        Resize::Height(p, y, h) => {
                            let panels: Vec<_> = this
                                .settings
                                .layout
                                .rows(this.effective_columns)
                                .into_iter()
                                .find(|r| r.iter().any(|t| t.panel == p))
                                .map(|r| r.into_iter().map(|t| t.panel).collect())
                                .unwrap_or_default();
                            for panel in panels {
                                if let Some(t) = this
                                    .settings
                                    .layout
                                    .tiles
                                    .iter_mut()
                                    .find(|t| t.panel == panel)
                                {
                                    t.height = (h + f32::from(e.position.y) - y)
                                        .clamp(panel.min_height(), 4000.);
                                }
                            }
                        }
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.resize.take().is_some() {
                        this.save();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.resize.take().is_some() {
                        this.save();
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .h(px(42.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_3()
                    .gap_1()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_size(px(11.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child("Strata"),
                    )
                    .child(
                        div()
                            .id("top-drag")
                            .flex_1()
                            .h_full()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move()
                            }),
                    )
                    .child(
                        button("top-pause", if self.paused { "Resume" } else { "Pause" })
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(icon(if self.paused { Icon::Play } else { Icon::Pause }))
                            .when(self.paused, |d| d.text_color(rgb(0xc64600)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_pause();
                                cx.notify();
                            })),
                    )
                    .child(
                        button(
                            "top-search",
                            SharedString::from(if self.entry == Entry::Search {
                                format!("Filter: {}▏", self.query)
                            } else if self.query.is_empty() {
                                "Filter".into()
                            } else {
                                format!("Filter: {}", self.query)
                            }),
                        )
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(icon(Icon::Search))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.entry = Entry::Search;
                            window.focus(&this.focus);
                            cx.notify();
                        })),
                    )
                    .child(
                        button("top-layouts", "Layout")
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(icon(Icon::Columns))
                            .when(self.menu, |s| s.text_color(rgb(ACCENT)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.menu = !this.menu;
                                cx.notify();
                            })),
                    )
                    .child(
                        icon_button("minimize", Icon::Minimize)
                            .on_click(|_, window, _| window.minimize_window()),
                    )
                    .child(
                        icon_button("maximize", Icon::Maximize)
                            .on_click(|_, window, _| window.zoom_window()),
                    )
                    .child(
                        icon_button("close", Icon::Close)
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(10.))
                    .child(self.workspace(cx)),
            )
            .when_some(self.selected.clone(), |d, p| {
                d.child(
                    div()
                        .px_4()
                        .py_2()
                        .bg(rgb(PANEL))
                        .border_t_1()
                        .border_color(rgb(LINE))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .child(div().flex_1().text_color(rgb(ACCENT)).child(format!(
                                    "{} · PID {} · {} · state {} · {} threads",
                                    p.name, p.pid, p.user, p.state, p.threads
                                )))
                                .child(
                                    button("copy-command", "Copy command")
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(icon(Icon::Copy))
                                        .on_click({
                                            let command = p.command.clone();
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    command.clone(),
                                                ))
                                            }
                                        }),
                                )
                                .child(icon_button("close-detail", Icon::Close).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.selected = None;
                                        cx.notify();
                                    }),
                                )),
                        )
                        .child(
                            div()
                                .id("command-detail")
                                .max_h(px(115.))
                                .overflow_y_scroll()
                                .text_size(px(11.))
                                .child(format!(
                                    "{}CPU {:.1}%  ·  RAM {}{}\n{}",
                                    if p.kind.is_empty() {
                                        String::new()
                                    } else {
                                        format!("{} · ", p.kind)
                                    },
                                    p.cpu,
                                    metrics::bytes(p.memory),
                                    p.gpu_memory
                                        .map(|v| format!("  ·  VRAM {}", metrics::bytes(v)))
                                        .unwrap_or_default(),
                                    p.command
                                )),
                        ),
                )
            })
            .when(self.menu, |d| d.child(self.layout_menu(cx)))
            .when(self.entry == Entry::Preset, |d| {
                d.child(div().px_4().py_2().bg(rgb(RAISED)).child(format!(
                    "Preset name: {}▏   Enter to save · Esc to cancel",
                    self.preset_name
                )))
            })
            .when(!self.notice.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .px_3()
                        .text_size(px(10.))
                        .text_color(rgb(MUTED))
                        .child(self.notice.clone())
                        .child(icon_button("dismiss", Icon::Close).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.notice.clear();
                                cx.notify();
                            },
                        ))),
                )
            })
            .child(
                div()
                    .px_3()
                    .h(px(28.))
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .items_center()
                    .text_size(px(10.))
                    .text_color(rgb(MUTED))
                    .bg(rgb(PANEL))
                    .child(muted(if self.paused {
                        "Measurements paused".into()
                    } else {
                        format!(
                            "Updating every {:.1}s",
                            self.settings.layout.poll_ms as f32 / 1000.
                        )
                    }))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move()
                            }),
                    )
                    .child(
                        div()
                            .id("window-resize")
                            .w(px(14.))
                            .h_full()
                            .cursor_nwse_resize()
                            .child(icon(Icon::Resize))
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_resize(ResizeEdge::BottomRight)
                            }),
                    ),
            )
    }
}
