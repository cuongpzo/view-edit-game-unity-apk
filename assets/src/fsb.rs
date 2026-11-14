//! FMOD Sound Bank (FSB5): liet ke stream va xuat ra file nghe duoc.
//! Vorbis -> dong goi lai vao Ogg (giu nguyen goi du lieu, khong nen lai); PCM16 -> WAV; MPEG -> MP3.
//! Bang setup header Vorbis (theo CRC32) lay tu du an fsbex (MIT/Apache-2.0).
use anyhow::{anyhow, bail, Result};
use l10n_core::unityfs::Rd;
use std::collections::HashMap;
use std::sync::OnceLock;

static SETUP_LZ4: &[u8] = include_bytes!("../data/vorbis_setup.lz4");

fn setups() -> &'static HashMap<u32, Vec<u8>> {
    static T: OnceLock<HashMap<u32, Vec<u8>>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m = HashMap::new();
        let n = u32::from_le_bytes(SETUP_LZ4[0..4].try_into().unwrap()) as usize;
        let raw_len = u32::from_le_bytes(SETUP_LZ4[4..8].try_into().unwrap()) as usize;
        let Ok(raw) = lz4_flex::block::decompress(&SETUP_LZ4[8..], raw_len) else { return m };
        let mut p = 0;
        for _ in 0..n {
            if p + 8 > raw.len() {
                break;
            }
            let crc = u32::from_le_bytes(raw[p..p + 4].try_into().unwrap());
            let len = u32::from_le_bytes(raw[p + 4..p + 8].try_into().unwrap()) as usize;
            p += 8;
            if p + len > raw.len() {
                break;
            }
            m.insert(crc, raw[p..p + len].to_vec());
            p += len;
        }
        m
    })
}

#[derive(Clone)]
pub struct Stream {
    pub name: String,
    pub rate: u32,
    pub channels: u8,
    pub samples: u32,
    /// vi tri du lieu tinh tu dau buffer chua bank
    pub off: usize,
    pub len: usize,
    pub crc: Option<u32>,
}

pub struct Bank {
    pub mode: u32,
    pub streams: Vec<Stream>,
    /// tong so byte cua bank (tinh tu chu ky FSB5)
    pub total: usize,
}

pub fn codec_name(mode: u32) -> &'static str {
    match mode {
        1 => "PCM8",
        2 => "PCM16",
        3 => "PCM24",
        4 => "PCM32",
        5 => "PCM float",
        6 => "GC ADPCM",
        7 => "IMA ADPCM",
        10 => "XMA",
        11 => "MPEG",
        12 => "CELT",
        13 => "AT9",
        15 => "Vorbis",
        16 => "FADPCM",
        17 => "Opus",
        _ => "?",
    }
}

/// Duoi file khi xuat stream cua bank voi codec `mode`.
pub fn ext(mode: u32) -> &'static str {
    match mode {
        15 => "ogg",
        2 => "wav",
        11 => "mp3",
        _ => "bin",
    }
}

/// Tim moi vi tri "FSB5" hop le trong buffer.
pub fn find_all(b: &[u8]) -> Vec<(usize, Bank)> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 60 <= b.len() {
        let Some(i) = b[p..].windows(4).position(|w| w == b"FSB5") else { break };
        let at = p + i;
        match parse(b, at) {
            Ok(bank) => {
                p = at + bank.total.max(4);
                out.push((at, bank));
            }
            Err(_) => p = at + 4,
        }
    }
    out
}

