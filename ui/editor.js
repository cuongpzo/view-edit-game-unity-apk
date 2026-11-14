// Tab, menu chuot phai va trinh soan thao bang (luoi ao hoa, du lieu lay tung khoi tu Rust).
const RH = 28; // chieu cao dong
const HEAD = 30; // chieu cao tieu de
const BLK = 200; // so dong moi lan lay
const RNW = 64; // cot so dong

const tabs = new Map(); // key -> tab
let activeKey = "scan";

// ------------------------------------------------------------------ menu chuot phai
const ctx = $("ctx");

function showCtx(e, items) {
  e.preventDefault();
  e.stopPropagation();
  ctx.replaceChildren();
  for (const it of items) {
    if (it === "-") {
      ctx.appendChild(Object.assign(document.createElement("div"), { className: "sep" }));
      continue;
    }
    const b = document.createElement("button");
    b.setAttribute("role", "menuitem");
    b.appendChild(document.createTextNode(it.label));
    if (it.kbd) b.appendChild(Object.assign(document.createElement("kbd"), { textContent: it.kbd }));
    b.disabled = !!it.disabled;
    if (it.bold) b.classList.add("bold");
    if (it.danger) b.classList.add("danger");
    b.onclick = () => {
      hideCtx();
      it.run();
    };
    ctx.appendChild(b);
  }
  ctx.hidden = false;
  const r = ctx.getBoundingClientRect();
  ctx.style.left = Math.max(4, Math.min(e.clientX, innerWidth - r.width - 4)) + "px";
  ctx.style.top = Math.max(4, Math.min(e.clientY, innerHeight - r.height - 4)) + "px";
  ctx.querySelector("button:not(:disabled)")?.focus();
}
function hideCtx() {
  ctx.hidden = true;
}
ctx.addEventListener("keydown", (e) => {
  const items = [...ctx.querySelectorAll("button:not(:disabled)")];
  const i = items.indexOf(document.activeElement);
  if (e.key === "ArrowDown") items[(i + 1) % items.length]?.focus();
  else if (e.key === "ArrowUp") items[(i - 1 + items.length) % items.length]?.focus();
  else if (e.key === "Escape") hideCtx();
  else return;
  e.preventDefault();
});
document.addEventListener("mousedown", (e) => { if (!ctx.contains(e.target)) hideCtx(); });
window.addEventListener("blur", hideCtx);
window.addEventListener("resize", hideCtx);
// tat menu mac dinh cua WebView (tru o nhap lieu va nhat ky)
document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input, textarea, pre")) e.preventDefault(); });

// ------------------------------------------------------------------ menu cua bang quet
function scanMenu(t) {
  const key = keyOf(t.split, t.entry, t.name);
  return [
    { label: tabs.has(key) ? "Chuyển tới tab đang mở" : "Mở để sửa trong tab", kbd: "Nhấp đúp", bold: true, run: () => openTable(t) },
    { label: "Xuất bảng này ra Excel", disabled: !t.cols, run: () => exportOne(t) },
    { label: "Mở file Excel đã xuất", disabled: !t.cols, run: () => openXlsx(t.name) },
    { label: "Chọn làm bảng cần xuất (bước 3)", disabled: !t.cols, run: () => chooseExport(t) },
    { label: "Sao chép tên bundle", run: () => copyText(t.bundle) },
    "-",
    { label: "Phiên bản / phục hồi…", run: () => showVersions({ split: t.split, entry: t.entry, asset: t.name }) },
    { label: "Khôi phục bản gốc…", danger: true, disabled: !t.modified, run: () => revertTable(t) },
  ];
}

// ------------------------------------------------------------------ phien ban / rollback
function fmtBytes(n) {
  return n >= 1048576 ? (n / 1048576).toFixed(2) + " MB" : Math.round(n / 1024) + " KB";
}

async function showVersions(t) {
  if (!need("work")) return;
  let list = [];
  try { list = await invoke("snapshots", { work: val("work"), split: t.split, entry: t.entry }); } catch (e) { log("✖ " + e); }
  const wrap = el("div");
  wrap.appendChild(el("p", "muted", t.asset));
  if (!list.length) {
    wrap.appendChild(el("div", "empty", "Chưa có phiên bản nào. Mỗi lần bạn Lưu bảng, một phiên bản được lưu lại ở đây để có thể phục hồi."));
  } else {
    const tb = el("table", "verlist");
    const head = el("thead");
    const hr = el("tr");
    ["Thời điểm lưu", "Kích thước", ""].forEach((h) => hr.appendChild(el("th", "", h)));
    head.appendChild(hr);
    const body = el("tbody");
    list.forEach((v, idx) => {
      const tr = el("tr");
      const td0 = el("td", "", fmtTime(v.ts) + (idx === 0 ? "  (mới nhất)" : ""));
      if (idx === 0) td0.classList.add("now");
      tr.append(td0, el("td", "num", fmtBytes(v.size)));
      const act = el("td");
      act.appendChild(btn("Phục hồi", "ghost small", () => doRollback(t, v.ts)));
      tr.appendChild(act);
      body.appendChild(tr);
    });
    tb.append(head, body);
    wrap.appendChild(tb);
  }
  modal("Phiên bản — " + t.asset, wrap, [btn("Đóng", "act", closeModal)]);
}

