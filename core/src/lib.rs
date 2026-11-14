//! Loi cong cu viet hoa game Unity (XAPK/APK, YooAsset + FlatBuffers config).
//! Thu muc lam viec: work/xapk (apk goc), work/apk_<split> (da giai nen),
//! work/text (xlsx), work/mod (bundle da sua), work/out (apk moi), work/out/signed.
pub mod flat;
pub mod pak;
pub mod serialized;
pub mod unityfs;

use anyhow::{anyhow, bail, Context, Result};
use flat::FlatTable;
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use serialized::{SFile, CLASS_TEXT_ASSET};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use unityfs::Bundle;

pub type Log<'a> = &'a dyn Fn(&str);

const KEEP: &str = "<<KEEP: chuoi qua dai voi Excel, khong sua o nay>>";

/// Chuan hoa de so sanh o Excel voi chuoi goc (Excel doi \r\n, bo ky tu dieu khien).
pub fn norm(s: &str) -> String {
    s.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !matches!(*c as u32, 0..=8 | 0xb | 0xc | 0xe..=0x1f))
        .collect()
}

pub struct Split {
    pub id: String,
    pub apk: PathBuf,
    pub dir: PathBuf,
}

pub fn splits(work: &Path) -> Result<Vec<Split>> {
    let x = work.join("xapk");
    let mf = x.join("manifest.json");
    let mut items: Vec<(String, String)> = Vec::new();
    if mf.exists() {
        let v: serde_json::Value = serde_json::from_slice(&fs::read(&mf)?)?;
        for s in v["split_apks"].as_array().ok_or_else(|| anyhow!("manifest.json thieu split_apks"))? {
            items.push((s["id"].as_str().unwrap_or("").into(), s["file"].as_str().unwrap_or("").into()));
        }
    } else {
        let rd = fs::read_dir(&x).with_context(|| format!("chua co {} - hay chay Giai nen truoc", x.display()))?;
        for e in rd {
            let n = e?.file_name().to_string_lossy().into_owned();
            if let Some(stem) = n.strip_suffix(".apk") {
                items.push((stem.to_string(), n.clone()));
            }
        }
        items.sort();
    }
    if items.is_empty() {
        bail!("khong thay apk nao trong {}", x.display());
    }
    Ok(items.into_iter().map(|(id, f)| Split { dir: work.join(format!("apk_{id}")), apk: x.join(f), id }).collect())
}

/// (split id, duong dan trong apk, duong dan tren dia) cua moi file .bundle.
pub fn bundles(work: &Path) -> Result<Vec<(String, String, PathBuf)>> {
    let mut out = Vec::new();
    for s in splits(work)? {
        for e in walkdir::WalkDir::new(&s.dir).into_iter().filter_map(|e| e.ok()) {
            if e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "bundle") {
                let rel = e.path().strip_prefix(&s.dir)?.to_string_lossy().replace('\\', "/");
                out.push((s.id.clone(), rel, e.path().to_path_buf()));
            }
        }
    }
    out.sort();
    Ok(out)
}

pub struct TextRef {
    pub node: usize,
    pub path_id: i64,
    pub name: String,
    pub data: Vec<u8>,
}

pub struct Loaded {
    pub bundle: Bundle,
    pub files: Vec<Option<SFile>>,
    pub texts: Vec<TextRef>,
}

pub fn load_bundle(path: &Path) -> Result<Loaded> {
    let raw = fs::read(path)?;
    let bundle = Bundle::read(&raw)?;
    let mut files = Vec::new();
    let mut texts = Vec::new();
    for (i, n) in bundle.nodes.iter().enumerate() {
        let sf = SFile::parse(&n.data).ok();
        if let Some(sf) = &sf {
            for o in sf.objs.iter().filter(|o| o.class_id == CLASS_TEXT_ASSET) {
                if let Ok((name, data, _)) = serialized::text_asset(sf.obj_data(o)) {
                    texts.push(TextRef { node: i, path_id: o.path_id, name, data: data.to_vec() });
                }
            }
        }
        files.push(sf);
    }
    Ok(Loaded { bundle, files, texts })
}

fn rx(pat: &str) -> Result<Regex> {
    RegexBuilder::new(pat).case_insensitive(true).build().map_err(|e| anyhow!("regex sai: {e}"))
}

