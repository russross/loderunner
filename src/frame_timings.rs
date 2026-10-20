//! Optional real-host timing capture; file writes follow paced execution.
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

struct Frame {
    cycles: u64,
    transition: bool,
    audio_ms: f64,
    graphics_ms: f64,
    work_ms: f64,
    interval_ms: Option<f64>,
}

pub struct FrameTimings {
    output: BufWriter<File>,
    frames: Vec<Frame>,
    previous: Option<Instant>,
}

impl FrameTimings {
    pub fn from_env() -> Result<Option<Self>, String> {
        let Some(path) = std::env::var_os("LODERUNNER_FRAME_TIMINGS") else {
            return Ok(None);
        };
        let output = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("frame timings {path:?}: {e}"))?;
        Ok(Some(Self {
            output: BufWriter::new(output),
            frames: Vec::with_capacity(4096),
            previous: None,
        }))
    }

    // Work ends before pacing. Driver submission may leave GPU execution queued;
    // call intervals include intervening simulation and the host pacing wait.
    pub fn record(
        &mut self,
        cycles: u64,
        transition: bool,
        started: Instant,
        audio: Instant,
        graphics: Instant,
    ) {
        let ended = Instant::now();
        self.frames.push(Frame {
            cycles,
            transition,
            audio_ms: graphics.duration_since(audio).as_secs_f64() * 1000.0,
            graphics_ms: ended.duration_since(graphics).as_secs_f64() * 1000.0,
            work_ms: ended.duration_since(started).as_secs_f64() * 1000.0,
            interval_ms: self
                .previous
                .map(|p| started.duration_since(p).as_secs_f64() * 1000.0),
        });
        self.previous = Some(started);
    }

    pub fn finish(mut self) -> Result<(), String> {
        writeln!(
            self.output,
            "checkpoint,cycles,transition,audio_ms,graphics_ms,work_ms,interval_ms"
        )
        .map_err(|e| e.to_string())?;
        for (index, frame) in self.frames.iter().enumerate() {
            let interval = frame
                .interval_ms
                .map(|v| format!("{v:.6}"))
                .unwrap_or_default();
            writeln!(
                self.output,
                "{},{},{},{:.6},{:.6},{:.6},{interval}",
                index + 1,
                frame.cycles,
                frame.transition,
                frame.audio_ms,
                frame.graphics_ms,
                frame.work_ms
            )
            .map_err(|e| e.to_string())?;
        }
        self.output.flush().map_err(|e| e.to_string())?;

        // Keep transition bursts visible separately from ordinary checkpoints.
        // These host times exclude pacing and are not GPU-completion measurements.
        for (label, transition) in [
            ("all", None),
            ("gameplay", Some(false)),
            ("transition", Some(true)),
        ] {
            let frames: Vec<_> = self
                .frames
                .iter()
                .filter(|f| transition.is_none_or(|value| f.transition == value))
                .collect();
            if frames.is_empty() {
                continue;
            }
            let mut graphics: Vec<_> = frames.iter().map(|f| f.graphics_ms).collect();
            graphics.sort_by(f64::total_cmp);
            let mean = graphics.iter().sum::<f64>() / graphics.len() as f64;
            let audio = frames.iter().map(|f| f.audio_ms).sum::<f64>() / frames.len() as f64;
            let max_work = frames.iter().map(|f| f.work_ms).fold(0.0, f64::max);
            println!(
                "Host timings {label}: checkpoints={} audio_mean={audio:.3}ms graphics_mean={mean:.3}ms graphics_p99={:.3}ms graphics_max={:.3}ms work_max={max_work:.3}ms",
                graphics.len(),
                graphics[(graphics.len() - 1) * 99 / 100],
                graphics.last().expect("nonempty timings")
            );
        }
        Ok(())
    }
}
