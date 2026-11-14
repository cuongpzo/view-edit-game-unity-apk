//! Doc/ghi bundle UnityFS (AssetBundle). Ho tro khoi nen: khong nen, LZMA, LZ4, LZ4HC.
//! YooAsset co the chen N byte truoc chu ky "UnityFS" -> luu vao `prefix` va ghi lai nguyen ven.
use anyhow::{anyhow, bail, Result};

pub const SIG: &[u8] = b"UnityFS";
const CHUNK: usize = 0x20000;

pub fn find_sig(raw: &[u8]) -> Option<usize> {
    let n = raw.len().min(4096);
    raw[..n].windows(SIG.len()).position(|w| w == SIG)
}

/// Bo doc nhi phan co kiem tra bien.
pub struct Rd<'a> {
    pub b: &'a [u8],
    pub p: usize,
    pub be: bool,
}

impl<'a> Rd<'a> {
    pub fn new(b: &'a [u8], p: usize, be: bool) -> Self {
        Rd { b, p, be }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.p.checked_add(n).filter(|e| *e <= self.b.len()).ok_or_else(|| anyhow!("doc qua cuoi du lieu"))?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        let a: [u8; 2] = self.take(2)?.try_into().unwrap();
        Ok(if self.be { u16::from_be_bytes(a) } else { u16::from_le_bytes(a) })
    }
    pub fn u32(&mut self) -> Result<u32> {
        let a: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(if self.be { u32::from_be_bytes(a) } else { u32::from_le_bytes(a) })
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64> {
        let a: [u8; 8] = self.take(8)?.try_into().unwrap();
        Ok(if self.be { u64::from_be_bytes(a) } else { u64::from_le_bytes(a) })
    }
    pub fn cstr(&mut self) -> Result<String> {
        let rest = self.b.get(self.p..).ok_or_else(|| anyhow!("doc qua cuoi du lieu"))?;
        let n = rest.iter().position(|c| *c == 0).ok_or_else(|| anyhow!("chuoi khong ket thuc"))?;
        let s = String::from_utf8_lossy(&rest[..n]).into_owned();
        self.p += n + 1;
        Ok(s)
    }
    /// Can le theo `n` byte, tinh tu vi tri `base`.
    pub fn align(&mut self, n: usize, base: usize) {
        let rel = self.p - base;
        self.p += (n - rel % n) % n;
    }
}

fn decompress(kind: u32, src: &[u8], out_len: usize) -> Result<Vec<u8>> {
    match kind {
        0 => Ok(src.to_vec()),
        1 => {
            use lzma_rs::decompress::{Options, UnpackedSize};
            let mut out = Vec::with_capacity(out_len);
            let opt = Options { unpacked_size: UnpackedSize::UseProvided(Some(out_len as u64)), memlimit: None, allow_incomplete: false };
            lzma_rs::lzma_decompress_with_options(&mut std::io::Cursor::new(src), &mut out, &opt)
                .map_err(|e| anyhow!("LZMA: {e:?}"))?;
            Ok(out)
        }
        2 | 3 => lz4_flex::block::decompress(src, out_len).map_err(|e| anyhow!("LZ4: {e}")),
        k => bail!("kieu nen khong ho tro: {k}"),
    }
}

pub struct Node {
    pub path: String,
    pub flags: u32,
    pub data: Vec<u8>,
}

pub struct Bundle {
    pub prefix: Vec<u8>,
    pub version: u32,
    pub player: String,
    pub engine: String,
    pub flags: u32,
    pub nodes: Vec<Node>,
}

impl Bundle {
    pub fn read(raw: &[u8]) -> Result<Bundle> {
        let off = find_sig(raw).ok_or_else(|| anyhow!("khong thay chu ky UnityFS (bundle bi ma hoa kieu khac?)"))?;
        let b = &raw[off..];
        let mut r = Rd::new(b, 0, true);
        r.cstr()?;
        let version = r.u32()?;
        let player = r.cstr()?;
        let engine = r.cstr()?;
        let _size = r.u64()?;
        let csize = r.u32()? as usize;
        let usize_ = r.u32()? as usize;
        let flags = r.u32()?;
        if version >= 7 {
            r.align(16, 0);
        }
        let binfo_c = if flags & 0x80 != 0 {
            b.get(b.len().saturating_sub(csize)..).ok_or_else(|| anyhow!("blocks info hong"))?
        } else {
            r.take(csize)?
        };
        let binfo = decompress(flags & 0x3F, binfo_c, usize_)?;
        if flags & 0x200 != 0 {
            r.align(16, 0);
        }
        let mut bi = Rd::new(&binfo, 16, true);
        let nblocks = bi.u32()? as usize;
        let mut data = Vec::new();
        for _ in 0..nblocks {
            let us = bi.u32()? as usize;
            let cs = bi.u32()? as usize;
            let bf = bi.u16()? as u32;
            data.extend_from_slice(&decompress(bf & 0x3F, r.take(cs)?, us)?);
        }
        let nnodes = bi.u32()? as usize;
        let mut nodes = Vec::new();
        for _ in 0..nnodes {
            let o = bi.u64()? as usize;
            let s = bi.u64()? as usize;
            let nf = bi.u32()?;
            let path = bi.cstr()?;
            let d = data.get(o..o.checked_add(s).unwrap_or(usize::MAX)).ok_or_else(|| anyhow!("node ngoai du lieu"))?;
            nodes.push(Node { path, flags: nf, data: d.to_vec() });
        }
        Ok(Bundle { prefix: raw[..off].to_vec(), version, player, engine, flags, nodes })
    }