pub fn parse(b: &[u8], at: usize) -> Result<Bank> {
    let mut r = Rd::new(b, at, false);
    if r.take(4)? != b"FSB5" {
        bail!("khong phai FSB5");
    }
    let version = r.u32()?;
    let n = r.u32()? as usize;
    let shs = r.u32()? as usize;
    let nts = r.u32()? as usize;
    let ds = r.u32()? as usize;
    let mode = r.u32()?;
    if version > 1 || n == 0 || n > 200_000 || mode == 0 || mode > 17 {
        bail!("FSB5 header khong hop le");
    }
    let hs = if version == 0 { 64 } else { 60 };
    let data0 = at + hs + shs + nts;
    if data0.checked_add(ds).map_or(true, |e| e > b.len()) {
        bail!("FSB5 vuot qua du lieu");
    }
    const RATES: [u32; 11] = [4000, 8000, 11000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 96000];
    let mut r = Rd::new(&b[..at + hs + shs], at + hs, false);
    let mut streams = Vec::with_capacity(n);
    for _ in 0..n {
        let raw = r.u64()?;
        let mut more = raw & 1 != 0;
        let mut rate = RATES.get(((raw >> 1) & 0xf) as usize).copied().unwrap_or(44100);
        let mut channels = [1u8, 2, 6, 8][((raw >> 5) & 3) as usize];
        let off = ((raw >> 7) & 0x7ff_ffff) as usize * 32;
        let samples = ((raw >> 34) & 0x3fff_ffff) as u32;
        let mut crc = None;
        while more {
            let c = r.u32()?;
            more = c & 1 != 0;
            let size = ((c >> 1) & 0xff_ffff) as usize;
            let kind = c >> 25;
            let body = r.take(size)?;
            match kind {
                1 if size >= 1 => channels = body[0],
                2 if size >= 4 => rate = u32::from_le_bytes(body[..4].try_into().unwrap()),
                11 if size >= 4 => crc = Some(u32::from_le_bytes(body[..4].try_into().unwrap())),
                _ => {}
            }
        }
        streams.push(Stream { name: String::new(), rate, channels, samples, off: data0 + off, len: 0, crc });
    }
    for i in 0..n {
        let end = if i + 1 < n { streams[i + 1].off } else { data0 + ds };
        if end < streams[i].off || end > data0 + ds {
            bail!("FSB5: vi tri stream khong hop le");
        }
        streams[i].len = end - streams[i].off;
    }
    if nts >= n * 4 {
        let nt = at + hs + shs;
        for (i, s) in streams.iter_mut().enumerate() {
            let o = u32::from_le_bytes(b[nt + i * 4..nt + i * 4 + 4].try_into().unwrap()) as usize;
            if let Some(rest) = b.get(nt + o..nt + nts) {
                let e = rest.iter().position(|c| *c == 0).unwrap_or(rest.len());
                s.name = String::from_utf8_lossy(&rest[..e]).into_owned();
            }
        }
    }
    for (i, s) in streams.iter_mut().enumerate() {
        if s.name.is_empty() {
            s.name = format!("stream_{i}");
        }
    }
    Ok(Bank { mode, streams, total: hs + shs + nts + ds })
}

/// Xuat mot stream ra dinh dang nghe duoc. Tra ve (du lieu, duoi file).
pub fn export(b: &[u8], mode: u32, s: &Stream) -> Result<(Vec<u8>, &'static str)> {
    let data = b.get(s.off..s.off + s.len).ok_or_else(|| anyhow!("stream nam ngoai du lieu"))?;
    match mode {
        15 => Ok((vorbis_to_ogg(data, s)?, "ogg")),
        2 => Ok((wav16(data, s), "wav")),
        11 => Ok((data.to_vec(), "mp3")),
        m => bail!("chua ho tro xuat codec {} - chi luu duoc ca bank", codec_name(m)),
    }
}

fn wav16(data: &[u8], s: &Stream) -> Vec<u8> {
    let ch = s.channels.max(1) as u32;
    let n = if s.samples == 0 { data.len() as u32 } else { (data.len() as u32).min(s.samples.saturating_mul(ch * 2)) };
    let mut o = Vec::with_capacity(44 + n as usize);
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&(36 + n).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    o.extend_from_slice(&16u32.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&(ch as u16).to_le_bytes());
    o.extend_from_slice(&s.rate.to_le_bytes());
    o.extend_from_slice(&(s.rate * ch * 2).to_le_bytes());
    o.extend_from_slice(&((ch * 2) as u16).to_le_bytes());
    o.extend_from_slice(&16u16.to_le_bytes());
    o.extend_from_slice(b"data");
    o.extend_from_slice(&n.to_le_bytes());
    o.extend_from_slice(&data[..n as usize]);
    o
}

// ------------------------------------------------------------------ Ogg
fn ogg_crc(data: &[u8]) -> u32 {
    static T: OnceLock<[u32; 256]> = OnceLock::new();
    let t = T.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04c1_1db7 } else { r << 1 };
            }
            *e = r;
        }
        t
    });
    data.iter().fold(0u32, |c, b| (c << 8) ^ t[((c >> 24) as u8 ^ b) as usize])
}

struct Ogg {
    out: Vec<u8>,
    serial: u32,
    seq: u32,
    segs: Vec<u8>,
    body: Vec<u8>,
    /// granule cua goi cuoi cung KET THUC trong trang hien tai (-1 neu chua co)
    granule: i64,
    cont: bool,
}

