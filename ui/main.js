const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const dialog = window.__TAURI__.dialog;

const $ = (id) => document.getElementById(id);
const FIELDS = ["src", "work", "scanPat", "expPat", "names", "signer"];
const val = (id) => $(id).value.trim();
const nf = new Intl.NumberFormat("vi-VN");
const reEsc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

// ---- luu tru nho (khong bat buoc phai co)
function lsGet(k, d) {
  try { const v = localStorage.getItem(k); return v === null ? d : JSON.parse(v); } catch (_) { return d; }
}
function lsSet(k, v) {
  try { localStorage.setItem(k, JSON.stringify(v)); } catch (_) {}
}
function loadFields() {
  try { for (const f of FIELDS) { const v = localStorage.getItem("f_" + f); if (v !== null) $(f).value = v; } } catch (_) {}
}
let cfg = {};
let cfgTimer = 0;
function saveFields() {
  try { for (const f of FIELDS) localStorage.setItem("f_" + f, $(f).value); } catch (_) {}
  for (const f of FIELDS) cfg[f] = $(f).value;
  clearTimeout(cfgTimer);
  cfgTimer = setTimeout(() => invoke("cfg_save", { cfg }).catch(() => {}), 400);
}
function pushCfg() { invoke("cfg_save", { cfg }).catch(() => {}); }

// ---- ghi lich su su dung vao Documents (backend them moc thoi gian)
function logEvent(ev) {
  try { invoke("log_event", { event: { work: val("work") || null, ...ev } }); } catch (_) {}
}

// ---- giao dien sang/toi/nau
function applyTheme(t) {
  document.documentElement.setAttribute("data-theme", t);
  const sel = $("theme");
  if (sel) sel.value = t;
  try { localStorage.setItem("theme", t); } catch (_) {}
}

function log(line) {
  const el = $("log");
  const t = new Date().toLocaleTimeString("vi-VN", { hour12: false });
  el.textContent += `[${t}] ${line}\n`;
  el.scrollTop = el.scrollHeight;
  if (logCollapsed) {
    logUnseen++;
    if (/✖|LỖI|!/.test(line)) logErr = true;
    const b = $("logNew");
    b.hidden = false;
    b.textContent = `${logUnseen} dòng mới`;
    b.classList.toggle("err", logErr);
  }
}
// ---- thu gon / mo rong nhat ky (nhap thanh tieu de hoac nut −/+)
let logCollapsed = false, logUnseen = 0, logErr = false;
function setLogCollapsed(c) {
  logCollapsed = c;
  document.body.classList.toggle("logmin", c);
  $("btnLogToggle").textContent = c ? "+" : "−";
  $("btnLogToggle").title = c ? "Mở rộng nhật ký" : "Thu gọn nhật ký";
  if (!c) {
    logUnseen = 0;
    logErr = false;
    $("logNew").hidden = true;
    $("log").scrollTop = $("log").scrollHeight;
  }
  try { localStorage.setItem("logmin", c ? "1" : "0"); } catch (_) {}
}
function status(text, cls) {
  const s = $("status");
  s.textContent = text;
  s.className = "status " + (cls || "");
}
function setBusy(b) {
  document.querySelectorAll("button.act").forEach((x) => (x.disabled = b));
  if (!b && typeof refreshEditorButtons === "function") refreshEditorButtons();
}

async function run(title, fn, evType) {
  saveFields();
  setBusy(true);
  status(title + "…", "busy");
  log("▶ " + title);
  try {
    const r = await fn();
    status(title + ": xong", "ok");
    if (evType) logEvent({ type: evType });
    return r;
  } catch (e) {
    log("✖ LỖI: " + e);
    status(title + ": lỗi", "err");
    return undefined;
  } finally {
    setBusy(false);
  }
}

function need(...ids) {
  for (const id of ids) {
    if (!val(id)) {
      $(id).focus();
      status("Chưa điền: " + $(id).closest("label").firstChild.textContent.trim(), "err");
      return false;
    }
  }
  return true;
}

async function copyText(s) {
  try { await navigator.clipboard.writeText(s); status("Đã sao chép", "ok"); } catch (e) { log("✖ Không sao chép được: " + e); }
}

const dirname = (p) => p.replace(/[\\/][^\\/]*$/, "");

