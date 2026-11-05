use crate::metrics::{Process, Snapshot, bytes};
use crate::theme::{self, colors};
use gpui::{prelude::*, *};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    ops::Add,
    sync::Arc,
};
const CHEVRON: f32 = 10.;
const ICON: f32 = 16.;
const GAP: f32 = 6.;
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
enum Row {
    Group(usize),
    Process { index: usize, child: bool },
}
// Several processes of one application, shown as one expandable row.
struct Group {
    total: Process,
    members: Vec<usize>,
}
pub enum TableEvent {
    Selected(Box<Process>),
    Copied(u32),
}
pub struct ProcessTable {
    pub sample: Arc<Snapshot>,
    pub gpu: bool,
    pub gpu_index: usize,
    pub query: String,
    pub font_size: f32,
    pub selected_pid: Option<u32>,
    dark: bool,
    rows: Vec<Row>,
    groups: Vec<Group>,
    expanded: HashSet<String>,
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
            selected_pid: None,
            dark: theme::is_dark(),
            rows: vec![],
            groups: vec![],
            expanded: HashSet::new(),
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
        selected_pid: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        if !Arc::ptr_eq(&sample, &self.sample)
            || self.gpu_index != gpu_index
            || self.query != query
            || self.font_size != font_size
            || self.selected_pid != selected_pid
            || self.dark != theme::is_dark()
        {
            self.dark = theme::is_dark();
            self.sample = sample;
            self.gpu_index = gpu_index;
            self.query = query;
            self.font_size = font_size;
            self.selected_pid = selected_pid;
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
        let mut slots: HashMap<&str, usize> = HashMap::new();
        let mut buckets: Vec<Vec<usize>> = vec![];
        for (i, p) in data.iter().enumerate() {
            if !q.is_empty()
                && !p.command.to_lowercase().contains(&q)
                && !p.name.to_lowercase().contains(&q)
                && !p.user.to_lowercase().contains(&q)
                && !p.pid.to_string().contains(&q)
                && !p
                    .app
                    .as_ref()
                    .is_some_and(|a| a.name.to_lowercase().contains(&q))
            {
                continue;
            }
            if p.group.is_empty() {
                buckets.push(vec![i]);
            } else {
                let slot = *slots.entry(&p.group).or_insert_with(|| {
                    buckets.push(vec![]);
                    buckets.len() - 1
                });
                buckets[slot].push(i);
            }
        }
        let mut groups = vec![];
        let mut top: Vec<Row> = buckets
            .into_iter()
            .map(|members| match members[..] {
                [index] => Row::Process {
                    index,
                    child: false,
                },
                _ => {
                    groups.push(Group {
                        total: total(data, &members),
                        members,
                    });
                    Row::Group(groups.len() - 1)
                }
            })
            .collect();
        let item = |row: &Row| match row {
            Row::Group(g) => &groups[*g].total,
            Row::Process { index, .. } => &data[*index],
        };
        top.sort_unstable_by(|a, b| self.order(item(a), item(b)));
        let mut rows = vec![];
        for row in top {
            if let Row::Group(g) = row {
                let group = &mut groups[g];
                // A filter opens every group so matches stay visible; clicking still toggles.
                if self.expanded.contains(&group.total.group) != q.is_empty() {
                    rows.push(row);
                    continue;
                }
                group
                    .members
                    .sort_unstable_by(|a, b| self.order(&data[*a], &data[*b]));
                rows.push(row);
                rows.extend(
                    group
                        .members
                        .iter()
                        .map(|&index| Row::Process { index, child: true }),
                );
            } else {
                rows.push(row);
            }
        }
        self.rows = rows;
        self.groups = groups;
    }
    fn order(&self, a: &Process, b: &Process) -> Ordering {
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
    }
    fn cells(&self, p: &Process) -> Vec<(String, f32, u32)> {
        if self.gpu {
            vec![
                (
                    p.gpu.map(|v| format!("{v}%")).unwrap_or("—".into()),
                    48.,
                    colors().accent,
                ),
                (
                    p.gpu_memory.map(bytes).unwrap_or("—".into()),
                    76.,
                    colors().vram,
                ),
                (format!("{:.1}%", p.cpu), 58., colors().muted),
            ]
        } else {
            vec![
                (p.user.clone(), 65., colors().muted),
                (format!("{:.1}%", p.cpu), 58., colors().accent),
                (bytes(p.memory), 76., colors().text),
                (
                    p.tcp.map(|v| v.to_string()).unwrap_or("—".into()),
                    35.,
                    colors().receive,
                ),
                (
                    p.udp.map(|v| v.to_string()).unwrap_or("—".into()),
                    35.,
                    colors().receive,
                ),
            ]
        }
    }
}
fn sum<T: Add<Output = T>>(values: impl Iterator<Item = Option<T>>) -> Option<T> {
    values.flatten().reduce(|a, b| a + b)
}
// Aggregate row for a group, named after its desktop app or its oldest process.
fn total(data: &[Process], members: &[usize]) -> Process {
    let all = || members.iter().map(|&i| &data[i]);
    let root = all().min_by_key(|p| (p.start_time, p.pid)).unwrap();
    let name = root
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or(root.name.clone());
    Process {
        pid: root.pid,
        command: name.clone(),
        name,
        user: if all().all(|p| p.user == root.user) {
            root.user.clone()
        } else {
            "—".into()
        },
        cpu: all().map(|p| p.cpu).sum(),
        memory: all().map(|p| p.memory).sum(),
        threads: all().map(|p| p.threads).sum(),
        gpu_memory: sum(all().map(|p| p.gpu_memory)),
        gpu: sum(all().map(|p| p.gpu)),
        tcp: sum(all().map(|p| p.tcp)),
        udp: sum(all().map(|p| p.udp)),
        group: root.group.clone(),
        app: root.app.clone(),
        ..Default::default()
    }
}
// Command cell: disclosure and icon slots keep names aligned; children indent past both.
fn command(p: &Process, text: String, lead: Option<Option<bool>>, count: usize) -> Div {
    let cell = div()
        .flex_1()
        .min_w_0()
        .pr_2()
        .flex()
        .items_center()
        .gap(px(GAP));
    let cell = match lead {
        None => cell.child(div().w(px(CHEVRON + GAP + ICON)).flex_shrink_0()),
        Some(open) => cell
            .child(
                div()
                    .w(px(CHEVRON))
                    .flex_shrink_0()
                    .text_size(px(9.))
                    .text_color(rgb(colors().muted))
                    .child(match open {
                        Some(true) => "▼",
                        Some(false) => "▶",
                        None => "",
                    }),
            )
            .child(
                div()
                    .size(px(ICON))
                    .flex_shrink_0()
                    .when_some(p.app.as_ref().and_then(|a| a.icon.clone()), |d, icon| {
                        d.child(img(icon).size_full())
                    }),
            ),
    };
    cell.child(
        div()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .text_color(rgb(colors().text))
            .when(count > 0, |d| d.font_weight(FontWeight::MEDIUM))
            .child(text),
    )
    .when(count > 0, |d| {
        d.child(
            div()
                .flex_shrink_0()
                .text_color(rgb(colors().muted))
                .child(count.to_string()),
        )
    })
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
                    .bg(rgb(colors().raised))
                    .border_b_1()
                    .border_color(rgb(colors().line))
                    .text_size(px(10.))
                    .children(headers.into_iter().map(|(label, sort, width)| {
                        div()
                            .id(SharedString::from(format!("sort-{label}")))
                            .cursor_pointer()
                            .text_color(rgb(if self.sort == sort {
                                colors().accent
                            } else {
                                colors().muted
                            }))
                            .hover(|d| d.text_color(rgb(colors().text)))
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
                d.child(div().p_3().text_color(rgb(colors().muted)).child(
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
                                let row = this.rows.get(i)?;
                                let (p, group) = match row {
                                    Row::Group(g) => (&this.groups[*g].total, this.groups.get(*g)),
                                    Row::Process { index, .. } => (this.data().get(*index)?, None),
                                };
                                let pid = p.pid;
                                let open = group.map(|_| {
                                    matches!(
                                        this.rows.get(i + 1),
                                        Some(Row::Process { child: true, .. })
                                    )
                                });
                                let lead = match row {
                                    Row::Process { child: true, .. } => None,
                                    _ => Some(open),
                                };
                                // A collapsed group shows the selection of a hidden member.
                                let selected = match group {
                                    Some(g) => {
                                        open == Some(false)
                                            && g.members.iter().any(|&m| {
                                                Some(this.data()[m].pid) == this.selected_pid
                                            })
                                    }
                                    None => this.selected_pid == Some(pid),
                                };
                                let text = if p.command.is_empty() {
                                    format!("[{}]", p.name)
                                } else {
                                    p.command.clone()
                                };
                                let id: ElementId = match group {
                                    Some(_) => {
                                        SharedString::from(format!("group-{}", p.group)).into()
                                    }
                                    None => (pid as usize).into(),
                                };
                                let pid_cell = div().w(px(62.)).flex_shrink_0();
                                let row = div()
                                    .id(id)
                                    .w_full()
                                    .h(px(this.font_size + 13.))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .text_size(px(this.font_size))
                                    .bg(rgb(colors().panel))
                                    .border_b_1()
                                    .border_color(rgb(colors().row_line))
                                    .when(i % 2 == 1, |d| d.bg(rgb(colors().row_alternate)))
                                    .when(selected, |d| d.bg(rgb(colors().selected)))
                                    .hover(|d| d.bg(rgb(colors().row_hover)))
                                    .cursor_pointer()
                                    .child(if group.is_some() {
                                        pid_cell.id(("pid", i))
                                    } else {
                                        pid_cell
                                            .id(("pid", pid as usize))
                                            .text_color(rgb(colors().muted))
                                            .hover(|d| d.text_color(rgb(colors().accent)))
                                            .child(pid.to_string())
                                            .on_click(cx.listener(move |_, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    pid.to_string(),
                                                ));
                                                cx.emit(TableEvent::Copied(pid));
                                                cx.stop_propagation();
                                            }))
                                    })
                                    .child(command(
                                        p,
                                        text,
                                        lead,
                                        group.map_or(0, |g| g.members.len()),
                                    ))
                                    .children(this.cells(p).into_iter().map(|(v, w, c)| {
                                        div()
                                            .text_color(rgb(c))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .w(px(w))
                                            .flex_shrink_0()
                                            .when(w != 65., |d| d.text_right().pr_2())
                                            .child(v)
                                    }));
                                Some(if group.is_some() {
                                    let key = p.group.clone();
                                    row.on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.expanded.remove(&key) {
                                            this.expanded.insert(key.clone());
                                        }
                                        this.reindex();
                                        cx.notify();
                                    }))
                                } else {
                                    let selected = p.clone();
                                    row.on_click(cx.listener(move |_, _, _, cx| {
                                        cx.emit(TableEvent::Selected(Box::new(selected.clone())));
                                    }))
                                })
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
    }
}
