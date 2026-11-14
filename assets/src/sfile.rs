//! SerializedFile (ban 17+) kem type tree: doc doi tuong bat ky theo cau truc luu trong file,
//! khong phu thuoc layout cung cua tung phien ban Unity.
use anyhow::{anyhow, bail, Result};
use l10n_core::unityfs::Rd;

static COMMON: &[u8] = include_bytes!("../data/common_strings.bin");

#[derive(Clone, Debug)]
pub struct Node {
    pub level: u8,
    pub ty: String,
    pub name: String,
    pub meta: u32,
}

pub struct TypeInfo {
    pub class_id: i32,
    pub nodes: Vec<Node>,
}

pub struct Obj {
    pub path_id: i64,
    pub start: usize,
    pub size: usize,
    pub ty: usize,
}

pub struct SFile {
    pub raw: Vec<u8>,
    pub unity: String,
    pub types: Vec<TypeInfo>,
    pub objs: Vec<Obj>,
}

/// Gia tri doc tu type tree. Mang byte lon chi giu vi tri (offset tuyet doi trong `SFile::raw`, do dai).
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(usize, usize),
    Arr(Vec<Val>),
    Obj(Vec<(String, Val)>),
}

impl Val {
    pub fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Obj(f) => f.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// Di theo duong dan "a/b/c".
    pub fn at(&self, path: &str) -> Option<&Val> {
        path.split('/').try_fold(self, |v, k| v.get(k))
    }
    pub fn int(&self, path: &str) -> Option<i64> {
        match self.at(path)? {
            Val::Int(i) => Some(*i),
            Val::Float(f) => Some(*f as i64),
            _ => None,
        }
    }
    pub fn float(&self, path: &str) -> Option<f64> {
        match self.at(path)? {
            Val::Int(i) => Some(*i as f64),
            Val::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn str(&self, path: &str) -> Option<&str> {
        match self.at(path)? {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn bytes(&self, path: &str) -> Option<(usize, usize)> {
        match self.at(path)? {
            Val::Bytes(o, n) => Some((*o, *n)),
            _ => None,
        }
    }
    pub fn arr(&self, path: &str) -> Option<&[Val]> {
        match self.at(path)? {
            Val::Arr(a) => Some(a),
            _ => None,
        }
    }
}

fn tstr(local: &[u8], off: u32) -> String {
    let (buf, o) = if off & 0x8000_0000 != 0 { (COMMON, (off & 0x7fff_ffff) as usize) } else { (local, off as usize) };
    match buf.get(o..) {
        Some(rest) => {
            let n = rest.iter().position(|c| *c == 0).unwrap_or(rest.len());
            String::from_utf8_lossy(&rest[..n]).into_owned()
        }
        None => String::new(),
    }
}

impl SFile {
    /// Neu `raw` khong phai SerializedFile ho tro duoc thi tra lai nguyen buffer (vd. node .resS/.resource).
    pub fn parse(raw: Vec<u8>) -> std::result::Result<SFile, Vec<u8>> {
        match Self::parse_meta(&raw) {
            Ok((unity, types, objs)) => Ok(SFile { raw, unity, types, objs }),
            Err(_) => Err(raw),
        }
    }

    fn parse_meta(raw: &[u8]) -> Result<(String, Vec<TypeInfo>, Vec<Obj>)> {
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
        let unity = r.cstr()?;
        r.i32()?; // platform
        let has_tree = r.u8()? != 0;
        let ntypes = r.i32()?;
        if !(0..100_000).contains(&ntypes) {
            bail!("so kieu khong hop le");
        }
        let mut types = Vec::with_capacity(ntypes as usize);
        for _ in 0..ntypes {
            let class_id = r.i32()?;
            r.u8()?; // isStripped
            r.u16()?; // scriptTypeIndex
            if class_id == 114 {
                r.take(16)?;
            }
            r.take(16)?;
            let mut nodes = Vec::new();
            if has_tree {
                let n = r.i32()? as usize;
                let strs = r.i32()? as usize;
                let nsz = if version >= 19 { 32 } else { 24 };
                let nb = r.take(n.checked_mul(nsz).ok_or_else(|| anyhow!("type tree hong"))?)?;
                let sb = r.take(strs)?;
                nodes.reserve(n);
                for i in 0..n {
                    let mut q = Rd::new(nb, i * nsz, false);
                    q.u16()?;
                    let level = q.u8()?;
                    q.u8()?;
                    let ty = tstr(sb, q.u32()?);
                    let name = tstr(sb, q.u32()?);
                    q.i32()?; // byteSize
                    q.i32()?; // index
                    let meta = q.u32()?;
                    nodes.push(Node { level, ty, name, meta });
                }
                if version >= 21 {
                    let deps = r.i32()? as usize;
                    r.take(deps.checked_mul(4).ok_or_else(|| anyhow!("type tree hong"))?)?;
                }
            }
            types.push(TypeInfo { class_id, nodes });
        }
        let nobj = r.i32()?;
        if !(0..10_000_000).contains(&nobj) {
            bail!("so doi tuong khong hop le");
        }
        let mut objs = Vec::with_capacity(nobj as usize);
        for _ in 0..nobj {
            r.align(4, 0);
            let path_id = r.u64()? as i64;
            let start = if version >= 22 { r.u64()? } else { r.u32()? as u64 };
            let size = r.u32()? as usize;
            let tid = r.i32()?;
            let start = (data_offset as usize).checked_add(start as usize).ok_or_else(|| anyhow!("doi tuong nam ngoai file"))?;
            if start.checked_add(size).map_or(true, |e| e > raw.len()) {
                bail!("doi tuong nam ngoai file");
            }
            if tid < 0 || tid as usize >= types.len() {
                bail!("chi so kieu khong hop le");
            }
            objs.push(Obj { path_id, start, size, ty: tid as usize });
        }
        Ok((unity, types, objs))
    }

    pub fn class_id(&self, o: &Obj) -> i32 {
        self.types[o.ty].class_id
    }

    pub fn has_tree(&self, o: &Obj) -> bool {
        !self.types[o.ty].nodes.is_empty()
    }

    /// Ten lop theo type tree (neu co).
    pub fn class_name(&self, o: &Obj) -> Option<&str> {
        self.types[o.ty].nodes.first().map(|n| n.ty.as_str())
    }

    pub fn data(&self, o: &Obj) -> &[u8] {
        &self.raw[o.start..o.start + o.size]
    }

    /// m_Name neu no la truong dau tien (dung voi moi NamedObject) - khong can doc ca doi tuong.
    pub fn peek_name(&self, o: &Obj) -> Option<String> {
        let nodes = &self.types[o.ty].nodes;
        let n = nodes.get(1)?;
        if n.name != "m_Name" || n.ty != "string" {
            return None;
        }
        let mut r = Rd::new(self.data(o), 0, false);
        let len = r.u32().ok()? as usize;
        Some(String::from_utf8_lossy(r.take(len).ok()?).into_owned())
    }

    /// Doc toan bo doi tuong theo type tree.
    pub fn read(&self, o: &Obj) -> Result<Val> {
        let nodes = &self.types[o.ty].nodes;
        if nodes.is_empty() {
            bail!("file khong kem type tree - khong doc duoc doi tuong nay");
        }
        let mut r = Rd::new(&self.raw[..o.start + o.size], o.start, false);
        let mut i = 0;
        read_node(nodes, &mut i, &mut r, o.start)
    }
}

/// Chi so node ngay sau cay con bat dau tai `i`.
fn skip(nodes: &[Node], i: usize) -> usize {
    let lv = nodes[i].level;
    let mut j = i + 1;
    while j < nodes.len() && nodes[j].level > lv {
        j += 1;
    }
    j
}

fn read_node(nodes: &[Node], i: &mut usize, r: &mut Rd, base: usize) -> Result<Val> {
    let n = &nodes[*i];
    let end = skip(nodes, *i);
    let mut align = n.meta & 0x4000 != 0;
    let v = match n.ty.as_str() {
        "SInt8" => Val::Int(r.u8()? as i8 as i64),
        "UInt8" | "char" | "bool" => Val::Int(r.u8()? as i64),
        "SInt16" | "short" => Val::Int(r.u16()? as i16 as i64),
        "UInt16" | "unsigned short" => Val::Int(r.u16()? as i64),
        "SInt32" | "int" => Val::Int(r.i32()? as i64),
        "UInt32" | "unsigned int" | "Type*" => Val::Int(r.u32()? as i64),
        "SInt64" | "long long" => Val::Int(r.u64()? as i64),
        "UInt64" | "unsigned long long" | "FileSize" => Val::Int(r.u64()? as i64),
        "float" => Val::Float(f32::from_bits(r.u32()?) as f64),
        "double" => Val::Float(f64::from_bits(r.u64()?)),
        "string" => {
            let len = r.u32()? as usize;
            let s = String::from_utf8_lossy(r.take(len)?).into_owned();
            align = true;
            Val::Str(s)
        }
        "TypelessData" => {
            let len = r.u32()? as usize;
            let p = r.p;
            r.take(len)?;
            align = true;
            Val::Bytes(p, len)
        }
        _ => {
            if end > *i + 1 && nodes[*i + 1].ty == "Array" {
                // vector / map: Array { int size; T data }
                align |= nodes[*i + 1].meta & 0x4000 != 0;
                let len = r.u32()? as usize;
                let elem = *i + 3;
                if elem >= end {
                    bail!("type tree: mang thieu kieu phan tu");
                }
                let ety = nodes[elem].ty.as_str();
                if skip(nodes, elem) == elem + 1 && matches!(ety, "UInt8" | "char" | "SInt8") {
                    let p = r.p;
                    r.take(len)?;
                    Val::Bytes(p, len)
                } else {
                    if len > r.b.len().saturating_sub(r.p) {
                        bail!("mang dai bat thuong");
                    }
                    let mut a = Vec::with_capacity(len.min(4096));
                    for _ in 0..len {
                        let mut j = elem;
                        a.push(read_node(nodes, &mut j, r, base)?);
                    }
                    Val::Arr(a)
                }
            } else {
                let mut fields = Vec::new();
                let mut j = *i + 1;
                while j < end {
                    let name = nodes[j].name.clone();
                    let v = read_node(nodes, &mut j, r, base)?;
                    fields.push((name, v));
                }
                Val::Obj(fields)
            }
        }
    };
    if align {
        r.align(4, base);
    }
    *i = end;
    Ok(v)
}
