//! Bang config FlatBuffers khong can schema: root { rows: [Row] }.
//! Kieu cua moi cot duoc suy ra tu du lieu (xem `infer`):
//!   - chuoi: moi dong deu tro toi chuoi UTF-8 hop le, khong co ky tu dieu khien
//!   - mang so / mang chuoi / mang bang con / bang con: truong la offset toi doi tuong hop le
//!   - con lai: so nguyen hoac so thuc nam ngay trong dong
//! Khi sua chuoi/mang/bang con: noi doi tuong moi vao cuoi buffer roi tro lai offset cua o.
//! Khi sua so: ghi de tai cho.
use serde_json::{Map, Number, Value};
use std::collections::HashMap;

fn u16at(b: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(p..p.checked_add(2)?)?.try_into().ok()?))
}
fn u32at(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(p..p.checked_add(4)?)?.try_into().ok()?))
}
fn u64at(b: &[u8], p: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(p..p.checked_add(8)?)?.try_into().ok()?))
}

fn fields(b: &[u8], pos: usize) -> Option<Vec<u16>> {
    let so = u32at(b, pos)? as i32 as i64;
    let vt = usize::try_from(pos as i64 - so).ok()?;
    let vsize = u16at(b, vt)? as usize;
    if vsize < 4 || vsize % 2 != 0 || vt + vsize > b.len() {
        return None;
    }
    (0..(vsize - 4) / 2).map(|i| u16at(b, vt + 4 + 2 * i)).collect()
}

/// Vi tri doi tuong ma truong offset tai p tro toi (can le 4, nam trong buffer).
fn target(b: &[u8], p: usize) -> Option<usize> {
    let off = u32at(b, p)? as usize;
    let t = p.checked_add(off)?;
    if off < 4 || p % 4 != 0 || t % 4 != 0 || t.checked_add(4)? > b.len() {
        return None;
    }
    Some(t)
}

fn str_at(b: &[u8], p: usize) -> Option<&str> {
    let t = p.checked_add(u32at(b, p)? as usize)?;
    if t % 4 != 0 || t <= p {
        return None;
    }
    let n = u32at(b, t)? as usize;
    let end = t.checked_add(4)?.checked_add(n)?;
    if end >= b.len() || b[end] != 0 {
        return None;
    }
    std::str::from_utf8(&b[t + 4..end]).ok()
}