async function doRollback(t, ts) {
  const ok = await dialog.ask(`Phục hồi "${t.asset}" về phiên bản lúc ${fmtTime(ts)}?\nTrạng thái đã lưu hiện tại sẽ bị thay thế.`, {
    title: "Phục hồi phiên bản", kind: "warning", okLabel: "Phục hồi", cancelLabel: "Huỷ",
  });
  if (!ok) return;
  closeModal();
  await run("Phục hồi phiên bản", async () => {
    await invoke("rollback", { work: val("work"), split: t.split, entry: t.entry, ts });
    log(`Đã phục hồi ${t.asset} về ${fmtTime(ts)}.`);
    await refreshModified();
    const tab = tabs.get(keyOf(t.split, t.entry, t.asset));
    if (tab) {
      if (tab.dirty > 0) log('! tab "' + t.asset + '" còn thay đổi chưa lưu nên chưa tải lại.');
      else await reloadTab(tab);
    }
  });
}

const keyOf = (split, entry, asset) => `${split}|${entry}|${asset}`;

function defaultNames(asset, n) {
  const list = val("names").split(",").map((s) => s.trim()).filter(Boolean);
  if (/language/i.test(asset) && list.length === n) return list;
  return null;
}

function exportOne(t) {
  if (!need("work")) return;
  const tab = tabs.get(keyOf(t.split, t.entry, t.name));
  if (tab && tab.dirty) log(`! "${t.name}" còn ${tab.dirty} ô chưa lưu — file Excel chỉ chứa phần đã lưu.`);
  // file Excel chi chua cot chuoi
  const names = tab ? tab.strIdx.map((i) => tab.names[i]) : defaultNames(t.name, t.cols) || [];
  return run("Xuất " + t.name, () => invoke("export_text", { work: val("work"), pattern: "^" + reEsc(t.entry) + "$", names: names.join(",") }));
}

function openXlsx(asset) {
  if (!need("work")) return;
  invoke("open_path", { path: `${val("work")}\\text\\${asset}.xlsx` }).catch(() => log(`✖ Chưa có ${asset}.xlsx — hãy “Xuất bảng này ra Excel” trước.`));
}

async function revertTable(t) {
  if (!need("work")) return;
  const tab = tabs.get(keyOf(t.split, t.entry, t.name));
  let msg = `Xoá bản đã sửa của "${t.name}" và quay về bản gốc?\nMọi ô đã sửa trong bảng này sẽ mất, không hoàn tác được.`;
  if (tab && tab.dirty) msg += `\n\nTab đang mở còn ${tab.dirty} ô chưa lưu, cũng sẽ mất.`;
  const ok = await dialog.ask(msg, { title: "Khôi phục bản gốc", kind: "warning", okLabel: "Khôi phục", cancelLabel: "Huỷ" });
  if (!ok) return;
  await run("Khôi phục " + t.name, async () => {
    const removed = await invoke("revert_bundle", { work: val("work"), split: t.split, entry: t.entry });
    log(removed ? `Đã xoá bản sửa của ${t.name}.` : `${t.name} không có bản sửa.`);
    await refreshModified();
    if (tab) await reloadTab(tab);
  });
}

function warnDirty(what) {
  const d = [...tabs.values()].filter((t) => t.dirty > 0);
  if (d.length) log(`! ${d.map((t) => `"${t.meta.asset}" (${t.dirty} ô)`).join(", ")} chưa lưu — ${what}.`);
}

// ------------------------------------------------------------------ tab
function activate(key) {
  activeKey = key;
  for (const el of $("tabbar").children) {
    el.classList.toggle("active", el.dataset.tab === key);
    el.setAttribute("aria-selected", el.dataset.tab === key);
  }
  for (const p of $("panes").children) p.classList.toggle("active", p.dataset.pane === key);
  const tab = tabs.get(key);
  if (tab) {
    tab.el.scrollIntoView({ block: "nearest", inline: "nearest" });
    requestAnimationFrame(() => {
      render(tab);
      if (!tab.editing) tab.grid.focus({ preventScroll: true });
    });
  }
}
$("tabbar").querySelector('[data-tab="scan"]').onclick = () => activate("scan");

