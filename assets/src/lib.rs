//! Duyet va trich xuat tai nguyen trong file Unity (bundle UnityFS / SerializedFile):
//! Texture2D, Sprite -> PNG; FMOD bank / AudioClip -> OGG/WAV/MP3; VideoClip -> MP4; TextAsset; Font.
pub mod fsb;
pub mod sfile;
pub mod tex;

use anyhow::{anyhow, bail, Context, Result};
use l10n_core::flat::FlatTable;
use l10n_core::unityfs::{self, Bundle};
use serde::{Deserialize, Serialize};
use sfile::{Obj, SFile, Val};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use tex::Image;

const TEXTURE2D: i32 = 28;
const TEXT_ASSET: i32 = 49;
const AUDIO_CLIP: i32 = 83;
const FONT: i32 = 128;
const SPRITE: i32 = 213;
const VIDEO_CLIP: i32 = 329;

fn class_name(id: i32) -> String {
    let s = match id {
        1 => "GameObject",
        4 => "Transform",
        21 => "Material",
        23 => "MeshRenderer",
        28 => "Texture2D",
        33 => "MeshFilter",
        43 => "Mesh",
        48 => "Shader",
        49 => "TextAsset",
        74 => "AnimationClip",
        83 => "AudioClip",
        91 => "AnimatorController",
        95 => "Animator",
        114 => "MonoBehaviour",
        115 => "MonoScript",
        128 => "Font",
        142 => "AssetBundle",
        198 => "ParticleSystem",
        212 => "SpriteRenderer",
        213 => "Sprite",
        224 => "RectTransform",
        329 => "VideoClip",
        687078895 => "SpriteAtlas",
        _ => return format!("class {id}"),
    };
    s.to_string()
}

/// Loai file roi tren dia theo duoi: image / audio / video, hoac "" neu khong phai media.
pub fn media_kind(path: &Path) -> &'static str {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" | "avif" => "image",
        "mp3" | "ogg" | "wav" | "m4a" | "aac" | "flac" | "opus" | "oga" | "weba" | "mid" | "midi" | "amr" | "3ga" => "audio",
        "mp4" | "webm" | "m4v" | "mov" | "mkv" | "ogv" => "video",
        _ => "",
    }
}

pub fn mime_of(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "m4a" | "aac" | "3ga" => "audio/mp4",
        "weba" => "audio/webm",
        "mid" | "midi" => "audio/midi",
        "amr" => "audio/amr",
        "flac" => "audio/flac",
        "mp4" | "m4v" | "mov" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "ogv" => "video/ogg",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "txt" | "tsv" | "json" | "xml" | "lua" | "csv" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// File nay co mo duoc nhu mot "kho" tai nguyen Unity khong (bundle UnityFS hoac SerializedFile roi).
/// File am thanh FMOD roi tren dia (.bank / .fsb): bat dau bang "FSB5" hoac "RIFF....FEV ".
pub fn is_sound_bank(path: &Path) -> bool {
    let mut head = [0u8; 12];
    let Ok(mut f) = fs::File::open(path) else { return false };
    if f.read_exact(&mut head).is_err() {
        return false;
    }
    &head[..4] == b"FSB5" || (&head[..4] == b"RIFF" && &head[8..12] == b"FEV ")
}

pub fn is_container(path: &Path) -> bool {
    let mut head = [0u8; 4096];
    let Ok(mut f) = fs::File::open(path) else { return false };
    let n = f.read(&mut head).unwrap_or(0);
    let head = &head[..n];
    if unityfs::find_sig(head).is_some() {
        return true;
    }
    if n >= 12 && (&head[..4] == b"FSB5" || (&head[..4] == b"RIFF" && &head[8..12] == b"FEV ")) {
        return true;
    }
    if !media_kind(path).is_empty() || n < 48 {
        return false;
    }
    // SerializedFile: version (BE) o byte 8..12, endian o byte 16, 3 byte du tru = 0
    let ver = u32::from_be_bytes([head[8], head[9], head[10], head[11]]);
    (17..=40).contains(&ver) && head[16] <= 1 && head[17..20] == [0, 0, 0] && head[0..3] != *b"PK\x03"
}

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    /// "<chi so file>:<path id>[:<chi so stream>]"
    pub id: String,
    pub class: String,
    pub name: String,
    /// image | sprite | audio | video | text | table | bank | font | data | other
    pub kind: &'static str,
    pub size: u64,
    pub info: String,
    /// duoi file khi trich xuat
    pub ext: String,
}