/// Chuoi that khong chua ky tu dieu khien ngoai \t \n \r
/// (vector<int>/vector<table> ngan cung "giong" chuoi, vd [1] = "\x01").
fn clean(s: &str) -> bool {
    !s.bytes().any(|b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r'))
}

/// Mot table: vi tri, vtable, kich thuoc phan inline va offset tung truong (0 = vang mat).
#[derive(Clone, Debug)]
pub struct Tab {
    pos: usize,
    vt: usize,
    tsize: usize,
    offs: Vec<u16>,
}

impl Tab {
    fn off(&self, c: usize) -> usize {
        self.offs.get(c).copied().unwrap_or(0) as usize
    }
    /// So byte tu truong c toi truong ke tiep (hoac het table) = can tren cua kich thuoc truong.
    fn gap(&self, c: usize) -> usize {
        let fo = self.off(c);
        let next = self.offs.iter().map(|o| *o as usize).filter(|o| *o > fo).min().unwrap_or(self.tsize.max(fo));
        next - fo
    }
    fn loose(b: &[u8], pos: usize) -> Option<Tab> {
        let offs = fields(b, pos)?;
        let vt = (pos as i64 - u32at(b, pos)? as i32 as i64) as usize;
        Some(Tab { pos, vt, tsize: u16at(b, vt + 2)? as usize, offs })
    }
    /// Kiem tra chat: dung de nhan dien bang con (khong co schema nen phai that chac).
    fn strict(b: &[u8], pos: usize) -> Option<Tab> {
        if pos % 4 != 0 {
            return None;
        }
        let t = Tab::loose(b, pos)?;
        if t.tsize < 4 || pos + t.tsize > b.len() || t.offs.len() > 512 {
            return None;
        }
        let mut seen: Vec<u16> = Vec::new();
        for &o in &t.offs {
            if o != 0 {
                if (o as usize) < 4 || o as usize >= t.tsize || seen.contains(&o) {
                    return None;
                }
                seen.push(o);
            }
        }
        (!seen.is_empty()).then_some(t)
    }
}

fn width(gap: usize) -> usize {
    match gap {
        0 => 4, // khong ro (table size la) -> coi nhu 4 byte
        1 => 1,
        2 | 3 => 2,
        4..=7 => 4,
        _ => 8,
    }
}

// ------------------------------------------------------------------ kieu cot
#[derive(Clone, Debug)]
pub enum Kind {
    Str,
    /// so nguyen 1/2/4/8 byte
    Int(u8),
    F32,
    F64,
    Vec(Elem),
    Obj(Sub),
    /// offset toi chuoi/mang rong: khong biet kieu nen chi doc
    Empty,
    /// khong chac kieu: chi hien so tho, chi doc
    Unknown,
}

#[derive(Clone, Debug)]
pub enum Elem {
    I32,
    F32,
    Str,
    Obj(Sub),
}

/// Bang con: kieu tung truong + cac mau vtable da gap (de tao phan tu moi).
#[derive(Clone, Debug)]
pub struct Sub {
    kinds: Vec<Kind>,
    layouts: Vec<Tab>,
}

impl Sub {
    fn rebuildable(&self) -> bool {
        // truong khong xuat hien o phan tu nao thi khong can biet kieu
        self.kinds.iter().enumerate().filter(|(c, _)| self.layouts.iter().any(|l| l.off(*c) != 0)).all(|(_, k)| match k {
            Kind::Unknown => false,
            Kind::Obj(s) | Kind::Vec(Elem::Obj(s)) => s.rebuildable(),
            _ => true,
        })
    }
    fn layout_for(&self, m: &Map<String, Value>) -> Result<&Tab, String> {
        let has = |c: usize| m.contains_key(&format!("f{c}"));
        let n = self.kinds.len();
        self.layouts.iter().find(|l| (0..n).all(|c| (l.off(c) != 0) == has(c))).ok_or_else(|| {
            let ok: Vec<String> = self
                .layouts
                .iter()
                .map(|l| (0..n).filter(|c| l.off(*c) != 0).map(|c| format!("f{c}")).collect::<Vec<_>>().join(","))
                .collect();
            format!("phan tu phai co dung bo truong nhu du lieu goc: {{{}}}", ok.join("} hoac {"))
        })
    }
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Str => "str",
            Kind::Int(_) => "int",
            Kind::F32 | Kind::F64 => "float",
            Kind::Vec(Elem::I32) => "int[]",
            Kind::Vec(Elem::F32) => "float[]",
            Kind::Vec(Elem::Str) => "str[]",
            Kind::Vec(Elem::Obj(_)) => "obj[]",
            Kind::Obj(_) => "obj",
            Kind::Empty => "rong",
            Kind::Unknown => "?",
        }
    }
    /// Mo ta day du, ke ca truong cua bang con: obj[]{f0:int, f1:str}
    pub fn desc(&self) -> String {
        let sub = |s: &Sub| {
            let used = |c: &usize| s.layouts.iter().any(|l| l.off(*c) != 0);
            s.kinds.iter().enumerate().filter(|(c, _)| used(c)).map(|(c, k)| format!("f{c}:{}", k.desc())).collect::<Vec<_>>().join(", ")
        };
        match self {
            Kind::Obj(s) => format!("obj{{{}}}", sub(s)),
            Kind::Vec(Elem::Obj(s)) => format!("obj[]{{{}}}", sub(s)),
            k => k.label().to_string(),
        }
    }
    pub fn editable(&self) -> bool {
        match self {
            Kind::Empty | Kind::Unknown => false,
            Kind::Obj(s) | Kind::Vec(Elem::Obj(s)) => s.rebuildable(),
            _ => true,
        }
    }
}

// ------------------------------------------------------------------ suy kieu
/// Cac vung da biet chac la doi tuong (dong, vtable, chuoi...): mang/bang con that khong de len chung.
#[derive(Default, Clone)]
struct Solid {
    iv: Vec<(usize, usize)>,
    pmax: Vec<usize>,
    dirty: bool,
}