async function openTable(t) {
  const key = keyOf(t.split, t.entry, t.name);
  if (tabs.has(key)) return activate(key);
  if (!need("work")) return;
  await run("Mở " + t.name, async () => {
    const o = await invoke("table_open", { work: val("work"), split: t.split, entry: t.entry, asset: t.name });
    const tab = buildTab(key, o);
    tabs.set(key, tab);
    activate(key);
    refetch(tab, false);
  });
}

async function closeTab(tab) {
  if (tab.editing) await tab.editing.finish(true);
  await tab.lastCommit;
  if (tab.dirty > 0) {
    const ok = await dialog.ask(`"${tab.meta.asset}" còn ${tab.dirty} ô chưa lưu.\nĐóng tab và bỏ các thay đổi này?`, {
      title: "Đóng tab", kind: "warning", okLabel: "Bỏ thay đổi", cancelLabel: "Huỷ",
    });
    if (!ok) return false;
  }
  invoke("table_close", { id: tab.id }).catch(() => {});
  const keys = [...tabs.keys()];
  const idx = keys.indexOf(tab.key);
  tabs.delete(tab.key);
  tab.el.remove();
  tab.pane.remove();
  if (activeKey === tab.key) activate(keys[idx + 1] ?? keys[idx - 1] ?? "scan");
  return true;
}

async function reloadTab(tab) {
  const m = tab.meta;
  const o = await invoke("table_open", { work: val("work"), split: m.split, entry: m.entry, asset: m.asset });
  invoke("table_close", { id: tab.id }).catch(() => {});
  tab.id = o.id;
  refetch(tab, true);
  log(`Đã tải lại ${m.asset} từ đĩa.`);
}

async function reloadOpenTabs() {
  for (const tab of tabs.values()) {
    if (tab.dirty > 0) log(`! "${tab.meta.asset}" có ${tab.dirty} ô chưa lưu nên chưa tải lại — nếu Lưu, tab sẽ ghi đè dữ liệu vừa nhập.`);
    else await reloadTab(tab);
  }
}

function tabMenu(tab) {
  return [
    { label: "Lưu", kbd: "Ctrl+S", disabled: !tab.dirty, run: () => saveTab(tab) },
    { label: "Tải lại từ đĩa", disabled: tab.dirty > 0, run: () => run("Tải lại " + tab.meta.asset, () => reloadTab(tab)) },
    { label: "Xuất bảng này ra Excel", disabled: !tab.strIdx.length, run: () => exportOne({ ...tab.meta, name: tab.meta.asset, cols: tab.strIdx.length }) },
    { label: "Phiên bản / phục hồi…", run: () => showVersions(tab.meta) },
    "-",
    { label: "Đóng", kbd: "Ctrl+W", run: () => closeTab(tab) },
    { label: "Đóng các tab khác", disabled: tabs.size < 2, run: async () => { for (const t of [...tabs.values()]) if (t !== tab && !(await closeTab(t))) break; } },
  ];
}

