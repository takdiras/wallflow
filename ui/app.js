const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const appWindow = window.__TAURI__.window.getCurrentWindow();

const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];

const SUGGESTED = ["nature", "mountains", "ocean", "forest", "city night", "minimal", "space", "architecture", "abstract", "desert", "aurora", "flowers", "snow", "sunset", "japan", "cars", "texture", "dark"];

let settings = null;
let snap = null;
let discover = { source: "", query: "", page: 1, loading: false };

/* ---------- theming ---------- */

const media = matchMedia("(prefers-color-scheme: dark)");
function resolvedTheme() {
  const t = settings?.theme || "system";
  return t === "system" ? (media.matches ? "dark" : "light") : t;
}

function hexToHsl(hex) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex || "");
  if (!m) return null;
  const n = parseInt(m[1], 16);
  let r = (n >> 16) / 255, g = ((n >> 8) & 255) / 255, b = (n & 255) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  let h = 0, s = 0, l = (max + min) / 2;
  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    h *= 60;
  }
  return { h, s, l };
}

function hslToHex({ h, s, l }) {
  const f = (n) => {
    const k = (n + h / 30) % 12;
    const a = s * Math.min(l, 1 - l);
    const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(c * 255).toString(16).padStart(2, "0");
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

function luminance(hex) {
  const n = parseInt(hex.slice(1), 16);
  const c = [n >> 16, (n >> 8) & 255, n & 255].map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; });
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** Keep the wallpaper's hue but tune lightness/saturation so it reads well on the current theme. */
function applyTheme() {
  const theme = resolvedTheme();
  document.documentElement.dataset.theme = theme;
  const raw = snap?.current?.accent || "#7c6cf2";
  const hsl = hexToHsl(raw) || { h: 250, s: 0.8, l: 0.65 };
  if (hsl.s < 0.08) hsl.s = 0; // greyscale wallpaper → neutral accent
  else hsl.s = Math.max(hsl.s, 0.5);
  hsl.l = theme === "dark" ? Math.min(Math.max(hsl.l, 0.58), 0.74) : Math.min(Math.max(hsl.l, 0.36), 0.52);
  const accent = hslToHex(hsl);
  const root = document.documentElement.style;
  root.setProperty("--accent", accent);
  root.setProperty("--accent-ink", luminance(accent) > 0.42 ? "#111318" : "#ffffff");
  $$("#theme-seg button").forEach((b) => b.classList.toggle("on", b.dataset.theme === (settings?.theme || "system")));
}
media.addEventListener("change", applyTheme);

/* ---------- helpers ---------- */

let toastTimer;
function toast(msg, error = false) {
  const t = $("#toast");
  t.textContent = msg;
  t.classList.toggle("error", error);
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), error ? 5000 : 2400);
}

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const srcName = (s) => ({ unsplash: "Unsplash", pexels: "Pexels", wallhaven: "Wallhaven", bing: "Bing" })[s] || s;
const openUrl = (url) => invoke("open_url", { url });

async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    toast(String(e), true);
    throw e;
  }
}

function fmtInterval(min) {
  if (min % 1440 === 0) return `${min / 1440} day${min > 1440 ? "s" : ""}`;
  if (min % 60 === 0) return `${min / 60} hour${min > 60 ? "s" : ""}`;
  return `${min} min`;
}

function fmtIn(ts) {
  const s = Math.max(0, ts - Date.now() / 1000);
  if (s < 60) return "less than a minute";
  if (s < 3600) return `${Math.round(s / 60)} min`;
  return `${Math.floor(s / 3600)}h ${Math.round((s % 3600) / 60)}m`;
}

/* ---------- navigation ---------- */

function show(view) {
  $$(".nav").forEach((n) => n.classList.toggle("active", n.dataset.view === view));
  $$(".view").forEach((v) => v.classList.toggle("active", v.id === `view-${view}`));
  if (view === "discover" && !$("#grid").children.length) loadDiscover(true);
}
$$(".nav").forEach((n) => n.addEventListener("click", () => show(n.dataset.view)));

