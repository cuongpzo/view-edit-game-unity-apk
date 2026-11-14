// Tab "Tệp game": 4 khung kiểu Explorer
//   cây thư mục | (2) tệp & thư mục | (3) nội dung bundle / FMOD bank đang chọn | (4) xem nội dung
// Bundle không cần "đi vào": chọn ở khung 2 là khung 3 hiện ngay các đối tượng bên trong.
(() => {
  const { convertFileSrc } = window.__TAURI__.core;
  if (!document.querySelector('[data-pane="files"]')) return;
  const tree = $("ftree"), view = $("fview"), addr = $("xaddr"), split = $("xsplit");

  const ICON = {
    dir: "📁", bundle: "📦", image: "🖼️", sprite: "🧩", audio: "🔊", video: "🎞️", text: "📄", table: "📊",
    bank: "🎵", font: "🔤", data: "🧱", other: "▫️", file: "📄",
  };
  const KIND_VI = {
    dir: "Thư mục", bundle: "Bundle Unity", image: "Hình ảnh", sprite: "Sprite", audio: "Âm thanh", video: "Video",
    text: "Văn bản", table: "Bảng text", bank: "FMOD bank", font: "Font", data: "Dữ liệu nhị phân", other: "Đối tượng", file: "Tệp",
  };
  const KINDS = [
    ["text", "Text (TextAsset, bảng FlatBuffers → .tsv)"],
    ["image", "Hình ảnh (Texture2D và ảnh rời → .png)"],
    ["sprite", "Sprite (cắt theo tên từ atlas → .png)"],
    ["audio", "Âm thanh (FMOD bank / AudioClip → .ogg)"],
    ["video", "Video (VideoClip → .mp4)"],
    ["font", "Font (.ttf / .otf)"],
    ["data", "Dữ liệu nhị phân khác (.bytes, .bank)"],
  ];
  const MODES = {
    details: { rh: 24 },
    list: { rh: 22, cw: 240 },
    medium: { tw: 104, th: 104, img: 64 },
    large: { tw: 140, th: 142, img: 104 },
  };
  const MINW = 210; // đủ thấy ~20 ký tự tên tệp (cả biểu tượng, lề, thanh cuộn)

  const size = (n) => (n >= 1073741824 ? (n / 1073741824).toFixed(2) + " GB" : n >= 1048576 ? (n / 1048576).toFixed(n >= 10485760 ? 0 : 1) + " MB" : n >= 1024 ? Math.ceil(n / 1024) + " KB" : n + " B");
  const date = (ms) => (ms ? new Date(ms).toLocaleString("vi-VN", { hour12: false, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }) : "");
  const gfs = (p, id, t) => convertFileSrc(JSON.stringify(t ? { p, id: id || "", t } : id ? { p, id } : { p }), "gfs");
  const basename = (p) => p.replace(/[\\/]+$/, "").replace(/^.*[\\/]/, "");
  const parentDir = (p) => { const s = p.replace(/[\\/]+$/, ""); const i = Math.max(s.lastIndexOf("\\"), s.lastIndexOf("/")); return i > 0 ? s.slice(0, i) + (i === 2 && s[1] === ":" ? "\\" : "") : ""; };
  const isAsset = (it) => it.id !== undefined;
  const isBox = (it) => !isAsset(it) && (it.kind === "bundle" || it.kind === "bank"); // tệp có nội dung con (khung 3)
  const keyOf = (it) => it.path + "|" + (it.id || "");
  const extOf = (n) => { const m = /\.([^.\\/]+)$/.exec(n); return m ? m[1].toUpperCase() : ""; };
  const typeOf = (it) => {
    if (isAsset(it)) return it.cls === "FSB5" ? "Âm thanh (FMOD)" : it.kind === "other" ? it.cls : `${KIND_VI[it.kind] || it.kind} (${it.cls})`;
    if (it.kind === "dir" || it.kind === "bundle") return KIND_VI[it.kind];
    if (it.kind === "bank") return "FMOD bank";
    const e = extOf(it.name);
    return e ? `${KIND_VI[it.kind] === "Tệp" ? "Tệp" : KIND_VI[it.kind]} ${e}` : KIND_VI[it.kind] || "Tệp";
  };
  const fullPath = (it) => (isAsset(it) ? `${it.path} › ${it.full || it.name}` : it.path);
  const coll = new Intl.Collator("vi", { numeric: true, sensitivity: "base" });

  function iconEl(it, px) {
    const box = el("span", "xic");
    if ((it.kind === "image" || it.kind === "sprite") && px >= 32) {
      const img = el("img");
      img.loading = "lazy";
      img.decoding = "async";
      img.draggable = false;
      img.src = isAsset(it) ? gfs(it.path, it.id, px * 2) : gfs(it.path);
      img.onerror = () => box.replaceChildren(document.createTextNode(ICON[it.kind]));
      box.appendChild(img);
      box.classList.add("th");
    } else {
      box.textContent = ICON[it.kind] || "▫️";
    }
    if (px) box.style.setProperty("--px", px + "px");
    return box;
  }

  // ================================================================ danh sách dùng chung (khung 2 và 3)
  // o: {el, id, cols:[{k,t,w,num}], defMode, cell(it,k), colTitle(c), onSel(v), onOpen(it,v), onMenu(e,v,chosen), onKey(e,v)}
  function makeView(o) {
    const v = {
      all: [], items: [], sel: new Set(), focus: -1, anchor: -1, msg: "", q: "", collapsed: new Set(),
      mode: lsGet(o.id + ":mode", o.defMode),
      widths: lsGet(o.id + ":colw", null),
      sort: lsGet(o.id + ":sort", { k: "name", d: 1 }),
    };
    if (!Array.isArray(v.widths) || v.widths.length !== o.cols.length) v.widths = o.cols.map((c) => c.w);
    const root = o.el;
    const head = el("div", "xhead"), inner = el("div", "xin");
    root.replaceChildren(head, inner);
    let raf = 0, last = { i: -1, t: 0, x: 0, y: 0 };

    const tpl = () => v.widths.map((w) => w + "px").join(" ");
    function geom() {
      const m = MODES[v.mode] || MODES.details;
      if (v.mode === "details") return { kind: "rows", rh: m.rh, head: 26 };
      if (v.mode === "list") return { kind: "cols", rh: m.rh, rows: Math.max(1, Math.floor((root.clientHeight - 8) / m.rh)), cw: m.cw };
      return { kind: "tiles", per: Math.max(1, Math.floor((root.clientWidth - 8) / m.tw)), tw: m.tw, th: m.th, img: m.img };
    }

    // ---- sắp xếp + lọc + cây (bank → các âm thanh con)
    function sortVal(it, k) {
      if (k === "size") return it.size || 0;
      if (k === "mtime") return it.mtime || 0;
      if (k === "type") return typeOf(it).toLowerCase();
      if (k === "name") return it.name.toLowerCase();
      return String(o.cell(it, k) || "").toLowerCase();
    }
    function cmp(a, b) {
      const fa = a.kind === "dir" ? 0 : 1, fb = b.kind === "dir" ? 0 : 1;
      if (fa !== fb) return fa - fb;
      const { k, d } = v.sort;
      const va = sortVal(a, k), vb = sortVal(b, k);
      const c = typeof va === "number" ? va - vb : coll.compare(va, vb);
      return (c || coll.compare(a.name, b.name)) * d;
    }
    function arrange() {
      const q = v.q.toLowerCase();
      const hit = (it) => !q || it.name.toLowerCase().includes(q);
      const kids = new Map();
      const roots = [];
      for (const it of v.all) {
        if (it.parent) { if (!kids.has(it.parent)) kids.set(it.parent, []); kids.get(it.parent).push(it); }
        else roots.push(it);
      }
      roots.sort(cmp);
      const out = [];
      for (const r of roots) {
        const ks = kids.get(keyOf(r)) || [];
        const rh = hit(r), kh = ks.filter(hit);
        if (!rh && !kh.length) continue;
        r.nkids = ks.length;
        out.push(r);
        if (ks.length && (!v.collapsed.has(keyOf(r)) || (q && kh.length))) {
          const show = rh ? ks : kh;
          out.push(...(v.sort.k === "name" && v.sort.d > 0 ? show : [...show].sort(cmp)));
        }
      }
      v.items = out;
    }
    function rearrange(keepKey) {
      const k = keepKey ?? (v.items[v.focus] ? keyOf(v.items[v.focus]) : null);
      arrange();
      v.focus = v.anchor = k ? v.items.findIndex((it) => keyOf(it) === k) : -1;
      render();
    }

    // ---- vẽ
    function renderHead() {
      if (v.mode !== "details") { head.hidden = true; return; }
      head.hidden = false;
      head.style.gridTemplateColumns = tpl();
      head.replaceChildren();
      o.cols.forEach((c, i) => {
        const h = el("div", "xh" + (c.num ? " num" : ""), o.colTitle ? o.colTitle(c) : c.t);
        if (v.sort.k === c.k) h.appendChild(el("span", "xarrow", v.sort.d > 0 ? "▲" : "▼"));
        h.onclick = () => setSort(c.k);
        const rz = el("div", "rz");
        rz.title = "Kéo để đổi độ rộng cột";
        rz.onmousedown = (e) => {
          e.preventDefault();
          e.stopPropagation();
          const x0 = e.clientX, w0 = v.widths[i];
          const mv = (ev) => { v.widths[i] = Math.max(50, w0 + ev.clientX - x0); head.style.gridTemplateColumns = tpl(); render(); };
          const up = () => { removeEventListener("mousemove", mv); removeEventListener("mouseup", up); lsSet(o.id + ":colw", v.widths); };
          addEventListener("mousemove", mv);
          addEventListener("mouseup", up);
        };
        rz.onclick = (e) => e.stopPropagation();
        h.appendChild(rz);
        head.appendChild(h);
      });
    }
    function nameCell(it) {
      const nm = el("div", "xc nm");
      if (it.depth) nm.style.paddingLeft = 8 + it.depth * 18 + "px";
      if (it.nkids) {
        const tw = el("span", "xtw", v.collapsed.has(keyOf(it)) && !v.q ? "▸" : "▾");
        tw.dataset.tw = "1";
        nm.appendChild(tw);
      }
      nm.append(iconEl(it, 0), el("span", "t", it.name));
      return nm;
    }
    function render() {
      renderHead();
      root.classList.toggle("tiles", v.mode === "medium" || v.mode === "large");
      root.dataset.mode = v.mode;
      if (!v.items.length) {
        inner.style.height = inner.style.width = "";
        inner.replaceChildren(el("div", "empty", v.msg));
        o.onStatus && o.onStatus(api);
        return;
      }
      const g = geom();
      const frag = document.createDocumentFragment();
      const cls = (base, it, i) => base + (v.sel.has(keyOf(it)) ? " sel" : "") + (i === v.focus ? " foc" : "");
      if (g.kind === "rows") {
        inner.style.width = v.widths.reduce((a, b) => a + b, 0) + "px";
        inner.style.height = v.items.length * g.rh + "px";
        const top = root.scrollTop, h = root.clientHeight - g.head;
        const first = Math.max(0, Math.floor(top / g.rh) - 10), last = Math.min(v.items.length, Math.ceil((top + h) / g.rh) + 10);
        for (let i = first; i < last; i++) {
          const it = v.items[i];
          const r = el("div", cls("xrow", it, i));
          r.style.top = i * g.rh + "px";
          r.style.gridTemplateColumns = tpl();
          r.dataset.i = i;
          o.cols.forEach((c, ci) => {
            if (ci === 0) return r.appendChild(nameCell(it));
            r.appendChild(el("div", "xc" + (c.num ? " num" : "") + (c.mut ? " mut" : ""), o.cell(it, c.k)));
          });
          r.title = it.name + (it.info ? "\n" + it.info : "");
          frag.appendChild(r);
        }
      } else if (g.kind === "cols") {
        const ncol = Math.ceil(v.items.length / g.rows);
        inner.style.width = ncol * g.cw + "px";
        inner.style.height = g.rows * g.rh + "px";
        const left = root.scrollLeft, w = root.clientWidth;
        const c0 = Math.max(0, Math.floor(left / g.cw) - 1), c1 = Math.min(ncol, Math.ceil((left + w) / g.cw) + 1);
        for (let c = c0; c < c1; c++) {
          for (let k = 0; k < g.rows; k++) {
            const i = c * g.rows + k;
            if (i >= v.items.length) break;
            const it = v.items[i];
            const r = el("div", cls("xrow lst", it, i));
            r.style.left = c * g.cw + "px";
            r.style.top = k * g.rh + "px";
            r.style.width = g.cw - 6 + "px";
            r.dataset.i = i;
            r.appendChild(nameCell({ ...it, depth: 0, nkids: 0 }));
            r.title = it.name;
            frag.appendChild(r);
          }
        }
      } else {
        const rows = Math.ceil(v.items.length / g.per);
        inner.style.width = "";
        inner.style.height = rows * g.th + "px";
        const top = root.scrollTop, h = root.clientHeight;
        const r0 = Math.max(0, Math.floor(top / g.th) - 2), r1 = Math.min(rows, Math.ceil((top + h) / g.th) + 2);
        for (let rr = r0; rr < r1; rr++) {
          for (let c = 0; c < g.per; c++) {
            const i = rr * g.per + c;
            if (i >= v.items.length) break;
            const it = v.items[i];
            const t = el("div", cls("xtile", it, i));
            t.style.left = 4 + c * g.tw + "px";
            t.style.top = rr * g.th + "px";
            t.style.width = g.tw - 6 + "px";
            t.style.height = g.th - 6 + "px";
            t.dataset.i = i;
            t.append(iconEl(it, g.img), el("span", "t", it.name));
            t.title = it.name + "\n" + typeOf(it) + (it.kind === "dir" ? "" : " · " + size(it.size)) + (it.info ? "\n" + it.info : "");
            frag.appendChild(t);
          }
        }
      }
      inner.replaceChildren(frag);
      o.onStatus && o.onStatus(api);
    }
    const schedule = () => { if (!raf) raf = requestAnimationFrame(() => { raf = 0; render(); }); };
    root.addEventListener("scroll", schedule);
    new ResizeObserver(schedule).observe(root);

    function ensureVisible(i) {
      const g = geom();
      if (g.kind === "rows") {
        const top = i * g.rh, vh = root.clientHeight - g.head;
        if (top < root.scrollTop) root.scrollTop = top;
        else if (top + g.rh > root.scrollTop + vh) root.scrollTop = top + g.rh - vh;
      } else if (g.kind === "cols") {
        const left = Math.floor(i / g.rows) * g.cw;
        if (left < root.scrollLeft) root.scrollLeft = left;
        else if (left + g.cw > root.scrollLeft + root.clientWidth) root.scrollLeft = left + g.cw - root.clientWidth;
      } else {
        const top = Math.floor(i / g.per) * g.th;
        if (top < root.scrollTop) root.scrollTop = top;
        else if (top + g.th > root.scrollTop + root.clientHeight) root.scrollTop = top + g.th - root.clientHeight;
      }
    }

    // ---- chọn
    const changed = () => { render(); o.onSel(api); };
    function pick(i, { ctrl, shift } = {}) {
      if (i < 0 || i >= v.items.length) return;
      if (shift && v.anchor >= 0) {
        const [a, b] = v.anchor < i ? [v.anchor, i] : [i, v.anchor];
        if (!ctrl) v.sel.clear();
        for (let k = a; k <= b; k++) v.sel.add(keyOf(v.items[k]));
      } else if (ctrl) {
        const k = keyOf(v.items[i]);
        v.sel.has(k) ? v.sel.delete(k) : v.sel.add(k);
        v.anchor = i;
      } else {
        v.sel = new Set([keyOf(v.items[i])]);
        v.anchor = i;
      }
      v.focus = i;
      ensureVisible(i);
      changed();
    }
    function toggleKids(it, open) {
      const k = keyOf(it);
      const isOpen = !v.collapsed.has(k);
      if (open === isOpen) return;
      isOpen ? v.collapsed.delete(k) : null;
      if (isOpen) v.collapsed.add(k); else v.collapsed.delete(k);
      rearrange(k);
    }
    const idxOf = (e) => { const r = e.target.closest("[data-i]"); return r && root.contains(r) ? +r.dataset.i : -1; };

    root.addEventListener("mousedown", (e) => {
      if (e.button === 3 || e.button === 4 || e.target.closest(".xhead")) return;
      const i = idxOf(e);
      root.focus({ preventScroll: true });
      o.onFocus && o.onFocus(api);
      if (i < 0) {
        if (e.button === 0 && !e.ctrlKey && !e.shiftKey) { v.sel.clear(); v.focus = -1; changed(); }
        return;
      }
      if (e.button === 0 && e.target.dataset.tw) { toggleKids(v.items[i]); return; }
      if (e.button === 2) { if (!v.sel.has(keyOf(v.items[i]))) pick(i); else { v.focus = i; render(); } return; }
      // Nhấp đúp tự nhận ở lần nhấn thứ hai: mỗi lần chọn, danh sách vẽ lại các dòng nên sự kiện
      // "dblclick" của trình duyệt rơi vào khung chứa chứ không vào dòng.
      const now = performance.now();
      const dbl = e.button === 0 && !e.ctrlKey && !e.shiftKey && !e.metaKey && last.i === i &&
        now - last.t < 500 && Math.abs(e.clientX - last.x) < 6 && Math.abs(e.clientY - last.y) < 6;
      last = dbl ? { i: -1, t: 0, x: 0, y: 0 } : { i, t: now, x: e.clientX, y: e.clientY };
      if (dbl) { e.preventDefault(); o.onOpen(v.items[i], api); return; }
      if (e.button === 0) pick(i, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey });
    });
    root.addEventListener("contextmenu", (e) => {
      const i = idxOf(e);
      o.onMenu(e, api, i < 0 ? null : api.selected());
    });
    root.addEventListener("keydown", (e) => {
      if (o.onKey && o.onKey(e, api)) { e.preventDefault(); return; }
      const g = geom();
      const ctrl = e.ctrlKey || e.metaKey;
      const n = v.focus < 0 ? 0 : v.focus;
      const step = (d) => pick(Math.max(0, Math.min(v.items.length - 1, v.focus < 0 ? 0 : n + d)), { shift: e.shiftKey });
      const page = g.kind === "rows" ? Math.max(1, Math.floor((root.clientHeight - g.head) / g.rh) - 1) : g.kind === "tiles" ? g.per * Math.max(1, Math.floor(root.clientHeight / g.th)) : g.rows;
      const cur = v.items[v.focus];
      switch (e.key) {
        case "ArrowDown": step(g.kind === "tiles" ? g.per : 1); break;
        case "ArrowUp": step(g.kind === "tiles" ? -g.per : -1); break;
        case "ArrowRight":
          if (e.altKey) return;
          if (g.kind === "rows" && cur && cur.nkids && v.collapsed.has(keyOf(cur))) toggleKids(cur, true);
          else step(g.kind === "tiles" ? 1 : g.kind === "cols" ? g.rows : 0);
          break;
        case "ArrowLeft":
          if (e.altKey) return;
          if (g.kind === "rows" && cur && cur.nkids && !v.collapsed.has(keyOf(cur))) toggleKids(cur, false);
          else if (g.kind === "rows" && cur && cur.parent) pick(v.items.findIndex((x) => keyOf(x) === cur.parent));
          else step(g.kind === "tiles" ? -1 : g.kind === "cols" ? -g.rows : 0);
          break;
        case "PageDown": step(page); break;
        case "PageUp": step(-page); break;
        case "Home": pick(0, { shift: e.shiftKey }); break;
        case "End": pick(v.items.length - 1, { shift: e.shiftKey }); break;
        case "Enter": if (cur) o.onOpen(cur, api); break;
        case "a": case "A": if (!ctrl) return; v.sel = new Set(v.items.map(keyOf)); changed(); break;
        case "c": case "C": if (!ctrl) return; copyText(api.selected().map(fullPath).join("\n")); break;
        default: return;
      }
      e.preventDefault();
    });

    function setSort(k) {
      v.sort = v.sort.k === k ? { k, d: -v.sort.d } : { k, d: k === "size" || k === "mtime" ? -1 : 1 };
      lsSet(o.id + ":sort", v.sort);
      rearrange();
    }

    const api = {
      v, el: root, render, schedule, ensureVisible, setSort,
      selected: () => v.items.filter((it) => v.sel.has(keyOf(it))),
      focused: () => v.items[v.focus],
      setItems(all, opts = {}) {
        v.all = all;
        v.msg = opts.msg || "";
        v.sel = new Set(opts.sel || []);
        if (opts.resetCollapse) v.collapsed = new Set(opts.collapsed || []);
        arrange();
        v.focus = opts.focus != null && opts.focus < v.items.length ? opts.focus : v.sel.size ? v.items.findIndex((it) => v.sel.has(keyOf(it))) : -1;
        if (opts.selectKey) {
          const i = v.items.findIndex((it) => keyOf(it) === opts.selectKey);
          if (i >= 0) { v.sel = new Set([opts.selectKey]); v.focus = i; }
        }
        v.anchor = v.focus;
        if (!opts.keepScroll) root.scrollTop = root.scrollLeft = 0;
        if (v.focus >= 0) ensureVisible(v.focus);
        render();
      },
      clear(msg) { v.all = []; v.items = []; v.sel.clear(); v.focus = v.anchor = -1; v.msg = msg; root.scrollTop = 0; render(); },
      setFilter(q) { v.q = q; rearrange(); },
      setMode(m) {
        v.mode = m;
        lsSet(o.id + ":mode", m);
        root.scrollTop = root.scrollLeft = 0;
        render();
        if (v.focus >= 0) { ensureVisible(v.focus); render(); }
      },
      selectAll() { v.sel = new Set(v.items.map(keyOf)); changed(); },
      toggleKids,
      focus() { root.focus({ preventScroll: true }); },
    };
    return api;
  }

  // ================================================================ trạng thái chung
  let loc = null; // khung 2: {t:'dir'|'search', path, q?}
  const back = [], fwd = [];
  let gen = 0, pvGen = 0, boxGen = 0;
  let box = null; // tệp đang mở ở khung 3 {path, name, kind}
  const bundles = new Map(); // path -> {unity, entries}
  const fileKinds = new Map(); // path -> Promise<kind>; inspect a generic file only when selected
  let active = null; // khung danh sách được thao tác gần nhất
  let showPv = lsGet("xpv", true), showTree = lsGet("xtree", true);

  async function bundleInfo(path) {
    if (!bundles.has(path)) bundles.set(path, await invoke("bundle_list", { path, all: $("fall").checked }));
    return bundles.get(path);
  }

  // ---------------------------------------------------------------- khung 2: tệp và thư mục
  const P2 = makeView({
    el: $("xlist"), id: "x2", defMode: "details",
    cols: [
      { k: "name", t: "Tên", w: 320 },
      { k: "type", t: "Loại", w: 120 },
      { k: "size", t: "Kích thước", w: 86, num: true },
      { k: "mtime", t: "Ngày sửa", w: 132, mut: true },
      { k: "info", t: "Thư mục", w: 260, mut: true },
    ],
    colTitle: (c) => (c.k === "info" ? (loc && loc.t === "search" ? "Thư mục" : "Thông tin") : c.t),
    cell: (it, k) => (k === "type" ? typeOf(it) : k === "size" ? (it.kind === "dir" ? "" : size(it.size)) : k === "mtime" ? date(it.mtime) : k === "info" ? it.folder || "" : ""),
    onSel: (v) => { active = v; sel2(); },
    onFocus: (v) => { active = v; },
    onOpen: open2,
    onMenu: (e, v, chosen) => showCtx(e, chosen ? menu2(chosen) : bg2()),
    onKey: (e) => {
      if (e.key === "Backspace") { goBack(); return true; }
      return false;
    },
    onStatus: (v) => {
      const parts = [`${nf.format(v.v.items.length)} mục`];
      const ch = v.selected();
      if (ch.length) {
        const sum = ch.filter((it) => it.kind !== "dir").reduce((a, it) => a + (it.size || 0), 0);
        parts.push(`chọn ${nf.format(ch.length)}` + (sum ? ` · ${size(sum)}` : ""));
      }
      $("xstatus").textContent = parts.join("  ·  ");
      $("xextract").disabled = !(active || P2).selected().length;
    },
  });

  // ---------------------------------------------------------------- khung 3: nội dung bundle / bank
  const P3 = makeView({
    el: $("xbun"), id: "x3", defMode: "details",
    cols: [
      { k: "name", t: "Tên", w: 230 },
      { k: "type", t: "Loại", w: 130 },
      { k: "size", t: "Kích thước", w: 86, num: true },
      { k: "info", t: "Thông tin", w: 230, mut: true },
      { k: "pid", t: "PathID", w: 170, mut: true },
    ],
    cell: (it, k) => (k === "type" ? typeOf(it) : k === "size" ? size(it.size) : k === "info" ? it.info || "" : k === "pid" ? (it.id || "").replace(/^\d+:/, "") : ""),
    onSel: (v) => { active = v; showPreview(v.selected()); renderStatus3(); },
    onFocus: (v) => { active = v; },
    onOpen: (it, v) => {
      if (it.nkids) return v.toggleKids(it);
      if (!showPv) togglePv(true);
      showPreview([it], true);
    },
    onMenu: (e, v, chosen) => showCtx(e, chosen ? menu3(chosen) : bg3()),
    onStatus: () => renderStatus3(),
  });
  function renderStatus3() {
    const v = P3.v;
    const parts = [];
    if (box) {
      const b = bundles.get(box.path);
      parts.push(`${nf.format(v.items.length)} mục`);
      const ch = P3.selected();
      if (ch.length) parts.push(`chọn ${nf.format(ch.length)} · ${size(ch.reduce((a, it) => a + (it.size || 0), 0))}`);
      if (b && b.unity) parts.push("Unity " + b.unity);
    }
    $("x3status").textContent = parts.join("  ·  ");
    $("xextract").disabled = !(active || P2).selected().length;
  }

  function fromFs(e) {
    const search = loc && loc.t === "search";
    return { kind: e.kind, name: search ? basename(e.name) : e.name, path: e.path, size: e.size, mtime: e.mtime, folder: search ? parentDir(e.name) : "" };
  }
  function fromEntry(e, path) {
    return { kind: e.kind, name: e.name || `(${e.class})`, path, id: e.id, size: e.size, info: e.info, cls: e.class, ext: e.ext };
  }

  // nạp nội dung một bundle / bank vào khung 3
  let boxTimer = 0;
  function showBox(it, now) {
    clearTimeout(boxTimer);
    if (box && box.path === it.path) return;
    const run = async () => {
      const my = ++boxGen;
      box = { path: it.path, name: it.name, kind: it.kind };
      $("x3title").textContent = (ICON[it.kind] || "📦") + " " + it.name;
      $("x3title").title = it.path;
      P3.clear("Đang đọc " + it.name + "…");
      try {
        const b = await bundleInfo(it.path);
        if (my !== boxGen) return;
        const all = [];
        for (const e of b.entries) {
          const x = fromEntry(e, it.path);
          const parts = e.id.split(":");
          if (parts.length === 3 && parts[0] !== "s") {
            x.parent = it.path + "|" + parts.slice(0, 2).join(":");
            x.depth = 1;
            x.full = x.name;
            x.name = x.name.replace(/^.*? \/ /, "");
          }
          all.push(x);
        }
        P3.setItems(all, { msg: "Không có ảnh / âm thanh / video / text nào. Bật “Mọi đối tượng” để xem tất cả.", resetCollapse: true });
        // tự chọn tệp con đầu tiên (bỏ qua dòng FMOD bank, lấy âm thanh đầu tiên bên trong) và hiện nội dung ở khung 4;
        // con trỏ bàn phím vẫn ở khung 2 để lướt ↑/↓ qua các bundle
        const first = P3.v.items.find((x) => !x.nkids);
        if (first) {
          P3.v.sel = new Set([keyOf(first)]);
          P3.v.focus = P3.v.anchor = P3.v.items.indexOf(first);
          P3.ensureVisible(P3.v.focus);
          P3.render();
          showPreview([first]);
        } else {
          showPreview([it]);
        }
        renderStatus3();
      } catch (e) {
        if (my === boxGen) P3.clear("✖ " + e);
      }
    };
    if (now) run(); else boxTimer = setTimeout(run, 160); // tránh đọc bundle lớn khi đang lướt phím mũi tên
  }
  function clearBox(msg) {
    clearTimeout(boxTimer);
    boxGen++;
    box = null;
    $("x3title").textContent = "📦 Nội dung bundle";
    $("x3title").title = "";
    P3.clear(msg);
    renderStatus3();
  }
  const BOX_HINT = "Chọn một 📦 bundle Unity hoặc 🎵 FMOD bank ở khung bên trái — các tệp con bên trong sẽ hiện ở đây.";

  let selGen = 0;
  async function classifyFs(it) {
    if (!it || isAsset(it) || it.kind !== "file") return it;
    if (!fileKinds.has(it.path)) fileKinds.set(it.path, invoke("fs_classify", { path: it.path }));
    try { it.kind = await fileKinds.get(it.path); }
    catch (_) { /* keep it as a generic file so the regular preview can report the I/O error */ }
    return it;
  }
  async function sel2() {
    const ch = P2.selected();
    const mine = ++selGen;
    if (ch.length === 1 && ch[0].kind === "file") {
      const it = ch[0];
      clearBox(BOX_HINT);
      pvGen++;
      stopMedia();
      view.replaceChildren(el("div", "empty", "Đang nhận dạng tệp…"));
      await classifyFs(it);
      if (mine !== selGen || P2.selected().length !== 1 || P2.selected()[0].path !== it.path) return;
      P2.render();
    }
    if (ch.length === 1 && isBox(ch[0])) {
      // bundle đang mở sẵn ở khung giữa: giữ tệp con đang chọn; bundle mới: tạm hiện thông tin bundle
      // cho tới khi khung giữa nạp xong và tự chọn tệp con đầu tiên
      const same = box && box.path === ch[0].path && P3.selected().length;
      showBox(ch[0]);
      showPreview(same ? P3.selected() : ch);
      return;
    }
    if (!box || ch.length) clearBox(ch.length > 1 ? "Đã chọn nhiều mục." : BOX_HINT);
    showPreview(ch);
  }

  // ---------------------------------------------------------------- điều hướng khung 2
  function sameLoc(a, b) { return a && b && a.t === b.t && a.path === b.path && (a.q || "") === (b.q || ""); }
  async function go(l, opts = {}) {
    if (!l) return;
    if (!opts.hist && loc && !sameLoc(loc, l)) { back.push({ ...loc, sel: [...P2.v.sel], focus: P2.v.focus }); fwd.length = 0; }
    const my = ++gen;
    loc = l;
    if (l.t !== "search") $("fq").value = "";
    renderAddr();
    navButtons();
    P2.clear("Đang đọc…");
    try {
      const raw = l.t === "dir" ? await invoke("fs_list", { dir: l.path }) : await invoke("fs_search", { root: l.path, query: l.q });
      if (my !== gen) return;
      const items = raw.map(fromFs);
      P2.setItems(items, {
        msg: l.t === "search" ? "Không có tệp nào khớp." : "Thư mục này trống.",
        sel: opts.sel, focus: opts.focus, selectKey: opts.selectKey,
      });
      active = P2;
      sel2();
      if (l.t === "dir" && !opts.skipTreeReveal) treeReveal(l.path); else renderTree();
      if (l.t === "search") status(raw.length >= 500 ? "Chỉ hiện 500 kết quả đầu" : `${raw.length} tệp khớp`, "");
    } catch (e) {
      if (my === gen) P2.clear("✖ " + e);
    }
  }
  function goBack() { const l = back.pop(); if (!l) return; fwd.push({ ...loc, sel: [...P2.v.sel], focus: P2.v.focus }); go(l, { hist: true, sel: l.sel, focus: l.focus }); }
  function goFwd() { const l = fwd.pop(); if (!l) return; back.push({ ...loc, sel: [...P2.v.sel], focus: P2.v.focus }); go(l, { hist: true, sel: l.sel, focus: l.focus }); }
  function upLoc(l) {
    if (!l) return null;
    if (l.t === "search") return { t: "dir", path: l.path };
    const p = parentDir(l.path);
    return p ? { t: "dir", path: p, child: l.path + "|" } : null;
  }
  function goUp() { const u = upLoc(loc); if (u) go({ t: u.t, path: u.path }, { selectKey: u.child }); }
  function navButtons() {
    $("xback").disabled = !back.length;
    $("xfwd").disabled = !fwd.length;
    $("xup").disabled = !upLoc(loc);
  }
  async function open2(it) {
    if (!it) return;
    if (it.kind === "dir") return go({ t: "dir", path: it.path });
    await classifyFs(it);
    if (isBox(it)) {
      // mở ngay ở khung giữa và chuyển con trỏ sang đó
      showBox(it, true);
      setTimeout(() => { P3.focus(); active = P3; }, 0);
      return;
    }
    if (!showPv) togglePv(true);
    showPreview([it], true);
  }

  // ---------------------------------------------------------------- thanh địa chỉ
  function renderAddr() {
    addr.replaceChildren();
    if (!loc) return;
    const parts = loc.path.split(/[\\/]+/).filter(Boolean);
    let acc = "";
    parts.forEach((p, i) => {
      acc = acc ? acc + "\\" + p : p.endsWith(":") ? p + "\\" : p;
      if (i) addr.appendChild(el("span", "xsep", "›"));
      const target = acc;
      const b = el("button", "xcrumb", p);
      b.title = target;
      b.onclick = (e) => { e.stopPropagation(); go({ t: "dir", path: target }); };
      addr.appendChild(b);
    });
    if (loc.t === "search") {
      addr.appendChild(el("span", "xsep", "›"));
      addr.appendChild(el("span", "xcrumb", `🔍 “${loc.q}”`));
    }
    requestAnimationFrame(() => (addr.scrollLeft = addr.scrollWidth));
  }
  addr.addEventListener("click", (e) => { if (e.target === addr) editAddr(); });
  function editAddr() {
    const inp = el("input", "xaddrin");
    inp.value = loc ? loc.path : val("work");
    inp.spellcheck = false;
    addr.replaceChildren(inp);
    inp.focus();
    inp.select();
    let done = false;
    const end = async (ok) => {
      if (done) return;
      done = true;
      const p = inp.value.trim().replace(/^"|"$/g, "");
      if (ok && p) {
        try {
          await invoke("fs_list", { dir: p });
          return go({ t: "dir", path: p });
        } catch (_) {
          const par = parentDir(p);
          try {
            const sib = par ? await invoke("fs_list", { dir: par }) : [];
            const f = sib.find((x) => x.path.toLowerCase() === p.toLowerCase());
            if (f) return go({ t: "dir", path: par }, { selectKey: f.path + "|" });
          } catch (_) {}
          status("Không tìm thấy: " + p, "err");
        }
      }
      renderAddr();
    };
    inp.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") end(true); if (e.key === "Escape") end(false); };
    inp.onblur = () => end(false);
  }

  // ---------------------------------------------------------------- menu
  function openWin(it) { invoke("open_path", { path: it.path }).catch((e) => log("✖ " + e)); }
  function menu2(chosen) {
    const one = chosen.length === 1 ? chosen[0] : null;
    const m = [];
    if (one && one.kind === "dir") m.push({ label: "Mở", bold: true, kbd: "Enter", run: () => open2(one) });
    if (one && isBox(one)) m.push({ label: "Xem tệp con (khung giữa)", bold: true, kbd: "Enter", run: () => open2(one) });
    if (one && one.kind !== "dir" && !isBox(one)) m.push({ label: "Xem nội dung", bold: true, kbd: "Enter", run: () => open2(one) });
    if (one) m.push({ label: one.kind === "dir" ? "Mở trong Windows Explorer" : "Mở bằng ứng dụng mặc định", run: () => openWin(one) });
    if (one && loc.t === "search") m.push({ label: "Mở thư mục chứa tệp", run: () => go({ t: "dir", path: parentDir(one.path) }, { selectKey: keyOf(one) }) });
    if (m.length) m.push("-");
    m.push({ label: chosen.length > 1 ? `Trích xuất ${chosen.length} mục…` : one.kind === "dir" || isBox(one) ? "Trích xuất hàng loạt từ mục này…" : "Lưu bản sao…", run: () => extractSel(chosen) });
    m.push("-", { label: "Sao chép đường dẫn", kbd: "Ctrl+C", run: () => copyText(chosen.map(fullPath).join("\n")) });
    m.push({ label: "Sao chép tên", run: () => copyText(chosen.map((it) => it.name).join("\n")) });
    if (one) m.push({ label: "Mở vị trí trong Explorer", run: () => invoke("open_path", { path: one.kind === "dir" ? one.path : parentDir(one.path) }).catch((e) => log("✖ " + e)) });
    return m;
  }
  function modeItems(api, sel, modes) {
    return modes.map(([v, t]) => ({ label: (api.v.mode === v ? "● " : "    ") + t, run: () => { api.setMode(v); $(sel).value = v; } }));
  }
  const MODES2 = [["details", "Chi tiết"], ["large", "Biểu tượng lớn"], ["medium", "Biểu tượng vừa"], ["list", "Danh sách"]];
  const MODES3 = MODES2.slice(0, 3);
  function bg2() {
    return [
      ...modeItems(P2, "xmode", MODES2),
      "-",
      { label: "Sắp xếp theo tên", run: () => P2.setSort("name") },
      { label: "Sắp xếp theo kích thước", run: () => P2.setSort("size") },
      { label: "Sắp xếp theo ngày sửa", run: () => P2.setSort("mtime") },
      "-",
      { label: "Làm mới", kbd: "F5", run: refresh },
      { label: "Chọn tất cả", kbd: "Ctrl+A", run: () => P2.selectAll() },
      ...(loc && loc.t === "dir" ? [{ label: "Trích xuất hàng loạt từ thư mục này…", run: () => batchDialog([loc.path]) }] : []),
    ];
  }
  function menu3(chosen) {
    const one = chosen.length === 1 ? chosen[0] : null;
    const m = [];
    if (one && one.nkids) m.push({ label: P3.v.collapsed.has(keyOf(one)) ? "Mở rộng" : "Thu gọn", bold: true, kbd: "Enter", run: () => P3.toggleKids(one) });
    if (one && !one.nkids) m.push({ label: "Xem nội dung", bold: true, kbd: "Enter", run: () => { if (!showPv) togglePv(true); showPreview([one], true); } });
    if (m.length) m.push("-");
    m.push({ label: chosen.length > 1 ? `Trích xuất ${chosen.length} mục…` : one && one.nkids ? `Trích xuất ${one.nkids} âm thanh trong bank…` : "Trích xuất…", run: () => extractSel(chosen) });
    m.push("-", { label: "Sao chép tên", run: () => copyText(chosen.map((it) => it.full || it.name).join("\n")) });
    m.push({ label: "Sao chép đường dẫn", kbd: "Ctrl+C", run: () => copyText(chosen.map(fullPath).join("\n")) });
    return m;
  }
  function bg3() {
    const m = [...modeItems(P3, "x3mode", MODES3), "-", { label: "Sắp xếp theo tên", run: () => P3.setSort("name") }, { label: "Sắp xếp theo loại", run: () => P3.setSort("type") }, { label: "Sắp xếp theo kích thước", run: () => P3.setSort("size") }];
    if (box) m.push("-", { label: "Chọn tất cả", kbd: "Ctrl+A", run: () => P3.selectAll() }, { label: "Trích xuất hàng loạt từ tệp này…", run: () => batchDialog([box.path]) });
    return m;
  }

  // ---------------------------------------------------------------- trích xuất
  async function extractSel(chosen) {
    if (!chosen.length) return;
    const disk = chosen.filter((it) => !isAsset(it));
    if (disk.length) {
      if (disk.length === 1 && disk[0].kind !== "dir" && !isBox(disk[0])) return saveOne(disk[0]);
      return batchDialog(disk.map((it) => it.path));
    }
    // đối tượng trong bundle: bank → các âm thanh con
    let arr = [];
    for (const it of chosen) {
      if (it.nkids) arr.push(...P3.v.all.filter((k) => k.parent === keyOf(it)));
      else arr.push(it);
    }
    arr = [...new Map(arr.map((it) => [keyOf(it), it])).values()];
    if (arr.length === 1) return saveOne(arr[0]);
    return saveAssets(arr);
  }

  async function saveAssets(arr) {
    if (!arr.length) return;
    const dir = await dialog.open({ directory: true, title: `Chọn thư mục lưu ${arr.length} mục` });
    if (!dir) return;
    await run(`Trích xuất ${arr.length} mục`, async () => {
      const used = new Set();
      let ok = 0, bytes = 0;
      for (const it of arr) {
        let name = (it.full || it.name).replace(/ \/ /g, "__").replace(/[\\/:*?"<>|]/g, "_");
        if (it.ext && !name.toLowerCase().endsWith("." + it.ext)) name += "." + it.ext;
        let n = name, k = 2;
        while (used.has(n.toLowerCase())) n = name.replace(/(\.[^.]*)?$/, `_${k++}$1`);
        used.add(n.toLowerCase());
        try { bytes += await invoke("asset_save", { path: it.path, id: it.id, dest: dir + "\\" + n }); ok++; }
        catch (e) { log(`  ! ${it.name}: ${e}`); }
      }
      log(`Đã lưu ${ok}/${arr.length} mục (${size(bytes)}) vào ${dir}.`);
      invoke("open_path", { path: dir }).catch(() => {});
    });
  }

  async function saveOne(n) {
    let name = isAsset(n) ? (n.full || n.name).replace(/ \/ /g, "__").replace(/[\\/:*?"<>|]/g, "_") : basename(n.path);
    const ext = isAsset(n) ? n.ext : "";
    if (ext && !name.toLowerCase().endsWith("." + ext)) name += "." + ext;
    const lastDir = lsGet("fsavedir", "");
    const dest = await dialog.save({ defaultPath: (lastDir ? lastDir + "\\" : "") + name, title: "Lưu thành" });
    if (!dest) return;
    lsSet("fsavedir", parentDir(dest));
    await run("Trích xuất " + name, async () => {
      const bytes = await invoke("asset_save", { path: n.path, id: n.id || "", dest });
      log(`Đã lưu ${dest} (${size(bytes)}).`);
    });
  }

  function batchDialog(srcs) {
    if (!need("work")) return;
    const wrap = el("div");
    const scope = el("div", "fld");
    scope.append(el("label", "", "Nguồn"));
    scope.appendChild(el("div", "sub", srcs ? (srcs.length === 1 ? srcs[0] : `${srcs.length} mục đã chọn`) : "Toàn bộ game đã giải nén: mọi thư mục apk_* trong thư mục làm việc"));
    wrap.appendChild(scope);
    const saved = lsGet("fkinds", { text: true, image: true, sprite: false, audio: true, video: true });
    const boxes = {};
    const kf = el("div", "fld");
    kf.appendChild(el("label", "", "Loại cần trích xuất"));
    for (const [k, label] of KINDS) {
      const l = el("label", "chk");
      const c = el("input");
      c.type = "checkbox";
      c.checked = !!saved[k];
      boxes[k] = c;
      l.append(c, document.createTextNode(" " + label));
      kf.appendChild(l);
    }
    wrap.appendChild(kf);
    const df = el("div", "fld");
    df.appendChild(el("label", "", "Thư mục đích"));
    const dst = el("input");
    dst.value = lsGet("fdest", "") || val("work") + "\\extract";
    dst.spellcheck = false;
    const row = el("div", "row");
    row.append(dst, btn("Chọn…", "ghost", async () => { const r = await dialog.open({ directory: true }); if (r) dst.value = r; }));
    const note = el("div", "sub");
    df.append(row, note);
    wrap.appendChild(df);
    // cách xếp tệp: tách theo thư mục gốc (mặc định) hoặc gom chung một thư mục
    const layout = el("div", "xlayout");
    layout.setAttribute("role", "radiogroup");
    const radio = (value, text, title) => {
      const l = el("label", "chk");
      l.title = title;
      const r = el("input");
      r.type = "radio";
      r.name = "xlayout";
      r.value = value;
      l.append(r, document.createTextNode(" " + text));
      layout.appendChild(l);
      return r;
    };
    const rSplit = radio("split", "Xuất tách theo thư mục gốc", "Mỗi bundle ra một thư mục con mang tên bundle");
    const rFlat = radio("flat", "Xuất chung 1 thư mục", "Gom mọi tệp vào thẳng thư mục đích");
    rSplit.checked = true;
    const syncNote = () => {
      note.textContent = rFlat.checked
        ? "Mọi tệp được gom vào thẳng thư mục đích. Tệp trùng tên giữa các bundle được thêm _2, _3…; tệp đã có sẵn trong thư mục đích sẽ bị ghi đè."
        : "Mỗi bundle ra một thư mục con mang tên bundle; tệp media rời nằm trong _files. Tệp trùng tên sẽ bị ghi đè.";
    };
    rSplit.onchange = rFlat.onchange = syncNote;
    syncNote();
    modal("Trích xuất hàng loạt", wrap, [
      layout,
      btn("Huỷ", "ghost", closeModal),
      btn("Trích xuất", "act", async () => {
        const kinds = {};
        for (const [k] of KINDS) kinds[k] = boxes[k].checked;
        if (!Object.values(kinds).some(Boolean)) return status("Chưa chọn loại nào để trích xuất", "err");
        const dest = dst.value.trim();
        if (!dest) return dst.focus();
        lsSet("fkinds", kinds);
        lsSet("fdest", dest);
        closeModal();
        await run("Trích xuất hàng loạt", async () => {
          let list = srcs;
          if (!list) {
            const top = await invoke("fs_list", { dir: val("work") });
            list = top.filter((e) => e.dir && /^apk_/i.test(e.name)).map((e) => e.path);
            if (!list.length) throw "Chưa có thư mục apk_* nào — hãy chạy bước 1 (Giải nén) trước.";
          }
          const flat = rFlat.checked;
          log(flat ? "Chế độ: xuất chung 1 thư mục." : "Chế độ: xuất tách theo thư mục gốc.");
          const st = await invoke("extract_assets", { srcs: list, dest, kinds, flat });
          log(`Đã trích xuất ${nf.format(st.written)} tệp (${size(st.bytes)}) vào ${st.dest}${st.errors ? ` — ${st.errors} mục lỗi, xem ở trên` : ""}.`);
          logEvent({ type: "extract", detail: `${st.written} tệp → ${st.dest}` });
          invoke("open_path", { path: st.dest }).catch(() => {});
        });
      }),
    ]);
  }

  // ---------------------------------------------------------------- khung 4: xem nội dung
  function pvHead(n) {
    const h = el("div", "fhead");
    const t = el("div", "ftitle");
    t.append(iconEl(n, 0), el("b", "", n.full || n.name));
    t.title = n.full || n.name;
    const p = el("input", "fpath");
    p.readOnly = true;
    p.value = n.path;
    p.title = "Đường dẫn trên đĩa";
    h.append(t, el("div", "fmeta", typeOf(n)), p);
    const meta = [];
    if (isAsset(n) && !n.id.startsWith("s:")) meta.push(`PathID ${n.id.replace(/^\d+:/, "")}`);
    if (n.info) meta.push(n.info);
    if (n.kind !== "dir") meta.push(size(n.size));
    if (n.mtime) meta.push("Sửa " + date(n.mtime));
    if (meta.length) h.appendChild(el("div", "fmeta", meta.join(" · ")));
    const row = el("div", "row wrap");
    if (n.kind === "dir" || isBox(n)) row.append(btn("Trích xuất hàng loạt…", "act small2", () => batchDialog([n.path])));
    else row.append(btn(isAsset(n) ? "⇩ Trích xuất…" : "⇩ Lưu bản sao…", "act small2", () => extractSel([n])));
    if (!isAsset(n)) row.append(btn(n.kind === "dir" ? "Mở bằng Explorer" : "Mở bằng Windows", "ghost small2", () => openWin(n)));
    row.append(btn("Sao chép đường dẫn", "ghost small2", () => copyText(fullPath(n))));
    h.appendChild(row);
    return h;
  }

  function stopMedia() { for (const m of view.querySelectorAll("audio, video")) { m.pause(); m.removeAttribute("src"); m.load(); } }

  async function showPreview(chosen, autoplay) {
    if (!showPv) return;
    const my = ++pvGen;
    stopMedia();
    if (chosen.length !== 1) {
      const msg = chosen.length
        ? `Đã chọn ${chosen.length} mục.`
        : "Chọn một tệp ở khung bên trái, hoặc một tệp con trong bundle ở khung giữa, để xem nội dung: văn bản, hình ảnh, âm thanh, video.";
      const e = el("div", "empty", msg);
      const parts = [e];
      if (chosen.length > 1) parts.push(btn(`⇩ Trích xuất ${chosen.length} mục…`, "act", () => extractSel(chosen)));
      view.replaceChildren(...parts);
      return;
    }
    const n = chosen[0];
    const body = el("div", "fbody");
    view.replaceChildren(pvHead(n), body);
    const put = (...c) => { if (my === pvGen) body.replaceChildren(...c); };
    const fail = (e) => put(el("div", "empty err", "✖ " + e));
    let kind = n.kind;
    try {
      if (kind === "dir") return put(el("div", "empty", "Thư mục — nhấp đúp để mở."));
      if (n.nkids) return put(el("div", "empty", `FMOD bank chứa ${n.nkids} âm thanh — chọn từng âm thanh bên dưới bank để nghe.`));
      if (isBox(n)) {
        const b = bundles.get(n.path);
        if (!b) return put(el("div", "empty", "Các tệp con hiện ở khung giữa."));
        const by = {};
        for (const e of b.entries) by[e.kind] = (by[e.kind] || 0) + 1;
        const tags = el("div", "facts");
        if (b.unity) tags.appendChild(el("span", "tag", "Unity " + b.unity));
        for (const [k, c] of Object.entries(by)) tags.appendChild(el("span", "tag", `${ICON[k] || ""} ${KIND_VI[k] || k}: ${c}`));
        return put(tags, el("p", "muted hint", "Các tệp con hiện ở khung giữa — chọn một mục ở đó để xem nội dung."));
      }
      let text = null;
      if (!isAsset(n) && kind === "file") {
        put(el("div", "empty", "Đang đọc…"));
        text = await invoke("fs_preview", { path: n.path });
        kind = text.kind;
      } else if (isAsset(n) && ["text", "table", "data", "other", "font"].includes(kind)) {
        put(el("div", "empty", "Đang đọc…"));
        text = await invoke("asset_preview", { path: n.path, id: n.id });
        kind = text.kind;
      }
      if (my !== pvGen) return;
      const url = gfs(n.path, n.id);
      if (kind === "image" || kind === "sprite") {
        const b = el("div", "fimg");
        const img = el("img");
        img.alt = n.name;
        const dim = el("div", "fmeta", "Đang giải mã…");
        img.onload = () => { if (my === pvGen) dim.textContent = `${img.naturalWidth} × ${img.naturalHeight} px — nhấp vào ảnh để xem cỡ thật / vừa khung`; };
        img.onerror = async () => {
          let why = "Không giải mã được ảnh này.";
          try { const r = await fetch(url); if (!r.ok) why = await r.text(); } catch (_) {}
          fail(why);
        };
        img.onclick = () => b.classList.toggle("real");
        img.src = url;
        b.appendChild(img);
        put(b, dim);
      } else if (kind === "audio" || kind === "video") {
        const m = el(kind);
        m.controls = true;
        m.preload = "metadata";
        m.autoplay = !!autoplay;
        m.onerror = async () => {
          let why = "Không phát được tệp này trong ứng dụng — thử “Mở bằng Windows”.";
          try { const r = await fetch(url, { headers: { Range: "bytes=0-0" } }); if (!r.ok) why = await r.text(); } catch (_) {}
          fail(why);
        };
        m.src = url;
        const b = el("div", kind === "video" ? "fvideo" : "faudio");
        b.appendChild(m);
        put(b);
      } else if (text) {
        const pre = el("pre", "ftext" + (kind === "hex" ? " hex" : ""), text.text);
        const note = [];
        if (kind === "hex") note.push("Tệp nhị phân — hiển thị dạng hex");
        if (n.kind === "table") note.push("Bảng FlatBuffers dạng TSV (chỉ cột chuỗi). Để sửa, mở bảng từ tab “Kết quả quét”.");
        if (text.truncated) note.push(`chỉ hiện phần đầu (${size(kind === "hex" ? 4096 : 409600)} / ${size(text.size)})`);
        put(...(note.length ? [el("div", "fmeta", note.join(" · "))] : []), pre);
      } else {
        put(el("div", "empty", "Không có bản xem trước cho loại này."));
      }
    } catch (e) {
      fail(e);
    }
  }

  // ---------------------------------------------------------------- cây thư mục
  let tnodes = [];
  const TRH = 24;
  function tmk(e, depth, parent) { return { name: e.name, path: e.path, depth, parent, open: false, kids: null }; }
  async function tkids(n) {
    if (!n.kids) n.kids = (await invoke("fs_list", { dir: n.path })).filter((e) => e.dir).map((e) => tmk(e, n.depth + 1, n));
    return n.kids;
  }
  async function ttoggle(n, force) {
    const open = force ?? !n.open;
    if (n.toggleTask) {
      await n.toggleTask;
      if (open !== n.open) return ttoggle(n, open);
      return;
    }
    if (open === n.open) return;
    const i = tnodes.indexOf(n);
    if (i < 0) return;
    n.toggleTask = (async () => {
      if (open) {
        n.loading = true; renderTree();
        try { await tkids(n); } catch (_) { n.kids = []; }
        n.loading = false;
        n.open = true;
        const at = tnodes.indexOf(n);
        if (at >= 0) tnodes.splice(at + 1, 0, ...n.kids);
      } else {
        const at = tnodes.indexOf(n);
        if (at >= 0) {
          let j = at + 1;
          while (j < tnodes.length && tnodes[j].depth > n.depth) j++;
          for (const k of tnodes.slice(at + 1, j)) k.open = false;
          tnodes.splice(at + 1, j - at - 1);
        }
        n.open = false;
      }
      renderTree();
    })();
    try { await n.toggleTask; } finally { n.toggleTask = null; }
  }
  async function treeRoot() {
    const w = val("work");
    tnodes = w ? [{ name: basename(w) || w, path: w, depth: 0, parent: null, open: false, kids: null, root: true }] : [];
    renderTree();
    if (tnodes[0]) await ttoggle(tnodes[0], true);
  }
  async function treeReveal(path) {
    const r = tnodes[0];
    if (!r) return renderTree();
    const lp = path.toLowerCase(), lr = r.path.toLowerCase().replace(/[\\/]+$/, "");
    if (lp !== lr && !lp.startsWith(lr + "\\")) return renderTree();
    let n = r;
    while (n.path.toLowerCase() !== lp) {
      if (!n.open) await ttoggle(n, true);
      const next = (n.kids || []).find((k) => lp === k.path.toLowerCase() || lp.startsWith(k.path.toLowerCase() + "\\"));
      if (!next) break;
      n = next;
    }
    const i = tnodes.indexOf(n);
    if (i >= 0) {
      const top = i * TRH;
      if (top < tree.scrollTop || top > tree.scrollTop + tree.clientHeight - TRH) tree.scrollTop = Math.max(0, top - tree.clientHeight / 2);
    }
    renderTree();
  }
  function renderTree() {
    if (!tnodes.length) { tree.replaceChildren(el("div", "empty", "Chưa chọn thư mục làm việc.")); return; }
    let tin = tree.firstElementChild;
    if (!tin || !tin.classList.contains("ftin")) { tin = el("div", "ftin"); tree.replaceChildren(tin); }
    tin.style.height = tnodes.length * TRH + "px";
    const cur = loc && loc.t === "dir" ? loc.path.toLowerCase() : "";
    const first = Math.max(0, Math.floor(tree.scrollTop / TRH) - 8), last = Math.min(tnodes.length, Math.ceil((tree.scrollTop + tree.clientHeight) / TRH) + 8);
    const frag = document.createDocumentFragment();
    for (let i = first; i < last; i++) {
      const n = tnodes[i];
      const r = el("div", "frow" + (n.path.toLowerCase() === cur ? " sel" : ""));
      r.style.top = i * TRH + "px";
      r.style.paddingLeft = 4 + n.depth * 14 + "px";
      r.dataset.i = i;
      r.title = n.path;
      r.append(el("span", "tw", n.loading ? "…" : n.kids && !n.kids.length ? "" : n.open ? "▾" : "▸"), el("span", "ic", n.root ? "🗂️" : n.open ? "📂" : "📁"), el("span", "nm", n.name));
      frag.appendChild(r);
    }
    tin.replaceChildren(frag);
  }
  tree.addEventListener("scroll", () => requestAnimationFrame(renderTree));
  new ResizeObserver(() => renderTree()).observe(tree);
  tree.addEventListener("click", (e) => {
    const r = e.target.closest(".frow");
    if (!r) return;
    const n = tnodes[+r.dataset.i];
    if (e.target.classList.contains("tw")) return ttoggle(n);
    // Node được mở ngay bên dưới; tránh go() gọi treeReveal() song song,
    // khiến hai ttoggle() cùng node có thể chèn trùng con vào cây.
    go({ t: "dir", path: n.path }, { skipTreeReveal: true });
    if (!n.open) ttoggle(n, true);
  });
  tree.addEventListener("contextmenu", (e) => {
    const r = e.target.closest(".frow");
    if (!r) return;
    const n = tnodes[+r.dataset.i];
    showCtx(e, [
      { label: "Mở", bold: true, run: () => go({ t: "dir", path: n.path }) },
      { label: n.open ? "Thu gọn" : "Mở rộng", run: () => ttoggle(n) },
      { label: "Trích xuất hàng loạt từ thư mục này…", run: () => batchDialog([n.path]) },
      "-",
      { label: "Mở trong Windows Explorer", run: () => invoke("open_path", { path: n.path }).catch((er) => log("✖ " + er)) },
      { label: "Sao chép đường dẫn", run: () => copyText(n.path) },
    ]);
  });

  // ---------------------------------------------------------------- co giãn các khung
  // [cây, khung 3, khung 4]; khung 2 tự nhận phần rộng còn lại, điều chỉnh qua hai splitter kề bên.
  let panes = lsGet("xpanes3", [200, 380, 400]);
  if (!Array.isArray(panes) || panes.length !== 3) panes = [200, 380, 400];
  const GUT = 6, TREE_MIN = 120;
  function avail() { return split.clientWidth; }
  function fitPanes(changed) {
    const W = avail();
    if (!W) return;
    const mins = [showTree ? TREE_MIN : 0, MINW, showPv ? MINW : 0];
    const eff = [showTree ? Math.max(TREE_MIN, panes[0]) : 0,
      Math.max(MINW, panes[1]), showPv ? Math.max(MINW, panes[2]) : 0];
    const gutters = GUT * (1 + (showTree ? 1 : 0) + (showPv ? 1 : 0));
    let over = Math.max(0, eff.reduce((a, b) => a + b, 0) + gutters + MINW - W);
    const order = changed === 0 ? [1, 2, 0] : changed === 1 ? [0, 2, 1] : changed === 2 ? [0, 1, 2] : [2, 1, 0];
    for (const i of order) {
      const take = Math.min(over, Math.max(0, eff[i] - mins[i]));
      eff[i] -= take; over -= take;
    }
    ["--tw", "--mw", "--pw"].forEach((name, i) => split.style.setProperty(name, `${Math.round(eff[i])}px`));
  }
  new ResizeObserver(() => fitPanes()).observe(split);
  for (const g of split.querySelectorAll(".xgut")) {
    g.onmousedown = (e) => {
      e.preventDefault();
      const k = +g.dataset.g, x0 = e.clientX;
      const hiddenTree = !showTree;
      const left = k === 0 ? 0 : 1;
      const start = [tree.clientWidth, $("xp3").clientWidth, view.clientWidth];
      if (k === 0 && hiddenTree) return;
      g.classList.add("drag");
      document.body.classList.add("xdragging");
      const mv = (ev) => {
        const delta = ev.clientX - x0;
        if (k === 0) {
          const maxTree = Math.max(TREE_MIN, avail() - GUT * (2 + (showPv ? 1 : 0)) - start[1] - (showPv ? start[2] : 0) - MINW);
          panes[0] = Math.round(Math.max(TREE_MIN, Math.min(maxTree, start[0] + delta)));
        } else if (k === 1) {
          const maxBundle = Math.max(MINW, avail() - GUT * (1 + (showTree ? 1 : 0) + (showPv ? 1 : 0)) - (showTree ? start[0] : 0) - (showPv ? start[2] : 0) - MINW);
          panes[1] = Math.round(Math.max(MINW, Math.min(maxBundle, start[1] - delta)));
        } else {
          const pair = start[1] + start[2], lo = MINW, hi = pair - MINW;
          panes[1] = Math.round(Math.max(lo, Math.min(hi, start[1] + delta)));
          panes[2] = Math.round(pair - panes[1]);
        }
        fitPanes(left);
        P2.schedule(); P3.schedule();
      };
      const up = () => {
        g.classList.remove("drag");
        document.body.classList.remove("xdragging");
        try { getSelection().removeAllRanges(); } catch (_) {}
        removeEventListener("mousemove", mv);
        removeEventListener("mouseup", up);
        lsSet("xpanes3", panes);
      };
      addEventListener("mousemove", mv);
      addEventListener("mouseup", up);
    };
    g.ondblclick = () => { panes = [200, 380, 400]; lsSet("xpanes3", panes); fitPanes(); };
  }

  function togglePv(on) {
    showPv = on ?? !showPv;
    lsSet("xpv", showPv);
    split.classList.toggle("nopv", !showPv);
    $("xpv").classList.toggle("on", showPv);
    fitPanes();
    if (showPv) showPreview((active || P2).selected()); else stopMedia();
  }
  function toggleTree(on) {
    showTree = on ?? !showTree;
    lsSet("xtree", showTree);
    split.classList.toggle("notree", !showTree);
    $("xtree").classList.toggle("on", showTree);
    fitPanes();
  }

  // ---------------------------------------------------------------- tìm kiếm / nút
  let deb = 0;
  function search() {
    const q = $("fq").value.trim();
    if (!q) { if (loc && loc.t === "search") goBack(); return; }
    if (!val("work")) return;
    const l = { t: "search", path: val("work"), q };
    if (loc && loc.t === "search") go(l, { hist: true }); else go(l);
  }
  $("fq").addEventListener("input", () => { clearTimeout(deb); deb = setTimeout(search, 400); });
  $("fq").addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") { clearTimeout(deb); search(); }
    if (e.key === "Escape") { $("fq").value = ""; search(); }
  });
  let deb3 = 0;
  $("x3q").addEventListener("input", () => { clearTimeout(deb3); deb3 = setTimeout(() => P3.setFilter($("x3q").value.trim()), 150); });
  $("x3q").addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") { $("x3q").value = ""; P3.setFilter(""); }
    if (e.key === "ArrowDown") { e.preventDefault(); P3.focus(); }
  });

  function refresh() {
    const keep = box;
    bundles.clear();
    fileKinds.clear();
    box = null;
    if (loc) go(loc, { hist: true, sel: [...P2.v.sel], focus: P2.v.focus });
    else if (keep) showBox(keep, true);
  }

  $("xback").onclick = goBack;
  $("xfwd").onclick = goFwd;
  $("xup").onclick = goUp;
  $("xreload").onclick = refresh;
  $("xpv").onclick = () => togglePv();
  $("xtree").onclick = () => toggleTree();
  $("xmode").value = P2.v.mode;
  $("xmode").onchange = () => P2.setMode($("xmode").value);
  $("x3mode").value = P3.v.mode === "list" ? "details" : P3.v.mode;
  $("x3mode").onchange = () => P3.setMode($("x3mode").value);
  $("xextract").onclick = () => extractSel((active || P2).selected());
  $("fbatch").onclick = () => batchDialog(null);
  $("xopen").onclick = () => {
    const ch = P2.selected();
    const one = ch.length === 1 ? ch[0] : null;
    invoke("open_path", { path: one ? one.path : loc && loc.t === "dir" ? loc.path : val("work") }).catch((e) => log("✖ " + e));
  };
  $("fall").onchange = () => {
    bundles.clear();
    const keep = box;
    box = null;
    if (keep) showBox(keep, true);
  };

  // nút chuột Back/Forward
  document.querySelector('[data-pane="files"]').addEventListener("mouseup", (e) => {
    if (e.button === 3) { e.preventDefault(); goBack(); }
    if (e.button === 4) { e.preventDefault(); goFwd(); }
  });
  document.addEventListener("keydown", (e) => {
    if (activeKey !== "files" || !$("mask").hidden) return;
    if (e.target.closest("input, textarea, select") && e.key !== "F5") return;
    if (e.altKey && e.key === "ArrowLeft") goBack();
    else if (e.altKey && e.key === "ArrowRight") goFwd();
    else if (e.altKey && e.key === "ArrowUp") goUp();
    else if (e.altKey && (e.key === "p" || e.key === "P")) togglePv();
    else if (e.key === "F5") refresh();
    else if ((e.ctrlKey || e.metaKey) && (e.key === "l" || e.key === "L")) editAddr();
    else if ((e.ctrlKey || e.metaKey) && (e.key === "e" || e.key === "E" || e.key === "f" || e.key === "F")) { $("fq").focus(); $("fq").select(); }
    else return;
    e.preventDefault();
  });

  // ---------------------------------------------------------------- tab
  // tab này cần cả chiều ngang: ẩn cột các bước bên trái khi đang mở (bật lại bằng nút ☰)
  let wide = lsGet("xwide", true);
  const syncWide = () => {
    document.querySelector("main").classList.toggle("wide", activeKey === "files" && wide);
    $("xside").classList.toggle("on", !wide);
    requestAnimationFrame(() => { fitPanes(); P2.schedule(); P3.schedule(); });
  };
  const origActivate = activate;
  activate = (k) => { origActivate(k); syncWide(); };
  $("xside").onclick = () => { wide = !wide; lsSet("xwide", wide); syncWide(); };

  let opened = "";
  $("tabbar").querySelector('[data-tab="files"]').onclick = () => {
    activate("files");
    const w = val("work");
    if (opened !== w) {
      opened = w;
      back.length = fwd.length = 0;
      bundles.clear();
      loc = null;
      clearBox(BOX_HINT);
      treeRoot();
      if (w) go({ t: "dir", path: w });
      else P2.clear("Hãy chọn Thư mục làm việc ở khung bên trái.");
    } else {
      P2.render();
      P3.render();
      renderTree();
    }
    requestAnimationFrame(() => P2.focus());
  };
  split.classList.toggle("nopv", !showPv);
  split.classList.toggle("notree", !showTree);
  $("xpv").classList.toggle("on", showPv);
  $("xtree").classList.toggle("on", showTree);
  navButtons();
  P2.clear("Mở tab này để duyệt các tệp của game.");
  clearBox(BOX_HINT);
  showPreview([]);
})();