// ------------------------------------------------------------------ dung tab soan thao
function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function buildTab(key, o) {
  const n = o.cols.length;
  // cot chuoi (dich duoc, xuat Excel); cac cot con lai la so / mang / bang con
  const strIdx = o.kinds.map((k, i) => (k === "str" ? i : -1)).filter((i) => i >= 0);
  const defW = o.kinds.map((k) => (k === "str" ? 260 : k === "int" || k === "float" || k === "?" || k === "rong" ? 110 : 220));
  const defNames = o.cols.map((c) => "f" + c);
  const langNames = defaultNames(o.asset, strIdx.length);
  if (langNames) strIdx.forEach((i, j) => (defNames[i] = langNames[j]));
  let names = lsGet("colnames:" + o.asset, null);
  // ten cot luu tu ban cu (chi co cot chuoi) -> gan lai vao dung cot chuoi
  if (names && names.length !== n && names.length === strIdx.length) {
    const old = names;
    names = defNames.slice();
    strIdx.forEach((i, j) => (names[i] = old[j]));
  }
  const tab = {
    key, id: o.id, meta: { split: o.split, entry: o.entry, asset: o.asset, cols: o.cols, kinds: o.kinds, descs: o.descs, ro: o.ro },
    strIdx,
    names: names && names.length === n ? names : defNames,
    widths: lsGet("colw:" + o.asset, null) || defW,
    vis: lsGet("colvis:" + o.asset, null) || o.cols.map((_, i) => i),
    cache: new Map(), pending: new Set(), gen: 0, total: 0, dirty: 0, changed: o.changed,
    sel: { p: 0, i: 0 }, editing: null, lastCommit: Promise.resolve(), raf: 0,
  };
  if (tab.widths.length !== n) {
    tab.widths = defW;
    tab.vis = o.cols.map((_, i) => i); // so cot da doi (ban cu chi co cot chuoi) -> hien lai tat ca
  }
  tab.vis = tab.vis.filter((i) => i < n);
  if (!tab.vis.length) tab.vis = o.cols.map((_, i) => i);
  tab.sel.i = strIdx.find((i) => tab.vis.includes(i)) ?? tab.vis[0];

  // nut tab
  tab.el = el("button", "tab");
  tab.el.dataset.tab = key;
  tab.el.title = o.entry;
  tab.el.setAttribute("role", "tab");
  tab.el.append(el("span", "tl", o.asset), el("span", "dot", "●"));
  const x = el("span", "x", "×");
  x.title = "Đóng (Ctrl+W)";
  tab.el.appendChild(x);
  tab.el.onclick = (e) => (e.target === x ? closeTab(tab) : activate(key));
  tab.el.onauxclick = (e) => { if (e.button === 1) closeTab(tab); };
  tab.el.oncontextmenu = (e) => showCtx(e, tabMenu(tab));
  $("tabbar").appendChild(tab.el);

  // thanh cong cu
  tab.pane = el("div", "pane");
  tab.pane.dataset.pane = key;
  const bar = el("div", "etool");
  tab.q = el("input");
  tab.q.placeholder = "Tìm chữ hoặc số dòng…  (Ctrl+F)";
  tab.q.spellcheck = false;
  tab.colSel = el("select");
  tab.mode = el("select");
  for (const [v, t] of [["all", "Mọi dòng"], ["changed", "Đã sửa so với gốc"], ["dirty", "Chưa lưu"], ["empty", "Có ô trống"]]) {
    tab.mode.appendChild(Object.assign(el("option", "", t), { value: v }));
  }
  tab.count = el("span", "count");
  const colsBtn = el("button", "ghost", "Cột ▾");
  tab.discardBtn = el("button", "ghost", "Bỏ thay đổi");
  tab.saveBtn = el("button", "act", "Lưu");
  tab.saveBtn.title = "Ctrl+S";
  bar.append(tab.q, tab.colSel, tab.mode, tab.count, el("span", "sp"), colsBtn, tab.discardBtn, tab.saveBtn);
  fillColSel(tab);

  let deb = 0;
  tab.q.addEventListener("input", () => { clearTimeout(deb); deb = setTimeout(() => refetch(tab, false), 250); });
  tab.q.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { clearTimeout(deb); refetch(tab, false); }
    if (e.key === "ArrowDown" || e.key === "Escape") { e.preventDefault(); tab.grid.focus(); }
  });
  tab.colSel.onchange = () => refetch(tab, false);
  tab.mode.onchange = () => refetch(tab, false);
  colsBtn.onclick = (e) => showCtx(e, colsMenu(tab));
  tab.saveBtn.onclick = () => saveTab(tab);
  tab.discardBtn.onclick = () => discardTab(tab);

  // luoi
  tab.grid = el("div", "grid");
  tab.grid.tabIndex = 0;
  tab.gin = el("div", "gin");
  tab.head = el("div", "ghead");
  tab.body = el("div", "gbody");
  tab.gin.append(tab.head, tab.body);
  tab.grid.appendChild(tab.gin);
  tab.ebar = el("div", "ebar");
  tab.pane.append(bar, tab.grid, tab.ebar);
  $("panes").appendChild(tab.pane);

  buildHead(tab);
  tab.grid.addEventListener("scroll", () => scheduleRender(tab));
  tab.grid.addEventListener("keydown", (e) => gridKey(tab, e));
  tab.body.addEventListener("mousedown", (e) => {
    const c = e.target.closest(".gc[data-i]");
    if (!c) return;
    select(tab, +c.parentElement.dataset.p, +c.dataset.i, false);
    // dua focus ve luoi sau khi chon, nhung khong gianh focus cua o dang sua (nhap dup)
    if (e.button === 0) setTimeout(() => { if (!tab.editing) tab.grid.focus({ preventScroll: true }); });
  });
  tab.body.addEventListener("dblclick", (e) => { if (e.target.closest(".gc[data-i]")) startEdit(tab); });
  tab.body.addEventListener("contextmenu", (e) => {
    const c = e.target.closest(".gc[data-i]");
    if (c) showCtx(e, cellMenu(tab));
  });
  new ResizeObserver(() => scheduleRender(tab)).observe(tab.grid);
  return tab;
}

