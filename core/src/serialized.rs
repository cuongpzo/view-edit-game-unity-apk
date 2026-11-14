//! SerializedFile cua Unity (ban >= 17, tuc Unity 2019+; little-endian).
//! Chi doc bang doi tuong de tim TextAsset va ghi lai du lieu doi tuong.
use crate::unityfs::Rd;
use anyhow::{bail, Result};
use std::collections::HashMap;

pub const CLASS_TEXT_ASSET: i32 = 49;

#[derive(Clone)]
pub struct Obj {
    pub path_id: i64,
    pub start: u64,
    pub size: u32,
    pub class_id: i32,
    entry: usize, // vi tri truong byteStart trong bang doi tuong
}

pub struct SFile {
    pub raw: Vec<u8>,
    pub version: u32,
    pub data_offset: usize,
    pub objs: Vec<Obj>,
}

impl SFile {
    pub fn parse(raw: &[u8]) -> Result<SFile> {
        let mut r = Rd::new(raw, 0, true);
        let _md = r.u32()?;
        let _fs = r.u32()?;
        let version = r.u32()?;
        let mut data_offset = r.u32()? as u64;
        if !(17..=40).contains(&version) {
            bail!("SerializedFile ban {version} chua ho tro");
        }
        let endian = r.u8()?;
        r.take(3)?;
        if version >= 22 {
            r.u32()?;
            r.u64()?;
            data_offset = r.u64()?;
            r.u64()?;
        }
        if endian != 0 {
            bail!("SerializedFile big-endian chua ho tro");
        }
        r.be = false;
        r.cstr()?; // unity version
        r.i32()?; // platform
        let type_tree = r.u8()? != 0;
        let ntypes = r.i32()?;
        let mut classes = Vec::new();
        for _ in 0..ntypes {
            let class_id = r.i32()?;
            r.u8()?; // isStripped
            r.u16()?; // scriptTypeIndex
            if class_id == 114 {
                r.take(16)?;
            }
            r.take(16)?;
            if type_tree {
                let nodes = r.i32()? as usize;
                let strs = r.i32()? as usize;
                r.take(nodes * if version >= 19 { 32 } else { 24 } + strs)?;
                if version >= 21 {
                    let deps = r.i32()? as usize;
                    r.take(deps * 4)?;
                }
            }
            classes.push(class_id);
        }
        let nobj = r.i32()?;
        let mut objs = Vec::new();
        for _ in 0..nobj {
            r.align(4, 0);
            let path_id = r.u64()? as i64;
            let entry = r.p;
            let start = if version >= 22 { r.u64()? } else { r.u32()? as u64 };
            let size = r.u32()?;
            let tid = r.i32()?;
            let class_id = *classes.get(tid as usize).unwrap_or(&-1);
            objs.push(Obj { path_id, start, size, class_id, entry });
        }
        let data_offset = data_offset as usize;
        for o in &objs {
            if data_offset + o.start as usize + o.size as usize > raw.len() {
                bail!("doi tuong nam ngoai file");
            }
        }
        Ok(SFile { raw: raw.to_vec(), version, data_offset, objs })
    }

    pub fn obj_data(&self, o: &Obj) -> &[u8] {
        let s = self.data_offset + o.start as usize;
        &self.raw[s..s + o.size as usize]
    }

    /// Ghi lai toan bo vung du lieu, thay cac doi tuong co trong `repl` (theo path_id).
    pub fn rebuild(&self, repl: &HashMap<i64, Vec<u8>>) -> Vec<u8> {
        let mut out = self.raw[..self.data_offset].to_vec();
        let mut order: Vec<&Obj> = self.objs.iter().collect();
        order.sort_by_key(|o| o.start);
        for o in order {
            let rel = out.len() - self.data_offset;
            out.resize(out.len() + (8 - rel % 8) % 8, 0);
            let start = (out.len() - self.data_offset) as u64;
            let d: &[u8] = repl.get(&o.path_id).map(|v| v.as_slice()).unwrap_or_else(|| self.obj_data(o));
            let size = d.len() as u32;
            out.extend_from_slice(d);
            if self.version >= 22 {
                out[o.entry..o.entry + 8].copy_from_slice(&start.to_le_bytes());
                out[o.entry + 8..o.entry + 12].copy_from_slice(&size.to_le_bytes());
            } else {
                out[o.entry..o.entry + 4].copy_from_slice(&(start as u32).to_le_bytes());
                out[o.entry + 4..o.entry + 8].copy_from_slice(&size.to_le_bytes());
            }
        }
        let total = out.len() as u64;
        if self.version >= 22 {
            out[24..32].copy_from_slice(&total.to_be_bytes());
        } else {
            out[4..8].copy_from_slice(&(total as u32).to_be_bytes());
        }
        out
    }
}

/// TextAsset: m_Name (chuoi) + m_Script (mang byte). Tra ve (ten, noi dung, vi tri truong m_Script).
pub fn text_asset(obj: &[u8]) -> Result<(String, &[u8], usize)> {
    let mut r = Rd::new(obj, 0, false);
    let n = r.u32()? as usize;
    let name = String::from_utf8_lossy(r.take(n)?).into_owned();
    r.align(4, 0);
    let pos = r.p;
    let n = r.u32()? as usize;
    Ok((name, r.take(n)?, pos))
}

/// Tao lai du lieu TextAsset voi noi dung moi (giu nguyen ten va phan duoi neu co).
pub fn text_asset_with(obj: &[u8], script: &[u8]) -> Result<Vec<u8>> {
    let (_name, old, pos) = text_asset(obj)?;
    let mut end = pos + 4 + old.len();
    end += (4 - end % 4) % 4;
    let mut out = obj[..pos].to_vec();
    out.extend_from_slice(&(script.len() as u32).to_le_bytes());
    out.extend_from_slice(script);
    out.resize(out.len() + (4 - out.len() % 4) % 4, 0);
    if end < obj.len() {
        out.extend_from_slice(&obj[end..]);
    }
    Ok(out)
}