impl Solid {
    fn add(&mut self, s: usize, e: usize) {
        self.iv.push((s, e));
        self.dirty = true;
    }
    fn add_tab(&mut self, t: &Tab) {
        self.add(t.pos, t.pos + t.tsize.max(4));
        self.add(t.vt, t.vt + 4 + 2 * t.offs.len());
    }
    fn hits(&mut self, s: usize, e: usize) -> bool {
        if self.dirty {
            self.iv.sort_unstable();
            self.pmax.clear();
            let mut m = 0;
            for &(_, x) in &self.iv {
                m = m.max(x);
                self.pmax.push(m);
            }
            self.dirty = false;
        }
        let k = self.iv.partition_point(|&(a, _)| a < e);
        k > 0 && self.pmax[k - 1] > s
    }
}

fn is_str_col(b: &[u8], rows: &[Tab], c: usize) -> bool {
    let (mut seen, mut nonempty) = (0, 0);
    for t in rows {
        let fo = t.off(c);
        if fo == 0 {
            continue;
        }
        // truong offset luon dai 4 byte va can le 4 (truong 1 byte + du lieu ben canh co the "ra" chuoi)
        let g = t.gap(c);
        if (t.pos + fo) % 4 != 0 || (g != 0 && g < 4) {
            return false;
        }
        match str_at(b, t.pos + fo) {
            Some(s) if clean(s) => {
                seen += 1;
                nonempty += !s.is_empty() as usize;
            }
            _ => return false,
        }
    }
    seen > 0 && nonempty > 0
}

fn short_repr(s: &str) -> bool {
    !s.contains('e') && s.bytes().filter(u8::is_ascii_digit).count() <= 6
}

/// Cot 4 byte la so thuc neu moi gia tri khac 0 deu nam trong khoang "binh thuong"
/// va da so viet ra duoc ngan gon (0.5, 8.4...) - so nguyen doc nham thanh float thi rat le.
fn floaty32(vals: &[u32]) -> bool {
    let (mut nz, mut short) = (0, 0);
    for &v in vals.iter().filter(|v| **v != 0) {
        let f = f32::from_bits(v);
        if !f.is_finite() || !(1e-5..=1e9).contains(&f.abs()) {
            return false;
        }
        nz += 1;
        short += short_repr(&format!("{f:?}")) as usize;
    }
    nz > 0 && short * 2 >= nz
}

fn floaty64(vals: &[u64]) -> bool {
    let (mut nz, mut short) = (0, 0);
    for &v in vals.iter().filter(|v| **v != 0) {
        let f = f64::from_bits(v);
        if !f.is_finite() || !(1e-9..=1e15).contains(&f.abs()) {
            return false;
        }
        nz += 1;
        short += short_repr(&format!("{f:?}")) as usize;
    }
    nz > 0 && short * 2 >= nz
}

fn make_sub(b: &[u8], tabs: Vec<Tab>, solid: &mut Solid, depth: u8) -> Sub {
    for t in &tabs {
        solid.add_tab(t);
    }
    let kinds = infer(b, &tabs, solid, depth);
    let mut layouts: Vec<Tab> = Vec::new();
    for t in tabs {
        let n = kinds.len();
        if !layouts.iter().any(|l| (0..n).all(|c| (l.off(c) != 0) == (t.off(c) != 0))) {
            layouts.push(t);
        }
    }
    Sub { kinds, layouts }
}

/// Cot ma moi dong deu tro toi `ts`: bang con, mang, hay khong phai offset (None)?
fn classify_ref(b: &[u8], ts: &[usize], solid: &mut Solid, depth: u8) -> Option<Kind> {
    if ts.iter().all(|t| u32at(b, *t) == Some(0)) {
        return Some(Kind::Empty);
    }
    // bang con
    let tabs: Option<Vec<Tab>> = ts.iter().map(|t| Tab::strict(b, *t)).collect();
    if let Some(tabs) = tabs {
        if !tabs.iter().any(|t| solid.hits(t.pos, t.pos + t.tsize)) {
            return Some(if depth >= 3 { Kind::Unknown } else { Kind::Obj(make_sub(b, tabs, solid, depth + 1)) });
        }
    }
    // mang phan tu 4 byte
    let mut eps = Vec::new();
    let mut ext = Vec::new();
    for &t in ts {
        let n = u32at(b, t)? as usize;
        let end = t.checked_add(4)?.checked_add(n.checked_mul(4)?)?;
        if end > b.len() || solid.hits(t, end) {
            return None;
        }
        ext.push((t, end));
        eps.extend((0..n).map(|i| t + 4 + 4 * i));
    }
    let tabs: Option<Vec<Tab>> = eps.iter().map(|ep| target(b, *ep).and_then(|x| Tab::strict(b, x))).collect();
    let elem = match tabs {
        Some(tabs) if !tabs.iter().any(|t| solid.hits(t.pos, t.pos + t.tsize)) => {
            if depth >= 3 {
                return Some(Kind::Unknown);
            }
            for (s, e) in &ext {
                solid.add(*s, *e);
            }
            return Some(Kind::Vec(Elem::Obj(make_sub(b, tabs, solid, depth + 1))));
        }
        _ => {
            let strs: Option<Vec<&str>> = eps.iter().map(|ep| str_at(b, *ep).filter(|s| clean(s))).collect();
            let vals: Vec<u32> = eps.iter().filter_map(|ep| u32at(b, *ep)).collect();
            if strs.is_some_and(|v| v.iter().any(|s| !s.is_empty())) {
                Elem::Str
            } else if floaty32(&vals) {
                Elem::F32
            } else {
                Elem::I32
            }
        }
    };
    for (s, e) in ext {
        solid.add(s, e);
    }
    Some(Kind::Vec(elem))
}