function fillColSel(tab) {
  const cur = tab.colSel.value;
  tab.colSel.replaceChildren(Object.assign(el("option", "", "Mọi cột"), { value: "-1" }));
  tab.names.forEach((nm, i) => tab.colSel.appendChild(Object.assign(el("option", "", "Cột " + nm), { value: String(i) })));
  tab.colSel.value = cur || "-1";
}

function applyTpl(tab) {
  const w = RNW + tab.vis.reduce((a, i) => a + tab.widths[i], 0);
  tab.gin.style.setProperty("--tpl", `${RNW}px ` + tab.vis.map((i) => tab.widths[i] + "px").join(" "));
  tab.gin.style.setProperty("--w", w + "px");
  tab.gin.style.width = w + "px";
}

function buildHead(tab) {
  applyTpl(tab);
  tab.head.replaceChildren(el("div", "hc rn", "#"));
  for (const i of tab.vis) {
    const h = el("div", "hc");
    const nm = el("span", "", tab.names[i]);
    nm.title = "Nhấp đúp để đổi tên cột";
    const k = tab.meta.kinds[i];
    const tag = el("small", "", "f" + tab.meta.cols[i] + (k === "str" ? "" : " · " + (k === "rong" ? "rỗng" : k)));
    if (k !== "str") tag.title = tab.meta.descs[i] + (tab.meta.ro[i] ? "\nChỉ đọc: không xác định chắc được kiểu dữ liệu" : "");
    h.append(nm, tag);
    nm.ondblclick = () => renameCol(tab, i, h, nm);
    const rz = el("div", "rz");
    rz.onmousedown = (e) => startResize(tab, i, e);
    rz.ondblclick = () => { tab.widths[i] = tab.meta.kinds[i] === "str" ? 260 : 220; buildHead(tab); scheduleRender(tab); lsSet("colw:" + tab.meta.asset, tab.widths); };
    h.appendChild(rz);
    tab.head.appendChild(h);
  }
}

function renameCol(tab, i, h, nm) {
  const inp = el("input");
  inp.value = tab.names[i];
  h.replaceChild(inp, nm);
  inp.focus();
  inp.select();
  let done = false;
  const end = (ok) => {
    if (done) return;
    done = true;
    if (ok && inp.value.trim()) {
      tab.names[i] = inp.value.trim();
      lsSet("colnames:" + tab.meta.asset, tab.names);
      fillColSel(tab);
    }
    buildHead(tab);
    updateBar(tab);
  };
  inp.onkeydown = (e) => { if (e.key === "Enter") end(true); if (e.key === "Escape") end(false); e.stopPropagation(); };
  inp.onblur = () => end(true);
}

function startResize(tab, i, e) {
  e.preventDefault();
  e.stopPropagation();
  const x0 = e.clientX, w0 = tab.widths[i];
  const move = (ev) => { tab.widths[i] = Math.max(70, w0 + ev.clientX - x0); applyTpl(tab); };
  const up = () => {
    removeEventListener("mousemove", move);
    removeEventListener("mouseup", up);
    lsSet("colw:" + tab.meta.asset, tab.widths);
  };
  addEventListener("mousemove", move);
  addEventListener("mouseup", up);
}

function colsMenu(tab) {
  const items = tab.names.map((nm, i) => ({
    label: (tab.vis.includes(i) ? "☑ " : "☐ ") + nm,
    disabled: tab.vis.length === 1 && tab.vis.includes(i),
    run: () => {
      tab.vis = tab.vis.includes(i) ? tab.vis.filter((x) => x !== i) : [...tab.vis, i].sort((a, b) => a - b);
      if (!tab.vis.includes(tab.sel.i)) tab.sel.i = tab.vis[0];
      lsSet("colvis:" + tab.meta.asset, tab.vis);
      buildHead(tab);
      scheduleRender(tab);
    },
  }));
  items.push("-", {
    label: "Hiện tất cả cột",
    run: () => { tab.vis = tab.names.map((_, i) => i); lsSet("colvis:" + tab.meta.asset, tab.vis); buildHead(tab); scheduleRender(tab); },
  });
  return items;
}