pub struct Blob {
    pub data: Vec<u8>,
    pub ext: String,
}

pub struct Opened {
    pub path: PathBuf,
    pub unity: String,
    files: Vec<(String, SFile)>,
    res: Vec<(String, Vec<u8>)>,
    /// file FMOD bank roi (khong phai bundle Unity)
    bank: Option<Vec<u8>>,
    tex: Mutex<Vec<((usize, i64), Arc<Image>)>>,
    vals: Mutex<HashMap<(usize, i64), Arc<Val>>>,
}

fn sanitize(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || (c as u32) < 32 { '_' } else { c })
        .collect();
    s = s.trim().trim_end_matches('.').to_string();
    if s.chars().count() > 120 {
        s = s.chars().take(120).collect();
    }
    s
}

fn text_ext(data: &[u8], name: &str) -> &'static str {
    let lower = name.to_lowercase();
    for e in ["json", "xml", "lua", "csv", "txt", "html", "yaml", "proto"] {
        if lower.ends_with(&format!(".{e}")) {
            return e;
        }
    }
    let t = data.iter().copied().find(|c| !c.is_ascii_whitespace() && *c != 0xef && *c != 0xbb && *c != 0xbf);
    match t {
        Some(b'{') | Some(b'[') => "json",
        Some(b'<') => "xml",
        _ => "txt",
    }
}

/// Du lieu co phai van ban UTF-8 khong (kiem tra phan dau, cho phep cat giua ky tu).
pub fn looks_text(data: &[u8]) -> bool {
    let head = &data[..data.len().min(65536)];
    if head.is_empty() || head.iter().any(|c| *c == 0 || (*c < 9) || (*c > 13 && *c < 32 && *c != 27)) {
        return false;
    }
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => head.len() - e.valid_up_to() < 4 && head.len() == 65536,
    }
}

fn try_table(data: &[u8]) -> Option<FlatTable> {
    if data.len() < 16 || data.len() > 256 << 20 {
        return None;
    }
    let root = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    if root < 4 || root >= data.len() {
        return None;
    }
    let d = data.to_vec();
    let ft = std::panic::catch_unwind(move || FlatTable::parse(d)).ok()??;
    if ft.str_cols.is_empty() || ft.nrows() == 0 {
        return None;
    }
    Some(ft)
}

fn table_tsv(ft: &FlatTable) -> String {
    let mut s = String::from("row");
    for c in &ft.str_cols {
        s.push_str(&format!("\tf{c}"));
    }
    s.push('\n');
    for r in 0..ft.nrows() {
        s.push_str(&r.to_string());
        for c in &ft.str_cols {
            s.push('\t');
            for ch in ft.get(r, *c).unwrap_or("").chars() {
                match ch {
                    '\t' => s.push_str("\\t"),
                    '\n' => s.push_str("\\n"),
                    '\r' => s.push_str("\\r"),
                    '\\' => s.push_str("\\\\"),
                    c => s.push(c),
                }
            }
        }
        s.push('\n');
    }
    s
}

fn parse_id(id: &str) -> Result<(usize, i64, Option<usize>)> {
    let mut it = id.split(':');
    let bad = || anyhow!("ma doi tuong khong hop le: {id}");
    let fi = it.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let pid = it.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let sub = match it.next() {
        Some(s) => Some(s.parse().map_err(|_| bad())?),
        None => None,
    };
    Ok((fi, pid, sub))
}

fn text_asset<'a>(sf: &'a SFile, o: &Obj) -> Result<(String, &'a [u8])> {
    let (name, data, _) = l10n_core::serialized::text_asset(sf.data(o))?;
    Ok((name, data))
}

/// Cac stream am thanh trong mot buffer: (vi tri bank, codec, stream).
fn fsb_streams(data: &[u8]) -> Vec<(u32, fsb::Stream)> {
    let mut out = Vec::new();
    for (_, bank) in fsb::find_all(data) {
        for s in bank.streams {
            out.push((bank.mode, s));
        }
    }
    out
}

fn audio_info(mode: u32, s: &fsb::Stream) -> String {
    let secs = if s.rate > 0 { s.samples as f64 / s.rate as f64 } else { 0.0 };
    format!("{} · {} Hz · {} kênh · {:.1}s", fsb::codec_name(mode), s.rate, s.channels, secs)
}