async function pick(id) {
  let r;
  if (id === "work") r = await dialog.open({ directory: true });
  else if (id === "src") r = await dialog.open({ filters: [{ name: "XAPK / APK", extensions: ["xapk", "apk", "apks", "zip"] }] });
  else r = await dialog.open({ filters: [{ name: "JAR", extensions: ["jar"] }] });
  if (!r) return;
  $(id).value = r;
  if (id === "src" && !val("work")) $("work").value = dirname(r) + "\\work";
  saveFields();
  if (id !== "signer") autoSigner();
}

async function autoSigner() {
  if (!val("work")) return;
  try {
    const info = await invoke("signer_info", { work: val("work") });
    const el = $("signer");
    if (info && info.startsWith("libra.pak")) {
      el.placeholder = "dùng libra.pak kèm theo (không cần chọn)";
    } else if (info && !val("signer")) {
      el.value = info; // dev: tự điền jar rời cạnh exe
      saveFields();
    } else if (!info) {
      el.placeholder = "chưa có libra.pak / uber-apk-signer.jar";
    }
  } catch (_) {}
}

// ---- ket qua quet
let scanRep = null;

function showScan(rep) {
  scanRep = rep;
  const yn = (b) => (b ? "có" : "không");
  const offs = rep.offsets.map(([o, n]) => `${o} byte: ${n}`).join(", ");
  const tags = [
    `Unity ${rep.unity || "?"}`, rep.scripting, `HybridCLR: ${yn(rep.hybridclr)}`, `Lua: ${yn(rep.lua)}`,
    `YooAsset: ${yn(rep.yooasset)}`, `FMOD: ${yn(rep.fmod)}`, `${rep.bundles} bundle`, `Byte chèn trước UnityFS — ${offs}`,
    `${rep.tables.length} bảng, ${rep.tables.filter((t) => t.chars > 0).length} bảng có chữ`,
  ];
  $("facts").replaceChildren(...tags.map((t) => Object.assign(document.createElement("span"), { className: "tag", textContent: t })));
  renderScanTable();
}

function renderScanTable() {
  const body = $("tables").tBodies[0];
  body.replaceChildren();
  if (!scanRep) return;
  scanRep.tables.forEach((t, idx) => {
    const tr = document.createElement("tr");
    tr.title = t.bundle;
    tr.dataset.idx = idx;
    const name = document.createElement("td");
    name.textContent = t.name;
    if (t.modified) name.appendChild(Object.assign(document.createElement("span"), { className: "badge", textContent: "đã sửa" }));
    tr.appendChild(name);
    for (const v of [nf.format(t.rows), t.cols, nf.format(t.chars)]) {
      tr.appendChild(Object.assign(document.createElement("td"), { className: "num", textContent: v }));
    }
    body.appendChild(tr);
  });
}

function scanRowOf(e) {
  const tr = e.target.closest("tr[data-idx]");
  return tr && scanRep ? [tr, scanRep.tables[+tr.dataset.idx]] : [null, null];
}
function pickRow(tr) {
  $("tables").querySelectorAll("tr.picked").forEach((x) => x.classList.remove("picked"));
  tr.classList.add("picked");
}
function chooseExport(t) {
  $("expPat").value = "^" + reEsc(t.entry) + "$";
  saveFields();
  status("Đã chọn bảng " + t.name + " để xuất", "");
}

$("tables").addEventListener("click", (e) => {
  const [tr, t] = scanRowOf(e);
  if (!t) return;
  pickRow(tr);
  chooseExport(t);
});
$("tables").addEventListener("dblclick", (e) => {
  const [, t] = scanRowOf(e);
  if (t) openTable(t);
});
$("tables").addEventListener("contextmenu", (e) => {
  const [tr, t] = scanRowOf(e);
  if (!t) return;
  pickRow(tr);
  showCtx(e, scanMenu(t));
});

/** Cap nhat co "da sua" tren bang quet theo work/mod. */
async function refreshModified() {
  if (!scanRep || !val("work")) return;
  try {
    const list = new Set(await invoke("modified_entries", { work: val("work") }));
    for (const t of scanRep.tables) t.modified = list.has(t.split + "/" + t.entry);
    const picked = $("tables").querySelector("tr.picked")?.dataset.idx;
    renderScanTable();
    if (picked !== undefined) $("tables").querySelector(`tr[data-idx="${picked}"]`)?.classList.add("picked");
  } catch (_) {}
}

// ---- cac buoc
$("btnUnpack").onclick = () => need("src", "work") && run("Giải nén", () => invoke("unpack", { src: val("src"), work: val("work") }), "unpack").then(autoSigner);

$("btnScan").onclick = () => need("work") && run("Quét", async () => {
  const rep = await invoke("scan", { work: val("work"), pattern: val("scanPat") || "config" });
  showScan(rep);
}, "scan");

