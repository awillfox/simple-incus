//! Metrics tab: current CPU / memory / disk / network values with 5-minute
//! sparklines drawn from the samples the detail page collects on every poll
//! (inline SVG, no chart library).
use leptos::prelude::*;

use crate::model::{InstanceDetail, InstanceStatus};
use crate::ui::components::{EmptyState, IconName};
use crate::ui::format::{format_bytes, format_rate};

/// The ring buffer holds at most this many samples (5 s polls x 60 = 5 min).
pub const MAX_SAMPLES: usize = 60;
/// Width of the chart window.
pub const WINDOW_MS: f64 = 300_000.0;
/// Extra refetches (events) closer together than this are not sampled.
const MIN_GAP_MS: f64 = 2_000.0;

const W: f64 = 300.0;
const H: f64 = 64.0;
const PAD: f64 = 3.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub t_ms: f64,
    pub cpu: Option<f32>,
    pub mem: u64,
    pub disk: u64,
    /// Cumulative network counters.
    pub rx: u64,
    pub tx: u64,
}

impl Sample {
    pub fn of(detail: &InstanceDetail, t_ms: f64) -> Sample {
        let s = &detail.summary;
        Sample { t_ms, cpu: s.cpu_pct, mem: s.mem_used, disk: s.disk_used, rx: s.net_rx, tx: s.net_tx }
    }
}

/// Appends `s`, dropping samples older than the window and beyond [`MAX_SAMPLES`].
/// A sample too close to the previous one is ignored (returns false).
pub fn push_sample(buf: &mut Vec<Sample>, s: Sample) -> bool {
    if buf.last().is_some_and(|l| s.t_ms - l.t_ms < MIN_GAP_MS) {
        return false;
    }
    let oldest = s.t_ms - WINDOW_MS;
    buf.push(s);
    buf.retain(|x| x.t_ms >= oldest);
    if buf.len() > MAX_SAMPLES {
        buf.drain(..buf.len() - MAX_SAMPLES);
    }
    true
}

/// Bytes per second between two samples of a cumulative counter; `None` when
/// the counter went backwards (instance restarted) or no time has passed.
pub fn rate(prev: &Sample, cur: &Sample, counter: fn(&Sample) -> u64) -> Option<f64> {
    let dt = (cur.t_ms - prev.t_ms) / 1000.0;
    let (a, b) = (counter(prev), counter(cur));
    (dt > 0.0 && b >= a).then(|| (b - a) as f64 / dt)
}

/// `(t_ms, bytes/s)` for every consecutive pair that has a valid rate.
pub fn rate_series(buf: &[Sample], counter: fn(&Sample) -> u64) -> Vec<(f64, f64)> {
    buf.windows(2).filter_map(|w| rate(&w[0], &w[1], counter).map(|r| (w[1].t_ms, r))).collect()
}

fn xy(t: f64, v: f64, t_end: f64, max: f64) -> (f64, f64) {
    let x = ((t - (t_end - WINDOW_MS)) / WINDOW_MS).clamp(0.0, 1.0) * W;
    let frac = if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    (x, H - PAD - frac * (H - 2.0 * PAD))
}