fn infer(b: &[u8], rows: &[Tab], solid: &mut Solid, depth: u8) -> Vec<Kind> {
    let ncols = rows.iter().map(|t| t.offs.len()).max().unwrap_or(0);
    let mut kinds: Vec<Option<Kind>> = vec![None; ncols];
    // 1. cot chuoi
    for c in 0..ncols {
        if is_str_col(b, rows, c) {
            kinds[c] = Some(Kind::Str);
            for t in rows.iter().filter(|t| t.off(c) != 0) {
                if let Some(x) = target(b, t.pos + t.off(c)) {
                    solid.add(x, x + 5 + u32at(b, x).unwrap_or(0) as usize);
                }
            }
        }
    }
    // 2. ung vien offset: moi dong deu tro toi vi tri hop le, khong nam trong doi tuong da biet
    let mut cand: Vec<Option<Vec<usize>>> = vec![None; ncols];
    for c in 0..ncols {
        if kinds[c].is_some() {
            continue;
        }
        let mut ts = Vec::new();
        let mut ok = true;
        for t in rows.iter().filter(|t| t.off(c) != 0) {
            let g = t.gap(c);
            match target(b, t.pos + t.off(c)) {
                Some(x) if (g == 0 || g >= 4) && !solid.hits(x, x + 4) => ts.push(x),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && !ts.is_empty() {
            cand[c] = Some(ts);
        }
    }
    // hai cot cung tro vao mot doi tuong (khong rong) -> it nhat mot cot la so trung hop.
    // Cot nao khong phai mang/bang con hop le thi la so; neu ca hai deu "hop le" thi khong doan.
    let mut owner: HashMap<usize, usize> = HashMap::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for c in 0..ncols {
        for &t in cand[c].iter().flatten() {
            if u32at(b, t) == Some(0) {
                continue;
            }
            let o = *owner.entry(t).or_insert(c);
            if o != c && !pairs.contains(&(o, c)) {
                pairs.push((o, c));
            }
        }
    }
    let mut pass: HashMap<usize, bool> = HashMap::new();
    for c in pairs.iter().flat_map(|(a, x)| [*a, *x]) {
        if !pass.contains_key(&c) {
            let mut dry = solid.clone();
            let ok = cand[c].as_ref().and_then(|ts| classify_ref(b, ts, &mut dry, depth)).is_some();
            pass.insert(c, ok);
        }
    }
    for (a, x) in &pairs {
        match (pass[a], pass[x]) {
            (true, true) => {
                kinds[*a] = Some(Kind::Unknown);
                kinds[*x] = Some(Kind::Unknown);
            }
            (false, false) => {
                cand[*a] = None;
                cand[*x] = None;
            }
            (true, false) => cand[*x] = None,
            (false, true) => cand[*a] = None,
        }
    }
    // 3. phan loai
    for c in 0..ncols {
        if kinds[c].is_some() {
            continue;
        }
        if let Some(ts) = &cand[c] {
            if let Some(k) = classify_ref(b, ts, solid, depth) {
                kinds[c] = Some(k);
                continue;
            }
        }
        // so nam trong dong
        let present: Vec<&Tab> = rows.iter().filter(|t| t.off(c) != 0).collect();
        let w = present.iter().map(|t| width(t.gap(c))).min().unwrap_or(0);
        kinds[c] = Some(match w {
            0 => Kind::Unknown,
            1 => Kind::Int(1),
            2 => Kind::Int(2),
            4 => {
                let v: Vec<u32> = present.iter().filter_map(|t| u32at(b, t.pos + t.off(c))).collect();
                if floaty32(&v) { Kind::F32 } else { Kind::Int(4) }
            }
            _ => {
                let v: Vec<u64> = present.iter().filter_map(|t| u64at(b, t.pos + t.off(c))).collect();
                if v.iter().all(|x| x >> 32 == 0) {
                    let lo: Vec<u32> = v.iter().map(|x| *x as u32).collect();
                    if floaty32(&lo) { Kind::F32 } else { Kind::Int(8) }
                } else if floaty64(&v) {
                    Kind::F64
                } else {
                    Kind::Int(8)
                }
            }
        });
    }
    kinds.into_iter().map(|k| k.unwrap_or(Kind::Unknown)).collect()
}

// ------------------------------------------------------------------ doc gia tri
fn f32v(f: f32) -> Value {
    // qua chuoi ngan nhat de 8.4f khong thanh 8.399999618530273
    format!("{f:?}").parse::<f64>().ok().and_then(Number::from_f64).map(Value::Number).unwrap_or(Value::Null)
}

fn int_at(b: &[u8], p: usize, w: usize) -> Option<i64> {
    Some(match w {
        1 => *b.get(p)? as i64,
        2 => u16at(b, p)? as i16 as i64,
        4 => u32at(b, p)? as i32 as i64,
        _ => u64at(b, p)? as i64,
    })
}

fn val(b: &[u8], p: usize, gap: usize, k: &Kind) -> Option<Value> {
    Some(match k {
        Kind::Str => Value::from(str_at(b, p)?),
        Kind::Int(sz) => Value::from(int_at(b, p, (*sz as usize).min(width(gap)))?),
        Kind::F32 => f32v(f32::from_bits(u32at(b, p)?)),
        Kind::F64 => Value::Number(Number::from_f64(f64::from_bits(u64at(b, p)?))?),
        Kind::Empty => Value::from(""),
        Kind::Unknown => Value::from(u32at(b, p)? as i32),
        Kind::Obj(sub) => obj(b, target(b, p)?, sub)?,
        Kind::Vec(e) => {
            let t = target(b, p)?;
            let n = u32at(b, t)? as usize;
            if n > (b.len() - t - 4) / 4 {
                return None;
            }
            let mut out = Vec::with_capacity(n);
            for ep in (0..n).map(|i| t + 4 + 4 * i) {
                out.push(match e {
                    Elem::I32 => Value::from(u32at(b, ep)? as i32),
                    Elem::F32 => f32v(f32::from_bits(u32at(b, ep)?)),
                    Elem::Str => Value::from(str_at(b, ep)?),
                    Elem::Obj(sub) => obj(b, target(b, ep)?, sub)?,
                });
            }
            Value::Array(out)
        }
    })
}

fn obj(b: &[u8], pos: usize, sub: &Sub) -> Option<Value> {
    let t = Tab::loose(b, pos)?;
    let mut m = Map::new();
    for (c, k) in sub.kinds.iter().enumerate() {
        let fo = t.off(c);
        if fo != 0 {
            m.insert(format!("f{c}"), val(b, pos + fo, t.gap(c), k)?);
        }
    }
    Some(Value::Object(m))
}

/// JSON gon, truong xep theo so thu tu: [{"f0": 1001, "f1": 5}]
fn fmt(v: &Value, out: &mut String) {
    match v {
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by_key(|k| k[1..].parse::<usize>().unwrap_or(usize::MAX));
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&Value::from(k.as_str()).to_string());
                out.push_str(": ");
                fmt(&m[k], out);
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

fn show(k: &Kind, v: &Value) -> String {
    match (k, v) {
        (Kind::Str | Kind::Empty, Value::String(s)) => s.clone(),
        _ => {
            let mut s = String::new();
            fmt(v, &mut s);
            s
        }
    }
}

// ------------------------------------------------------------------ kiem tra / chuan hoa gia tri nhap
fn parse_text(k: &Kind, text: &str) -> Result<Value, String> {
    match k {
        Kind::Str => Ok(Value::from(text)),
        Kind::Empty | Kind::Unknown => Err("o nay chi doc (khong ro kieu du lieu)".into()),
        Kind::Int(_) | Kind::F32 | Kind::F64 => serde_json::from_str(text.trim()).map_err(|_| "can mot so".to_string()),
        Kind::Vec(_) => serde_json::from_str(text).map_err(|e| format!("sai cu phap mang, vd [1, 2]: {e}")),
        Kind::Obj(_) => serde_json::from_str(text).map_err(|e| format!("sai cu phap, vd {{\"f0\": 1}}: {e}")),
    }
}

fn norm_int(v: &Value, sz: u8) -> Result<Value, String> {
    let n = v.as_i64().ok_or("can so nguyen")?;
    let (lo, hi) = match sz {
        1 => (-128, 255),
        2 => (-32768, 65535),
        4 => (i32::MIN as i64, u32::MAX as i64),
        _ => (i64::MIN, i64::MAX),
    };
    if n < lo || n > hi {
        return Err(format!("so {n} vuot khoang cho phep ({lo}..{hi})"));
    }
    Ok(Value::from(match sz {
        1 => n as u8 as i64,
        2 => n as i16 as i64,
        4 => n as i32 as i64,
        _ => n,
    }))
}

fn norm_f32(v: &Value) -> Result<Value, String> {
    let f = v.as_f64().ok_or("can so thuc")? as f32;
    if !f.is_finite() {
        return Err("so thuc qua lon".into());
    }
    Ok(f32v(f))
}

fn norm(k: &Kind, v: &Value) -> Result<Value, String> {
    match k {
        Kind::Str => v.as_str().map(Value::from).ok_or_else(|| "can chuoi trong dau nhay kep".to_string()),
        Kind::Int(sz) => norm_int(v, *sz),
        Kind::F32 => norm_f32(v),
        Kind::F64 => v.as_f64().and_then(Number::from_f64).map(Value::Number).ok_or_else(|| "can so thuc".to_string()),
        Kind::Empty => (v.as_str() == Some("")).then(|| Value::from("")).ok_or_else(|| "truong rong khong ro kieu, phai giu \"\"".to_string()),
        Kind::Unknown => Err("co truong khong ro kieu, khong sua duoc".into()),
        Kind::Obj(sub) => norm_obj(sub, v),
        Kind::Vec(e) => {
            let a = v.as_array().ok_or("can mot mang, vd [1, 2]")?;
            let out: Result<Vec<Value>, String> = a
                .iter()
                .map(|x| match e {
                    Elem::I32 => norm_int(x, 4),
                    Elem::F32 => norm_f32(x),
                    Elem::Str => norm(&Kind::Str, x),
                    Elem::Obj(sub) => norm_obj(sub, x),
                })
                .collect();
            Ok(Value::Array(out?))
        }
    }
}

fn norm_obj(sub: &Sub, v: &Value) -> Result<Value, String> {
    let m = v.as_object().ok_or("can mot doi tuong, vd {\"f0\": 1}")?;
    let mut out = Map::new();
    for (key, x) in m {
        let k = key.strip_prefix('f').and_then(|s| s.parse::<usize>().ok()).and_then(|c| sub.kinds.get(c));
        let k = k.ok_or_else(|| format!("truong la: {key}"))?;
        out.insert(key.clone(), norm(k, x).map_err(|e| format!("{key}: {e}"))?);
    }
    sub.layout_for(&out)?;
    Ok(Value::Object(out))
}

/// Kiem tra chuoi nguoi dung nhap cho cot kieu `k`; tra ve dang chuan (giong dang hien thi).
pub fn canon(k: &Kind, text: &str) -> Result<String, String> {
    let v = norm(k, &parse_text(k, text)?)?;
    Ok(show(k, &v))
}

// ------------------------------------------------------------------ ghi
fn pad4(b: &mut Vec<u8>) {
    b.resize(b.len() + (4 - b.len() % 4) % 4, 0);
}

fn patch(b: &mut [u8], p: usize, t: usize) {
    b[p..p + 4].copy_from_slice(&((t - p) as u32).to_le_bytes());
}

fn put_str(b: &mut Vec<u8>, s: &str) -> usize {
    pad4(b);
    let t = b.len();
    b.extend_from_slice(&(s.len() as u32).to_le_bytes());
    b.extend_from_slice(s.as_bytes());
    b.push(0);
    pad4(b);
    t
}

/// Ghi gia tri (da chuan hoa) vao truong tai p. So: ghi de tai cho; con lai: noi vao cuoi roi tro toi.
fn put_field(b: &mut Vec<u8>, p: usize, gap: usize, k: &Kind, v: &Value) -> Result<(), String> {
    let bad = || "gia tri khong hop kieu".to_string();
    match k {
        Kind::Int(sz) => {
            let w = (*sz as usize).min(width(gap));
            let n = v.as_i64().ok_or_else(bad)?;
            b.get_mut(p..p + w).ok_or_else(bad)?.copy_from_slice(&n.to_le_bytes()[..w]);
        }
        Kind::F32 => b.get_mut(p..p + 4).ok_or_else(bad)?.copy_from_slice(&(v.as_f64().ok_or_else(bad)? as f32).to_le_bytes()),
        Kind::F64 => b.get_mut(p..p + 8).ok_or_else(bad)?.copy_from_slice(&v.as_f64().ok_or_else(bad)?.to_le_bytes()),
        Kind::Str => {
            let t = put_str(b, v.as_str().ok_or_else(bad)?);
            patch(b, p, t);
        }
        Kind::Empty => {
            pad4(b);
            let t = b.len();
            b.extend_from_slice(&[0; 8]);
            patch(b, p, t);
        }
        Kind::Unknown => return Err("khong ro kieu, khong ghi duoc".into()),
        Kind::Obj(sub) => {
            let t = put_obj(b, sub, v)?;
            patch(b, p, t);
        }
        Kind::Vec(e) => {
            let a = v.as_array().ok_or_else(bad)?;
            pad4(b);
            let t = b.len();
            b.extend_from_slice(&(a.len() as u32).to_le_bytes());
            let start = b.len();
            b.resize(start + 4 * a.len(), 0);
            for (i, x) in a.iter().enumerate() {
                let ep = start + 4 * i;
                match e {
                    Elem::I32 => b[ep..ep + 4].copy_from_slice(&(x.as_i64().ok_or_else(bad)? as i32).to_le_bytes()),
                    Elem::F32 => b[ep..ep + 4].copy_from_slice(&(x.as_f64().ok_or_else(bad)? as f32).to_le_bytes()),
                    Elem::Str => {
                        let s = put_str(b, x.as_str().ok_or_else(bad)?);
                        patch(b, ep, s);
                    }
                    Elem::Obj(sub) => {
                        let o = put_obj(b, sub, x)?;
                        patch(b, ep, o);
                    }
                }
            }
            patch(b, p, t);
        }
    }
    Ok(())
}

/// Tao table moi o cuoi buffer, dung lai vtable goc co cung bo truong.
fn put_obj(b: &mut Vec<u8>, sub: &Sub, v: &Value) -> Result<usize, String> {
    let m = v.as_object().ok_or("can mot doi tuong")?;
    let l = sub.layout_for(m)?;
    pad4(b);
    if b.len() % 8 != l.pos % 8 {
        b.extend_from_slice(&[0; 4]); // giu can le 8 byte nhu table goc
    }
    let pos = b.len();
    b.resize(pos + l.tsize, 0);
    b[pos..pos + 4].copy_from_slice(&((pos as i64 - l.vt as i64) as i32).to_le_bytes());
    for (c, k) in sub.kinds.iter().enumerate() {
        let fo = l.off(c);
        if fo != 0 {
            let x = m.get(&format!("f{c}")).ok_or("thieu truong")?;
            put_field(b, pos + fo, l.gap(c), k, x).map_err(|e| format!("f{c}: {e}"))?;
        }
    }
    Ok(pos)
}

// ------------------------------------------------------------------ bang
pub struct FlatTable {
    pub buf: Vec<u8>,
    rows: Vec<Tab>,
    /// chi so truong cua cac cot chuoi (dung cho quet / xuat / nhap Excel)
    pub str_cols: Vec<usize>,
    /// moi cot co du lieu: (chi so truong, kieu)
    pub cols: Vec<(usize, Kind)>,
}

impl FlatTable {
    fn rows_of(buf: &[u8]) -> Option<(Vec<Tab>, Solid)> {
        if buf.len() < 16 {
            return None;
        }
        let root = u32at(buf, 0)? as usize;
        let rt = Tab::loose(buf, root)?;
        let f0 = rt.off(0);
        if f0 == 0 {
            return None;
        }
        let p = root + f0;
        let vec = p.checked_add(u32at(buf, p)? as usize)?;
        let n = u32at(buf, vec)? as usize;
        if n == 0 || vec.checked_add(4)?.checked_add(n.checked_mul(4)?)? > buf.len() {
            return None;
        }
        let mut solid = Solid::default();
        solid.add(0, 4);
        solid.add_tab(&rt);
        solid.add(vec, vec + 4 + 4 * n);
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            let ep = vec + 4 + 4 * i;
            let t = Tab::loose(buf, ep.checked_add(u32at(buf, ep)? as usize)?)?;
            solid.add_tab(&t);
            rows.push(t);
        }
        Some((rows, solid))
    }

    pub fn parse(buf: Vec<u8>) -> Option<FlatTable> {
        let (rows, mut solid) = Self::rows_of(&buf)?;
        let kinds = infer(&buf, &rows, &mut solid, 0);
        let cols: Vec<(usize, Kind)> = kinds.into_iter().enumerate().filter(|(c, _)| rows.iter().any(|t| t.off(*c) != 0)).collect();
        Some(Self::with(buf, rows, cols))
    }

    /// Doc buffer DA SUA theo kieu cot cua ban goc (du lieu sua co the lam viec suy kieu ra khac).
    pub fn parse_as(buf: Vec<u8>, cols: &[(usize, Kind)]) -> Option<FlatTable> {
        let (rows, _) = Self::rows_of(&buf)?;
        Some(Self::with(buf, rows, cols.to_vec()))
    }

    fn with(buf: Vec<u8>, rows: Vec<Tab>, cols: Vec<(usize, Kind)>) -> FlatTable {
        let str_cols = cols.iter().filter(|(_, k)| matches!(k, Kind::Str)).map(|(c, _)| *c).collect();
        FlatTable { buf, rows, str_cols, cols }
    }

    pub fn nrows(&self) -> usize {
        self.rows.len()
    }

    /// Chuoi tai (dong, chi so truong); None neu truong vang mat trong du lieu goc.
    pub fn get(&self, r: usize, c: usize) -> Option<&str> {
        let t = self.rows.get(r)?;
        let fo = t.off(c);
        if fo == 0 {
            return None;
        }
        str_at(&self.buf, t.pos + fo)
    }

    /// Tong so ky tu chu trong bang, ke ca chuoi nam trong mang / bang con.
    pub fn chars(&self) -> usize {
        fn count(v: &Value) -> usize {
            match v {
                Value::String(s) => s.chars().count(),
                Value::Array(a) => a.iter().map(count).sum(),
                Value::Object(m) => m.values().map(count).sum(),
                _ => 0,
            }
        }
        let mut n = 0;
        for t in &self.rows {
            for (c, k) in &self.cols {
                let fo = t.off(*c);
                if fo != 0 && matches!(k, Kind::Str | Kind::Vec(Elem::Str | Elem::Obj(_)) | Kind::Obj(_)) {
                    n += val(&self.buf, t.pos + fo, t.gap(*c), k).as_ref().map(count).unwrap_or(0);
                }
            }
        }
        n
    }

    /// Gia tri dang van ban cua o (dong, vi tri trong `cols`); None neu truong vang mat.
    pub fn text(&self, r: usize, i: usize) -> Option<String> {
        let (c, k) = self.cols.get(i)?;
        let t = self.rows.get(r)?;
        let fo = t.off(*c);
        if fo == 0 {
            return None;
        }
        if matches!(k, Kind::Str) {
            return str_at(&self.buf, t.pos + fo).map(str::to_string);
        }
        Some(val(&self.buf, t.pos + fo, t.gap(*c), k).map(|v| show(k, &v)).unwrap_or_else(|| "?".into()))
    }

    /// Ghi o (dong, vi tri trong `cols`) tu van ban. Ok(false) neu truong vang mat.
    pub fn set_text(&mut self, r: usize, i: usize, text: &str) -> Result<bool, String> {
        let Some((c, k)) = self.cols.get(i) else { return Ok(false) };
        let Some(t) = self.rows.get(r) else { return Ok(false) };
        let fo = t.off(*c);
        if fo == 0 {
            return Ok(false);
        }
        let v = norm(k, &parse_text(k, text)?)?;
        put_field(&mut self.buf, t.pos + fo, t.gap(*c), k, &v)?;
        Ok(true)
    }
}