// ------------------------------------------------------------------ unpack
pub fn unpack(src: &Path, work: &Path, log: Log) -> Result<()> {
    let x = work.join("xapk");
    fs::create_dir_all(&x)?;
    let mut z = zip::ZipArchive::new(File::open(src).with_context(|| format!("khong mo duoc {}", src.display()))?)?;
    let is_xapk = z.file_names().any(|n| n == "manifest.json" || (n.ends_with(".apk") && !n.contains('/')));
    if is_xapk {
        log(&format!("XAPK -> giai nen {} muc vao {}", z.len(), x.display()));
        z.extract(&x)?;
    } else {
        drop(z);
        fs::copy(src, x.join("base.apk"))?;
    }
    for s in splits(work)? {
        if s.dir.is_dir() {
            log(&format!("bo qua (da co): {}", s.dir.display()));
            continue;
        }
        log(&format!("giai nen {} ...", s.apk.file_name().unwrap().to_string_lossy()));
        zip::ZipArchive::new(File::open(&s.apk)?)?.extract(&s.dir)?;
    }
    log("Giai nen xong.");
    Ok(())
}

// ------------------------------------------------------------------ scan
#[derive(Serialize)]
pub struct TableInfo {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    pub chars: usize,
    pub bundle: String,
    pub split: String,
    pub entry: String,
    pub modified: bool,
}

#[derive(Serialize)]
pub struct ScanReport {
    pub unity: String,
    pub scripting: String,
    pub hybridclr: bool,
    pub lua: bool,
    pub yooasset: bool,
    pub fmod: bool,
    pub bundles: usize,
    pub offsets: Vec<(i64, usize)>,
    pub tables: Vec<TableInfo>,
}

pub fn scan(work: &Path, pattern: &str, log: Log) -> Result<ScanReport> {
    let re = rx(pattern)?;
    let mut libs = Vec::new();
    for s in splits(work)? {
        for e in walkdir::WalkDir::new(&s.dir).into_iter().filter_map(|e| e.ok()) {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if n.ends_with(".so") {
                libs.push(n);
            }
        }
    }
    let has = |k: &str| libs.iter().any(|l| l.contains(k));
    let all = bundles(work)?;
    let mut offsets: BTreeMap<i64, usize> = BTreeMap::new();
    let mut unity = String::new();
    for (_, _, p) in &all {
        let mut head = vec![0u8; 4096];
        let n = File::open(p)?.read(&mut head)?;
        head.truncate(n);
        let off = unityfs::find_sig(&head);
        *offsets.entry(off.map(|o| o as i64).unwrap_or(-1)).or_default() += 1;
        if unity.is_empty() {
            if let Some(o) = off {
                let mut r = unityfs::Rd::new(&head, o + 12, true);
                if let (Ok(_), Ok(v)) = (r.cstr(), r.cstr()) {
                    unity = v;
                }
            }
        }
    }
    let names: String = all.iter().map(|(_, rel, _)| rel.to_lowercase()).collect::<Vec<_>>().join(" ");
    let cand: Vec<_> = all.iter().filter(|(_, rel, _)| re.is_match(rel)).collect();
    log(&format!("{} bundle; doc {} bundle khop /{}/ ...", all.len(), cand.len(), pattern));
    let mut tables = Vec::new();
    for (sid, rel, p) in cand {
        let l = match load_bundle(p) {
            Ok(l) => l,
            Err(e) => {
                log(&format!("  ! {rel}: {e}"));
                continue;
            }
        };
        for t in l.texts {
            if let Some(ft) = FlatTable::parse(t.data) {
                // liet ke moi bang: bang khong co cot chuoi van co the chua chu trong mang / bang con,
                // va cac cot so / mang cung sua duoc trong trinh soan thao
                let chars = ft.chars();
                tables.push(TableInfo {
                    name: t.name,
                    rows: ft.nrows(),
                    cols: ft.str_cols.len(),
                    chars,
                    bundle: rel.rsplit('/').next().unwrap_or(rel).to_string(),
                    split: sid.clone(),
                    entry: rel.clone(),
                    modified: mod_path(work, sid, rel).is_file(),
                });
            }
        }
    }
    tables.sort_by(|a, b| b.chars.cmp(&a.chars));
    log(&format!("Quet xong: {} bang, {} bang co chu.", tables.len(), tables.iter().filter(|t| t.chars > 0).count()));
    Ok(ScanReport {
        unity,
        scripting: if has("libil2cpp") { "IL2CPP" } else if has("libmono") { "Mono" } else { "?" }.into(),
        hybridclr: names.contains("hotdll"),
        lua: has("xlua") || has("tolua") || has("slua"),
        yooasset: names.contains("/yoo/"),
        fmod: has("libfmod"),
        bundles: all.len(),
        offsets: offsets.into_iter().collect(),
        tables,
    })
}

