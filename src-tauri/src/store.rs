//! Luu tru cau hinh, lich su su dung va ban sao luu de rollback, trong thu muc Documents cua
//! tai khoan dang dang nhap: <Documents>\UnityL10nTool\
//!   config.json           cau hinh chuong trinh (theme, thoi han giu log, duong dan gan day...)
//!   history.jsonl         lich su (moi dong mot su kien JSON): mo/xuat/nhap/sua/luu/ky/rollback
//!   backups\<id>\         ban sao luu tung phien ban cua mot bundle da sua (de rollback)
//!     index.json            { work, split, entry, asset, versions:[{ts,size}] }
//!     <ts>.bin              noi dung bundle tai thoi diem ts
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Store {
    pub base: PathBuf,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Bam FNV-1a 64-bit -> hex 16 ky tu (dat ten thu muc an toan tu duong dan dai).
fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

impl Store {
    pub fn new(base: PathBuf) -> Self {
        Store { base }
    }
    fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.base)?;
        Ok(())
    }
    fn config_path(&self) -> PathBuf {
        self.base.join("config.json")
    }
    fn history_path(&self) -> PathBuf {
        self.base.join("history.jsonl")
    }
    fn backups(&self) -> PathBuf {
        self.base.join("backups")
    }
    fn bdir(&self, work: &str, split: &str, entry: &str) -> PathBuf {
        self.backups().join(fnv(&format!("{work}\u{1}{split}\u{1}{entry}")))
    }

    // ---------------------------------------------------------------- config
    pub fn load_config(&self) -> Value {
        fs::read(self.config_path())
            .ok()
            .and_then(|b| {
                // bo qua BOM UTF-8 neu nguoi dung tu sua file bang trinh soan thao khac
                let s = b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&b);
                serde_json::from_slice(s).ok()
            })
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Default::default()))
    }
    pub fn save_config(&self, v: &Value) -> Result<()> {
        self.ensure()?;
        fs::write(self.config_path(), serde_json::to_vec_pretty(v)?)?;
        Ok(())
    }
    pub fn retention_days(&self) -> i64 {
        self.load_config().get("retention_days").and_then(|v| v.as_i64()).unwrap_or(30)
    }

    // ---------------------------------------------------------------- history
    pub fn append_history(&self, mut entry: Value) -> Result<()> {
        self.ensure()?;
        if !entry.is_object() {
            entry = json!({ "type": entry });
        }
        if entry.get("ts").is_none() {
            entry["ts"] = json!(now_ms());
        }
        let mut f = fs::OpenOptions::new().create(true).append(true).open(self.history_path())?;
        writeln!(f, "{}", serde_json::to_string(&entry)?)?;
        Ok(())
    }
    pub fn list_history(&self, work: Option<&str>, limit: usize) -> Vec<Value> {
        let data = fs::read_to_string(self.history_path()).unwrap_or_default();
        let mut v: Vec<Value> = data.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        if let Some(w) = work {
            v.retain(|e| e.get("work").and_then(|x| x.as_str()).map_or(true, |x| x == w));
        }
        v.reverse();
        v.truncate(limit);
        v
    }

    // ---------------------------------------------------------------- backups / rollback
    fn read_index(&self, dir: &Path) -> Value {
        fs::read(dir.join("index.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| json!({ "versions": [] }))
    }

    /// Luu mot phien ban cua file bundle da sua. Bo qua neu giong het phien ban moi nhat.
    pub fn snapshot(&self, work: &str, split: &str, entry: &str, asset: &str, src: &Path) -> Result<Option<i64>> {
        let data = fs::read(src).with_context(|| format!("khong doc duoc {}", src.display()))?;
        let dir = self.bdir(work, split, entry);
        fs::create_dir_all(&dir)?;
        let mut idx = self.read_index(&dir);
        if idx["versions"].as_array().is_none() {
            idx["versions"] = json!([]);
        }
        // trung voi ban moi nhat -> bo qua
        if let Some(last) = idx["versions"].as_array().and_then(|a| a.last()) {
            if last["size"].as_u64() == Some(data.len() as u64) {
                let p = dir.join(format!("{}.bin", last["ts"].as_i64().unwrap_or(0)));
                if fs::read(&p).map(|b| b == data).unwrap_or(false) {
                    return Ok(None);
                }
            }
        }
        let ts = now_ms();
        fs::write(dir.join(format!("{ts}.bin")), &data)?;
        idx["work"] = json!(work);
        idx["split"] = json!(split);
        idx["entry"] = json!(entry);
        idx["asset"] = json!(asset);
        idx["versions"].as_array_mut().unwrap().push(json!({ "ts": ts, "size": data.len() }));
        fs::write(dir.join("index.json"), serde_json::to_vec_pretty(&idx)?)?;
        Ok(Some(ts))
    }

    pub fn list_snapshots(&self, work: &str, split: &str, entry: &str) -> Vec<Value> {
        let idx = self.read_index(&self.bdir(work, split, entry));
        let mut v = idx["versions"].as_array().cloned().unwrap_or_default();
        v.reverse();
        v
    }

    /// Phuc hoi mot phien ban ve `dest` (duong dan mod bundle). Tra ve so byte.
    pub fn restore(&self, work: &str, split: &str, entry: &str, ts: i64, dest: &Path) -> Result<u64> {
        let p = self.bdir(work, split, entry).join(format!("{ts}.bin"));
        if !p.is_file() {
            bail!("khong tim thay ban luu ts={ts}");
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(fs::copy(&p, dest)?)
    }

    // ---------------------------------------------------------------- don dep theo thoi han
    /// Xoa ban luu/lich su cu hon `days` ngay (0 = giu mai mai). Tra ve so muc da xoa.
    pub fn cleanup(&self, days: i64) -> Result<usize> {
        let mut removed = 0;
        // temp cua lan giai ma pak (neu con sot)
        if let Ok(rd) = fs::read_dir(std::env::temp_dir()) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("l10n-pak-") {
                    if let Ok(md) = e.metadata() {
                        if md.modified().ok().and_then(|m| m.elapsed().ok()).map(|d| d.as_secs() > 86400).unwrap_or(false) {
                            let _ = fs::remove_dir_all(e.path());
                        }
                    }
                }
            }
        }
        if days <= 0 {
            return Ok(0);
        }
        let cutoff = now_ms() - days * 86_400_000;
        // ban sao luu: bo phien ban cu, luon giu phien ban moi nhat
        if let Ok(rd) = fs::read_dir(self.backups()) {
            for e in rd.flatten().filter(|e| e.path().is_dir()) {
                let dir = e.path();
                let mut idx = self.read_index(&dir);
                let Some(vers) = idx["versions"].as_array().cloned() else { continue };
                if vers.is_empty() {
                    let _ = fs::remove_dir_all(&dir);
                    continue;
                }
                let keep_ts = vers.last().and_then(|x| x["ts"].as_i64()).unwrap_or(0);
                let mut left = Vec::new();
                for v in vers {
                    let ts = v["ts"].as_i64().unwrap_or(0);
                    if ts >= cutoff || ts == keep_ts {
                        left.push(v);
                    } else {
                        let _ = fs::remove_file(dir.join(format!("{ts}.bin")));
                        removed += 1;
                    }
                }
                idx["versions"] = json!(left);
                let _ = fs::write(dir.join("index.json"), serde_json::to_vec_pretty(&idx).unwrap_or_default());
            }
        }
        // lich su: giu dong moi hon cutoff, va toi da 5000 dong gan nhat
        let data = fs::read_to_string(self.history_path()).unwrap_or_default();
        if !data.is_empty() {
            let mut keep: Vec<&str> = data
                .lines()
                .filter(|l| {
                    serde_json::from_str::<Value>(l).ok().and_then(|e| e["ts"].as_i64()).map(|ts| ts >= cutoff).unwrap_or(true)
                })
                .collect();
            let before = data.lines().count();
            if keep.len() > 5000 {
                keep = keep.split_off(keep.len() - 5000);
            }
            if keep.len() != before {
                removed += before - keep.len();
                let _ = fs::write(self.history_path(), keep.join("\n") + "\n");
            }
        }
        Ok(removed)
    }

    pub fn data_dir(&self) -> String {
        self.base.to_string_lossy().into_owned()
    }
}
