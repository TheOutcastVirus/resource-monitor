use crate::metrics::Snapshot;
use crate::theme::colors;
use gpui::{prelude::*, *};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

const CURVE_STEPS: usize = 4;
const HISTORY_WINDOW_MS: u64 = 62_000;
const MIN_POLL_MS: u64 = 250;
const MAX_HISTORY_SAMPLES: usize = (HISTORY_WINDOW_MS / MIN_POLL_MS) as usize + 1;

fn catmull(values: &[f32], segment: usize, t: f32) -> f32 {
    let p0 = values[segment.saturating_sub(1)];
    let p1 = values[segment];
    let p2 = values[segment + 1];
    let p3 = values[(segment + 2).min(values.len() - 1)];
    let t2 = t * t;
    let t3 = t2 * t;
    let value = 0.5
        * (2. * p1
            + (-p0 + p2) * t
            + (2. * p0 - 5. * p1 + 4. * p2 - p3) * t2
            + (-p0 + 3. * p1 - 3. * p2 + p3) * t3);
    value.clamp(p1.min(p2), p1.max(p2))
}

fn push_solid_triangle(
    path: &mut Path<Pixels>,
    a: Point<Pixels>,
    b: Point<Pixels>,
    c: Point<Pixels>,
) {
    path.push_triangle((a, b, c), (point(0., 1.), point(0., 1.), point(0., 1.)));
}

fn push_solid_quad(
    path: &mut Path<Pixels>,
    a: Point<Pixels>,
    b: Point<Pixels>,
    c: Point<Pixels>,
    d: Point<Pixels>,
) {
    push_solid_triangle(path, a, b, c);
    push_solid_triangle(path, a, c, d);
}

fn push_degenerate_triangles(path: &mut Path<Pixels>, at: Point<Pixels>, count: usize) {
    for _ in 0..count {
        push_solid_triangle(path, at, at, at);
    }
}

