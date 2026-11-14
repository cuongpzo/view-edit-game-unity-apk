//! Phien soan thao bang trong bo nho: mo, loc/phan trang, sua o, luu, bo thay doi.
use l10n_core as core;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Session {
    pub work: PathBuf,
    pub split: String,
    pub entry: String,
    pub asset: String,
    pub cols: Vec<usize>,
    pub kinds: Vec<core::flat::Kind>,
    pub orig: core::Vals,
    /// trang thai tren dia (ban sua da luu hoac ban goc)
    pub saved: core::Vals,
    /// trang thai dang sua (co the chua luu)
    pub cur: core::Vals,
}

impl Session {
    pub fn dirty(&self) -> usize {
        count_diff(&self.cur, &self.saved)
    }
    pub fn changed(&self) -> usize {
        count_diff(&self.cur, &self.orig)
    }
    fn cell(&self, r: usize, i: usize) -> Cell {
        let v = self.cur[r][i].clone();
        let o = v != self.orig[r][i];
        Cell { d: v != self.saved[r][i], orig: if o { self.orig[r][i].clone() } else { None }, o, v }
    }
}

fn count_diff(a: &core::Vals, b: &core::Vals) -> usize {
    a.iter().zip(b).map(|(x, y)| x.iter().zip(y).filter(|(p, q)| p != q).count()).sum()
}

#[derive(Default)]
pub struct Tables {
    next: u32,
    pub map: HashMap<u32, Session>,
}

pub type Shared = Arc<Mutex<Tables>>;

impl Tables {
    pub fn insert(&mut self, s: Session) -> u32 {
        self.next += 1;
        self.map.insert(self.next, s);
        self.next
    }
}

#[derive(Serialize)]
pub struct Opened {
    pub id: u32,
    pub asset: String,
    pub split: String,
    pub entry: String,
    pub cols: Vec<usize>,
    /// kieu tung cot: "str", "int", "float", "int[]", "float[]", "str[]", "obj[]", "obj", "rong", "?"
    pub kinds: Vec<String>,
    /// mo ta day du (ke ca truong cua bang con)
    pub descs: Vec<String>,
    /// cot chi doc (khong ro kieu)
    pub ro: Vec<bool>,
    pub nrows: usize,
    pub changed: usize,
}

impl Opened {
    pub fn of(id: u32, s: &Session) -> Opened {
        Opened {
            id,
            asset: s.asset.clone(),
            split: s.split.clone(),
            entry: s.entry.clone(),
            cols: s.cols.clone(),
            kinds: s.kinds.iter().map(|k| k.label().to_string()).collect(),
            descs: s.kinds.iter().map(|k| k.desc()).collect(),
            ro: s.kinds.iter().map(|k| !k.editable()).collect(),
            nrows: s.cur.len(),
            changed: s.changed(),
        }
    }
}

#[derive(Serialize)]
pub struct Cell {
    /// gia tri hien tai; null = truong vang mat (khong sua duoc)
    pub v: Option<String>,
    /// khac ban goc
    pub o: bool,
    /// chua luu
    pub d: bool,
    /// gia tri goc (chi gui khi o = true)
    pub orig: Option<String>,
}

#[derive(Serialize)]
pub struct Row {
    pub r: usize,
    pub cells: Vec<Cell>,
}

#[derive(Serialize)]
pub struct Page {
    pub total: usize,
    pub rows: Vec<Row>,
    pub dirty: usize,
    pub changed: usize,
}

/// mode: "all" | "changed" (khac goc) | "dirty" (chua luu) | "empty" (o trong o cot dang loc)
pub fn page(s: &Session, offset: usize, limit: usize, query: &str, col: i64, mode: &str) -> Page {
    let q = query.trim().to_lowercase();
    let cols: Vec<usize> = if col >= 0 && (col as usize) < s.cols.len() { vec![col as usize] } else { (0..s.cols.len()).collect() };
    let hit = |r: usize| -> bool {
        let mode_ok = match mode {
            "changed" => cols.iter().any(|&i| s.cur[r][i] != s.orig[r][i]),
            "dirty" => cols.iter().any(|&i| s.cur[r][i] != s.saved[r][i]),
            "empty" => cols.iter().any(|&i| s.cur[r][i].as_deref() == Some("")),
            _ => true,
        };
        if !mode_ok {
            return false;
        }
        if q.is_empty() {
            return true;
        }
        if let Ok(n) = q.parse::<usize>() {
            if n == r {
                return true;
            }
        }
        cols.iter().any(|&i| s.cur[r][i].as_deref().is_some_and(|v| v.to_lowercase().contains(&q)))
    };
    let mut total = 0;
    let mut rows = Vec::new();
    for r in 0..s.cur.len() {
        if !hit(r) {
            continue;
        }
        if total >= offset && rows.len() < limit {
            rows.push(Row { r, cells: (0..s.cols.len()).map(|i| s.cell(r, i)).collect() });
        }
        total += 1;
    }
    Page { total, rows, dirty: s.dirty(), changed: s.changed() }
}

#[derive(Serialize)]
pub struct SetResult {
    pub cell: Cell,
    pub dirty: usize,
    pub changed: usize,
}

pub fn set(s: &mut Session, r: usize, i: usize, value: String) -> Result<SetResult, String> {
    let kind = s.kinds.get(i).ok_or("o ngoai bang")?;
    let slot = s.cur.get_mut(r).and_then(|row| row.get_mut(i)).ok_or("o ngoai bang")?;
    let Some(old) = slot else { return Err("truong nay vang mat trong du lieu goc, khong sua duoc".into()) };
    if !matches!(kind, core::flat::Kind::Str) {
        // so / mang / bang con: kiem tra cu phap + kieu, luu o dang chuan (giong dang hien thi)
        *slot = Some(core::flat::canon(kind, &value)?);
    } else if core::norm(old) != core::norm(&value) {
        // giu nguyen gia tri goc neu nguoi dung go lai dung chuoi goc (tranh lech \r\n)
        let orig = s.orig[r][i].clone().unwrap_or_default();
        *slot = Some(if core::norm(&orig) == core::norm(&value) { orig } else { value });
    }
    Ok(SetResult { cell: s.cell(r, i), dirty: s.dirty(), changed: s.changed() })
}