// ------------------------------------------------------------------ du lieu
function refetch(tab, keep) {
  tab.gen++;
  tab.cache.clear();
  tab.pending = new Set();
  if (!keep) {
    tab.grid.scrollTop = 0;
    tab.sel.p = 0;
  }
  fetchBlock(tab, Math.floor(Math.floor(tab.grid.scrollTop / RH) / BLK));
}

function fetchBlock(tab, b) {
  if (tab.pending.has(b) || tab.cache.has(b) || !tabs.has(tab.key)) return;
  tab.pending.add(b);
  const gen = tab.gen;
  invoke("table_rows", { id: tab.id, offset: b * BLK, limit: BLK, query: tab.q.value, col: +tab.colSel.value, mode: tab.mode.value })
    .then((pg) => {
      if (gen !== tab.gen) return;
      tab.pending.delete(b);
      tab.cache.set(b, pg.rows);
      tab.total = pg.total;
      setCounts(tab, pg.dirty, pg.changed);
      if (tab.sel.p >= tab.total) tab.sel.p = Math.max(0, tab.total - 1);
      scheduleRender(tab);
    })
    .catch((e) => {
      tab.pending.delete(b);
      log("✖ " + e);
    });
}

function setCounts(tab, dirty, changed) {
  tab.dirty = dirty;
  tab.changed = changed;
  tab.count.textContent = `${nf.format(tab.total)} dòng · ${nf.format(changed)} ô khác gốc · ${nf.format(dirty)} chưa lưu`;
  tab.el.classList.toggle("dirty", dirty > 0);
  tab.el.title = tab.meta.entry + (dirty ? `\n${dirty} ô chưa lưu` : "");
  refreshEditorButtons();
}

function refreshEditorButtons() {
  for (const t of tabs.values()) {
    t.saveBtn.disabled = !t.dirty;
    t.discardBtn.disabled = !t.dirty;
  }
}

function getCell(tab, p, i) {
  const row = tab.cache.get(Math.floor(p / BLK))?.[p % BLK];
  return row ? { row, cell: row.cells[i] } : { row: null, cell: null };
}

// ------------------------------------------------------------------ ve luoi
function scheduleRender(tab) {
  if (tab.raf) return;
  tab.raf = requestAnimationFrame(() => {
    tab.raf = 0;
    render(tab);
  });
}

function render(tab) {
  if (!tab.pane.classList.contains("active")) return;
  const g = tab.grid;
  tab.gin.style.height = HEAD + tab.total * RH + "px";
  const first = Math.max(0, Math.floor(g.scrollTop / RH) - 6);
  const last = Math.min(tab.total, Math.ceil((g.scrollTop + g.clientHeight - HEAD) / RH) + 6);
  if (last > first) {
    for (let b = Math.floor(first / BLK); b <= Math.floor((last - 1) / BLK); b++) fetchBlock(tab, b);
  }
  const frag = document.createDocumentFragment();
  for (let p = first; p < last; p++) {
    const row = tab.cache.get(Math.floor(p / BLK))?.[p % BLK];
    const r = el("div", "gr");
    r.style.top = p * RH + "px";
    r.dataset.p = p;
    if (tab.sel.p === p) r.classList.add("selrow");
    r.appendChild(el("div", "gc rn", row ? String(row.r) : ""));
    for (const i of tab.vis) {
      const c = el("div", "gc");
      c.dataset.i = i;
      if (tab.meta.kinds[i] !== "str") c.classList.add(tab.meta.ro[i] ? "ro" : "data");
      if (!row) {
        c.classList.add("loading");
        c.textContent = "…";
      } else {
        const cell = row.cells[i];
        if (cell.v === null) {
          c.classList.add("absent");
          c.textContent = "—";
        } else {
          c.textContent = cell.v.replace(/\r?\n/g, " ↵ ");
          if (cell.o) {
            c.classList.add("chg");
            c.title = "Gốc: " + (cell.orig ?? "");
          }
          if (cell.d) c.classList.add("dirty");
        }
      }
      if (tab.sel.p === p && tab.sel.i === i) c.classList.add("sel");
      r.appendChild(c);
    }
    frag.appendChild(r);
  }
  tab.body.replaceChildren(frag);
  updateBar(tab);
}

function updateBar(tab) {
  const { row, cell } = getCell(tab, tab.sel.p, tab.sel.i);
  if (!row) {
    tab.ebar.replaceChildren(el("span", "muted", tab.total ? "" : "Không có dòng nào khớp bộ lọc."));
    return;
  }
  const k = tab.meta.kinds[tab.sel.i];
  const v = cell.v === null ? "(trường vắng mặt trong dữ liệu gốc — không sửa được)" : cell.v || "(trống)";
  tab.ebar.replaceChildren(
    el("b", "", `Dòng ${row.r} · ${tab.names[tab.sel.i]}` + (k === "str" ? "" : ` (${tab.meta.ro[tab.sel.i] ? "chỉ đọc" : k})`)),
    el("span", "v", v),
    el("span", "o", cell.o ? "Gốc: " + (cell.orig ?? "") : ""),
  );
}