impl Opened {
    pub fn open(path: &Path) -> Result<Opened> {
        let raw = fs::read(path).with_context(|| format!("khong doc duoc {}", path.display()))?;
        let mut files = Vec::new();
        let mut res = Vec::new();
        if unityfs::find_sig(&raw).is_some() {
            let b = Bundle::read(&raw)?;
            drop(raw);
            for n in b.nodes {
                let lower = n.path.to_lowercase();
                if lower.ends_with(".ress") || lower.ends_with(".resource") {
                    res.push((n.path, n.data));
                    continue;
                }
                match SFile::parse(n.data) {
                    Ok(sf) => files.push((n.path, sf)),
                    Err(d) => res.push((n.path, d)),
                }
            }
        } else {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match SFile::parse(raw) {
                Ok(sf) => files.push((name, sf)),
                // file FMOD roi (.bank / .fsb): chi chua am thanh
                Err(raw) if !fsb::find_all(&raw).is_empty() => {
                    return Ok(Opened { path: path.to_path_buf(), unity: String::new(), files, res, bank: Some(raw), tex: Mutex::new(Vec::new()), vals: Mutex::new(HashMap::new()) });
                }
                Err(_) => bail!("khong phai bundle UnityFS, SerializedFile hay FMOD bank ho tro duoc"),
            }
        }
        let unity = files.first().map(|(_, f)| f.unity.clone()).unwrap_or_default();
        Ok(Opened { path: path.to_path_buf(), unity, files, res, bank: None, tex: Mutex::new(Vec::new()), vals: Mutex::new(HashMap::new()) })
    }

