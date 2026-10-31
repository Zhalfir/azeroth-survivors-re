//! Where a frame's time goes, for the combat log's `tick`: the CPU's share (the main world's
//! `First` to `Last`) and, with `WOW_GPU_MS=1`, the GPU's. A tick's `fps` says a run is slow; the
//! split says whether to cut simulation or draw work, which is what a tablet run has to answer
//! before a graphics change is worth keeping.

use bevy::platform::time::Instant;
use bevy::prelude::*;

use super::Phase;

/// One second's frame times in milliseconds, as [`FrameSamples::take`] reduces them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FrameSplit {
    pub cpu_ms_mean: f32,
    pub cpu_ms_max: f32,
    /// `None` when the GPU meter is off or read nothing this second.
    pub gpu_ms_mean: Option<f32>,
}

/// The frame times gathered since the last tick; [`super::combatlog::tick`] takes them each second.
#[derive(Resource, Default)]
pub(super) struct FrameSamples {
    started: Option<Instant>,
    cpu_sum: f32,
    cpu_max: f32,
    cpu_n: u32,
    gpu_sum: f32,
    gpu_n: u32,
}

impl FrameSamples {
    /// When this frame began (`First`), for the frame-rate cap to measure what is left.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(super) fn frame_started(&self) -> Option<Instant> {
        self.started
    }

    pub(super) fn add_cpu(&mut self, ms: f32) {
        self.cpu_sum += ms;
        self.cpu_max = self.cpu_max.max(ms);
        self.cpu_n += 1;
    }

    /// Zero means the meter has not read a frame yet; it is not a sample.
    pub(super) fn add_gpu(&mut self, ms: f32) {
        if ms > 0.0 {
            self.gpu_sum += ms;
            self.gpu_n += 1;
        }
    }

    /// The second's mean and worst, and start the next second empty.
    pub(super) fn take(&mut self) -> FrameSplit {
        let split = FrameSplit {
            cpu_ms_mean: if self.cpu_n > 0 {
                self.cpu_sum / self.cpu_n as f32
            } else {
                0.0
            },
            cpu_ms_max: self.cpu_max,
            gpu_ms_mean: (self.gpu_n > 0).then(|| self.gpu_sum / self.gpu_n as f32),
        };
        *self = Self {
            started: self.started,
            ..Self::default()
        };
        split
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<FrameSamples>()
        .add_systems(First, frame_start)
        .add_systems(Last, frame_end.run_if(in_state(Phase::Playing)));
    #[cfg(feature = "dev")]
    app.add_systems(Last, sample_gpu.run_if(in_state(Phase::Playing)));
}

fn frame_start(mut samples: ResMut<FrameSamples>) {
    samples.started = Some(Instant::now());
}

pub(super) fn frame_end(mut samples: ResMut<FrameSamples>) {
    if let Some(started) = samples.started {
        samples.add_cpu(started.elapsed().as_secs_f32() * 1000.0);
    }
}

/// The meter keeps the freshest GPU frame in nanoseconds; read it once a frame and average the
/// readings over the second. The meter is a `dev` feature (`perf::gpu`), so a build without it
/// has no `gpu_ms` and the log omits the key.
#[cfg(feature = "dev")]
fn sample_gpu(gpu: Option<Res<crate::perf::GpuMsShared>>, mut samples: ResMut<FrameSamples>) {
    if let Some(gpu) = gpu {
        samples.add_gpu(gpu.0.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1_000_000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_mean_and_max() {
        let mut s = FrameSamples::default();
        for ms in [4.0, 6.0, 20.0] {
            s.add_cpu(ms);
        }
        let split = s.take();
        assert_eq!(split.cpu_ms_mean, 10.0);
        assert_eq!(split.cpu_ms_max, 20.0);
        assert_eq!(split.gpu_ms_mean, None);
        // The next second starts empty.
        assert_eq!(s.take().cpu_ms_max, 0.0);
    }

    #[test]
    fn gpu_mean_skips_zero_readings() {
        let mut s = FrameSamples::default();
        for ms in [0.0, 8.0, 0.0, 12.0] {
            s.add_gpu(ms);
        }
        assert_eq!(s.take().gpu_ms_mean, Some(10.0));
    }
}