/// SVG `points` for a polyline in a `300 x 64` viewBox: x by time inside the
/// window ending at `t_end`, y scaled so `max` touches the top.
pub fn sparkline_points(points: &[(f64, f64)], t_end: f64, max: f64) -> String {
    points
        .iter()
        .map(|&(t, v)| {
            let (x, y) = xy(t, v, t_end, max);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Closed polygon under the line, for the soft area fill.
fn area_points(points: &[(f64, f64)], t_end: f64, max: f64) -> String {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return String::new() };
    let line = sparkline_points(points, t_end, max);
    let (x0, _) = xy(first.0, 0.0, t_end, max);
    let (x1, _) = xy(last.0, 0.0, t_end, max);
    format!("{x0:.1},{H:.1} {line} {x1:.1},{H:.1}")
}

fn peak(series: &[&[(f64, f64)]]) -> f64 {
    series.iter().flat_map(|s| s.iter()).map(|p| p.1).fold(0.0, f64::max)
}

#[component]
fn Chart(
    lines: Vec<(Vec<(f64, f64)>, &'static str)>,
    t_end: f64,
    max: f64,
    #[prop(into)] label: String,
) -> impl IntoView {
    let grid = [H * 0.25, H * 0.5, H * 0.75];
    let empty = lines.iter().all(|(pts, _)| pts.len() < 2);
    view! {
        <div class="relative mt-3 h-16">
        <svg
            viewBox=format!("0 0 {W} {H}")
            preserveAspectRatio="none"
            class="size-full overflow-visible"
            role="img"
            aria-label=label
        >
            {grid
                .into_iter()
                .map(|y| {
                    view! {
                        <line x1="0" x2=W y1=y y2=y class="stroke-border" stroke-width="1" vector-effect="non-scaling-stroke"/>
                    }
                })
                .collect_view()}
            <line x1="0" x2=W y1=H y2=H class="stroke-border-strong" stroke-width="1" vector-effect="non-scaling-stroke"/>
            {lines
                .into_iter()
                .map(|(pts, tone)| {
                    let fill = area_points(&pts, t_end, max);
                    let line = sparkline_points(&pts, t_end, max);
                    view! {
                        <g class=tone>
                            <polygon points=fill fill="currentColor" fill-opacity="0.12" stroke="none"></polygon>
                            <polyline
                                points=line
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.75"
                                stroke-linejoin="round"
                                stroke-linecap="round"
                                vector-effect="non-scaling-stroke"
                            ></polyline>
                        </g>
                    }
                })
                .collect_view()}
        </svg>
        {empty
            .then(|| {
                view! {
                    <span class="absolute inset-0 flex items-center justify-center text-[12px] text-muted">
                        "Collecting data\u{2026}"
                    </span>
                }
            })}
        </div>
    }
}

#[component]
fn Card(
    #[prop(into)] title: String,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="panel p-4">
            <div class="flex items-center justify-between">
                <h3 class="text-[12px] font-medium uppercase tracking-wide text-muted">{title}</h3>
                <span class="text-[11px] text-muted">"Last 5 min"</span>
            </div>
            {children()}
        </section>
    }
}

fn value_row(value: String, sub: String) -> impl IntoView {
    view! {
        <div class="mt-2 flex items-baseline gap-2">
            <span class="num text-[24px] font-semibold leading-none text-text">{value}</span>
            <span class="truncate text-[12px] text-muted">{sub}</span>
        </div>
    }
}

/// Current values and sparklines for the running instance.
#[component]
pub fn MetricsTab(
    #[prop(into)] detail: Signal<Option<InstanceDetail>>,
    #[prop(into)] samples: Signal<Vec<Sample>>,
) -> impl IntoView {
    move || {
        let Some(d) = detail.get() else { return ().into_any() };
        let s = d.summary;
        if s.status != InstanceStatus::Running {
            return view! {
                <EmptyState
                    icon=IconName::Info
                    title="Metrics are available while the instance is running"
                    body="Start the instance to see live CPU, memory, disk and network usage."
                />
            }
            .into_any();
        }
        let buf = samples.get();
        let t_end = buf.last().map_or(0.0, |x| x.t_ms);

        let cpu: Vec<(f64, f64)> = buf.iter().filter_map(|x| x.cpu.map(|c| (x.t_ms, f64::from(c)))).collect();
        let mem: Vec<(f64, f64)> = buf.iter().map(|x| (x.t_ms, x.mem as f64)).collect();
        let disk: Vec<(f64, f64)> = buf.iter().map(|x| (x.t_ms, x.disk as f64)).collect();
        let rx = rate_series(&buf, |x| x.rx);
        let tx = rate_series(&buf, |x| x.tx);

        let mem_max = s.mem_limit.filter(|&l| l > 0).map_or_else(|| peak(&[&mem]) * 1.15, |l| l as f64);
        let disk_max = peak(&[&disk]) * 1.15;
        let net_max = (peak(&[&rx, &tx]) * 1.15).max(1024.0);

        let cpu_now = s.cpu_pct.map_or("\u{2014}".to_string(), |c| format!("{c:.1}%"));
        let mem_sub = match s.mem_limit.filter(|&l| l > 0) {
            Some(l) => format!("of {} limit", format_bytes(l)),
            None => "no limit".to_string(),
        };
        let disk_now = if s.disk_used == 0 { "Not reported".to_string() } else { format_bytes(s.disk_used) };
        let rx_now = rx.last().map_or(0.0, |p| p.1);
        let tx_now = tx.last().map_or(0.0, |p| p.1);

        view! {
            <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
                <Card title="CPU">
                    {value_row(cpu_now.clone(), "of allotted capacity".into())}
                    <Chart
                        lines=vec![(cpu, "text-accent")]
                        t_end=t_end
                        max=100.0
                        label=format!("CPU usage over the last 5 minutes, now {cpu_now}")
                    />
                </Card>
                <Card title="Memory">
                    {value_row(format_bytes(s.mem_used), mem_sub)}
                    <Chart
                        lines=vec![(mem, "text-accent")]
                        t_end=t_end
                        max=mem_max
                        label=format!("Memory usage over the last 5 minutes, now {}", format_bytes(s.mem_used))
                    />
                </Card>
                <Card title="Disk">
                    {value_row(disk_now.clone(), "used on the root disk".into())}
                    <Chart
                        lines=vec![(disk, "text-accent")]
                        t_end=t_end
                        max=disk_max
                        label=format!("Disk usage over the last 5 minutes, now {disk_now}")
                    />
                </Card>
                <Card title="Network">
                    <div class="mt-2 flex flex-wrap items-baseline gap-x-5 gap-y-1">
                        <span class="flex items-baseline gap-1.5">
                            <span class="size-2 rounded-full bg-accent" aria-hidden="true"></span>
                            <span class="text-[12px] text-muted">"Receive"</span>
                            <span class="num text-[18px] font-semibold text-text">{format_rate(rx_now)}</span>
                        </span>
                        <span class="flex items-baseline gap-1.5">
                            <span class="size-2 rounded-full bg-info" aria-hidden="true"></span>
                            <span class="text-[12px] text-muted">"Send"</span>
                            <span class="num text-[18px] font-semibold text-text">{format_rate(tx_now)}</span>
                        </span>
                    </div>
                    <Chart
                        lines=vec![(rx, "text-accent"), (tx, "text-info")]
                        t_end=t_end
                        max=net_max
                        label=format!(
                            "Network over the last 5 minutes, receiving {} and sending {}",
                            format_rate(rx_now),
                            format_rate(tx_now),
                        )
                    />
                    <p class="mt-2 text-[12px] text-muted">
                        {format!("Total received {}, sent {}", format_bytes(s.net_rx), format_bytes(s.net_tx))}
                    </p>
                </Card>
            </div>
        }
        .into_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t_s: f64, rx: u64) -> Sample {
        Sample { t_ms: t_s * 1000.0, cpu: Some(10.0), mem: 1, disk: 1, rx, tx: 0 }
    }

    #[test]
    fn ring_keeps_at_most_sixty_and_five_minutes() {
        let mut buf = Vec::new();
        for i in 0..100 {
            assert!(push_sample(&mut buf, sample(f64::from(i) * 5.0, 0)));
        }
        assert_eq!(buf.len(), MAX_SAMPLES);
        assert_eq!(buf.last().unwrap().t_ms, 495_000.0);
        assert!(buf.first().unwrap().t_ms >= 495_000.0 - WINDOW_MS);
    }

    #[test]
    fn samples_too_close_are_ignored() {
        let mut buf = Vec::new();
        assert!(push_sample(&mut buf, sample(0.0, 0)));
        assert!(!push_sample(&mut buf, sample(1.0, 0)));
        assert!(push_sample(&mut buf, sample(5.0, 0)));
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn rates_come_from_counter_deltas() {
        let buf = [sample(0.0, 1000), sample(5.0, 6000), sample(10.0, 6000)];
        assert_eq!(rate_series(&buf, |x| x.rx), vec![(5000.0, 1000.0), (10_000.0, 0.0)]);
    }

    #[test]
    fn counter_reset_has_no_rate() {
        let buf = [sample(0.0, 9000), sample(5.0, 100), sample(10.0, 600)];
        assert_eq!(rate_series(&buf, |x| x.rx), vec![(10_000.0, 100.0)]);
        assert_eq!(rate(&sample(1.0, 0), &sample(1.0, 5), |x| x.rx), None);
    }

    #[test]
    fn sparkline_maps_time_and_value_into_the_viewbox() {
        let t_end = 300_000.0;
        // Oldest edge of the window, bottom; newest, top.
        let pts = [(0.0, 0.0), (300_000.0, 100.0)];
        assert_eq!(sparkline_points(&pts, t_end, 100.0), "0.0,61.0 300.0,3.0");
        // Values above max are clamped, a zero max draws a flat line.
        assert_eq!(sparkline_points(&[(300_000.0, 500.0)], t_end, 100.0), "300.0,3.0");
        assert_eq!(sparkline_points(&[(300_000.0, 5.0)], t_end, 0.0), "300.0,61.0");
        assert_eq!(sparkline_points(&[], t_end, 100.0), "");
    }

    #[test]
    fn area_closes_along_the_baseline() {
        let a = area_points(&[(150_000.0, 50.0), (300_000.0, 50.0)], 300_000.0, 100.0);
        assert_eq!(a, "150.0,64.0 150.0,32.0 300.0,32.0 300.0,64.0");
    }
}
