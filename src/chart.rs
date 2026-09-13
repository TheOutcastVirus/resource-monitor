use crate::metrics::Snapshot;
use gpui::{prelude::*, *};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
pub const COLORS: [u32; 24] = [
    0x55d6be, 0xff667f, 0x7da5ff, 0xffd166, 0xc084fc, 0x9ce167, 0xff9f54, 0x4dd9f1, 0xf781d1,
    0x9ba6b5, 0xe8ed6d, 0x6488e8, 0x2fa78f, 0xd34562, 0xabc4ff, 0xd79c27, 0x975bd9, 0x5ba83c,
    0xf6bd8b, 0x219dbc, 0xc146a0, 0xd2d9e4, 0xadb439, 0x495bc0,
];
pub struct GraphSample {
    pub at: Instant,
    pub cores: Vec<f32>,
    pub rx: f32,
    pub tx: f32,
    pub gpus: Vec<GraphGpu>,
}
pub struct GraphGpu {
    pub uuid: String,
    pub utilization: Option<u32>,
    pub memory_used: u64,
    pub memory_total: u64,
}
#[derive(Clone)]
pub struct History {
    pub samples: VecDeque<Arc<GraphSample>>,
}
impl History {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(245),
        }
    }
    pub fn push(&mut self, s: Arc<Snapshot>, offset: Duration) {
        let at = s.at.checked_sub(offset).unwrap_or(s.at);
        if self.samples.back().is_some_and(|s| s.at >= at) {
            return;
        }
        let cutoff = at.checked_sub(Duration::from_secs(62)).unwrap_or(at);
        while self.samples.front().is_some_and(|s| s.at < cutoff) {
            self.samples.pop_front();
        }
        self.samples.push_back(Arc::new(GraphSample {
            at,
            cores: s.cores.clone(),
            rx: s.network.rx_rate,
            tx: s.network.tx_rate,
            gpus: s
                .gpus
                .iter()
                .map(|g| GraphGpu {
                    uuid: g.uuid.clone(),
                    utilization: g.utilization,
                    memory_used: g.memory_used,
                    memory_total: g.memory_total,
                })
                .collect(),
        }));
    }
}
/// Timestamped, linearly interpolated paths; CPU layers are weighted by core count,
/// so a completely busy machine ends at 100%, rather than N × 100%.
pub fn graph(
    history: History,
    gpu_uuid: Option<String>,
    network: bool,
    now: Instant,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if !bounds.intersects(&window.content_mask().bounds) {
                return;
            }
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            if w < 1. || h < 1. {
                return;
            }
            let origin = bounds.origin;
            let pt = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
            for i in 0..=4 {
                let y = h * i as f32 / 4.;
                let mut p = PathBuilder::stroke(px(1.));
                p.move_to(pt(0., y));
                p.line_to(pt(w, y));
                if let Ok(p) = p.build() {
                    window.paint_path(p, rgb(0x2a3037));
                }
            }
            for i in 0..=6 {
                let x = w * i as f32 / 6.;
                let mut p = PathBuilder::stroke(px(1.));
                p.move_to(pt(x, 0.));
                p.line_to(pt(x, h));
                if let Ok(p) = p.build() {
                    window.paint_path(p, rgba(0x2a303780));
                }
            }
            // A one-poll visual delay permits interpolation using only real samples.
            let visible: Vec<_> = history
                .samples
                .iter()
                .map(|s| {
                    let age = if s.at <= now {
                        now.duration_since(s.at).as_secs_f32()
                    } else {
                        -(s.at.duration_since(now).as_secs_f32())
                    };
                    (w * (1. - age / 60.), s)
                })
                .collect();
            if visible.len() < 2 {
                return;
            }
            if network {
                let max = visible
                    .iter()
                    .map(|(_, s)| s.rx.max(s.tx))
                    .fold(1024., f32::max)
                    * 1.1;
                for series in 0..2 {
                    let mut p = PathBuilder::stroke(px(1.6));
                    for (i, (x, s)) in visible.iter().enumerate() {
                        let v = if series == 0 { s.rx } else { s.tx };
                        let pos = pt(*x, h * (1. - v / max));
                        if i == 0 {
                            p.move_to(pos);
                        } else {
                            p.line_to(pos);
                        }
                    }
                    if let Ok(path) = p.build() {
                        window.paint_path(path, rgb(if series == 0 { 0x7da5ff } else { 0xe8bc70 }));
                    }
                }
            } else if let Some(uuid) = &gpu_uuid {
                for series in 0..2 {
                    let mut p = PathBuilder::stroke(px(1.6));
                    let mut started = false;
                    for (x, s) in &visible {
                        let Some(g) = s.gpus.iter().find(|g| &g.uuid == uuid) else {
                            started = false;
                            continue;
                        };
                        let v = if series == 0 {
                            g.utilization.map(|u| u as f32)
                        } else if g.memory_total > 0 {
                            Some(g.memory_used as f32 / g.memory_total as f32 * 100.)
                        } else {
                            None
                        };
                        let Some(v) = v else {
                            started = false;
                            continue;
                        };
                        let point = pt(*x, h * (1. - v.clamp(0., 100.) / 100.));
                        if started {
                            p.line_to(point);
                        } else {
                            p.move_to(point);
                            started = true;
                        }
                    }
                    if let Ok(path) = p.build() {
                        window.paint_path(path, rgb(if series == 0 { 0x7bbfa9 } else { 0xc4a478 }));
                    }
                }
            } else {
                let n = history.samples.back().map(|s| s.cores.len()).unwrap_or(0);
                if n == 0 {
                    return;
                }
                let mut base = vec![0.; visible.len()];
                for core in 0..n {
                    // Adjacent samples form convex trapezoids. Emit triangles directly;
                    // avoid running a general polygon tessellator on every animation frame.
                    let mut path = Path::new(pt(0., 0.));
                    for i in 1..visible.len() {
                        let (x0, s0) = visible[i - 1];
                        let (x1, s1) = visible[i];
                        let top0 =
                            base[i - 1] + s0.cores.get(core).copied().unwrap_or(0.) / n as f32;
                        let top1 = base[i] + s1.cores.get(core).copied().unwrap_or(0.) / n as f32;
                        path.move_to(pt(x0, h * (1. - top0 / 100.)));
                        path.line_to(pt(x1, h * (1. - top1 / 100.)));
                        path.line_to(pt(x1, h * (1. - base[i] / 100.)));
                        path.line_to(pt(x0, h * (1. - base[i - 1] / 100.)));
                    }
                    window.paint_path(path, rgb(COLORS[core % COLORS.len()]));
                    for (i, (_, s)) in visible.iter().enumerate() {
                        base[i] += s.cores.get(core).copied().unwrap_or(0.) / n as f32;
                    }
                }
            }
        },
    )
    .size_full()
}