impl Ogg {
    fn flush(&mut self, eos: bool, next_cont: bool) {
        if self.segs.is_empty() && !eos {
            return;
        }
        let mut flags = 0u8;
        if self.cont {
            flags |= 1;
        }
        if self.seq == 0 {
            flags |= 2;
        }
        if eos {
            flags |= 4;
        }
        let start = self.out.len();
        self.out.extend_from_slice(b"OggS");
        self.out.push(0);
        self.out.push(flags);
        self.out.extend_from_slice(&self.granule.to_le_bytes());
        self.out.extend_from_slice(&self.serial.to_le_bytes());
        self.out.extend_from_slice(&self.seq.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]);
        self.out.push(self.segs.len() as u8);
        self.out.extend_from_slice(&self.segs);
        self.out.extend_from_slice(&self.body);
        let crc = ogg_crc(&self.out[start..]);
        self.out[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
        self.seq += 1;
        self.segs.clear();
        self.body.clear();
        self.granule = -1;
        self.cont = next_cont;
    }

    /// Them mot goi; `granule` la vi tri mau sau khi giai ma xong goi nay.
    fn packet(&mut self, data: &[u8], granule: i64) {
        let mut rest = data;
        loop {
            let n = rest.len().min(255);
            self.segs.push(n as u8);
            self.body.extend_from_slice(&rest[..n]);
            rest = &rest[n..];
            let done = n < 255;
            if done {
                self.granule = granule;
            }
            if self.segs.len() == 255 {
                self.flush(false, !done);
            }
            if done {
                break;
            }
        }
    }
}

fn vorbis_to_ogg(data: &[u8], s: &Stream) -> Result<Vec<u8>> {
    use lewton::audio::get_decoded_sample_count;
    use lewton::header::{read_header_ident, read_header_setup};
    let crc = s.crc.ok_or_else(|| anyhow!("stream Vorbis thieu CRC cua setup header"))?;
    let setup = setups().get(&crc).ok_or_else(|| anyhow!("khong co setup header Vorbis cho CRC {crc:08x}"))?;
    // FMOD luon dung blocksize 256 / 2048
    let mut id = vec![1u8];
    id.extend_from_slice(b"vorbis");
    id.extend_from_slice(&[0; 4]);
    id.push(s.channels);
    id.extend_from_slice(&s.rate.to_le_bytes());
    id.extend_from_slice(&[0; 12]);
    id.push((11 << 4) | 8);
    id.push(1);
    let mut comment = vec![3u8];
    comment.extend_from_slice(b"vorbis");
    let vendor = b"UnityL10nTool";
    comment.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    comment.extend_from_slice(vendor);
    comment.extend_from_slice(&0u32.to_le_bytes());
    comment.push(1);
    let ident = read_header_ident(&id).map_err(|e| anyhow!("ident header: {e:?}"))?;
    let setup_h = read_header_setup(setup, s.channels, (8, 11)).map_err(|e| anyhow!("setup header: {e:?}"))?;

    let mut packets: Vec<&[u8]> = Vec::new();
    let mut p = 0;
    while p + 2 <= data.len() {
        let n = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        p += 2;
        if n == 0 || n == 0xffff || p + n > data.len() {
            break;
        }
        packets.push(&data[p..p + n]);
        p += n;
    }
    if packets.is_empty() {
        bail!("stream Vorbis khong co goi du lieu nao");
    }
    let mut o = Ogg { out: Vec::with_capacity(data.len() + data.len() / 40 + setup.len() + 256), serial: crc ^ s.off as u32, seq: 0, segs: Vec::new(), body: Vec::new(), granule: -1, cont: false };
    o.packet(&id, 0);
    o.flush(false, false);
    o.packet(&comment, 0);
    o.packet(setup, 0);
    o.flush(false, false);
    let total = s.samples as i64;
    let mut pos = 0i64;
    let last = packets.len() - 1;
    for (i, pk) in packets.iter().enumerate() {
        if i > 0 {
            pos += get_decoded_sample_count(&ident, &setup_h, pk).map_err(|e| anyhow!("goi Vorbis hong: {e:?}"))? as i64;
        }
        let g = if i == last && total > 0 { pos.min(total) } else { pos };
        o.packet(pk, g);
        if i == last {
            if o.segs.is_empty() {
                o.granule = g;
            }
            o.flush(true, false);
        } else if o.body.len() >= 4096 {
            o.flush(false, false);
        }
    }
    Ok(o.out)
}