// ------------------------------------------------------------------ export
pub fn export(work: &Path, pattern: &str, names: &str, log: Log) -> Result<Vec<String>> {
    use rust_xlsxwriter::{Format, Workbook};
    let re = rx(pattern)?;
    let out = work.join("text");
    fs::create_dir_all(&out)?;
    let names: Vec<&str> = names.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    let mut done = Vec::new();
    for (sid, rel, p) in bundles(work)? {
        if !re.is_match(&rel) {
            continue;
        }
        // xuat theo trang thai hien tai: ban da sua (work/mod) neu co, khong thi ban goc
        let mp = mod_path(work, &sid, &rel);
        for t in load_bundle(if mp.is_file() { &mp } else { &p })?.texts {
            let Some(ft) = FlatTable::parse(t.data) else { continue };
            if ft.str_cols.is_empty() {
                continue;
            }
            let mut wb = Workbook::new();
            let ws = wb.add_worksheet();
            ws.set_name("text")?;
            let bold = Format::new().set_bold();
            ws.write_string_with_format(0, 0, "row", &bold)?;
            for (i, c) in ft.str_cols.iter().enumerate() {
                let h = names.get(i).map(|s| s.to_string()).unwrap_or_else(|| format!("f{c}"));
                ws.write_string_with_format(0, i as u16 + 1, &h, &bold)?;
                ws.set_column_width(i as u16 + 1, 45)?;
            }
            ws.set_freeze_panes(1, 1)?;
            for r in 0..ft.nrows() {
                ws.write_number(r as u32 + 1, 0, r as f64)?;
                for (i, c) in ft.str_cols.iter().enumerate() {
                    let s = norm(ft.get(r, *c).unwrap_or(""));
                    if s.is_empty() {
                        continue;
                    }
                    let s = if s.encode_utf16().count() > 32767 { KEEP.to_string() } else { s };
                    ws.write_string(r as u32 + 1, i as u16 + 1, &s)?;
                }
            }
            let meta = wb.add_worksheet();
            meta.set_name("_meta")?;
            let cols = ft.str_cols.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(",");
            let rows = ft.nrows().to_string();
            for (i, (k, v)) in [("split", sid.as_str()), ("entry", rel.as_str()), ("asset", t.name.as_str()), ("cols", cols.as_str()), ("rows", rows.as_str())]
                .iter()
                .enumerate()
            {
                meta.write_string(i as u32, 0, *k)?;
                meta.write_string(i as u32, 1, *v)?;
            }
            let dst = out.join(format!("{}.xlsx", t.name));
            wb.save(&dst).with_context(|| format!("khong ghi duoc {} (dang mo trong Excel?)", dst.display()))?;
            log(&format!("xuat {} : {} dong x {} cot -> {}", t.name, ft.nrows(), ft.str_cols.len(), dst.display()));
            done.push(dst.to_string_lossy().into_owned());
        }
    }
    log(&format!("Xuat xong {} bang. Sua file .xlsx (giu cot 'row' va sheet _meta) roi Nhap lai.", done.len()));
    Ok(done)
}

// ------------------------------------------------------------------ import
#[derive(Serialize)]
pub struct ImportResult {
    pub asset: String,
    pub changed: usize,
    pub skipped: usize,
    pub out: String,
    pub verified: bool,
}

fn cell(d: &calamine::Data) -> String {
    use calamine::Data::*;
    match d {
        String(s) => s.clone(),
        Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
        Empty => std::string::String::new(),
        other => other.to_string(),
    }
}

