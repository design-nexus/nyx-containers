# Nyx Containers

A native GTK4 app for your VMs, Linux containers and Docker on this computer and
one remote server: Proxmox's settings and VMware Workstation's tabs, for a
desktop rather than a datacenter. No agent, no web UI.

## Features

- **Overview** — every VM, LXC container, Docker container and Compose stack as
  a short card: status, address and ports (click one to open it), uptime and
  snapshots, live CPU and memory graphs, and Web UI, Display, Terminal, Logs,
  Power and ⋯ buttons. Filter by kind or state, sort, pin to the top. A host
  strip shows CPU, memory, disk and engines.
- **Tabs** — open any workload or stack in a tab (Ctrl+Tab, Ctrl+W, Ctrl+1…9).
- **VM display** — the VM's screen inside its tab over SPICE: keyboard and
  mouse (click to capture, Ctrl+Alt releases), fit or actual size, resize the
  guest when its agent runs, Ctrl+Alt+Del and other keys, type the clipboard,
  screenshots, sound, fullscreen with a floating bar.
- **Terminal and Logs** — a shell in any running workload, the boot console for
  Incus guests; logs to filter, follow, wrap and save.
- **Hardware and Options** (Incus) — cores, pinning, CPU limit and priority,
  memory, root disk size, shared folders, ISO images, volumes, network cards,
  port forwards, USB devices, GPUs and a TPM; autostart and boot order, Secure
  Boot or nesting, scheduled snapshots, the card's web UI address, and the raw
  config as YAML. Changes are a draft until Apply.
- **Snapshots** — take, restore and delete; scheduled ones from Options.
- **Docker** — Inspect (environment with secrets hidden, mounts, networks,
  labels, raw JSON); stacks with Services, a Compose editor (Validate, Pull,
  Deploy) and combined logs.
- **New workload** — a VM or container from the Incus image server, a VM from
  an ISO, a Docker container with ports, folders and environment, or a stack.
- **Hosts** — this computer and the remote: usage, engines, VM feature checks
  (KVM, nesting, IOMMU, virtiofs, TPM, UEFI), storage pools, Docker cleanup.
- **Images, Storage, Networks** — list, pull, create and remove.
- **Connections** — local engines plus one remote. Docker over SSH, Incus over
  HTTPS. SSH host key and TLS pin required before traffic.
- **Diagnostics** — Settings card with Fix (pkexec) or Copy for missing
  packages, groups, daemons, Incus storage, and VTE.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-containers/main/install.sh | bash
```

From a checkout, `./install.sh` does the same.

Asks for sudo when Docker, Incus, or VTE is missing. Installs those packages,
adds your user to `docker`, `incus-admin`, and `incus`, starts the daemons, and
runs `incus admin init --auto` if Incus has no storage yet.

```
./install.sh --skip-engines
```

Builds and installs the app only. Use this when engines are already set up or
sudo is unavailable.

After a group change, log out of the desktop and back in. Reloading Hyprland
does not pick up new groups.

Binary: `~/.local/bin/nyx-containers`. Application id:
`io.github.design_nexus.nyx.Containers`.

```
./uninstall.sh
./uninstall.sh --purge
```

`--purge` also removes config and cache.

## Run

```
nyx-containers
cargo run -p nyx-containers
nyx-containers --section overview
```

`--section` accepts `overview`, `vms`, `lxc`, `docker`, `stacks`, `images`,
`storage`, `networks`, `host`, `homelab`, `connections`. `--open NAME` opens a
workload or stack in a tab (with `--view display`, `terminal`, `logs`,
`snapshots`, `hardware`, `options` or `compose`); `--new` opens New workload.

## Limits

- One remote profile.
- No USB redirection over SPICE yet, and no host↔guest clipboard sync: paste
  into a VM types the text instead. Guest resizing needs spice-vdagent.
- Compose deploys run on this computer; remote stacks can be viewed and
  started or stopped.
- No Kubernetes, clusters, HA, users and permissions, or web dashboard.

Suite registration and a public GitHub repo stay off until you ask.
