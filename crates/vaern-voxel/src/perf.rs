//! Lightweight per-system frame-time profiler.
//!
//! `SystemFrameTimes` is a Bevy `Resource` keyed by static strings —
//! systems write per-tick durations into it via `record(name, dt)` and
//! UI code reads back rolling averages + peaks for display.
//!
//! Lives in `vaern-voxel` (instead of `vaern-editor`) so the voxel
//! crate's own systems (`dispatch_mesh_tasks`, `collect_completed_meshes`)
//! can write into it without `vaern-voxel` having to depend on
//! `vaern-editor`. The editor's UI reads the same resource.

use bevy::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

/// Number of recent samples kept per system. 60 ≈ 1s at 60 FPS.
const ROLLING_WINDOW: usize = 60;

/// Rolling-window stats for one system.
#[derive(Clone, Debug, Default)]
pub struct RollingAvg {
    samples: Vec<Duration>,
    write_idx: usize,
    filled: bool,
}

impl RollingAvg {
    pub fn record(&mut self, dt: Duration) {
        if self.samples.len() < ROLLING_WINDOW {
            self.samples.push(dt);
        } else {
            self.samples[self.write_idx] = dt;
            self.write_idx = (self.write_idx + 1) % ROLLING_WINDOW;
            self.filled = true;
        }
    }

    pub fn mean(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.samples.iter().sum();
        total / self.samples.len() as u32
    }

    pub fn max(&self) -> Duration {
        self.samples.iter().copied().max().unwrap_or_default()
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }
}

/// Bevy resource: per-system frame-time samples. Use `record` to add a
/// sample; UI iterates `entries()` for display.
#[derive(Resource, Default)]
pub struct SystemFrameTimes {
    by_name: HashMap<&'static str, RollingAvg>,
}

impl SystemFrameTimes {
    /// Record one sample for the named system.
    pub fn record(&mut self, name: &'static str, dt: Duration) {
        self.by_name.entry(name).or_default().record(dt);
    }

    /// Iterate all (name, stats) pairs. Order is HashMap-iteration
    /// (undefined); UI sorts by mean for display.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, &RollingAvg)> {
        self.by_name.iter().map(|(k, v)| (*k, v))
    }
}

/// RAII timer: records the elapsed time from construction to Drop
/// against `name` in `SystemFrameTimes`. Use at the top of a system
/// body so all early-return paths still record the measurement.
///
/// ```ignore
/// fn my_system(mut perf: ResMut<SystemFrameTimes>, /* ... */) {
///     let _timer = SystemTimer::new(&mut perf, "my_system");
///     if condition { return; }
///     // ... body
/// }  // _timer drops here, recording elapsed
/// ```
pub struct SystemTimer<'a> {
    perf: &'a mut SystemFrameTimes,
    name: &'static str,
    start: std::time::Instant,
}

impl<'a> SystemTimer<'a> {
    pub fn new(perf: &'a mut SystemFrameTimes, name: &'static str) -> Self {
        Self {
            perf,
            name,
            start: std::time::Instant::now(),
        }
    }
}

impl<'a> Drop for SystemTimer<'a> {
    fn drop(&mut self) {
        self.perf.record(self.name, self.start.elapsed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_avg_means_what_it_says() {
        let mut avg = RollingAvg::default();
        avg.record(Duration::from_micros(100));
        avg.record(Duration::from_micros(200));
        avg.record(Duration::from_micros(300));
        assert_eq!(avg.mean(), Duration::from_micros(200));
        assert_eq!(avg.max(), Duration::from_micros(300));
        assert_eq!(avg.sample_count(), 3);
    }

    #[test]
    fn rolling_avg_caps_at_window_size() {
        let mut avg = RollingAvg::default();
        for i in 0..(ROLLING_WINDOW * 3) {
            avg.record(Duration::from_micros(i as u64));
        }
        assert_eq!(avg.sample_count(), ROLLING_WINDOW);
    }

    #[test]
    fn system_frame_times_records_by_name() {
        let mut sft = SystemFrameTimes::default();
        sft.record("foo", Duration::from_micros(100));
        sft.record("bar", Duration::from_micros(200));
        sft.record("foo", Duration::from_micros(150));
        let foo = sft.by_name.get("foo").unwrap();
        assert_eq!(foo.sample_count(), 2);
        assert_eq!(foo.mean(), Duration::from_micros(125));
        assert_eq!(sft.by_name.get("bar").unwrap().sample_count(), 1);
    }
}