fn smooth_stroke(points: &[(f32, f32)], origin: Point<Pixels>, width: f32) -> Option<Path<Pixels>> {
    let first = *points.first()?;
    let absolute = |(x, y): (f32, f32)| point(origin.x + px(x), origin.y + px(y));
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(absolute(first));
    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(points.len() - 1)];
        let y_min = p1.1.min(p2.1);
        let y_max = p1.1.max(p2.1);
        let control_a = (
            p1.0 + (p2.0 - p0.0) / 6.,
            (p1.1 + (p2.1 - p0.1) / 6.).clamp(y_min, y_max),
        );
        let control_b = (
            p2.0 - (p3.0 - p1.0) / 6.,
            (p2.1 - (p3.1 - p1.1) / 6.).clamp(y_min, y_max),
        );
        path.cubic_bezier_to(absolute(p2), absolute(control_a), absolute(control_b));
    }
    path.build().ok()
}
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
            samples: VecDeque::with_capacity(MAX_HISTORY_SAMPLES),
        }
    }
    pub fn push(&mut self, s: Arc<Snapshot>, offset: Duration) {
        let at = s.at.checked_sub(offset).unwrap_or(s.at);
        if self.samples.back().is_some_and(|s| s.at >= at) {
            return;
        }
        let cutoff = at
            .checked_sub(Duration::from_millis(HISTORY_WINDOW_MS))
            .unwrap_or(at);
        while self.samples.front().is_some_and(|s| s.at < cutoff) {
            self.samples.pop_front();
        }
        while self.samples.len() >= MAX_HISTORY_SAMPLES {
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

/// Rounds the retained peak up to a 1–1.5–2–2.5–3–4–5–6–8 step in the displayed
/// KiB/MiB unit. A saturated link then lands just above its ceiling instead of
/// doubling to the next power of two, and every fifth of the scale stays readable.
pub fn network_scale(history: &History) -> f32 {
    const STEPS: [f32; 10] = [1., 1.5, 2., 2.5, 3., 4., 5., 6., 8., 10.];
    let peak = history
        .samples
        .iter()
        .map(|sample| sample.rx.max(sample.tx))
        .fold(0., f32::max)
        * 1.05;
    if peak <= 2048. {
        return 2048.;
    }
    let unit = if peak >= 1024. * 1024. {
        1024. * 1024.
    } else {
        1024.
    };
    let value = peak / unit;
    let decade = 10_f32.powf(value.log10().floor());
    let step = STEPS
        .into_iter()
        .find(|step| step * decade >= value)
        .unwrap_or(10.);
    step * decade * unit
}

/// Timestamped, smoothly interpolated paths; CPU layers are weighted by core count,
/// so a completely busy machine ends at 100%, rather than N × 100%.
pub fn graph(
    history: History,
    gpu_uuid: Option<String>,
    network: bool,
    now: Instant,
    poll_ms: u64,
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
            let c = colors();
            let max_intervals = (HISTORY_WINDOW_MS / poll_ms.max(MIN_POLL_MS)) as usize;
            let pt = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
            for i in 0..=5 {
                let y = h * i as f32 / 5.;
                let mut p = PathBuilder::stroke(px(1.));
                p.move_to(pt(0., y));
                p.line_to(pt(w, y));
                if let Ok(p) = p.build() {
                    window.paint_path(p, rgb(c.chart_grid));
                }
            }
            for i in 0..=6 {
                let x = w * i as f32 / 6.;
                let mut p = PathBuilder::stroke(px(1.));
                p.move_to(pt(x, 0.));
                p.line_to(pt(x, h));
                if let Ok(p) = p.build() {
                    window.paint_path(p, rgba(c.chart_grid_minor));
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
            let max_intervals = max_intervals.max(visible.len() - 1);
            if network {
                let max = network_scale(&history);
                for series in 0..2 {
                    let points: Vec<_> = visible
                        .iter()
                        .map(|(x, s)| {
                            let v = if series == 0 { s.rx } else { s.tx };
                            (*x, h * (1. - v / max))
                        })
                        .collect();
                    if let Some(path) = smooth_stroke(&points, origin, 1.8) {
                        window.paint_path(
                            path,
                            rgb(if series == 0 {
                                c.receive_graph
                            } else {
                                c.transmit_graph
                            }),
                        );
                    }
                }
            } else if let Some(uuid) = &gpu_uuid {
                for series in 0..2 {
                    let color = rgb(if series == 0 {
                        c.gpu_graph
                    } else {
                        c.vram_graph
                    });
                    // Missing samples split the line; never bridge a gap with a curve.
                    let mut points = vec![];
                    for (x, s) in &visible {
                        let v = s.gpus.iter().find(|g| &g.uuid == uuid).and_then(|g| {
                            if series == 0 {
                                g.utilization.map(|u| u as f32)
                            } else if g.memory_total > 0 {
                                Some(g.memory_used as f32 / g.memory_total as f32 * 100.)
                            } else {
                                None
                            }
                        });
                        let Some(v) = v else {
                            if let Some(path) = smooth_stroke(&points, origin, 1.8) {
                                window.paint_path(path, color);
                            }
                            points.clear();
                            continue;
                        };
                        points.push((*x, h * (1. - v.clamp(0., 100.) / 100.)));
                    }
                    if let Some(path) = smooth_stroke(&points, origin, 1.8) {
                        window.paint_path(path, color);
                    }
                }
            } else {
                let n = history.samples.back().map(|s| s.cores.len()).unwrap_or(0);
                if n == 0 {
                    return;
                }
                let mut base = vec![0.; visible.len()];
                let actual_intervals = visible.len() - 1;
                let triangles_per_layer = actual_intervals * CURVE_STEPS * 2;
                let mut painted_triangles = 0;
                for core in 0..n {
                    let top: Vec<_> = visible
                        .iter()
                        .enumerate()
                        .map(|(i, (_, s))| {
                            base[i] + s.cores.get(core).copied().unwrap_or(0.) / n as f32
                        })
                        .collect();
                    // Smooth each interval with bounded Catmull-Rom interpolation, then emit
                    // triangles directly. This keeps the chart fluid without per-frame polygon
                    // tessellation or values that overshoot the sampled data.
                    let mut path = Path::new(pt(0., 0.));
                    let mut min_y = f32::INFINITY;
                    let mut max_y = f32::NEG_INFINITY;
                    for i in 0..visible.len() - 1 {
                        let x_start = visible[i].0;
                        let x_end = visible[i + 1].0;
                        for step in 0..CURVE_STEPS {
                            let t0 = step as f32 / CURVE_STEPS as f32;
                            let t1 = (step + 1) as f32 / CURVE_STEPS as f32;
                            let x0 = x_start + (x_end - x_start) * t0;
                            let x1 = x_start + (x_end - x_start) * t1;
                            let top0 = catmull(&top, i, t0).clamp(0., 100.);
                            let top1 = catmull(&top, i, t1).clamp(0., 100.);
                            let base0 = catmull(&base, i, t0).clamp(0., 100.);
                            let base1 = catmull(&base, i, t1).clamp(0., 100.);
                            let top_y0 = h * (1. - top0 / 100.);
                            let top_y1 = h * (1. - top1 / 100.);
                            let base_y0 = h * (1. - base0 / 100.);
                            let base_y1 = h * (1. - base1 / 100.);
                            min_y = min_y.min(top_y0).min(top_y1).min(base_y0).min(base_y1);
                            max_y = max_y.max(top_y0).max(top_y1).max(base_y0).max(base_y1);
                            push_solid_quad(
                                &mut path,
                                pt(x0, top_y0),
                                pt(x1, top_y1),
                                pt(x1, base_y1),
                                pt(x0, base_y0),
                            );
                        }
                    }
                    if max_y - min_y > f32::EPSILON {
                        window.paint_path(path, rgb(c.cores[core % c.cores.len()]));
                        painted_triangles += triangles_per_layer;
                    }
                    base = top;
                }
                // Blade's reusable upload belt retains every larger path buffer it sees.
                // Keep this batch at its final bounded size from the first usable frame,
                // so later samples reuse one allocation instead of building a staircase.
                let maximum_triangles = n * max_intervals * CURVE_STEPS * 2;
                let primer_triangles = maximum_triangles.saturating_sub(painted_triangles);
                if primer_triangles > 0 {
                    let mut primer = Path::new(origin);
                    push_solid_triangle(
                        &mut primer,
                        origin,
                        point(origin.x + px(1.), origin.y),
                        point(origin.x, origin.y + px(1.)),
                    );
                    push_degenerate_triangles(&mut primer, origin, primer_triangles - 1);
                    window.paint_path(primer, rgba(0x00000000));
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
            self.poll_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Arc, Duration, GraphSample, History, Instant, Snapshot, catmull, network_scale};

    #[test]
    fn smoothing_stays_within_each_sample_interval() {
        let values: [f32; 5] = [0., 80., 20., 100., 10.];
        for segment in 0..values.len() - 1 {
            let low = values[segment].min(values[segment + 1]);
            let high = values[segment].max(values[segment + 1]);
            for step in 0..=20 {
                let value = catmull(&values, segment, step as f32 / 20.);
                assert!((low..=high).contains(&value));
            }
        }
    }

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

    #[test]
    fn history_has_a_hard_sample_bound_at_high_poll_rates() {
        let mut history = History::new();
        let start = Instant::now();
        for millisecond in 0..1000 {
            history.push(
                Arc::new(Snapshot {
                    at: start + Duration::from_millis(millisecond),
                    ..Default::default()
                }),
                Duration::ZERO,
            );
        }

        assert_eq!(history.samples.len(), super::MAX_HISTORY_SAMPLES);
    }

    #[test]
    fn network_scale_is_stable_and_covers_the_visible_peak() {
        let mut history = History::new();
        history.samples.push_back(Arc::new(GraphSample {
            at: Instant::now(),
            cores: vec![],
            rx: 5_000.,
            tx: 2_000.,
            gpus: vec![],
        }));

        assert_eq!(network_scale(&history), 6_144.);
        assert_eq!(network_scale(&History::new()), 2_048.);
    }

    #[test]
    fn saturated_link_does_not_double_the_network_scale() {
        let mut history = History::new();
        // A link capped at 120 Mbit/s carries 15 MB/s, about 14.3 MiB/s.
        history.samples.push_back(Arc::new(GraphSample {
            at: Instant::now(),
            cores: vec![],
            rx: 15_000_000.,
            tx: 0.,
            gpus: vec![],
        }));

        assert_eq!(network_scale(&history), 20. * 1024. * 1024.);
    }
}