pub fn import(work: &Path, files: &[PathBuf], log: Log) -> Result<Vec<ImportResult>> {
    use calamine::{open_workbook_auto, Reader};
    let mut src: Vec<PathBuf> = files.to_vec();
    if src.is_empty() {
        let dir = work.join("text");
        for e in fs::read_dir(&dir).with_context(|| format!("chua co {} - hay Xuat text truoc", dir.display()))? {
            let p = e?.path();
            let n = p.file_name().unwrap().to_string_lossy().into_owned();
            if n.ends_with(".xlsx") && !n.starts_with("~$") {
                src.push(p);
            }
        }
        src.sort();
    }
    let mut results = Vec::new();
    for x in src {
        let fname = x.file_name().unwrap().to_string_lossy().into_owned();
        let mut wb = open_workbook_auto(&x).with_context(|| format!("khong mo duoc {fname}"))?;
        let meta: HashMap<String, String> = wb
            .worksheet_range("_meta")
            .with_context(|| format!("{fname}: thieu sheet _meta"))?
            .rows()
            .filter(|r| r.len() >= 2)
            .map(|r| (cell(&r[0]), cell(&r[1])))
            .collect();
        let get = |k: &str| meta.get(k).cloned().ok_or_else(|| anyhow!("{fname}: _meta thieu '{k}'"));
        let cols: Vec<usize> = get("cols")?.split(',').filter_map(|c| c.trim().parse().ok()).collect();
        let asset = get("asset")?;
        let split = get("split")?;
        let entry = get("entry")?;
        let td = read_table(work, &split, &entry, &asset)?;
        // cot Excel thu i -> vi tri trong bang (chi cot chuoi)
        let pos: Option<Vec<usize>> =
            cols.iter().map(|c| td.cols.iter().position(|x| x == c).filter(|i| matches!(td.kinds[*i], flat::Kind::Str))).collect();
        let Some(pos) = pos else {
            let cur: Vec<usize> = td.cols.iter().zip(&td.kinds).filter(|(_, k)| matches!(k, flat::Kind::Str)).map(|(c, _)| *c).collect();
            bail!("{fname}: cot trong _meta ({cols:?}) khong con la cot chuoi cua bang ({cur:?}) - hay Xuat lai file Excel");
        };
        let mut vals = td.cur;
        let (mut changed, mut skipped) = (0, 0);
        let range = wb.worksheet_range("text").with_context(|| format!("{fname}: thieu sheet text"))?;
        for line in range.rows().skip(1) {
            let Some(Ok(r)) = line.first().map(|d| cell(d).parse::<usize>()) else { continue };
            let Some(row) = vals.get_mut(r) else { continue };
            for (i, p) in pos.iter().enumerate() {
                let slot = &mut row[*p];
                let new = norm(&line.get(i + 1).map(cell).unwrap_or_default());
                if new.starts_with("<<KEEP") {
                    continue;
                }
                if new == slot.as_deref().map(norm).unwrap_or_default() {
                    continue;
                }
                match slot {
                    Some(v) => {
                        *v = new;
                        changed += 1;
                    }
                    None => skipped += 1,
                }
            }
        }
        if skipped > 0 {
            log(&format!("  ! {skipped} o khong ghi duoc (truong vang mat trong du lieu goc)"));
        }
        if changed == 0 {
            log(&format!("khong doi: {fname}"));
            results.push(ImportResult { asset, changed, skipped, out: String::new(), verified: true });
            continue;
        }
        let w = write_table(work, &split, &entry, &asset, &vals, log)?;
        log(&format!("{asset}: nhap {changed} o tu {fname} (tong {} o khac ban goc)", w.diff));
        results.push(ImportResult { asset, changed, skipped, out: w.path, verified: true });
    }
    log("Nhap xong.");
    Ok(results)
}

// ------------------------------------------------------------------ bang (cho trinh soan thao)
/// Gia tri moi cot dang van ban: vals[dong][vi tri trong cols]; None = truong vang mat (khong sua duoc).
pub type Vals = Vec<Vec<Option<String>>>;

pub fn orig_path(work: &Path, split: &str, entry: &str) -> Result<PathBuf> {
    let s = splits(work)?.into_iter().find(|s| s.id == split).ok_or_else(|| anyhow!("khong co split '{split}'"))?;
    Ok(s.dir.join(entry))
}

pub fn mod_path(work: &Path, split: &str, entry: &str) -> PathBuf {
    work.join("mod").join(split).join(entry)
}