$("btnExport").onclick = () => need("work", "expPat") && run("Xuất text", async () => {
  warnDirty("Excel chỉ chứa phần đã lưu");
  const files = await invoke("export_text", { work: val("work"), pattern: val("expPat"), names: val("names") });
  if (!files.length) log("Không có bảng nào khớp bộ lọc.");
}, "export");

$("btnImport").onclick = () => need("work") && run("Nhập bản dịch", async () => {
  const res = await invoke("import_text", { work: val("work") });
  const n = res.reduce((a, r) => a + r.changed, 0);
  log(n ? `Tổng cộng ${n} ô đã ghi vào bundle.` : "Không có ô nào thay đổi so với hiện tại.");
  await refreshModified();
  await reloadOpenTabs();
});

$("btnPack").onclick = () => need("work") && run("Đóng gói", async () => {
  warnDirty("APK chỉ chứa phần đã lưu");
  const r = await invoke("pack", { work: val("work"), signer: val("signer") });
  log(`${r.signed ? "Đã ký" : "CHƯA KÝ"} — ${r.apks.length} APK, thay ${r.replaced} file — ${r.dir}`);
});

const openDir = (sub) => need("work") && invoke("open_path", { path: val("work") + "\\" + sub }).catch((e) => log("✖ " + e));
$("btnOpenText").onclick = () => openDir("text");
$("btnOpenOut").onclick = () => openDir("out");
$("btnClear").onclick = (e) => { e.stopPropagation(); $("log").textContent = ""; };
$("logHead").onclick = () => setLogCollapsed(!logCollapsed);
setLogCollapsed((() => { try { return localStorage.getItem("logmin") === "1"; } catch (_) { return false; } })());
document.querySelectorAll("[data-pick]").forEach((b) => (b.onclick = () => pick(b.dataset.pick)));
FIELDS.forEach((f) => $(f).addEventListener("change", saveFields));

// ---- hộp thoại chung (dùng cho Cấu hình, Lịch sử, Phiên bản)
function modal(title, bodyNode, footButtons) {
  const m = $("modal");
  const h = el("h3");
  h.appendChild(document.createTextNode(title));
  const x = el("span", "x", "×");
  x.onclick = closeModal;
  h.appendChild(x);
  const body = el("div", "body");
  if (bodyNode) body.appendChild(bodyNode);
  m.replaceChildren(h, body);
  if (footButtons && footButtons.length) {
    const f = el("div", "foot");
    footButtons.forEach((b) => f.appendChild(b));
    m.appendChild(f);
  }
  $("mask").hidden = false;
  return m;
}
function closeModal() {
  $("mask").hidden = true;
  $("modal").replaceChildren();
}
$("mask").addEventListener("mousedown", (e) => { if (e.target === $("mask")) closeModal(); });

function btn(label, cls, fn) {
  const b = el("button", cls, label);
  b.onclick = fn;
  return b;
}

// ---- Cấu hình
function openSettings() {
  const wrap = el("div");
  const mkFld = (labelText, node, sub) => {
    const f = el("div", "fld");
    f.appendChild(el("label", "", labelText));
    f.appendChild(node);
    if (sub) f.appendChild(el("div", "sub", sub));
    return f;
  };
  // theme
  const th = el("select");
  for (const [v, t] of [["auto", "Tự động (theo hệ điều hành)"], ["light", "Sáng"], ["dark", "Tối"], ["brown", "Tối + Nâu"]]) {
    th.appendChild(Object.assign(el("option", "", t), { value: v }));
  }
  th.value = document.documentElement.getAttribute("data-theme") || "auto";
  th.onchange = () => { applyTheme(th.value); cfg.theme = th.value; pushCfg(); logEvent({ type: "config", detail: "theme=" + th.value }); };
  wrap.appendChild(mkFld("Giao diện", th));
  // retention
  const ret = el("input");
  ret.type = "number";
  ret.min = "0";
  ret.value = cfg.retention_days ?? 30;
  const retRow = el("div", "row");
  retRow.append(ret, el("span", "muted", "ngày"));
  ret.onchange = () => {
    const d = Math.max(0, parseInt(ret.value) || 0);
    ret.value = d;
    cfg.retention_days = d;
    pushCfg();
    logEvent({ type: "config", detail: "retention_days=" + d });
  };
  wrap.appendChild(mkFld("Thời hạn giữ lịch sử và bản sao lưu", retRow, "Bản sao lưu/lịch sử cũ hơn số ngày này sẽ bị xoá khi mở ứng dụng. 0 = giữ mãi mãi (luôn giữ ít nhất bản mới nhất của mỗi bảng)."));
  // data dir
  invoke("data_dir").then((d) => {
    const inp = el("input");
    inp.value = d;
    inp.readOnly = true;
    const row = el("div", "row");
    row.append(inp, btn("Mở", "ghost", () => invoke("open_path", { path: d }).catch((e) => log("✖ " + e))));
    wrap.appendChild(mkFld("Thư mục dữ liệu (trong Documents)", row, "Chứa config.json, history.jsonl và thư mục backups để rollback."));
  });
  modal("Cấu hình", wrap, [
    btn("Dọn log/temp cũ ngay", "ghost", async () => {
      try { const n = await invoke("cleanup_now"); log(`Đã dọn ${n} mục cũ.`); status("Đã dọn dẹp", "ok"); } catch (e) { log("✖ " + e); }
    }),
    btn("Đóng", "act", closeModal),
  ]);
}