function colLeft(tab, i) {
  let x = RNW;
  for (const j of tab.vis) {
    if (j === i) break;
    x += tab.widths[j];
  }
  return x;
}

function select(tab, p, i, scroll = true) {
  tab.sel.p = Math.max(0, Math.min(p, tab.total - 1));
  tab.sel.i = tab.vis.includes(i) ? i : tab.vis[0];
  if (scroll) ensureVisible(tab);
  scheduleRender(tab);
}

function ensureVisible(tab) {
  const g = tab.grid;
  const top = tab.sel.p * RH;
  const viewH = g.clientHeight - HEAD;
  if (top < g.scrollTop) g.scrollTop = top;
  else if (top + RH > g.scrollTop + viewH) g.scrollTop = top + RH - viewH;
  const left = colLeft(tab, tab.sel.i), w = tab.widths[tab.sel.i];
  if (left - RNW < g.scrollLeft) g.scrollLeft = left - RNW;
  else if (left + w > g.scrollLeft + g.clientWidth) g.scrollLeft = left + w - g.clientWidth;
}

function gridKey(tab, e) {
  if (tab.editing) return;
  const vi = tab.vis.indexOf(tab.sel.i);
  const page = Math.max(1, Math.floor((tab.grid.clientHeight - HEAD) / RH) - 1);
  const { p, i } = tab.sel;
  const ctrl = e.ctrlKey || e.metaKey;
  switch (e.key) {
    case "ArrowDown": select(tab, p + 1, i); break;
    case "ArrowUp": select(tab, p - 1, i); break;
    case "PageDown": select(tab, p + page, i); break;
    case "PageUp": select(tab, p - page, i); break;
    case "ArrowRight": select(tab, p, tab.vis[Math.min(vi + 1, tab.vis.length - 1)]); break;
    case "ArrowLeft": select(tab, p, tab.vis[Math.max(vi - 1, 0)]); break;
    case "Tab": select(tab, p, tab.vis[Math.max(0, Math.min(vi + (e.shiftKey ? -1 : 1), tab.vis.length - 1))]); break;
    case "Home": ctrl ? select(tab, 0, i) : select(tab, p, tab.vis[0]); break;
    case "End": ctrl ? select(tab, tab.total - 1, i) : select(tab, p, tab.vis[tab.vis.length - 1]); break;
    case "Enter": case "F2": startEdit(tab); break;
    case "c": case "C":
      if (!ctrl) return;
      copyText(getCell(tab, p, i).cell?.v ?? "");
      break;
    default: return;
  }
  e.preventDefault();
}

// ------------------------------------------------------------------ sua o
function startEdit(tab) {
  const { p, i } = tab.sel;
  const { row, cell } = getCell(tab, p, i);
  if (!row || tab.editing) return;
  if (cell.v === null) {
    status("Ô này vắng mặt trong dữ liệu gốc, không sửa được", "err");
    return;
  }
  if (tab.meta.ro[i]) {
    status("Cột này chỉ đọc: không xác định chắc được kiểu dữ liệu", "err");
    return;
  }
  ensureVisible(tab);
  const ta = el("textarea", "gedit");
  ta.value = cell.v;
  ta.spellcheck = false;
  ta.style.left = colLeft(tab, i) + "px";
  ta.style.top = HEAD + p * RH + "px";
  ta.style.width = Math.max(tab.widths[i], 380) + "px";
  tab.gin.appendChild(ta);
  const grow = () => {
    ta.style.height = "auto";
    ta.style.height = Math.min(Math.max(ta.scrollHeight + 4, RH * 3), 320) + "px";
  };
  grow();
  ta.focus();
  ta.setSelectionRange(ta.value.length, ta.value.length);

  let done = false;
  const finish = (commit, move) => {
    if (done) return tab.lastCommit;
    done = true;
    const v = ta.value;
    ta.remove();
    tab.editing = null;
    tab.grid.focus({ preventScroll: true });
    if (commit && v !== cell.v) commitCell(tab, row.r, i, v);
    if (move) select(tab, p + move.dp, move.i ?? i);
    else scheduleRender(tab);
    return tab.lastCommit;
  };
  tab.editing = { finish };
  ta.addEventListener("input", grow);
  ta.addEventListener("blur", () => finish(true));
  ta.addEventListener("keydown", (e) => {
    const vi = tab.vis.indexOf(i);
    if (e.key === "Escape") finish(false);
    else if (e.key === "Enter" && !e.shiftKey && !e.altKey && !e.isComposing) finish(true, { dp: 1 });
    else if (e.key === "Tab") finish(true, { dp: 0, i: tab.vis[Math.max(0, Math.min(vi + (e.shiftKey ? -1 : 1), tab.vis.length - 1))] });
    else return;
    e.preventDefault();
    e.stopPropagation();
  });
}