/* ---------- window controls ---------- */

$("#win-min").onclick = () => appWindow.minimize();
$("#win-max").onclick = () => appWindow.toggleMaximize();
$("#win-close").onclick = () => appWindow.hide();

/* ---------- now view ---------- */

function renderNow() {
  const cur = snap?.current;
  const p = cur?.photo;
  $("#empty-hero").hidden = !!p;
  $("#hero-img").style.backgroundImage = p ? `url("${p.preview}")` : "";
  $("#backdrop").style.backgroundImage = p ? `url("${p.thumb}")` : "";
  $("#now-title").textContent = p ? (p.description || "Untitled") : "Nothing set yet";
  $("#now-credit").innerHTML = p
    ? `Photo by <a href="#" data-url="${esc(p.author_url)}">${esc(p.author)}</a> on <a href="#" data-url="${esc(p.page_url)}">${srcName(p.source)}</a>`
    : "";
  $("#btn-prev").disabled = !snap?.has_prev;
  document.body.classList.toggle("busy", !!snap?.busy);

  const s = settings;
  if (s) {
    $("#pill-rotate").innerHTML = s.rotate
      ? `Auto-rotate <b>every ${fmtInterval(s.interval_minutes)}</b>${snap?.next_change_at ? ` · next in ${fmtIn(snap.next_change_at)}` : ""}`
      : `Auto-rotate <b>off</b>`;
    $("#pill-cats").innerHTML = `Categories <b>${esc(s.categories.join(", ") || "any")}</b>`;
  }
  $("#pill-count").innerHTML = `History <b>${snap?.total ?? 0}</b>`;
  applyTheme();
  renderHistory();
}

$("#btn-next").onclick = () => call("next_wallpaper").catch(() => {});
$("#btn-prev").onclick = () => call("prev_wallpaper").catch(() => {});

/* ---------- tiles ---------- */

function tile(photo, { label = "Set", onSet, current = false } = {}) {
  const el = document.createElement("div");
  el.className = "tile" + (current ? " current" : "");
  el.innerHTML = `
    <img alt="${esc(photo.description)}" loading="lazy" />
    <span class="src">${srcName(photo.source)}</span>
    <div class="over">
      <span class="by"><a href="#" data-url="${esc(photo.author_url)}">${esc(photo.author)}</a></span>
      <button class="btn primary set">${current ? "Current" : label}</button>
    </div>`;
  const img = $("img", el);
  img.onload = () => img.classList.add("loaded");
  img.src = photo.thumb;
  img.style.background = photo.color;
  el.addEventListener("click", (e) => {
    if (e.target.closest("a")) return;
    if (!current) onSet?.();
  });
  return el;
}

/* ---------- discover ---------- */

async function loadDiscover(reset) {
  if (discover.loading) return;
  discover.loading = true;
  if (reset) {
    discover.page = 1;
    $("#grid").innerHTML = "";
  }
  const empty = $("#discover-empty");
  empty.hidden = true;
  $("#load-more").hidden = true;
  try {
    const photos = await invoke("browse", { source: discover.source || null, query: discover.query, page: discover.page });
    for (const p of photos) {
      $("#grid").append(tile(p, {
        onSet: async () => {
          toast("Setting wallpaper…");
          await call("set_photo", { photo: p }).then(() => toast("Wallpaper set")).catch(() => {});
        },
      }));
    }
    if (!photos.length && reset) {
      empty.hidden = false;
      empty.textContent = "No photos found. Try another search.";
    }
    $("#load-more").hidden = photos.length === 0;
    discover.page++;
  } catch (e) {
    empty.hidden = false;
    empty.innerHTML = `${esc(e)}<br><br><button class="btn" onclick="show('settings')">Open Settings</button>`;
  } finally {
    discover.loading = false;
  }
}

