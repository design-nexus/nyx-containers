//! Live CPU and memory for every running workload, for the cards' graphs. CPU comes
//! from cumulative counters, so each reading is the change since the last one.

use crate::engines::{docker, incus};
use crate::models::{Engine, HostId, LiveSample, workload_key};
use crate::prefs::RemoteProfile;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Default)]
pub struct Sampler {
    incus: HashMap<String, (u64, Instant)>,
    docker: HashMap<String, (u64, u64)>,
}

impl Sampler {
    pub async fn sample(&mut self, remote: &RemoteProfile) -> Vec<LiveSample> {
        let mut out = Vec::new();
        let mut hosts = vec![HostId::Local];
        if remote.configured() {
            hosts.push(HostId::Remote);
        }
        let host_cpus = std::thread::available_parallelism().map(|n| n.get() as f64).unwrap_or(1.0);
        for host in hosts {
            if (host == HostId::Local || remote.incus_enabled)
                && let Ok(list) = incus::live_usage(host, remote).await
            {
                let now = Instant::now();
                for (name, ns, mem, cpus, limit) in list {
                    let key = workload_key(host, Engine::Incus, &name);
                    let cpu = self.incus.get(&key).and_then(|(prev, at)| {
                        let wall = now.duration_since(*at).as_nanos() as f64;
                        (wall > 0.0 && ns >= *prev).then(|| {
                            let cores = cpus.unwrap_or(if host == HostId::Local { host_cpus } else { 1.0 }).max(1.0);
                            ((ns - prev) as f64 / wall / cores * 100.0).clamp(0.0, 100.0)
                        })
                    });
                    self.incus.insert(key.clone(), (ns, now));
                    out.push(LiveSample { key, cpu, mem, mem_limit: limit });
                }
            }
            if (host == HostId::Local || remote.docker_enabled)
                && let Ok(list) = docker::live_usage(host, remote).await
            {
                for (id, total, system, _online, mem, limit) in list {
                    let key = workload_key(host, Engine::Docker, &id);
                    let cpu = self.docker.get(&key).and_then(|(pt, ps)| {
                        let (dt, ds) = (total.checked_sub(*pt)? as f64, system.checked_sub(*ps)? as f64);
                        (ds > 0.0).then(|| (dt / ds * 100.0).clamp(0.0, 100.0))
                    });
                    self.docker.insert(key.clone(), (total, system));
                    out.push(LiveSample { key, cpu, mem, mem_limit: limit });
                }
            }
        }
        out
    }
}