/// Isolates animation invalidation from process tables and the rest of the dashboard.
pub struct AnimatedChart {
    pub history: History,
    pub gpu: Option<String>,
    pub network: bool,
    pub frozen: Option<Instant>,
    pub clock_offset: Duration,
    pub enabled: bool,
    pub poll_ms: u64,
    pub fps: u64,
}
impl AnimatedChart {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            history: History::new(),
            gpu: None,
            network: false,
            frozen: None,
            clock_offset: Duration::ZERO,
            enabled: true,
            poll_ms: 1000,
            fps: 30,
        }
    }
}
impl Render for AnimatedChart {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let now = self.frozen.unwrap_or_else(Instant::now);
        let now = now.checked_sub(self.clock_offset).unwrap_or(now);
        graph(
            self.history.clone(),
            self.gpu.clone(),
            self.network,
            now.checked_sub(Duration::from_millis(self.poll_ms))
                .unwrap_or(now),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Arc, Duration, History, Instant, Snapshot};
    #[test]
    fn pause_preserves_history_and_excludes_paused_wall_time() {
        let mut h = History::new();
        let start = Instant::now();
        h.push(
            Arc::new(Snapshot {
                at: start,
                ..Default::default()
            }),
            Duration::ZERO,
        );
        h.push(
            Arc::new(Snapshot {
                at: start + Duration::from_secs(121),
                ..Default::default()
            }),
            Duration::from_secs(120),
        );
        assert_eq!(h.samples.len(), 2);
        assert_eq!(
            h.samples[1].at.duration_since(h.samples[0].at),
            Duration::from_secs(1)
        );
    }
    #[test]
    fn history_has_a_bounded_time_window() {
        let mut h = History::new();
        let start = Instant::now();
        for i in 0..1000 {
            h.push(
                Arc::new(Snapshot {
                    at: start + Duration::from_secs(i),
                    ..Default::default()
                }),
                Duration::ZERO,
            );
        }
        assert_eq!(h.samples.len(), 63);
    }
}