    /// Du lieu ngoai (.resS / .resource): trong bundle, hoac file cung thu muc.
    fn resource(&self, src: &str, off: u64, size: u64) -> Result<Cow<'_, [u8]>> {
        let name = src.rsplit('/').next().unwrap_or(src);
        let (off, size) = (off as usize, size as usize);
        if let Some((_, d)) = self.res.iter().find(|(n, _)| n.rsplit('/').next().unwrap_or(n) == name) {
            let s = d.get(off..off.checked_add(size).unwrap_or(usize::MAX)).ok_or_else(|| anyhow!("{name}: vung du lieu nam ngoai file"))?;
            return Ok(Cow::Borrowed(s));
        }
        let p = self.path.parent().unwrap_or(Path::new(".")).join(name);
        let mut f = fs::File::open(&p).with_context(|| format!("khong thay du lieu ngoai {name}"))?;
        if size > 1 << 31 {
            bail!("{name}: kich thuoc bat thuong");
        }
        f.seek(SeekFrom::Start(off as u64))?;
        let mut buf = vec![0u8; size];
        f.read_exact(&mut buf).with_context(|| format!("{name}: doc thieu du lieu"))?;
        Ok(Cow::Owned(buf))
    }

    fn find(&self, fi: usize, pid: i64) -> Result<(&SFile, &Obj)> {
        let (_, sf) = self.files.get(fi).ok_or_else(|| anyhow!("khong co file thu {fi} trong bundle"))?;
        let o = sf.objs.iter().find(|o| o.path_id == pid).ok_or_else(|| anyhow!("khong thay doi tuong {pid}"))?;
        Ok((sf, o))
    }

    fn val(&self, fi: usize, pid: i64) -> Result<Arc<Val>> {
        if let Some(v) = self.vals.lock().unwrap().get(&(fi, pid)) {
            return Ok(v.clone());
        }
        let (sf, o) = self.find(fi, pid)?;
        let v = Arc::new(sf.read(o)?);
        let mut g = self.vals.lock().unwrap();
        if g.len() > 64 {
            g.clear();
        }
        g.insert((fi, pid), v.clone());
        Ok(v)
    }

    pub fn list(&self, all: bool) -> Vec<Entry> {
        let mut out = Vec::new();
        if let Some(raw) = &self.bank {
            for (k, (mode, s)) in fsb_streams(raw).iter().enumerate() {
                out.push(Entry { id: format!("s:{k}"), class: "FSB5".into(), name: s.name.clone(), kind: "audio", size: s.len as u64, info: audio_info(*mode, s), ext: fsb::ext(*mode).into() });
            }
            return out;
        }
        for (fi, (_, sf)) in self.files.iter().enumerate() {
            for o in &sf.objs {
                let cid = sf.class_id(o);
                let class = sf.class_name(o).map(str::to_string).unwrap_or_else(|| class_name(cid));
                let id = format!("{fi}:{}", o.path_id);
                let mut e = Entry { id, class, name: String::new(), kind: "other", size: o.size as u64, info: String::new(), ext: String::new() };
                match cid {
                    TEXTURE2D => match sf.read(o) {
                        Ok(v) => {
                            e.name = v.str("m_Name").unwrap_or("").to_string();
                            let (w, h, f) = (v.int("m_Width").unwrap_or(0), v.int("m_Height").unwrap_or(0), v.int("m_TextureFormat").unwrap_or(0));
                            let inline = v.bytes("image data").map(|b| b.1).unwrap_or(0) as u64;
                            e.size = if inline > 0 { inline } else { v.int("m_StreamData/size").unwrap_or(0) as u64 };
                            e.info = format!("{w}×{h} · {}", tex::format_name(f));
                            if e.size > 0 {
                                e.kind = "image";
                                e.ext = "png".into();
                            } else {
                                e.info.push_str(" · không kèm dữ liệu ảnh");
                            }
                        }
                        Err(err) => e.info = format!("{err}"),
                    },
                    SPRITE => {
                        e.name = sf.peek_name(o).unwrap_or_default();
                        if sf.has_tree(o) {
                            e.kind = "sprite";
                            e.ext = "png".into();
                        }
                    }
                    TEXT_ASSET => {
                        let Ok((name, data)) = text_asset(sf, o) else { continue };
                        e.name = name;
                        e.size = data.len() as u64;
                        let streams = fsb_streams(data);
                        if !streams.is_empty() {
                            e.kind = "bank";
                            e.ext = if e.name.to_lowercase().ends_with(".bank") { String::new() } else { "bank".into() };
                            e.info = format!("FMOD bank · {} âm thanh", streams.len());
                            let bank_name = e.name.clone();
                            let base = e.id.clone();
                            out.push(e);
                            for (k, (mode, s)) in streams.iter().enumerate() {
                                out.push(Entry {
                                    id: format!("{base}:{k}"),
                                    class: "FSB5".into(),
                                    name: format!("{} / {}", bank_name.trim_end_matches(".bank"), s.name),
                                    kind: "audio",
                                    size: s.len as u64,
                                    info: audio_info(*mode, s),
                                    ext: fsb::ext(*mode).into(),
                                });
                            }
                            continue;
                        } else if let Some(ft) = try_table(data) {
                            e.kind = "table";
                            e.ext = "tsv".into();
                            e.info = format!("bảng FlatBuffers · {} dòng · {} cột chuỗi", ft.nrows(), ft.str_cols.len());
                        } else if looks_text(data) {
                            e.kind = "text";
                            e.ext = text_ext(data, &e.name).into();
                        } else {
                            e.kind = "data";
                            e.ext = "bytes".into();
                        }
                        if e.name.to_lowercase().ends_with(&format!(".{}", e.ext)) {
                            e.ext.clear();
                        }
                    }
                    AUDIO_CLIP => match sf.read(o) {
                        Ok(v) => {
                            e.name = v.str("m_Name").unwrap_or("").to_string();
                            e.size = v.int("m_Resource/m_Size").unwrap_or(0) as u64;
                            e.info = format!(
                                "{} Hz · {} kênh · {:.1}s",
                                v.int("m_Frequency").unwrap_or(0),
                                v.int("m_Channels").unwrap_or(0),
                                v.float("m_Length").unwrap_or(0.0)
                            );
                            e.kind = "audio";
                            e.ext = "ogg".into();
                        }
                        Err(err) => e.info = format!("{err}"),
                    },
                    VIDEO_CLIP => match sf.read(o) {
                        Ok(v) => {
                            e.name = v.str("m_Name").unwrap_or("").to_string();
                            e.size = v.int("m_ExternalResources/m_Size").unwrap_or(0) as u64;
                            e.info = format!("{}×{}", v.int("Width").unwrap_or(0), v.int("Height").unwrap_or(0));
                            e.kind = "video";
                            let orig = v.str("m_OriginalPath").unwrap_or("").to_lowercase();
                            let ext = orig.rsplit('.').next().unwrap_or("");
                            e.ext = if matches!(ext, "mp4" | "webm" | "mov" | "m4v" | "ogv" | "mkv") { ext.to_string() } else { "mp4".into() };
                        }
                        Err(err) => e.info = format!("{err}"),
                    },
                    FONT => {
                        e.name = sf.peek_name(o).unwrap_or_default();
                        if let Ok(v) = sf.read(o) {
                            if let Some((off, n)) = v.bytes("m_FontData").filter(|b| b.1 > 4) {
                                e.kind = "font";
                                e.size = n as u64;
                                e.ext = if &sf.raw[off..off + 4] == b"OTTO" { "otf".into() } else { "ttf".into() };
                            }
                        }
                    }
                    _ => e.name = sf.peek_name(o).unwrap_or_default(),
                }
                if e.kind == "other" && !all {
                    continue;
                }
                out.push(e);
            }
        }
        // nhom theo loai; bank va cac stream cua no giu nguyen thu tu (sort on dinh)
        let group = |k: &str| match k {
            "table" => 0,
            "text" => 1,
            "image" => 2,
            "sprite" => 3,
            "bank" | "audio" => 4,
            "video" => 5,
            "font" => 6,
            "data" => 7,
            _ => 8,
        };
        out.sort_by(|a, b| {
            let (ga, gb) = (group(a.kind), group(b.kind));
            ga.cmp(&gb).then_with(|| if ga == 4 { std::cmp::Ordering::Equal } else { a.name.to_lowercase().cmp(&b.name.to_lowercase()) })
        });
        out
    }

    fn texture(&self, fi: usize, pid: i64) -> Result<Arc<Image>> {
        if let Some((_, img)) = self.tex.lock().unwrap().iter().find(|(k, _)| *k == (fi, pid)) {
            return Ok(img.clone());
        }
        let (sf, o) = self.find(fi, pid)?;
        if sf.class_id(o) != TEXTURE2D {
            bail!("doi tuong {pid} khong phai Texture2D");
        }
        let v = sf.read(o)?;
        let w = v.int("m_Width").unwrap_or(0) as usize;
        let h = v.int("m_Height").unwrap_or(0) as usize;
        let f = v.int("m_TextureFormat").unwrap_or(0);
        let inline = v.bytes("image data").filter(|b| b.1 > 0);
        let img = match inline {
            Some((off, n)) => tex::decode(f, w, h, &sf.raw[off..off + n])?,
            None => {
                let path = v.str("m_StreamData/path").unwrap_or("");
                if path.is_empty() {
                    bail!("texture khong co du lieu anh");
                }
                let d = self.resource(path, v.int("m_StreamData/offset").unwrap_or(0) as u64, v.int("m_StreamData/size").unwrap_or(0) as u64)?;
                tex::decode(f, w, h, &d)?
            }
        };
        let img = Arc::new(img);
        let mut g = self.tex.lock().unwrap();
        if g.len() >= 6 {
            g.remove(0);
        }
        g.push(((fi, pid), img.clone()));
        Ok(img)
    }

    fn sprite(&self, fi: usize, pid: i64) -> Result<Image> {
        let v = self.val(fi, pid)?;
        let rect = |r: &Val| -> Option<(f64, f64, f64, f64)> { Some((r.float("x")?, r.float("y")?, r.float("width")?, r.float("height")?)) };
        let mut tex_ptr = (v.int("m_RD/texture/m_FileID").unwrap_or(0), v.int("m_RD/texture/m_PathID").unwrap_or(0));
        let mut r = v.at("m_RD/textureRect").and_then(rect);
        if tex_ptr.1 == 0 {
            // sprite nam trong SpriteAtlas: tra theo m_RenderDataKey
            let ap = (v.int("m_SpriteAtlas/m_FileID").unwrap_or(0), v.int("m_SpriteAtlas/m_PathID").unwrap_or(0));
            if ap.1 == 0 {
                bail!("sprite khong tro toi texture nao");
            }
            if ap.0 != 0 {
                bail!("SpriteAtlas cua sprite nam o bundle khac");
            }
            let atlas = self.val(fi, ap.1)?;
            let key = v.at("m_RenderDataKey").ok_or_else(|| anyhow!("sprite thieu m_RenderDataKey"))?;
            let hit = atlas
                .arr("m_RenderDataMap")
                .and_then(|a| a.iter().find(|p| p.get("first") == Some(key)))
                .and_then(|p| p.get("second"))
                .ok_or_else(|| anyhow!("khong thay sprite trong SpriteAtlas"))?;
            tex_ptr = (hit.int("texture/m_FileID").unwrap_or(0), hit.int("texture/m_PathID").unwrap_or(0));
            r = hit.get("textureRect").and_then(rect);
        }
        if tex_ptr.0 != 0 {
            bail!("texture cua sprite nam o bundle khac");
        }
        let (x, y, w, h) = r.ok_or_else(|| anyhow!("sprite thieu textureRect"))?;
        self.texture(fi, tex_ptr.1)?.crop_unity(x, y, w, h)
    }

    /// Anh thu nho PNG cua Texture2D / Sprite.
    pub fn thumb_png(&self, id: &str, max: usize) -> Result<Vec<u8>> {
        let (fi, pid, _) = parse_id(id)?;
        let (sf, o) = self.find(fi, pid)?;
        match sf.class_id(o) {
            TEXTURE2D => self.texture(fi, pid)?.thumb(max).png(),
            SPRITE => self.sprite(fi, pid)?.thumb(max).png(),
            _ => bail!("doi tuong khong phai anh"),
        }
    }

    /// Xuat doi tuong ra dang dung duoc. `as_text`: bang FlatBuffers -> TSV thay vi byte goc.
    pub fn render(&self, id: &str, as_text: bool) -> Result<Blob> {
        if let (Some(raw), Some(k)) = (&self.bank, id.strip_prefix("s:")) {
            let k: usize = k.parse().map_err(|_| anyhow!("ma stream khong hop le: {id}"))?;
            let streams = fsb_streams(raw);
            let (mode, s) = streams.get(k).ok_or_else(|| anyhow!("khong co stream thu {k}"))?;
            let (d, ext) = fsb::export(raw, *mode, s)?;
            return Ok(Blob { data: d, ext: ext.into() });
        }
        let (fi, pid, sub) = parse_id(id)?;
        let (sf, o) = self.find(fi, pid)?;
        match sf.class_id(o) {
            TEXTURE2D => Ok(Blob { data: self.texture(fi, pid)?.png()?, ext: "png".into() }),
            SPRITE => Ok(Blob { data: self.sprite(fi, pid)?.png()?, ext: "png".into() }),
            TEXT_ASSET => {
                let (name, data) = text_asset(sf, o)?;
                if let Some(k) = sub {
                    let streams = fsb_streams(data);
                    let (mode, s) = streams.get(k).ok_or_else(|| anyhow!("khong co stream thu {k}"))?;
                    let (d, ext) = fsb::export(data, *mode, s)?;
                    return Ok(Blob { data: d, ext: ext.into() });
                }
                if as_text {
                    if let Some(ft) = try_table(data) {
                        return Ok(Blob { data: table_tsv(&ft).into_bytes(), ext: "tsv".into() });
                    }
                }
                let ext = if !fsb::find_all(data).is_empty() {
                    "bank"
                } else if looks_text(data) {
                    text_ext(data, &name)
                } else {
                    "bytes"
                };
                Ok(Blob { data: data.to_vec(), ext: ext.into() })
            }
            AUDIO_CLIP => {
                let v = sf.read(o)?;
                let src = v.str("m_Resource/m_Source").unwrap_or("");
                let d = self.resource(src, v.int("m_Resource/m_Offset").unwrap_or(0) as u64, v.int("m_Resource/m_Size").unwrap_or(0) as u64)?;
                let streams = fsb_streams(&d);
                let (mode, s) = streams.get(sub.unwrap_or(0)).ok_or_else(|| anyhow!("AudioClip khong chua FSB5"))?;
                let (d, ext) = fsb::export(&d, *mode, s)?;
                Ok(Blob { data: d, ext: ext.into() })
            }
            VIDEO_CLIP => {
                let v = sf.read(o)?;
                let src = v.str("m_ExternalResources/m_Source").unwrap_or("");
                let d = self.resource(src, v.int("m_ExternalResources/m_Offset").unwrap_or(0) as u64, v.int("m_ExternalResources/m_Size").unwrap_or(0) as u64)?;
                let orig = v.str("m_OriginalPath").unwrap_or("").to_lowercase();
                let ext = orig.rsplit('.').next().unwrap_or("");
                let ext = if matches!(ext, "mp4" | "webm" | "mov" | "m4v" | "ogv" | "mkv") { ext } else { "mp4" };
                Ok(Blob { data: d.into_owned(), ext: ext.into() })
            }
            FONT => {
                let v = sf.read(o)?;
                let (off, n) = v.bytes("m_FontData").filter(|b| b.1 > 4).ok_or_else(|| anyhow!("font khong kem du lieu (font he thong)"))?;
                let d = &sf.raw[off..off + n];
                Ok(Blob { data: d.to_vec(), ext: if &d[..4] == b"OTTO" { "otf".into() } else { "ttf".into() } })
            }
            _ => Ok(Blob { data: sf.data(o).to_vec(), ext: "dat".into() }),
        }
    }
}

