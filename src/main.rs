mod chart;
mod layout;
mod metrics;
mod network;
mod table;
mod theme;
mod ui;
use gpui::{prelude::*, *};
fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("Strata {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if std::env::args().any(|a| a == "--check") {
        let mut sampler = metrics::Sampler::new();
        sampler.sample();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let s = sampler.sample();
        println!(
            "CPU: {} · {} cores · {:.1}% · temperature {:?}\nRAM: {} / {} · {} processes\nSample: {:.1} ms",
            s.cpu_name,
            s.cores.len(),
            s.cpu,
            s.temperature,
            metrics::bytes(s.memory_used),
            metrics::bytes(s.memory_total),
            s.processes.len(),
            s.sample_ms
        );
        for g in &s.gpus {
            println!(
                "GPU: {} · {:?}% · {} / {} · {} processes · {:?}",
                g.name,
                g.utilization,
                metrics::bytes(g.memory_used),
                metrics::bytes(g.memory_total),
                g.processes.len(),
                g.process_error
            );
        }
        if let Some(e) = s.gpu_error {
            println!("{e}");
        }
        return;
    }
    Application::new().run(|cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1240.), px(880.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(540.), px(420.))),
                app_id: Some("io.strata.Monitor".into()),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |window, cx| {
                window.set_window_title("Strata — Resource Monitor");
                window.request_decorations(WindowDecorations::Client);
                cx.new(|cx| ui::Monitor::new(window, cx))
            },
        )
        .expect("open Strata window");
        cx.activate(true);
    });
}
