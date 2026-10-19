use std::num::NonZero;
use std::thread;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug)]
pub struct Usage {
    system: System,
    cores: f32,
}

impl Default for Usage {
    fn default() -> Self {
        Self::new()
    }
}

impl Usage {
    pub fn new() -> Self {
        let cores = thread::available_parallelism().map_or(1, NonZero::get);
        Self {
            system: System::new(),
            cores: f32::from(u16::try_from(cores).unwrap_or(u16::MAX)),
        }
    }

    pub fn sample(&mut self, pids: &[u32]) -> Vec<(u32, f32, u64)> {
        let wanted: Vec<Pid> = pids.iter().copied().map(Pid::from_u32).collect();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&wanted),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        pids.iter()
            .filter_map(|pid| {
                let process = self.system.process(Pid::from_u32(*pid))?;
                Some((*pid, process.cpu_usage() / self.cores, process.memory()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_process_has_memory_and_a_sane_cpu_share() {
        let mut usage = Usage::new();
        let own = std::process::id();

        let samples = usage.sample(&[own, u32::MAX]);

        assert_eq!(samples.len(), 1);
        let (pid, cpu, memory) = samples[0];
        assert_eq!(pid, own);
        assert!(memory > 0);
        assert!((0.0..=100.0).contains(&cpu), "{cpu}");
    }
}
