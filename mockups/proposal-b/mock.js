// Helpers for the proposal-B mockup: icons, sparklines, gauges and the window frame.
// Screens live in index.html and call these.

const I = {
  logo: '<path d="M12 2.8 20 7.4v9.2L12 21.2 4 16.6V7.4z"/><path d="M4 7.4 12 12l8-4.6M12 12v9.2"/><path d="M8 5.1l8 4.6"/>',
  grid: '<rect x="3" y="3" width="7.5" height="7.5" rx="1.8"/><rect x="13.5" y="3" width="7.5" height="7.5" rx="1.8"/><rect x="3" y="13.5" width="7.5" height="7.5" rx="1.8"/><rect x="13.5" y="13.5" width="7.5" height="7.5" rx="1.8"/>',
  vm: '<rect x="2.5" y="3.5" width="19" height="13" rx="2"/><path d="M8 20.5h8M12 16.5v4"/>',
  lxc: '<path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5M12 22V12"/>',
  docker: '<path d="M22 7.7c0-.6-.4-1.2-.8-1.5l-6.3-3.9a1.72 1.72 0 0 0-1.7 0l-10.3 6c-.5.2-.9.8-.9 1.4v6.6c0 .5.4 1.2.8 1.5l6.3 3.9a1.72 1.72 0 0 0 1.7 0l10.3-6c.5-.3.9-1 .9-1.5Z"/><path d="M10 21.9V14L2.1 9.1M10 14l11.9-6.9M14 19.8v-8.1M18 17.5V9.4"/>',
  stack: '<path d="m12 2.5 9.5 4.8L12 12 2.5 7.3z"/><path d="m2.5 16.7 9.5 4.8 9.5-4.8M2.5 12l9.5 4.8 9.5-4.8"/>',
  image: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="2.5"/><path d="M12 3a9 9 0 0 1 9 9"/>',
  storage: '<ellipse cx="12" cy="5.5" rx="8" ry="3"/><path d="M4 5.5v13c0 1.66 3.58 3 8 3s8-1.34 8-3v-13M4 12c0 1.66 3.58 3 8 3s8-1.34 8-3"/>',
  network: '<rect x="9" y="2.5" width="6" height="6" rx="1.2"/><rect x="2.5" y="15.5" width="6" height="6" rx="1.2"/><rect x="15.5" y="15.5" width="6" height="6" rx="1.2"/><path d="M5.5 15.5V13a1 1 0 0 1 1-1h11a1 1 0 0 1 1 1v2.5M12 12V8.5"/>',
  template: '<rect x="14" y="3" width="7" height="7" rx="1.5"/><path d="M10 21V8a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-5a1 1 0 0 0-1-1H3"/>',
  laptop: '<rect x="4" y="4.5" width="16" height="11" rx="1.6"/><path d="M2 19.5h20"/>',
  server: '<rect x="2.5" y="3" width="19" height="7.5" rx="2"/><rect x="2.5" y="13.5" width="19" height="7.5" rx="2"/><path d="M6.5 6.8h.01M6.5 17.3h.01"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  settings: '<path d="M4 8h9M17 8h3M4 16h3M11 16h9"/><circle cx="15" cy="8" r="2"/><circle cx="9" cy="16" r="2"/>',
  tasks: '<path d="m3 7 2 2 4-4M3 17l2 2 4-4M13 6h8M13 12h8M13 18h8"/>',
  terminal: '<rect x="2.5" y="3.5" width="19" height="17" rx="2.5"/><path d="m7 9 3 3-3 3M12.5 15H17"/>',
  logs: '<path d="M8 6h13M8 12h13M8 18h9M3.5 6h.01M3.5 12h.01M3.5 18h.01"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
  power: '<path d="M12 2.5v9"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/>',
  play: '<path d="M7.5 4.8v14.4a.6.6 0 0 0 .9.5l11.4-7.2a.6.6 0 0 0 0-1L8.4 4.3a.6.6 0 0 0-.9.5z"/>',
  pause: '<rect x="6" y="4.5" width="4" height="15" rx="1"/><rect x="14" y="4.5" width="4" height="15" rx="1"/>',
  restart: '<path d="M21 12a9 9 0 1 1-2.64-6.36L21 8"/><path d="M21 3v5h-5"/>',
  more: '<circle cx="5" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="19" cy="12" r="1.4" fill="currentColor" stroke="none"/>',
  chev: '<path d="m6 9 6 6 6-6"/>',
  chevr: '<path d="m9 6 6 6-6 6"/>',
  chevu: '<path d="m6 15 6-6 6 6"/>',
  star: '<path d="M12 3.2l2.6 5.5 6 .8-4.4 4.2 1.1 6L12 16.8l-5.3 2.9 1.1-6-4.4-4.2 6-.8z" fill="currentColor"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/>',
  fit: '<path d="M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3"/>',
  actual: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M8 9.5 9.5 8.5v7M14.5 9.5l1.5-1v7M12 11v.01M12 13.5v.01"/>',
  resize: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M14 9h3v3M10 15H7v-3M17 9l-4 4M7 15l4-4"/>',
  keyboard: '<rect x="2" y="5" width="20" height="14" rx="2.5"/><path d="M6 9h.01M10 9h.01M14 9h.01M18 9h.01M6 13h.01M18 13h.01M8 16h8M10 13h4"/>',
  clipboard: '<rect x="8" y="2.5" width="8" height="4" rx="1"/><path d="M16 4.5h2a2 2 0 0 1 2 2v13a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-13a2 2 0 0 1 2-2h2"/>',
  usb: '<path d="M12 21V5M12 5l-2.5 3M12 5l2.5 3M12 15l-5-3V9.5M12 17l5-3v-2.5"/><circle cx="12" cy="21" r=".6"/><rect x="5.5" y="7.5" width="3" height="2" rx=".5"/><circle cx="17" cy="10" r="1.5"/>',
  speaker: '<path d="M11 5 6 9H3v6h3l5 4z"/><path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13"/>',
  camera: '<path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3z"/><circle cx="12" cy="13" r="3.2"/>',
  expand: '<path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/>',
  shrink: '<path d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  cpu: '<rect x="5" y="5" width="14" height="14" rx="2"/><rect x="9" y="9" width="6" height="6" rx="1"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/>',
  memory: '<rect x="2" y="7" width="20" height="10" rx="1.5"/><path d="M6 11v2M10 11v2M14 11v2M18 11v2M6 17v3M18 17v3"/>',
  disk: '<rect x="2.5" y="13" width="19" height="8" rx="2"/><path d="M5.5 13 8 5h8l2.5 8M6.5 17h.01M10 17h.01"/>',
  disc: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="2.5"/>',
  nic: '<path d="m15 20 3-3h2a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h2l3 3z"/><path d="M6 8v1M10 8v1M14 8v1M18 8v1"/>',
  forward: '<path d="M4 12h12M12 7l5 5-5 5M20 5v14"/>',
  gpu: '<rect x="2" y="6" width="20" height="12" rx="2"/><circle cx="9" cy="12" r="2.6"/><path d="M15 10h3M15 14h3M6 18v2.5"/>',
  shield: '<path d="M12 21.5s8-3.8 8-10V5l-8-2.8L4 5v6.5c0 6.2 8 10 8 10"/><path d="m9 12 2 2 4-4"/>',
  code: '<path d="m16 18 6-6-6-6M8 6l-6 6 6 6"/>',
  git: '<circle cx="6" cy="5.5" r="2.5"/><circle cx="6" cy="18.5" r="2.5"/><circle cx="18" cy="8" r="2.5"/><path d="M6 8v8M18 10.5c0 4-4 4.5-9.6 6.6"/>',
  download: '<path d="M12 3v12M7 10l5 5 5-5M5 21h14"/>',
  upload: '<path d="M12 21V9M7 14l5-5 5 5M5 3h14"/>',
  ext: '<path d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  checkc: '<circle cx="12" cy="12" r="9"/><path d="m8.5 12 2.5 2.5 4.5-5"/>',
  alert: '<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0zM12 9v4M12 17h.01"/>',
  trash: '<path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>',
  snapshot: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7.5V12l3 2"/>',
  backup: '<rect x="2" y="3" width="20" height="5" rx="1.2"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8M10 12h4"/>',
  sliders: '<path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1.5 14h5M9.5 8h5M17.5 16h5"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 16v-4.5M12 8h.01"/>',
  split: '<rect x="3" y="3.5" width="18" height="17" rx="2"/><path d="M12 3.5v17"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
  wrap: '<path d="M3 6h18M3 12h15a3 3 0 1 1 0 6h-4M16 16l-2 2 2 2M3 18h7"/>',
  follow: '<path d="M12 3v13M6 10l6 6 6-6M5 21h14"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  sort: '<path d="m21 16-4 4-4-4M17 20V4M3 8l4-4 4 4M7 4v16"/>',
  rows: '<rect x="3" y="4" width="18" height="6.5" rx="1.8"/><rect x="3" y="13.5" width="18" height="6.5" rx="1.8"/>',
  list: '<path d="M3 6h18M3 12h18M3 18h18"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
  sparkle: '<path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z"/><path d="M19 15.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z"/>',
  rocket: '<path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"/><path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"/><path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5"/>',
  window: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18"/>',
  grip: '<circle cx="9" cy="6" r="1.3" fill="currentColor" stroke="none"/><circle cx="15" cy="6" r="1.3" fill="currentColor" stroke="none"/><circle cx="9" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="15" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="9" cy="18" r="1.3" fill="currentColor" stroke="none"/><circle cx="15" cy="18" r="1.3" fill="currentColor" stroke="none"/>',
  pin: '<path d="M12 17v5M9 10.76V6h6v4.76a2 2 0 0 0 1.11 1.79l1.78.9A2 2 0 0 1 19 15.24V17H5v-1.76a2 2 0 0 1 1.11-1.79l1.78-.9A2 2 0 0 0 9 10.76zM8 2h8"/>',
  heart: '<path d="M22 12h-4l-3 8L9 4l-3 8H2"/>',
  cloud: '<path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/>',
  filter: '<path d="M3 5h18l-7 8v6l-4 2v-8z"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  minus: '<path d="M5 12h14"/>',
  arrowr: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  broom: '<path d="M19.5 3.5 13 10M9.5 9.5l5 5M4 21c0-4 1.5-7.5 5.5-11.5l5 5C10.5 18.5 8 21 4 21zM7 17.5l1.5-1.5"/>',
};

