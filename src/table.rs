use crate::metrics::{Process, Snapshot, bytes};
use crate::theme::{ACCENT, LINE, MUTED, RAISED, TEXT};
use gpui::{prelude::*, *};
use std::sync::Arc;
#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Pid,
    Command,
    User,
    Cpu,
    Memory,
    Gpu,
    Tcp,
    Udp,
}
pub enum TableEvent {
    Selected(Process),
    Copied(u32),
}
pub struct ProcessTable {
    pub sample: Arc<Snapshot>,
    pub gpu: bool,
    pub gpu_index: usize,
    pub query: String,
    pub font_size: f32,
    rows: Vec<usize>,
    sort: Sort,
    descending: bool,
}
impl EventEmitter<TableEvent> for ProcessTable {}
impl ProcessTable {
    pub fn new(gpu: bool) -> Self {
        Self {
            sample: Arc::new(Snapshot::default()),
            gpu,
            gpu_index: 0,
            query: String::new(),
            font_size: 12.,
            rows: vec![],
            sort: if gpu { Sort::Memory } else { Sort::Cpu },
            descending: true,
        }
    }
    pub fn set(
        &mut self,
        sample: Arc<Snapshot>,
        gpu_index: usize,
        query: String,
        font_size: f32,
        cx: &mut Context<Self>,
    ) {
        if !Arc::ptr_eq(&sample, &self.sample)
            || self.gpu_index != gpu_index
            || self.query != query
            || self.font_size != font_size
        {
            self.sample = sample;
            self.gpu_index = gpu_index;
            self.query = query;
            self.font_size = font_size;
            self.reindex();
            cx.notify();
        }
    }
    fn data(&self) -> &[Process] {
        if self.gpu {
            self.sample
                .gpus
                .get(self.gpu_index)
                .map(|g| g.processes.as_slice())
                .unwrap_or_default()
        } else {
            &self.sample.processes
        }
    }
    fn reindex(&mut self) {
        let q = self.query.to_lowercase();
        let data = self.data();
        let mut rows: Vec<_> = data
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                q.is_empty()
                    || p.command.to_lowercase().contains(&q)
                    || p.name.to_lowercase().contains(&q)
                    || p.user.to_lowercase().contains(&q)
                    || p.pid.to_string().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        rows.sort_unstable_by(|a, b| {
            let (a, b) = (&data[*a], &data[*b]);
            let order = match self.sort {
                Sort::Pid => a.pid.cmp(&b.pid),
                Sort::Command => a.command.cmp(&b.command),
                Sort::User => a.user.cmp(&b.user),
                Sort::Cpu => a.cpu.total_cmp(&b.cpu),
                Sort::Memory => {
                    if self.gpu {
                        a.gpu_memory.cmp(&b.gpu_memory)
                    } else {
                        a.memory.cmp(&b.memory)
                    }
                }
                Sort::Gpu => a.gpu.cmp(&b.gpu),
                Sort::Tcp => a.tcp.cmp(&b.tcp),
                Sort::Udp => a.udp.cmp(&b.udp),
            };
            let order = if self.descending {
                order.reverse()
            } else {
                order
            };
            order.then_with(|| a.pid.cmp(&b.pid))
        });
        self.rows = rows;
    }
}
impl Render for ProcessTable {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let headers = if self.gpu {
            vec![
                ("PID", Sort::Pid, 62.),
                ("Command", Sort::Command, 0.),
                ("GPU", Sort::Gpu, 48.),
                ("VRAM", Sort::Memory, 76.),
                ("CPU", Sort::Cpu, 58.),
            ]
        } else {
            vec![
                ("PID", Sort::Pid, 62.),
                ("Command", Sort::Command, 0.),
                ("User", Sort::User, 65.),
                ("CPU", Sort::Cpu, 58.),
                ("RAM", Sort::Memory, 76.),
                ("TCP", Sort::Tcp, 35.),
                ("UDP", Sort::Udp, 35.),
            ]
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .w_full()
                    .h(px(26.))
                    .flex_shrink_0()
                    .items_center()
                    .px_3()
                    .bg(rgb(RAISED))
                    .text_size(px(10.))
                    .children(headers.into_iter().map(|(label, sort, width)| {
                        div()
                            .id(SharedString::from(format!("sort-{label}")))
                            .cursor_pointer()
                            .text_color(rgb(if self.sort == sort { ACCENT } else { MUTED }))
                            .hover(|d| d.text_color(rgb(TEXT)))
                            .when(width > 0., |d| {
                                d.w(px(width))
                                    .flex_shrink_0()
                                    .when(!matches!(sort, Sort::Pid | Sort::User), |d| {
                                        d.text_right().pr_2()
                                    })
                            })
                            .when(width == 0., |d| d.flex_1().min_w_0())
                            .child(format!(
                                "{label}{}",
                                if self.sort == sort {
                                    if self.descending { " ↓" } else { " ↑" }
                                } else {
                                    ""
                                }
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.sort == sort {
                                    this.descending = !this.descending;
                                } else {
                                    this.sort = sort;
                                    this.descending = true;
                                }
                                this.reindex();
                                cx.notify();
                            }))
                    })),
            )
            .when(self.rows.is_empty(), |d| {
                d.child(div().p_3().text_color(rgb(MUTED)).child(
                    if self.sample.cpu_name.is_empty() {
                        "Reading processes…".into()
                    } else if !self.query.is_empty() {
                        format!("No processes match “{}”.", self.query)
                    } else if self.gpu {
                        "No GPU processes reported.".into()
                    } else {
                        "No readable processes.".into()
                    },
                ))
            })
            .child(
                uniform_list(
                    "rows",
                    self.rows.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|i| {
                                let p = this.data().get(*this.rows.get(i)?)?;
                                let selected = p.clone();
                                let pid = p.pid;
                                let command = if p.command.is_empty() {
                                    format!("[{}]", p.name)
                                } else {
                                    p.command.clone()
                                };
                                let mut cells = vec![(command, 0., TEXT)];
                                if this.gpu {
                                    cells.extend([
                                        (
                                            p.gpu.map(|v| format!("{v}%")).unwrap_or("—".into()),
                                            48.,
                                            ACCENT,
                                        ),
                                        (
                                            p.gpu_memory.map(bytes).unwrap_or("—".into()),
                                            76.,
                                            0xc4a478,
                                        ),
                                        (format!("{:.1}%", p.cpu), 58., MUTED),
                                    ]);
                                } else {
                                    cells.extend([
                                        (p.user.clone(), 65., MUTED),
                                        (format!("{:.1}%", p.cpu), 58., ACCENT),
                                        (bytes(p.memory), 76., TEXT),
                                        (
                                            p.tcp.map(|v| v.to_string()).unwrap_or("—".into()),
                                            35.,
                                            0x7da5ff,
                                        ),
                                        (
                                            p.udp.map(|v| v.to_string()).unwrap_or("—".into()),
                                            35.,
                                            0x7da5ff,
                                        ),
                                    ]);
                                }
                                Some(
                                    div()
                                        .id(pid as usize)
                                        .w_full()
                                        .h(px(this.font_size + 13.))
                                        .px_3()
                                        .flex()
                                        .items_center()
                                        .text_size(px(this.font_size))
                                        .when(i % 2 == 1, |d| d.bg(rgba(0xffffff03)))
                                        .hover(|d| d.bg(rgb(LINE)))
                                        .cursor_pointer()
                                        .child(
                                            div()
                                                .id(("pid", pid as usize))
                                                .w(px(62.))
                                                .flex_shrink_0()
                                                .text_color(rgb(MUTED))
                                                .hover(|d| d.text_color(rgb(ACCENT)))
                                                .child(pid.to_string())
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(pid.to_string()),
                                                    );
                                                    cx.emit(TableEvent::Copied(pid));
                                                    cx.stop_propagation();
                                                })),
                                        )
                                        .children(cells.into_iter().map(|(v, w, c)| {
                                            div()
                                                .text_color(rgb(c))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .when(w > 0., |d| {
                                                    d.w(px(w))
                                                        .flex_shrink_0()
                                                        .when(w != 65., |d| d.text_right().pr_2())
                                                })
                                                .when(w == 0., |d| d.flex_1().min_w_0().pr_2())
                                                .child(v)
                                        }))
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.emit(TableEvent::Selected(selected.clone()));
                                        })),
                                )
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
    }
}