fn find_table(l: Loaded, asset: &str, from: &Path) -> Result<(Loaded, usize, i64, FlatTable)> {
    let t = l.texts.iter().find(|t| t.name == asset).ok_or_else(|| anyhow!("khong thay {asset} trong {}", from.display()))?;
    let (node, pid) = (t.node, t.path_id);
    let ft = FlatTable::parse(t.data.clone()).ok_or_else(|| anyhow!("{asset}: khong phai bang FlatBuffers"))?;
    Ok((l, node, pid, ft))
}

fn table_vals(ft: &FlatTable) -> Vals {
    (0..ft.nrows()).map(|r| (0..ft.cols.len()).map(|i| ft.text(r, i)).collect()).collect()
}

pub struct TableData {
    /// chi so truong cua moi cot
    pub cols: Vec<usize>,
    /// kieu cua moi cot (suy tu ban goc)
    pub kinds: Vec<flat::Kind>,
    pub orig: Vals,
    /// trang thai hien tai: ban da sua (work/mod) neu co, khong thi giong `orig`
    pub cur: Vals,
}

pub fn read_table(work: &Path, split: &str, entry: &str, asset: &str) -> Result<TableData> {
    let op = orig_path(work, split, entry)?;
    let (_, _, _, oft) = find_table(load_bundle(&op)?, asset, &op)?;
    let orig = table_vals(&oft);
    let mp = mod_path(work, split, entry);
    let cur = if mp.is_file() {
        // doc ban da sua theo kieu cot cua ban GOC (du lieu da sua co the lam viec suy kieu ra khac)
        let l = load_bundle(&mp)?;
        let mft = l.texts.into_iter().find(|t| t.name == asset).and_then(|t| FlatTable::parse_as(t.data, &oft.cols));
        match mft {
            Some(m) if m.nrows() == oft.nrows() => table_vals(&m),
            _ => bail!("ban da sua {} khong khop cau truc ban goc - hay Khoi phuc ban goc", mp.display()),
        }
    } else {
        orig.clone()
    };
    Ok(TableData { cols: oft.cols.iter().map(|(c, _)| *c).collect(), kinds: oft.cols.iter().map(|(_, k)| k.clone()).collect(), orig, cur })
}

#[derive(Serialize)]
pub struct WriteResult {
    /// so o khac ban goc
    pub diff: usize,
    /// duong dan bundle da ghi ("" neu khong con o nao khac -> da xoa ban sua)
    pub path: String,
}

/// Ghi bang: luon dung lai tu bundle GOC + cac o khac goc (khong phinh to khi luu nhieu lan).
pub fn write_table(work: &Path, split: &str, entry: &str, asset: &str, vals: &Vals, log: Log) -> Result<WriteResult> {
    let op = orig_path(work, split, entry)?;
    let (mut l, node, pid, mut ft) = find_table(load_bundle(&op)?, asset, &op)?;
    if vals.len() != ft.nrows() {
        bail!("{asset}: so dong khong khop ({} != {})", vals.len(), ft.nrows());
    }
    let mut diff = 0;
    for (r, row) in vals.iter().enumerate() {
        for (i, v) in row.iter().enumerate() {
            let Some(v) = v else { continue };
            if ft.text(r, i).as_deref() == Some(v.as_str()) {
                continue;
            }
            let c = ft.cols.get(i).map(|(c, _)| *c).unwrap_or(0);
            if ft.set_text(r, i, v).map_err(|e| anyhow!("{asset}: dong {r} cot f{c}: {e}"))? {
                diff += 1;
            }
        }
    }
    let dst = mod_path(work, split, entry);
    if diff == 0 {
        if dst.is_file() {
            fs::remove_file(&dst)?;
            log(&format!("{asset}: khong con o nao khac ban goc -> xoa ban sua {}", dst.display()));
        }
        return Ok(WriteResult { diff, path: String::new() });
    }
    let sf = l.files[node].as_ref().unwrap();
    let obj = sf.objs.iter().find(|o| o.path_id == pid).unwrap();
    let new_obj = serialized::text_asset_with(sf.obj_data(obj), &ft.buf)?;
    l.bundle.nodes[node].data = sf.rebuild(&HashMap::from([(pid, new_obj)]));
    fs::create_dir_all(dst.parent().unwrap())?;
    let tmp = dst.with_extension("bundle.tmp");
    fs::write(&tmp, l.bundle.write()?)?;
    // tu kiem tra: doc lai bundle vua ghi truoc khi thay file cu
    let ok = load_bundle(&tmp).map(|v| v.texts.iter().any(|t| t.name == asset && t.data == ft.buf)).unwrap_or(false);
    if !ok {
        let _ = fs::remove_file(&tmp);
        bail!("{asset}: bundle ghi ra khong doc lai duoc dung noi dung");
    }
    fs::rename(&tmp, &dst)?;
    log(&format!("{asset}: {diff} o khac ban goc -> {} [kiem tra OK]", dst.display()));
    Ok(WriteResult { diff, path: dst.to_string_lossy().into_owned() })
}

