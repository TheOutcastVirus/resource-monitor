use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    Cpu,
    Memory,
    Processes,
    Network,
    Gpu,
    GpuDetails,
    GpuProcesses,
}
impl Panel {
    pub const ALL: [Self; 6] = [
        Self::Cpu,
        Self::Memory,
        Self::Processes,
        Self::Network,
        Self::Gpu,
        Self::GpuProcesses,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Processes => "Processes",
            Self::Network => "Network",
            Self::Gpu => "GPU",
            Self::GpuDetails => "GPU details",
            Self::GpuProcesses => "GPU processes",
        }
    }
    pub fn min_height(self) -> f32 {
        match self {
            Self::Cpu => 280.,
            Self::Memory => 120.,
            Self::Gpu => 360.,
            Self::GpuDetails => 170.,
            Self::Network => 250.,
            _ => 140.,
        }
    }
    pub fn height(self) -> f32 {
        match self {
            Self::Cpu => 340.,
            Self::Memory => 120.,
            Self::Processes => 300.,
            Self::Gpu => 400.,
            Self::Network => 280.,
            Self::GpuDetails => 170.,
            Self::GpuProcesses => 300.,
        }
    }
}
fn one() -> usize {
    1
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tile {
    pub panel: Panel,
    pub height: f32,
    pub lane: usize,
    #[serde(default = "one")]
    pub span: usize,
    pub visible: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub tiles: Vec<Tile>,
    pub auto_fit: bool,
    pub columns: usize,
    pub split: f32,
    pub font_size: f32,
    pub poll_ms: u64,
    pub fps: u64,
}
impl Default for Layout {
    fn default() -> Self {
        Self::portrait()
    }
}
impl Layout {
    pub fn landscape() -> Self {
        Self {
            tiles: Panel::ALL
                .into_iter()
                .map(|p| Tile {
                    panel: p,
                    height: p.height(),
                    span: if matches!(p, Panel::Cpu | Panel::Gpu | Panel::Network) {
                        2
                    } else {
                        1
                    },
                    lane: usize::from(matches!(p, Panel::Gpu | Panel::GpuProcesses)),
                    visible: true,
                })
                .collect(),
            auto_fit: true,
            columns: 2,
            split: 0.5,
            font_size: 12.,
            poll_ms: 1000,
            fps: 30,
        }
    }
    pub fn portrait() -> Self {
        let mut l = Self::landscape();
        l.columns = 1;
        l.tiles = [
            Panel::Cpu,
            Panel::Memory,
            Panel::Processes,
            Panel::Network,
            Panel::Gpu,
            Panel::GpuProcesses,
        ]
        .into_iter()
        .map(|p| Tile {
            panel: p,
            height: match p {
                Panel::Processes | Panel::GpuProcesses => 240.,
                _ => p.height(),
            },
            span: 1,
            lane: 0,
            visible: true,
        })
        .collect();
        l
    }
    pub fn rows(&self, columns: usize) -> Vec<Vec<&Tile>> {
        let mut rows = vec![];
        let mut row = vec![];
        let mut filled = 0;
        for t in self.tiles.iter().filter(|t| t.visible) {
            let span = t.span.min(columns).max(1);
            if filled + span > columns && !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                filled = 0;
            }
            row.push(t);
            filled += span;
            if filled == columns {
                rows.push(std::mem::take(&mut row));
                filled = 0;
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        rows
    }
    pub fn fitted_heights(&self, columns: usize, available: f32) -> Vec<(Panel, f32)> {
        if !self.auto_fit {
            return self
                .tiles
                .iter()
                .filter(|t| t.visible)
                .map(|t| (t.panel, t.height))
                .collect();
        }
        let rows = self.rows(columns);
        let mut pending: Vec<usize> = (0..rows.len()).collect();
        let mut out = vec![0.; rows.len()];
        let mut remaining = (available - 10. * rows.len().saturating_sub(1) as f32).max(0.);
        let weights: Vec<f32> = rows
            .iter()
            .map(|r| r.iter().map(|t| t.height).fold(0., f32::max))
            .collect();
        while !pending.is_empty() {
            let weight: f32 = pending.iter().map(|i| weights[*i]).sum();
            let pass_budget = remaining;
            let mut fixed = vec![];
            for &i in &pending {
                let proposed = pass_budget * weights[i] / weight.max(1.);
                let min = rows[i]
                    .iter()
                    .map(|t| t.panel.min_height())
                    .fold(0., f32::max);
                let max = rows[i]
                    .iter()
                    .map(|t| match t.panel {
                        Panel::Memory => 120.,
                        Panel::GpuDetails => 220.,
                        _ => f32::MAX,
                    })
                    .fold(0., f32::max);
                if proposed < min || proposed > max {
                    out[i] = proposed.clamp(min, max);
                    remaining -= out[i];
                    fixed.push(i);
                }
            }
            if fixed.is_empty() {
                for i in pending {
                    out[i] = remaining * weights[i] / weight.max(1.);
                }
                break;
            }
            pending.retain(|i| !fixed.contains(i));
        }
        rows.into_iter()
            .enumerate()
            .flat_map(|(i, row)| {
                let h = out[i];
                row.into_iter().map(move |t| (t.panel, h))
            })
            .collect()
    }
    pub fn resize_panel(&mut self, panel: Panel, height: f32, fitted: &[(Panel, f32)]) {
        let Some(index) = self.tiles.iter().position(|t| t.panel == panel) else {
            return;
        };
        if !height.is_finite() {
            return;
        }
        // Capture the displayed sizes once so dragging one panel cannot refit its neighbors.
        if self.auto_fit {
            for tile in &mut self.tiles {
                if let Some((_, height)) = fitted.iter().find(|(p, _)| *p == tile.panel) {
                    tile.height = height.clamp(tile.panel.min_height(), 4000.);
                }
            }
            self.auto_fit = false;
        }
        self.tiles[index].height = height.clamp(panel.min_height(), 4000.);
    }
    pub fn normalize(&mut self) {
        self.columns = self.columns.clamp(1, 3);
        self.split = if self.split.is_finite() {
            self.split.clamp(0.3, 0.7)
        } else {
            0.5
        };
        self.font_size = self.font_size.clamp(10., 16.);
        self.poll_ms = self.poll_ms.clamp(250, 5000);
        self.fps = self.fps.clamp(1, 60);
        let mut seen = vec![];
        self.tiles.retain(|t| {
            if t.panel == Panel::GpuDetails || seen.contains(&t.panel) {
                false
            } else {
                seen.push(t.panel);
                true
            }
        });
        for t in &mut self.tiles {
            t.lane = t.lane.min(2);
            t.span = t.span.clamp(1, 3);
            t.height = if t.height.is_finite() {
                t.height.clamp(t.panel.min_height(), 4000.)
            } else {
                t.panel.height()
            };
        }
        for p in Panel::ALL {
            if !seen.contains(&p) {
                self.tiles.push(Tile {
                    panel: p,
                    height: p.height(),
                    span: 1,
                    lane: 0,
                    visible: p == Panel::Network,
                });
            }
        }
    }
    pub fn move_before(&mut self, panel: Panel, target: Panel) {
        if panel == target {
            return;
        }
        let Some(from) = self.tiles.iter().position(|t| t.panel == panel) else {
            return;
        };
        let mut tile = self.tiles.remove(from);
        if let Some(to) = self.tiles.iter().position(|t| t.panel == target) {
            tile.lane = self.tiles[to].lane;
            self.tiles.insert(to, tile);
        } else {
            self.tiles.push(tile);
        }
    }
}
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub layout: Layout,
    pub presets: BTreeMap<String, Layout>,
    pub theme: Theme,
}
impl Settings {
    pub fn path() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            })
            .join("strata/settings.json")
    }
    pub fn load() -> (Self, Option<String>) {
        match fs::read(Self::path()) {
            Ok(data) => match serde_json::from_slice::<Self>(&data) {
                Ok(mut s) => {
                    s.layout.normalize();
                    for l in s.presets.values_mut() {
                        l.normalize();
                    }
                    (s, None)
                }
                Err(e) => (
                    Self::default(),
                    Some(format!("Could not read settings: {e}")),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (
                Self::default(),
                Some(format!("Could not read settings: {e}")),
            ),
        }
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(tmp, path).map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_spans_pack_without_overlap_and_fill_rows() {
        let l = Layout::landscape();
        let rows = l.rows(2);
        assert_eq!(rows[0].len(), 1);
        assert_eq!(rows[0][0].panel, Panel::Cpu);
        assert_eq!(rows[1].len(), 2);
        for row in rows {
            assert!(row.iter().map(|t| t.span.min(2)).sum::<usize>() <= 2);
        }
        assert_eq!(l.rows(1).len(), 6);
    }
    #[test]
    fn default_is_cpu_first_and_panels_fill_a_tall_window() {
        let l = Layout::default();
        assert_eq!(l.columns, 1);
        assert_eq!(l.tiles[2].panel, Panel::Processes);
        assert_eq!(l.tiles[3].panel, Panel::Network);
        assert_eq!(l.tiles[4].panel, Panel::Gpu);
        let sizes = l.fitted_heights(1, 1800.);
        assert!((sizes.iter().map(|(_, h)| h).sum::<f32>() + 50. - 1800.).abs() < 0.1);
        for (p, h) in l.fitted_heights(1, 500.) {
            assert!(h >= p.min_height());
        }
        assert!(l.fitted_heights(1, 2400.)[0].1 > sizes[0].1);
    }
    #[test]
    fn manual_resize_preserves_other_panels_in_every_column_layout() {
        for columns in 1..=3 {
            let mut layout = Layout::portrait();
            layout.columns = columns;
            let before = layout.fitted_heights(columns, 2400.);
            let memory_height = before.iter().find(|(p, _)| *p == Panel::Memory).unwrap().1;

            layout.resize_panel(Panel::Memory, memory_height + 80., &before);

            assert!(!layout.auto_fit);
            for (panel, height) in &before {
                let expected = if *panel == Panel::Memory {
                    height + 80.
                } else {
                    *height
                };
                // Neither viewport size nor responsive column changes may undo a drag.
                for actual in [
                    layout.fitted_heights(columns, 500.),
                    layout.fitted_heights(1, 3000.),
                ] {
                    assert_eq!(actual.iter().find(|(p, _)| p == panel).unwrap().1, expected);
                }
            }

            let current = layout.fitted_heights(columns, 2400.);
            layout.resize_panel(Panel::Memory, memory_height + 160., &current);
            assert_eq!(
                layout
                    .tiles
                    .iter()
                    .find(|t| t.panel == Panel::Memory)
                    .unwrap()
                    .height,
                memory_height + 160.
            );
        }
    }
    #[test]
    fn manual_resize_respects_readable_minimums_and_maximums() {
        let mut layout = Layout::portrait();
        let fitted = layout.fitted_heights(1, 1800.);
        for panel in Panel::ALL {
            layout.resize_panel(panel, 0., &fitted);
            assert_eq!(
                layout
                    .tiles
                    .iter()
                    .find(|t| t.panel == panel)
                    .unwrap()
                    .height,
                panel.min_height()
            );
            layout.resize_panel(panel, 5000., &fitted);
            assert_eq!(
                layout
                    .tiles
                    .iter()
                    .find(|t| t.panel == panel)
                    .unwrap()
                    .height,
                4000.
            );
        }
    }
    #[test]
    fn manual_heights_survive_settings_and_presets_round_trip() {
        let mut settings = Settings::default();
        let fitted = settings.layout.fitted_heights(2, 1800.);
        settings.layout.resize_panel(Panel::Memory, 250., &fitted);
        settings
            .presets
            .insert("Custom".into(), settings.layout.clone());
        let mut decoded: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        decoded.layout.normalize();
        assert!(!decoded.presets["Custom"].auto_fit);
        assert_eq!(
            decoded.layout.fitted_heights(2, 1000.),
            settings.layout.fitted_heights(2, 1800.)
        );

        // Older settings have no sizing mode and must retain automatic fitting.
        let mut legacy = serde_json::to_value(Layout::portrait()).unwrap();
        legacy.as_object_mut().unwrap().remove("auto_fit");
        assert!(serde_json::from_value::<Layout>(legacy).unwrap().auto_fit);
    }
    #[test]
    fn automatic_fit_can_be_restored_after_manual_resize() {
        let mut layout = Layout::portrait();
        let fitted = layout.fitted_heights(1, 1800.);
        layout.resize_panel(Panel::Memory, 500., &fitted);
        layout.auto_fit = true;
        let restored = layout.fitted_heights(1, 1800.);
        assert_eq!(
            restored
                .iter()
                .find(|(p, _)| *p == Panel::Memory)
                .unwrap()
                .1,
            120.
        );
        assert!((restored.iter().map(|(_, h)| h).sum::<f32>() + 50. - 1800.).abs() < 0.1);
    }
    #[test]
    fn reorder_moves_across_lanes_without_losing_panels() {
        let mut l = Layout::landscape();
        l.move_before(Panel::Cpu, Panel::Gpu);
        assert_eq!(
            l.tiles.iter().find(|t| t.panel == Panel::Cpu).unwrap().lane,
            1
        );
        assert_eq!(l.tiles.len(), 6);
    }
    #[test]
    fn presets_round_trip_and_normalize() {
        let mut s = Settings::default();
        s.presets.insert("Desk".into(), Layout::portrait());
        let decoded: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(decoded.presets["Desk"].columns, 1);
        s.layout.tiles.push(s.layout.tiles[0].clone());
        s.layout.poll_ms = 0;
        s.layout.normalize();
        assert_eq!(s.layout.tiles.len(), 6);
        assert_eq!(s.layout.poll_ms, 250);
    }

    #[test]
    fn legacy_gpu_details_panel_is_folded_into_gpu_panel() {
        let mut layout = Layout::portrait();
        layout.tiles.push(Tile {
            panel: Panel::GpuDetails,
            height: Panel::GpuDetails.height(),
            span: 1,
            lane: 0,
            visible: true,
        });

        layout.normalize();

        assert!(layout.tiles.iter().any(|tile| tile.panel == Panel::Gpu));
        assert!(
            layout
                .tiles
                .iter()
                .all(|tile| tile.panel != Panel::GpuDetails)
        );
    }
}
