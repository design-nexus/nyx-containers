# Containers: proposal B (Nyx Design 2.0)

An independent take on the Containers redesign, next to the earlier mockup in
`../index.html`. Nothing in `apps/` changed.

Open `index.html?screen=<name>&theme=dark|light` in a browser, or look at `shots/`.
`./render.sh` re-renders every shot with headless Firefox (nothing opens on screen);
`./render.sh overview vm` renders only shots whose names start with those words.

Colours come from `tokens.css`, copied from the Nyx Design polish mockups (Nyx Dark and
Nyx Light). The guest desktop is CSS art standing in for a real SPICE stream.

## Shots

| Shot | Shows |
| --- | --- |
| `overview.png`, `overview-full.png` | Every VM, LXC and Docker workload as a short full-width card: status on the type tile, name with pinned star and tags (3D, Update, Exited), IP and clickable port chips, uptime and snapshots, live CPU and memory sparklines (dashed line at the memory limit), then actions: Display or Web UI, Terminal, Logs, Power (split: stop, restart, pause, force off) and More. A host strip on top. Chips filter by type; cards group by type. A stopped LXC is dimmed and shows only Start; a crashed container shows why. The Compose stack card is expanded to show its four services as slim rows. The hovered Web UI button shows its tooltip and key. |
| `overview-800.png` | Tiled at 800 px: icon sidebar, IP moves into the subtitle, the info column drops, actions shrink to the main action, Power and More. |
| `overview-light.png` | The same page in Nyx Light. |
| `vm-display.png`, `vm-display-light.png` | A VM opened as a tab (VMware Workstation style). Views: Summary, Display, Hardware, Snapshots, Backups, Logs, Options. The sidebar folds to icons for the display. Toolbar: fit or 1:1, resize the guest to the window, send keys (Ctrl+Alt+Del and others), clipboard sync, USB redirection (1 attached), drop files in, audio, screenshot, fullscreen, pause, restart, power. A caption shows the address, resolution, SPICE and 3D, and how to release the keyboard. |
| `vm-fullscreen.png` | Fullscreen: no app chrome, a small floating toolbar that can be pinned or auto-hide, and a hint toast for the release and exit keys. |
| `vm-hardware.png` | Proxmox-style hardware: every device in one list (processors, memory, display, disks, CD/DVD with the virtio ISO, network card, port forward, shared folder, USB, GPU passthrough, firmware with Secure Boot and vTPM). The selected device opens on the right: cores, CPU limit, pinning to host cores, topology, hot-add, CPU model, priority and nested virtualization. Changes wait for Apply and say when they take effect. Add device and Edit YAML are in the toolbar. |
| `lxc-terminal.png` | LXC tab on Terminal: several sessions, user picker, split, find, paste hints, open in an external terminal. A side panel lists ports (with forwarding to this computer), shared folders, the SSH command, open folder in an editor over SSH, and push or pull a file. |
| `docker-logs.png` | Docker container tab on Logs: filter with matches highlighted, level switch, timestamps, wrap, follow, download. A side panel offers the image update ("Recreate with new image"), container facts (health, ports, network, restart policy, GPU) and mounts. |
| `stack-compose.png` | Compose stack tab: `compose.yaml` and `.env` editor with changed lines marked, Git source with auto-update, Validate, Pull and Deploy. A side panel shows what a deploy will change, the services, and past deploys. The `nyx.webui` label sets the card's Web UI button. |
| `create.png` | New workload wizard over the overview: VM, LXC, Docker or Compose stack; image picker (Incus image server, Windows from an ISO with virtio drivers, any ISO), name, start when created, share ~/Projects, quick size. |
| `host.png`, `host-800.png` | Host page (from HOSTS in the sidebar): usage rings, the last hour of CPU and memory, Docker and Incus status, VM feature checks (KVM, IOMMU, nested, SPICE and 3D, virtiofs, vTPM, UEFI), storage pools, and a cleanup card that frees unused images, build cache, image cache and volumes in one step. |

## Layout in one paragraph

Header: app name, the current place, search, a host filter, activity, New, Settings.
Under it a tab row: Overview is always first, and every workload you open gets a tab with
its status dot (Ctrl+Tab cycles, Ctrl+W closes, a tab can move to its own window). The
sidebar lists workloads by type, resources (images, storage, networks, templates) and
hosts (this computer plus remotes such as a homelab server). No status bar, as 2.0
requires.

## Not in scope

Clusters, HA, users and permissions, Ceph, SDN, Kubernetes, live migration and Incus
projects. This is for one person's desktop and maybe a homelab server, not a datacenter.