    /// Ghi lai bundle: du lieu chia khoi 128KB nen LZ4, blocks info dat ngay sau header.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut dir = Vec::new();
        for n in &self.nodes {
            dir.push((data.len() as u64, n.data.len() as u64));
            data.extend_from_slice(&n.data);
        }
        let mut blocks = Vec::new();
        let mut table = Vec::new();
        for chunk in data.chunks(CHUNK) {
            let c = lz4_flex::block::compress(chunk);
            let (body, flag): (&[u8], u16) = if c.len() < chunk.len() { (&c, 3) } else { (chunk, 0) };
            table.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
            table.extend_from_slice(&(body.len() as u32).to_be_bytes());
            table.extend_from_slice(&flag.to_be_bytes());
            blocks.extend_from_slice(body);
        }
        let mut bi = vec![0u8; 16];
        bi.extend_from_slice(&((table.len() / 10) as u32).to_be_bytes());
        bi.extend_from_slice(&table);
        bi.extend_from_slice(&(self.nodes.len() as u32).to_be_bytes());
        for (n, (o, s)) in self.nodes.iter().zip(&dir) {
            bi.extend_from_slice(&o.to_be_bytes());
            bi.extend_from_slice(&s.to_be_bytes());
            bi.extend_from_slice(&n.flags.to_be_bytes());
            bi.extend_from_slice(n.path.as_bytes());
            bi.push(0);
        }
        let bi_c = lz4_flex::block::compress(&bi);
        let flags = (self.flags & !0x3F & !0x80) | 3;

        let mut h = Vec::new();
        h.extend_from_slice(SIG);
        h.push(0);
        h.extend_from_slice(&self.version.to_be_bytes());
        h.extend_from_slice(self.player.as_bytes());
        h.push(0);
        h.extend_from_slice(self.engine.as_bytes());
        h.push(0);
        let size_pos = h.len();
        h.extend_from_slice(&[0u8; 8]);
        h.extend_from_slice(&(bi_c.len() as u32).to_be_bytes());
        h.extend_from_slice(&(bi.len() as u32).to_be_bytes());
        h.extend_from_slice(&flags.to_be_bytes());
        let pad16 = |v: &mut Vec<u8>| v.resize(v.len() + (16 - v.len() % 16) % 16, 0);
        if self.version >= 7 {
            pad16(&mut h);
        }
        h.extend_from_slice(&bi_c);
        if flags & 0x200 != 0 {
            pad16(&mut h);
        }
        h.extend_from_slice(&blocks);
        let total = h.len() as u64;
        h[size_pos..size_pos + 8].copy_from_slice(&total.to_be_bytes());
        let mut out = self.prefix.clone();
        out.extend_from_slice(&h);
        Ok(out)
    }
}