// ---- Lịch sử sử dụng
const HTYPE = {
  unpack: "Giải nén", scan: "Quét", export: "Xuất Excel", import: "Nhập bản dịch",
  pack: "Đóng gói/Ký", open: "Mở bảng", save: "Lưu bảng", save_clean: "Lưu (về gốc)",
  revert: "Khôi phục gốc", rollback: "Phục hồi phiên bản", config: "Đổi cấu hình", extract: "Trích xuất tài nguyên",
};
function fmtTime(ts) {
  if (!ts) return "";
  const d = new Date(ts);
  return d.toLocaleString("vi-VN", { hour12: false });
}
function histDetail(e) {
  const p = [];
  if (e.asset) p.push(e.asset);
  if (e.cells != null) p.push(e.cells + " ô");
  if (e.apks != null) p.push(`${e.apks} APK${e.signed ? " (đã ký)" : ""}`);
  if (e.detail) p.push(e.detail);
  if (e.ts && e.type === "rollback") p.push("→ " + fmtTime(e.ts));
  return p.join(" · ");
}
async function openHistory() {
  let rows = [];
  try { rows = await invoke("history_list", { work: val("work") || null, limit: 500 }); } catch (e) { log("✖ " + e); }
  const wrap = el("div");
  if (!rows.length) {
    wrap.appendChild(el("div", "empty", "Chưa có lịch sử."));
  } else {
    const w = el("div", "histwrap");
    const tb = el("table", "histtbl");
    const thead = el("tr");
    ["Thời gian", "Việc", "Chi tiết"].forEach((h) => thead.appendChild(el("th", "", h)));
    const head = el("thead");
    head.appendChild(thead);
    const body = el("tbody");
    for (const e of rows) {
      const tr = el("tr");
      tr.appendChild(el("td", "when", fmtTime(e.ts)));
      const ty = el("td");
      ty.appendChild(el("span", "pill", HTYPE[e.type] || e.type || "?"));
      tr.appendChild(ty);
      tr.appendChild(el("td", "detail", histDetail(e)));
      body.appendChild(tr);
    }
    tb.append(head, body);
    w.appendChild(tb);
    wrap.appendChild(w);
  }
  modal(`Lịch sử sử dụng${val("work") ? " — " + val("work") : ""}`, wrap, [
    btn("Mở thư mục dữ liệu", "ghost", () => invoke("data_dir").then((d) => invoke("open_path", { path: d }))),
    btn("Đóng", "act", closeModal),
  ]);
}

$("btnSettings").onclick = openSettings;
$("btnHistory").onclick = openHistory;
$("theme").onchange = () => { applyTheme($("theme").value); cfg.theme = $("theme").value; pushCfg(); logEvent({ type: "config", detail: "theme=" + $("theme").value }); };
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !$("mask").hidden) closeModal(); });

// ---- khởi động: áp theme ngay (tránh nháy), rồi nạp cấu hình từ Documents
applyTheme((() => { try { return localStorage.getItem("theme") || "auto"; } catch (_) { return "auto"; } })());
listen("log", (e) => log(e.payload));

async function init() {
  loadFields();
  try {
    const saved = await invoke("cfg_load");
    if (saved && typeof saved === "object") {
      cfg = saved;
      for (const f of FIELDS) if (cfg[f] != null && !$(f).value) $(f).value = cfg[f];
      if (cfg.theme) applyTheme(cfg.theme);
    }
  } catch (_) {}
  autoSigner();
}
init();