function ic(name, cls = '') {
  return `<i data-i class="${cls}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">${I[name] || ''}</svg></i>`;
}
const ib = (name, cls = '', badge = '') => `<span class="ib ${cls}">${ic(name)}${badge ? `<em class="badge">${badge}</em>` : ''}</span>`;
const power = (cls = '') => `<span class="split ${cls}"><span class="ib">${ic('power')}</span><span class="ib">${ic('chev')}</span></span>`;

// Seeded random so every render is identical.
function rng(seed) {
  let a = seed >>> 0;
  return () => { a |= 0; a = (a + 0x6d2b79f5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}
function series(seed, n, base, vol, drift = 0) {
  const r = rng(seed); const out = []; let v = base;
  for (let k = 0; k < n; k++) { v += (r() - 0.5) * vol + (base - v) * 0.25 + drift; out.push(Math.max(0.02, Math.min(0.98, v))); }
  return out;
}
function pathOf(vals, w, h) {
  const step = w / (vals.length - 1);
  return vals.map((v, k) => `${k ? 'L' : 'M'}${(k * step).toFixed(2)} ${(h - 1 - v * (h - 3)).toFixed(2)}`).join(' ');
}
// A sparkline in the accent; `cap` draws a dashed line at a limit (memory limit).
function spark(seed, base, { vol = 0.14, n = 36, h = 22, cls = '', cap = null, drift = 0 } = {}) {
  const vals = series(seed, n, base, vol, drift); const w = 100; const line = pathOf(vals, w, h);
  const capY = cap == null ? '' : `<line class="cap" x1="0" x2="${w}" y1="${(h - 1 - cap * (h - 3)).toFixed(2)}" y2="${(h - 1 - cap * (h - 3)).toFixed(2)}"/>`;
  return `<svg class="spark ${cls}" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">${capY}<path class="ar" d="${line} L${w} ${h} L0 ${h} Z"/><path class="ln" d="${line}"/></svg>`;
}
function gauge(frac, label, sub) {
  const r = 30, c = 2 * Math.PI * r, d = (c * frac).toFixed(1);
  return `<div class="gauge"><svg viewBox="0 0 72 72"><circle class="ring-t" cx="36" cy="36" r="${r}" fill="none" stroke-width="6"/>
    <circle class="ring-v" cx="36" cy="36" r="${r}" fill="none" stroke-width="6" stroke-linecap="round" stroke-dasharray="${d} ${c}" transform="rotate(-90 36 36)"/>
    <text class="num" x="36" y="41" text-anchor="middle">${Math.round(frac * 100)}%</text></svg><div><b style="color:var(--text);font-weight:500">${label}</b></div><div class="ter" style="font-size:11.5px;margin-top:-4px">${sub}</div></div>`;
}

// ---------- Window frame ----------
const NAV = [
  { g: 'first', items: [['grid', 'Overview', 9, 'overview']] },
  { g: 'Workloads', plus: true, items: [['vm', 'Virtual machines', 2, 'vms'], ['lxc', 'Containers (LXC)', 2, 'lxc'], ['docker', 'Docker', 7, 'docker'], ['stack', 'Stacks', 1, 'stacks']] },
  { g: 'Resources', items: [['image', 'Images', 18, 'images'], ['storage', 'Storage', 6, 'storage'], ['network', 'Networks', 5, 'networks'], ['template', 'Templates', '', 'templates']] },
  { g: 'Hosts', plus: true, items: [['laptop', 'This computer', 'run', 'host'], ['server', 'homelab', 'run', 'homelab']] },
];

function sidebar(active) {
  let html = '';
  for (const grp of NAV) {
    if (grp.g === 'first') html += '<div class="grp first"></div>';
    else html += `<div class="grp">${grp.g}${grp.plus ? ic('plus') : ''}</div>`;
    for (const [icon, label, cnt, id] of grp.items) {
      const tail = cnt === 'run' ? '<em class="dot run" style="box-shadow:none"></em>' : (cnt !== '' ? `<em class="cnt">${cnt}</em>` : '');
      html += `<div class="nav ${id === active ? 'active' : ''}">${ic(icon)}<span>${label}</span>${tail}</div>`;
    }
  }
  return `<aside class="side"><div class="scroll">${html}</div></aside>`;
}

function header(sub) {
  return `<header class="hdr">
    ${ic('logo', 'app-ic')}<span class="app-name">Containers</span><span class="app-sub">${sub}</span>
    <div class="hdr-mid"><div class="search">${ic('search')}<span>Search workloads, images, stacks</span><span class="key">Ctrl F</span></div></div>
    <div class="hdr-end">
      <span class="dd sm">${ic('laptop')}<span>All hosts</span>${ic('chev')}</span><span class="gap"></span>
      ${ib('tasks', '', '1')}${ib('plus', 'primary')}<span class="gap"></span>${ib('settings')}
    </div></header>`;
}

// tabs: [{home} | {icon, label, dot, active}]
function tabRow(tabs) {
  const t = tabs.map((tb) => tb.home
    ? `<div class="tab home ${tb.active ? 'active' : ''}">${ic('grid')}<span>Overview</span></div>`
    : `<div class="tab ${tb.active ? 'active' : ''}">${ic(tb.icon)}<span class="ell">${tb.label}</span><span class="dot ${tb.dot || 'run'}" style="box-shadow:none"></span><span class="x">${ic('x')}</span></div>`).join('');
  return `<div class="tab-row">${t}<span class="add">${ic('plus')}</span><div class="end">${ib('window', 'sm')}</div></div>`;
}

const TABS = (active) => [
  { home: true, active: active === 'overview' },
  { icon: 'vm', label: 'win11-dev', active: active === 'win11' },
  { icon: 'lxc', label: 'dev-box', active: active === 'devbox' },
  { icon: 'stack', label: 'paperless', active: active === 'paperless' },
  ...(active === 'plex' ? [{ icon: 'docker', label: 'plex', active: true }] : []),
];

function frame({ sub, nav, tabs, body, rail = false, full = false, overlay = '' }) {
  const narrow = window.innerWidth < 980;
  return `<div class="win ${rail || narrow ? 'rail' : ''} ${full ? 'full' : ''}">
    ${header(sub)}${sidebar(nav)}
    <main class="page">${tabRow(tabs)}${body}</main>${overlay}</div>`;
}

function boot(screens) {
  const q = new URLSearchParams(location.search);
  document.documentElement.dataset.theme = q.get('theme') || 'dark';
  const name = q.get('screen') || 'overview';
  document.getElementById('app').innerHTML = (screens[name] || screens.overview)();
}
