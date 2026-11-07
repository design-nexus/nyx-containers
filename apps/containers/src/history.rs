//! The last few minutes of CPU and memory per workload, for the sparklines.

use crate::models::LiveSample;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

/// About three minutes at one reading every three seconds.
pub const POINTS: usize = 60;

#[derive(Clone, Default)]
pub struct History {
    /// Percent, 0–100.
    pub cpu: VecDeque<f64>,
    /// Bytes.
    pub mem: VecDeque<u64>,
    pub last: Option<LiveSample>,
}

thread_local! {
    static STORE: RefCell<HashMap<String, History>> = RefCell::new(HashMap::new());
}

pub fn push(samples: &[LiveSample]) {
    STORE.with(|s| {
        let mut store = s.borrow_mut();
        let live: std::collections::HashSet<&str> = samples.iter().map(|x| x.key.as_str()).collect();
        store.retain(|k, _| live.contains(k.as_str()));
        for sample in samples {
            let h = store.entry(sample.key.clone()).or_default();
            if let Some(c) = sample.cpu {
                h.cpu.push_back(c);
            }
            if let Some(m) = sample.mem {
                h.mem.push_back(m);
            }
            while h.cpu.len() > POINTS {
                h.cpu.pop_front();
            }
            while h.mem.len() > POINTS {
                h.mem.pop_front();
            }
            h.last = Some(sample.clone());
        }
    });
}

pub fn get(key: &str) -> Option<History> {
    STORE.with(|s| s.borrow().get(key).cloned())
}

/// Sum of the latest readings over `keys` (a Compose stack's services).
pub fn total(keys: &[String]) -> (Option<f64>, Option<u64>) {
    STORE.with(|s| {
        let store = s.borrow();
        let mut cpu = None;
        let mut mem = None;
        for k in keys {
            if let Some(last) = store.get(k).and_then(|h| h.last.as_ref()) {
                if let Some(c) = last.cpu {
                    *cpu.get_or_insert(0.0) += c;
                }
                if let Some(m) = last.mem {
                    *mem.get_or_insert(0) += m;
                }
            }
        }
        (cpu, mem)
    })
}

/// The summed history over `keys`, point by point from the newest.
pub fn total_series(keys: &[String]) -> (Vec<f64>, Vec<u64>) {
    STORE.with(|s| {
        let store = s.borrow();
        let mut cpu: Vec<f64> = Vec::new();
        let mut mem: Vec<u64> = Vec::new();
        for k in keys {
            let Some(h) = store.get(k) else { continue };
            for (i, v) in h.cpu.iter().rev().enumerate() {
                if cpu.len() <= i {
                    cpu.push(0.0);
                }
                cpu[i] += v;
            }
            for (i, v) in h.mem.iter().rev().enumerate() {
                if mem.len() <= i {
                    mem.push(0);
                }
                mem[i] += v;
            }
        }
        cpu.reverse();
        mem.reverse();
        (cpu, mem)
    })
}
