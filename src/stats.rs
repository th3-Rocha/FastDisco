//! The F12 overlay's numbers. The process is sampled once a second on its
//! own thread; frames are counted as they are drawn.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

#[derive(Clone, Copy, Debug, Default)]
pub struct Process {
    /// Resident memory, in bytes.
    pub memory: u64,
    pub virtual_memory: u64,
    /// Share of the whole machine, 0 to 100.
    pub cpu: f32,
    /// Share of one core, as `top` shows it.
    pub cpu_core: f32,
    pub threads: usize,
    pub cores: usize,
    pub system_memory: u64,
}

pub struct Stats {
    process: Arc<Mutex<Process>>,
    frames: VecDeque<Instant>,
    frame_times: VecDeque<f32>,
    started: Instant,
    pub visible: bool,
}

impl Stats {
    pub fn start() -> Self {
        let process = Arc::new(Mutex::new(Process::default()));
        let shared = process.clone();
        std::thread::Builder::new()
            .name("stats".into())
            .spawn(move || sample(&shared))
            .expect("the stats thread starts");
        Self {
            process,
            frames: VecDeque::new(),
            frame_times: VecDeque::new(),
            started: Instant::now(),
            visible: false,
        }
    }

    /// Call once per drawn frame, with the time the last one took on the CPU.
    pub fn frame(&mut self, cpu_seconds: Option<f32>) {
        let now = Instant::now();
        self.frames.push_back(now);
        while self
            .frames
            .front()
            .is_some_and(|t| now - *t > Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
        if let Some(seconds) = cpu_seconds {
            self.frame_times.push_back(seconds * 1000.0);
            if self.frame_times.len() > 120 {
                self.frame_times.pop_front();
            }
        }
    }

    /// Frames drawn in the last second.
    pub fn fps(&self) -> usize {
        self.frames.len()
    }

    /// Average and worst CPU time per frame, in milliseconds.
    pub fn frame_ms(&self) -> (f32, f32) {
        if self.frame_times.is_empty() {
            return (0.0, 0.0);
        }
        let sum: f32 = self.frame_times.iter().sum();
        let max = self.frame_times.iter().copied().fold(0.0, f32::max);
        (sum / self.frame_times.len() as f32, max)
    }

    pub fn process(&self) -> Process {
        *self.process.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }
}

fn sample(shared: &Mutex<Process>) {
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new_with_specifics(RefreshKind::nothing());
    system.refresh_memory();
    system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    let cores = system.cpus().len().max(1);
    let system_memory = system.total_memory();
    let kind = ProcessRefreshKind::nothing()
        .with_memory()
        .with_cpu()
        .with_tasks();
    loop {
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, kind);
        if let Some(process) = system.process(pid) {
            let cpu_core = process.cpu_usage();
            let sample = Process {
                memory: process.memory(),
                virtual_memory: process.virtual_memory(),
                cpu: cpu_core / cores as f32,
                cpu_core,
                threads: process.tasks().map_or(0, |t| t.len()),
                cores,
                system_memory,
            };
            *shared.lock().unwrap_or_else(|e| e.into_inner()) = sample;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