/// Xoa ban da sua cua mot bundle (quay ve ban goc).
pub fn revert(work: &Path, split: &str, entry: &str) -> Result<bool> {
    let p = mod_path(work, split, entry);
    if p.is_file() {
        fs::remove_file(&p)?;
        return Ok(true);
    }
    Ok(false)
}

/// Danh sach "split/entry" dang co ban sua trong work/mod.
pub fn modified_entries(work: &Path) -> Vec<String> {
    let m = work.join("mod");
    walkdir::WalkDir::new(&m)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "bundle"))
        .filter_map(|e| e.path().strip_prefix(&m).ok().map(|p| p.to_string_lossy().replace('\\', "/")))
        .collect()
}

// ------------------------------------------------------------------ pack
#[derive(Serialize)]
pub struct PackResult {
    pub dir: String,
    pub signed: bool,
    pub apks: Vec<String>,
    pub replaced: usize,
}

/// Cac thu muc co the chua libra.pak hoac uber-apk-signer.jar.
fn signer_dirs(work: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![work.to_path_buf()];
    if let Ok(exe) = std::env::current_exe() {
        dirs.extend(exe.parent().map(|p| p.to_path_buf()));
    }
    dirs.push(work.join("tools"));
    dirs.extend(work.parent().map(|p| p.join("tools")));
    dirs
}

/// Tim file libra.pak (dependency da ma hoa) o cac thu muc chuan.
pub fn find_pak(work: &Path) -> Option<PathBuf> {
    for d in signer_dirs(work) {
        let p = d.join("libra.pak");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Giai ma uber-apk-signer.jar tu libra.pak ra file tam; tu xoa khi `TempJar` bi huy.
pub struct TempJar {
    pub jar: PathBuf,
    dir: PathBuf,
}

impl Drop for TempJar {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Lay duong dan jar de ky: uu tien tham so, roi libra.pak (giai ra temp), roi jar roi tren dia.
pub fn resolve_signer(work: &Path, explicit: Option<&Path>) -> Result<(PathBuf, Option<TempJar>)> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Ok((p.to_path_buf(), None));
        }
    }
    if let Some(pakp) = find_pak(work) {
        let files = pak::unpack(&fs::read(&pakp)?).with_context(|| format!("khong doc duoc {}", pakp.display()))?;
        if let Some((name, data)) = files.into_iter().find(|(n, _)| n.to_lowercase().ends_with(".jar")) {
            let dir = std::env::temp_dir().join(format!("l10n-pak-{}", std::process::id()));
            fs::create_dir_all(&dir)?;
            let jar = dir.join(&name);
            fs::write(&jar, data)?;
            return Ok((jar.clone(), Some(TempJar { jar, dir })));
        }
    }
    if let Some(j) = find_signer(work) {
        return Ok((j, None));
    }
    bail!("khong tim thay libra.pak hoac uber-apk-signer.jar de ky")
}

/// Mo ta nguon ky dang co (cho giao dien), hoac None neu chua san sang.
pub fn signer_info(work: &Path) -> Option<String> {
    find_pak(work).map(|p| format!("libra.pak ({})", p.display())).or_else(|| find_signer(work).map(|p| p.display().to_string()))
}

pub fn find_signer(work: &Path) -> Option<PathBuf> {
    for d in signer_dirs(work) {
        for e in fs::read_dir(d).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("uber-apk-signer") && n.ends_with(".jar") {
                return Some(e.path());
            }
        }
    }
    None
}