$("#search-form").addEventListener("submit", (e) => {
  e.preventDefault();
  discover.query = $("#search").value.trim();
  loadDiscover(true);
});
$$("#source-seg button").forEach((b) =>
  b.addEventListener("click", () => {
    $$("#source-seg button").forEach((x) => x.classList.toggle("on", x === b));
    discover.source = b.dataset.src;
    loadDiscover(true);
  })
);
$("#load-more").onclick = () => loadDiscover(false);

function renderQuick() {
  const q = $("#quick-tags");
  q.innerHTML = "";
  const rec = document.createElement("button");
  rec.className = "chip";
  rec.textContent = "✦ Recommended";
  rec.onclick = () => { $("#search").value = ""; discover.query = ""; loadDiscover(true); };
  q.append(rec);
  const tags = [...new Set([...(settings?.categories || []), ...SUGGESTED])].slice(0, 12);
  for (const t of tags) {
    const c = document.createElement("button");
    c.className = "chip";
    c.textContent = t;
    c.onclick = () => { $("#search").value = t; discover.query = t; loadDiscover(true); };
    q.append(c);
  }
}

/* ---------- history ---------- */

function renderHistory() {
  const g = $("#history-grid");
  g.innerHTML = "";
  if (!snap?.history?.length) {
    g.innerHTML = `<div class="empty glass">No wallpapers yet.</div>`;
    return;
  }
  // history is newest-first; backend index counts oldest-first
  snap.history.forEach((e, i) => {
    const idx = snap.total - 1 - i;
    g.append(tile(e.photo, {
      label: "Use",
      current: idx === snap.index,
      onSet: () => call("goto_history", { index: idx }).catch(() => {}),
    }));
  });
}

/* ---------- rotation & settings ---------- */

async function save(patch) {
  settings = await call("save_settings", { settings: { ...settings, ...patch } });
  renderSettings();
  renderNow();
}

function renderSettings() {
  const s = settings;
  const tags = $("#cat-tags");
  tags.innerHTML = "";
  for (const c of s.categories) {
    const t = document.createElement("span");
    t.className = "tag";
    t.innerHTML = `${esc(c)}<button title="Remove"><svg viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6"/></svg></button>`;
    $("button", t).onclick = () => save({ categories: s.categories.filter((x) => x !== c) });
    tags.append(t);
  }
  if (!s.categories.length) tags.innerHTML = `<span class="muted small">No categories: random picks use “wallpaper”.</span>`;

  const sug = $("#cat-suggest");
  sug.innerHTML = "";
  for (const c of SUGGESTED.filter((x) => !s.categories.includes(x))) {
    const b = document.createElement("button");
    b.className = "chip";
    b.textContent = "+ " + c;
    b.onclick = () => save({ categories: [...s.categories, c] });
    sug.append(b);
  }

  $("#opt-rotate").checked = s.rotate;
  const sel = $("#opt-interval");
  if (![...sel.options].some((o) => +o.value === s.interval_minutes)) {
    sel.add(new Option(fmtInterval(s.interval_minutes), s.interval_minutes));
  }
  sel.value = String(s.interval_minutes);
  $("#opt-autostart").checked = s.launch_at_startup;
  $("#opt-startup-change").checked = s.change_on_startup;
  // Only takes effect when Windows launches Wallflow at sign-in.
  $("#opt-startup-change").disabled = !s.launch_at_startup;
  $("#row-startup-change").classList.toggle("disabled", !s.launch_at_startup);
  $("#startup-change-hint").hidden = s.launch_at_startup;
  $("#opt-unsplash").checked = s.use_unsplash;
  $("#opt-pexels").checked = s.use_pexels;
  $("#opt-wallhaven").checked = s.use_wallhaven;
  $("#opt-bing").checked = s.use_bing;
  $("#opt-fit").value = s.fit;
  if (document.activeElement !== $("#key-unsplash")) $("#key-unsplash").value = s.unsplash_key;
  if (document.activeElement !== $("#key-pexels")) $("#key-pexels").value = s.pexels_key;
  renderQuick();
  applyTheme();
}