/** Ghi o vao phien soan thao (chua ghi file). Tra ve promise de Luu co the doi. */
function commitCell(tab, r, i, value) {
  tab.lastCommit = tab.lastCommit.then(() =>
    invoke("table_set", { id: tab.id, r, i, value })
      .then((res) => {
        for (const rows of tab.cache.values()) {
          const row = rows.find((x) => x.r === r);
          if (row) row.cells[i] = res.cell;
        }
        setCounts(tab, res.dirty, res.changed);
        scheduleRender(tab);
      })
      .catch((e) => {
        log(`✖ Dòng ${r} · ${tab.names[i]}: ${e} — ô giữ nguyên giá trị cũ.`);
        status("Giá trị không hợp lệ, xem nhật ký", "err");
      }),
  );
  return tab.lastCommit;
}

function cellMenu(tab) {
  const { row, cell } = getCell(tab, tab.sel.p, tab.sel.i);
  const absent = !row || cell.v === null || tab.meta.ro[tab.sel.i];
  return [
    { label: "Sửa ô", kbd: "Enter / F2", bold: true, disabled: absent, run: () => startEdit(tab) },
    { label: "Sao chép", kbd: "Ctrl+C", disabled: !row || cell.v === null, run: () => copyText(cell.v) },
    { label: "Sao chép giá trị gốc", disabled: !row || !cell.o, run: () => copyText(cell.orig ?? "") },
    { label: "Khôi phục ô về bản gốc", disabled: !row || !cell.o, run: () => commitCell(tab, row.r, tab.sel.i, cell.orig ?? "") },
    "-",
    { label: "Lưu bảng", kbd: "Ctrl+S", disabled: !tab.dirty, run: () => saveTab(tab) },
  ];
}

async function saveTab(tab) {
  if (tab.editing) tab.editing.finish(true);
  await tab.lastCommit;
  if (!tab.dirty) return;
  await run("Lưu " + tab.meta.asset, async () => {
    const w = await invoke("table_save", { id: tab.id });
    log(w.path ? `Đã lưu: ${w.diff} ô khác bản gốc.` : "Đã lưu: bảng giống hệt bản gốc, đã xoá bản sửa.");
    refetch(tab, true);
    await refreshModified();
  });
}

async function discardTab(tab) {
  if (tab.editing) tab.editing.finish(false);
  await tab.lastCommit;
  if (!tab.dirty) return;
  const ok = await dialog.ask(`Bỏ ${tab.dirty} ô chưa lưu trong "${tab.meta.asset}" và quay về lần lưu trước?`, {
    title: "Bỏ thay đổi", kind: "warning", okLabel: "Bỏ thay đổi", cancelLabel: "Huỷ",
  });
  if (!ok) return;
  await invoke("table_discard", { id: tab.id });
  refetch(tab, true);
}

// ------------------------------------------------------------------ phim tat chung
document.addEventListener("keydown", (e) => {
  const ctrl = e.ctrlKey || e.metaKey;
  const tab = tabs.get(activeKey);
  if (!ctrl || !tab) return;
  const k = e.key.toLowerCase();
  if (k === "s") saveTab(tab);
  else if (k === "w") closeTab(tab);
  else if (k === "f") { tab.q.focus(); tab.q.select(); }
  else return;
  e.preventDefault();
});

// hoi truoc khi dong cua so neu con tab chua luu
window.__TAURI__.window.getCurrentWindow().onCloseRequested(async (e) => {
  for (const t of tabs.values()) if (t.editing) t.editing.finish(true);
  await Promise.all([...tabs.values()].map((t) => t.lastCommit));
  const d = [...tabs.values()].filter((t) => t.dirty > 0);
  if (!d.length) return;
  const ok = await dialog.ask(`Còn thay đổi chưa lưu trong: ${d.map((t) => t.meta.asset).join(", ")}.\nThoát và bỏ các thay đổi này?`, {
    title: "Thoát", kind: "warning", okLabel: "Thoát", cancelLabel: "Huỷ",
  });
  if (!ok) e.preventDefault();
});