// ------------------------------------------------------------------ trich xuat hang loat
#[derive(Deserialize, Serialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Kinds {
    pub text: bool,
    pub image: bool,
    pub sprite: bool,
    pub audio: bool,
    pub video: bool,
    pub font: bool,
    pub data: bool,
}

impl Kinds {
    fn wants(&self, kind: &str) -> bool {
        match kind {
            "text" | "table" => self.text,
            "image" => self.image,
            "sprite" => self.sprite,
            "audio" => self.audio,
            "video" => self.video,
            "font" => self.font,
            "data" | "bank" => self.data,
            _ => false,
        }
    }
    fn wants_media(&self, kind: &str) -> bool {
        match kind {
            "image" => self.image,
            "audio" => self.audio,
            "video" => self.video,
            _ => false,
        }
    }
}

#[derive(Serialize, Default, Debug)]
pub struct Stats {
    pub scanned: usize,
    pub bundles: usize,
    pub written: usize,
    pub errors: usize,
    pub bytes: u64,
    pub text: usize,
    pub image: usize,
    pub audio: usize,
    pub video: usize,
    pub other: usize,
    pub dest: String,
}

/// Ten ngan cua bundle: bo duoi va ma bam 32 ky tu hex o cuoi.
pub fn short_name(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(i) = stem.rfind('_') {
        let tail = &stem[i + 1..];
        if tail.len() == 32 && tail.bytes().all(|c| c.is_ascii_hexdigit()) {
            return stem[..i].to_string();
        }
    }
    stem
}