$("#cat-form").addEventListener("submit", (e) => {
  e.preventDefault();
  const v = $("#cat-input").value.trim().toLowerCase();
  if (v && !settings.categories.includes(v)) save({ categories: [...settings.categories, v] });
  $("#cat-input").value = "";
});
$("#opt-rotate").onchange = (e) => save({ rotate: e.target.checked });
$("#opt-interval").onchange = (e) => save({ interval_minutes: +e.target.value });
$("#opt-autostart").onchange = (e) => save({ launch_at_startup: e.target.checked });
$("#opt-startup-change").onchange = (e) => save({ change_on_startup: e.target.checked });
$("#opt-unsplash").onchange = (e) => save({ use_unsplash: e.target.checked });
$("#opt-pexels").onchange = (e) => save({ use_pexels: e.target.checked });
$("#opt-wallhaven").onchange = (e) => save({ use_wallhaven: e.target.checked });
$("#opt-bing").onchange = (e) => save({ use_bing: e.target.checked });
$("#opt-fit").onchange = (e) => save({ fit: e.target.value });
$$("#theme-seg button").forEach((b) => (b.onclick = () => save({ theme: b.dataset.theme })));
$("#save-keys").onclick = async () => {
  await save({ unsplash_key: $("#key-unsplash").value.trim(), pexels_key: $("#key-pexels").value.trim() });
  toast("Keys saved");
  $("#grid").innerHTML = "";
};

/* ---------- updates ---------- */

let update = null;
let updating = false;
function renderUpdate() {
  const btn = $("#update-btn");
  $('.nav[data-view="settings"]').classList.toggle("has-update", !!update);
  if (updating) return;
  btn.disabled = false;
  if (!update) {
    btn.textContent = "Check for updates";
    btn.classList.remove("primary");
    return;
  }
  btn.textContent = update.installable ? `Update to ${update.version}` : "Download";
  btn.classList.add("primary");
  $("#update-status").textContent = update.installable
    ? `Version ${update.version} is available. Wallflow restarts when it's done.`
    : `Version ${update.version} is available. This portable copy can't update itself.`;
}

$("#update-btn").onclick = async () => {
  const btn = $("#update-btn");
  const status = $("#update-status");
  btn.disabled = true;
  if (update) {
    updating = update.installable;
    if (updating) status.textContent = "Downloading update…";
    try {
      await call("install_update");
    } catch {
      updating = false;
    }
    renderUpdate();
    return;
  }
  btn.textContent = "Checking…";
  try {
    update = await call("check_update");
    if (!update) status.textContent = "You're on the latest version.";
  } catch {
    status.textContent = "Couldn't reach GitHub. Try again later.";
  }
  renderUpdate();
};

/* ---------- links ---------- */

document.addEventListener("click", (e) => {
  const a = e.target.closest("a[data-url]");
  if (a) {
    e.preventDefault();
    e.stopPropagation();
    openUrl(a.dataset.url);
  }
}, true);
document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input")) e.preventDefault(); });

/* ---------- boot ---------- */

window.show = show;
(async function init() {
  settings = await invoke("get_settings");
  snap = await invoke("get_state");
  renderSettings();
  renderNow();
  if (!snap.current) show("discover");

  listen("state-changed", (e) => { snap = e.payload; renderNow(); });
  listen("settings-changed", (e) => { settings = e.payload; renderSettings(); renderNow(); });
  listen("busy", (e) => document.body.classList.toggle("busy", e.payload));
  listen("error", (e) => toast(e.payload, true));

  $("#app-version").textContent = await window.__TAURI__.app.getVersion();
  update = await invoke("pending_update");
  renderUpdate();
  listen("update-available", (e) => {
    if (update?.version !== e.payload.version) toast(`Wallflow ${e.payload.version} is available. See Settings.`);
    update = e.payload;
    renderUpdate();
  });
  listen("update-progress", (e) => { $("#update-status").textContent = `Downloading update… ${e.payload}%`; });
  listen("open-view", (e) => show(e.payload));
  setInterval(() => settings?.rotate && renderNow(), 30000);
})();
