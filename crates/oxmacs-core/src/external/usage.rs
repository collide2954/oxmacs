use std::collections::HashMap;
use std::hash::Hash;
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

fn total<K: Copy + Eq + Hash>(
    root: K,
    children: &HashMap<K, Vec<K>>,
    usage: impl Fn(K) -> Option<(f32, u64)>,
) -> (f32, u64) {
    let mut cpu = 0.0;
    let mut memory = 0;
    let mut pending = vec![root];
    while let Some(next) = pending.pop() {
        if let Some((used, bytes)) = usage(next) {
            cpu += used;
            memory += bytes;
        }
        pending.extend(children.get(&next).into_iter().flatten().copied());
    }
    (cpu, memory)
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
        let usage = |pid: Pid| {
            self.system
                .process(pid)
                .map(|process| (process.cpu_usage(), process.memory()))
        };
        pids.iter()
            .filter_map(|pid| {
                let root = Pid::from_u32(*pid);
                self.system.process(root)?;
                let (cpu, memory) = total(root, &children, usage);
                Some((*pid, cpu / self.cores, memory))
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

    #[test]
    fn descendants_count_towards_their_ancestor() {
        let children = HashMap::from([(1, vec![2, 3]), (3, vec![4]), (9, vec![10])]);
        let usage = |pid: u32| Some((f32::from(u8::try_from(pid).unwrap()), u64::from(pid) * 100));

        assert_eq!(total(1, &children, usage), (10.0, 1000));
        assert_eq!(total(3, &children, usage), (7.0, 700));
        assert_eq!(total(5, &children, usage), (5.0, 500));
    }
}
