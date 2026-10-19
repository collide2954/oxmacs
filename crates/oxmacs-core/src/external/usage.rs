use std::collections::HashMap;
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
        if pids.is_empty() {
            return Vec::new();
        }
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (pid, process) in self.system.processes() {
            if let Some(parent) = process.parent() {
                children.entry(parent).or_default().push(*pid);
            }
        }
        pids.iter()
            .filter_map(|pid| {
                let root = Pid::from_u32(*pid);
                self.system.process(root)?;
                let mut cpu = 0.0;
                let mut memory = 0;
                let mut pending = vec![root];
                while let Some(next) = pending.pop() {
                    if let Some(process) = self.system.process(next) {
                        cpu += process.cpu_usage();
                        memory += process.memory();
                    }
                    pending.extend(children.get(&next).into_iter().flatten().copied());
                }
                Some((*pid, cpu / self.cores, memory))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

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

    #[test]
    fn children_count_towards_their_parent() {
        let mut usage = Usage::new();
        let own = std::process::id();
        let alone = usage.sample(&[own])[0].2;
        let program = if cfg!(windows) { "cmd" } else { "sleep" };
        let args: &[&str] = if cfg!(windows) {
            &["/C", "ping -n 3 127.0.0.1 >NUL"]
        } else {
            &["2"]
        };
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();

        let together = usage.sample(&[own])[0].2;
        let _ = child.kill();
        let _ = child.wait();

        assert!(together > alone, "{together} <= {alone}");
    }
}