pub fn pack(work: &Path, signer: Option<&Path>, log: Log) -> Result<PackResult> {
    use zip::write::SimpleFileOptions;
    let out = work.join("out");
    fs::create_dir_all(&out)?;
    let sig = Regex::new(r"^META-INF/.*\.(SF|RSA|DSA|EC|MF)$").unwrap();
    let mut built = Vec::new();
    let mut replaced = 0;
    for s in splits(work)? {
        let mdir = work.join("mod").join(&s.id);
        let mut repl: HashMap<String, PathBuf> = HashMap::new();
        for e in walkdir::WalkDir::new(&mdir).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
            repl.insert(e.path().strip_prefix(&mdir)?.to_string_lossy().replace('\\', "/"), e.path().to_path_buf());
        }
        let name = s.apk.file_name().unwrap().to_string_lossy().into_owned();
        log(&format!("dong goi {name} - thay {} file", repl.len()));
        let dst = out.join(&name);
        let mut zi = zip::ZipArchive::new(File::open(&s.apk)?)?;
        let mut zo = zip::ZipWriter::new(File::create(&dst)?);
        for i in 0..zi.len() {
            let f = zi.by_index_raw(i)?;
            let fname = f.name().to_string();
            if sig.is_match(&fname) {
                continue; // bo chu ky cu
            }
            if let Some(p) = repl.remove(&fname) {
                let method = f.compression();
                drop(f);
                let size = fs::metadata(&p)?.len();
                zo.start_file(fname, SimpleFileOptions::default().compression_method(method).large_file(size > u32::MAX as u64))?;
                io::copy(&mut File::open(&p)?, &mut zo)?;
                replaced += 1;
            } else {
                zo.raw_copy_file(f)?;
            }
        }
        zo.finish()?;
        for k in repl.keys() {
            log(&format!("  ! file mod khong co trong apk goc, bo qua: {k}"));
        }
        built.push(dst);
    }
    let (jar, _tmp) = match resolve_signer(work, signer) {
        Ok(v) => v,
        Err(_) => {
            log(&format!("CHUA KY: khong co libra.pak/uber-apk-signer.jar. APK chua ky nam o {}", out.display()));
            return Ok(PackResult { dir: out.to_string_lossy().into_owned(), signed: false, apks: built.iter().map(|p| p.to_string_lossy().into_owned()).collect(), replaced });
        }
    };
    let signed = out.join("signed");
    let _ = fs::remove_dir_all(&signed);
    log(if _tmp.is_some() { "ky bang uber-apk-signer trong libra.pak ..." } else { "ky bang uber-apk-signer ..." });
    let mut cmd = std::process::Command::new("java");
    cmd.arg("-jar").arg(&jar).arg("--apks").arg(&out).arg("--out").arg(&signed).arg("--allowResign");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let o = cmd.output().context("khong chay duoc java (da cai Java chua?)")?;
    for line in String::from_utf8_lossy(&o.stdout).lines().chain(String::from_utf8_lossy(&o.stderr).lines()) {
        let line = line.trim_end();
        if line.contains("success") || line.contains("verified") || line.contains("rror") || line.contains("Successfully") {
            log(&format!("  {}", line.trim()));
        }
    }
    if !o.status.success() {
        bail!("uber-apk-signer bao loi (ma {:?})", o.status.code());
    }
    let mut apks = Vec::new();
    for b in &built {
        let stem = b.file_stem().unwrap().to_string_lossy().into_owned();
        let got: Vec<PathBuf> = fs::read_dir(&signed)
            .context("uber-apk-signer khong tao thu muc dau ra")?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().unwrap().to_string_lossy().into_owned();
                n.starts_with(&format!("{stem}-aligned-")) && n.ends_with(".apk")
            })
            .collect();
        if got.len() != 1 {
            bail!("khong thay apk da ky cho {stem}");
        }
        let fin = signed.join(format!("{stem}.apk"));
        fs::rename(&got[0], &fin)?;
        apks.push(fin.to_string_lossy().into_owned());
    }
    for e in fs::read_dir(&signed)?.flatten() {
        if e.path().extension().is_some_and(|x| x == "idsig") {
            let _ = fs::remove_file(e.path());
        }
    }
    log(&format!("Da ky {} APK -> {}", apks.len(), signed.display()));
    Ok(PackResult { dir: signed.to_string_lossy().into_owned(), signed: true, apks, replaced })
}