enum Job {
    Bundle(PathBuf, PathBuf),
    Copy(PathBuf, PathBuf, &'static str),
}

struct Done {
    bundle: bool,
    written: Vec<(&'static str, u64)>,
    errors: Vec<String>,
}

fn unique(used: &mut HashSet<String>, base: &str, ext: &str) -> String {
    let base = if base.is_empty() { "unnamed" } else { base };
    let mk = |n: usize| {
        let stem = if n == 0 { base.to_string() } else { format!("{base}_{}", n + 1) };
        if ext.is_empty() { stem } else { format!("{stem}.{ext}") }
    };
    let mut n = 0;
    loop {
        let name = mk(n);
        if used.insert(name.to_lowercase()) {
            return name;
        }
        n += 1;
    }
}

/// `shared`: bo ten dung chung khi xuat moi thu vao mot thu muc (nhieu luong cung ghi) - tranh trung ten.
fn extract_bundle(src: &Path, dir: &Path, kinds: &Kinds, shared: Option<&Mutex<HashSet<String>>>) -> Done {
    let mut done = Done { bundle: false, written: Vec::new(), errors: Vec::new() };
    let label = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let op = match Opened::open(src) {
        Ok(o) => o,
        Err(e) => {
            done.errors.push(format!("{label}: {e:#}"));
            return done;
        }
    };
    done.bundle = true;
    let mut used = HashSet::new();
    let mut made = false;
    for e in op.list(false) {
        if !kinds.wants(e.kind) {
            continue;
        }
        let blob = match op.render(&e.id, true) {
            Ok(b) => b,
            Err(err) => {
                done.errors.push(format!("{label} › {}: {err:#}", e.name));
                continue;
            }
        };
        if !made {
            if let Err(err) = fs::create_dir_all(dir) {
                done.errors.push(format!("{}: {err}", dir.display()));
                return done;
            }
            made = true;
        }
        let mut base = sanitize(&e.name.replace(" / ", "__"));
        let ext = blob.ext.as_str();
        if !ext.is_empty() && base.to_lowercase().ends_with(&format!(".{ext}")) {
            base.truncate(base.len() - ext.len() - 1);
        }
        if base.is_empty() {
            base = format!("{}_{}", e.class, e.id.replace(':', "_"));
        }
        let name = match shared {
            Some(m) => unique(&mut m.lock().unwrap_or_else(|e| e.into_inner()), &base, ext),
            None => unique(&mut used, &base, ext),
        };
        match fs::write(dir.join(&name), &blob.data) {
            Ok(()) => done.written.push((e.kind, blob.data.len() as u64)),
            Err(err) => done.errors.push(format!("{label} › {name}: {err}")),
        }
    }
    done
}

/// Trich xuat moi tai nguyen thuoc `kinds` trong cac file/thu muc `srcs` vao `dest`.
/// `flat = false`: bundle -> dest/<ten bundle>/..., file media roi -> dest/_files/<duong dan tuong doi>.
/// `flat = true`: moi file vao thang `dest`; trung ten thi them _2, _3...
pub fn extract(srcs: &[PathBuf], dest: &Path, kinds: &Kinds, flat: bool, log: &dyn Fn(&str)) -> Result<Stats> {
    fs::create_dir_all(dest).with_context(|| format!("khong tao duoc {}", dest.display()))?;
    let mut jobs = Vec::new();
    let mut used_dirs: HashSet<String> = HashSet::new();
    let shared: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    let mut scanned = 0;
    for src in srcs {
        let root = if src.is_dir() { src.clone() } else { src.parent().map(|p| p.to_path_buf()).unwrap_or_default() };
        for e in walkdir::WalkDir::new(src).sort_by_file_name().into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
            let p = e.path();
            if p.starts_with(dest) {
                continue;
            }
            scanned += 1;
            let mk = media_kind(p);
            if !mk.is_empty() {
                if kinds.wants_media(mk) {
                    let dst = if flat {
                        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        let ext = p.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        dest.join(unique(&mut shared.lock().unwrap(), &sanitize(&stem), &ext))
                    } else {
                        let rel = p.strip_prefix(&root).unwrap_or(Path::new(p.file_name().unwrap()));
                        dest.join("_files").join(rel)
                    };
                    jobs.push(Job::Copy(p.to_path_buf(), dst, mk));
                }
            } else if is_container(p) {
                let dir = if flat { dest.to_path_buf() } else { dest.join(unique(&mut used_dirs, &sanitize(&short_name(p)), "")) };
                jobs.push(Job::Bundle(p.to_path_buf(), dir));
            }
        }
    }
    log(&format!("Tim thay {} file, {} viec can lam ...", scanned, jobs.len()));
    let mut st = Stats { scanned, dest: dest.to_string_lossy().into_owned(), ..Default::default() };
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<Done>();
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let total = jobs.len();
    std::thread::scope(|sc| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (jobs, next) = (&jobs, &next);
            let sh = if flat { Some(&shared) } else { None };
            sc.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(job) = jobs.get(i) else { break };
                let done = match job {
                    Job::Bundle(src, dir) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract_bundle(src, dir, kinds, sh))).unwrap_or_else(|_| Done {
                        bundle: true,
                        written: Vec::new(),
                        errors: vec![format!("{}: loi noi bo khi doc bundle", src.display())],
                    }),
                    Job::Copy(src, dst, kind) => {
                        let mut d = Done { bundle: false, written: Vec::new(), errors: Vec::new() };
                        let r = dst.parent().map(fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| fs::copy(src, dst));
                        match r {
                            Ok(n) => d.written.push((kind, n)),
                            Err(e) => d.errors.push(format!("{}: {e}", src.display())),
                        }
                        d
                    }
                };
                if tx.send(done).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        let mut n = 0;
        let mut shown = 0;
        for d in rx {
            n += 1;
            if d.bundle {
                st.bundles += 1;
            }
            for (k, b) in d.written {
                st.written += 1;
                st.bytes += b;
                match k {
                    "text" | "table" => st.text += 1,
                    "image" | "sprite" => st.image += 1,
                    "audio" => st.audio += 1,
                    "video" => st.video += 1,
                    _ => st.other += 1,
                }
            }
            for e in d.errors {
                st.errors += 1;
                if shown < 40 {
                    shown += 1;
                    log(&format!("  ! {e}"));
                }
            }
            if n % 100 == 0 || n == total {
                log(&format!("  {n}/{total} ... da ghi {} file ({} MB)", st.written, st.bytes >> 20));
            }
        }
        if st.errors > shown {
            log(&format!("  ... va {} loi khac", st.errors - shown));
        }
    });
    log(&format!(
        "Trich xuat xong: {} file ({} MB) - text {}, anh {}, am thanh {}, video {}, khac {}; {} loi -> {}",
        st.written,
        st.bytes >> 20,
        st.text,
        st.image,
        st.audio,
        st.video,
        st.other,
        st.errors,
        dest.display()
    ));
    Ok(st)
}
