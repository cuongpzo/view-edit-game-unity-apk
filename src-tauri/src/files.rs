//! Tab "Tệp game": duyet thu muc lam viec, xem truoc va trich xuat tai nguyen.
//! Anh / am thanh / video duoc phat qua giao thuc rieng `gfs://` (ho tro Range de tua video).
use l10n_assets::{self as assets, Entry, Kinds, Opened, Stats};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Emitter};

const TEXT_MAX: usize = 400 * 1024;
const HEX_MAX: usize = 4096;

async fn bg<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f().map_err(|e| format!("{e:#}"))).await.map_err(|e| e.to_string())?
}

// ------------------------------------------------------------------ bo dem
type OpenCache = Mutex<Vec<(PathBuf, Option<SystemTime>, Arc<Opened>)>>;
static OPENED: OpenCache = Mutex::new(Vec::new());
static BLOBS: Mutex<Vec<(String, Arc<Vec<u8>>, String)>> = Mutex::new(Vec::new());

static THUMBS: Mutex<Vec<(String, Arc<Vec<u8>>)>> = Mutex::new(Vec::new());
/// mo bundle lan luot: nhieu yeu cau anh thu nho cung luc khong doc trung mot bundle
static OPEN_LOCK: Mutex<()> = Mutex::new(());

fn opened(path: &Path) -> anyhow::Result<Arc<Opened>> {
    let _g = OPEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
    if let Some((_, _, o)) = OPENED.lock().unwrap().iter().find(|(p, t, _)| p == path && *t == mtime) {
        return Ok(o.clone());
    }
    let o = Arc::new(Opened::open(path)?);
    let mut g = OPENED.lock().unwrap();
    g.retain(|(p, _, _)| p != path);
    if g.len() >= 3 {
        g.remove(0);
    }
    g.push((path.to_path_buf(), mtime, o.clone()));
    Ok(o)
}

/// Du lieu da dung san (PNG/OGG/MP4...) cua mot doi tuong trong bundle, co dem lai de phat/tua.
fn blob(path: &Path, id: &str) -> anyhow::Result<(Arc<Vec<u8>>, String)> {
    let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
    let key = format!("{}|{:?}|{id}", path.display(), mtime);
    if let Some((_, d, ext)) = BLOBS.lock().unwrap().iter().find(|(k, _, _)| *k == key) {
        return Ok((d.clone(), ext.clone()));
    }
    let b = opened(path)?.render(id, true)?;
    let d = Arc::new(b.data);
    let mut g = BLOBS.lock().unwrap();
    while g.len() >= 12 || (!g.is_empty() && g.iter().map(|(_, d, _)| d.len()).sum::<usize>() > 256 << 20) {
        g.remove(0);
    }
    g.push((key, d.clone(), b.ext.clone()));
    Ok((d, b.ext))
}

// ------------------------------------------------------------------ duyet thu muc
#[derive(Serialize)]
pub struct FsEntry {
    name: String,
    path: String,
    dir: bool,
    size: u64,
    /// dir | bundle | bank | image | audio | video | file; fs_list never probes file content
    kind: &'static str,
    /// thoi diem sua (ms tu 1970), 0 neu khong ro
    mtime: u64,
}

fn ms(t: std::io::Result<SystemTime>) -> u64 {
    t.ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn fs_entry(p: &Path, name: String, is_dir: bool, size: u64, mtime: u64) -> FsEntry {
    let kind = if is_dir {
        "dir"
    } else {
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "bundle" => "bundle",
            "bank" | "fsb" => "bank",
            _ => match assets::media_kind(p) {
                "" => "file",
                k => k,
            },
        }
    };
    FsEntry { name, path: p.to_string_lossy().into_owned(), dir: is_dir, size, kind, mtime }
}

#[tauri::command]
pub async fn fs_list(dir: String) -> Result<Vec<FsEntry>, String> {
    bg(move || {
        let mut out = Vec::new();
        for e in fs::read_dir(&dir).map_err(|e| anyhow::anyhow!("khong mo duoc {dir}: {e}"))? {
            let e = e?;
            let md = e.metadata()?;
            out.push(fs_entry(&e.path(), e.file_name().to_string_lossy().into_owned(), md.is_dir(), md.len(), ms(md.modified())));
        }
        out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(out)
    })
    .await
}

/// Phan loai noi dung mot file theo yeu cau, khong probe header khi liet ke thu muc.
#[tauri::command]
pub async fn fs_classify(path: String) -> Result<&'static str, String> {
    bg(move || {
        let p = PathBuf::from(path);
        let kind = match assets::media_kind(&p) {
            k if !k.is_empty() => k,
            _ if assets::is_sound_bank(&p) => "bank",
            _ if assets::is_container(&p) => "bundle",
            _ => "file",
        };
        Ok(kind)
    })
    .await
}

/// Tim file theo ten (khong phan biet hoa thuong) trong `root`; ten tra ve la duong dan tuong doi.
#[tauri::command]
pub async fn fs_search(root: String, query: String) -> Result<Vec<FsEntry>, String> {
    bg(move || {
        let q = query.to_lowercase();
        let rootp = PathBuf::from(&root);
        let mut out = Vec::new();
        for e in walkdir::WalkDir::new(&rootp).sort_by_file_name().into_iter().filter_map(|e| e.ok()) {
            if !e.file_type().is_file() || !e.file_name().to_string_lossy().to_lowercase().contains(&q) {
                continue;
            }
            let rel = e.path().strip_prefix(&rootp).unwrap_or(e.path()).to_string_lossy().into_owned();
            let md = e.metadata().ok();
            out.push(fs_entry(e.path(), rel, false, md.as_ref().map(|m| m.len()).unwrap_or(0), md.and_then(|m| m.modified().ok()).map(|t| ms(Ok(t))).unwrap_or(0)));
            if out.len() >= 500 {
                break;
            }
        }
        Ok(out)
    })
    .await
}

// ------------------------------------------------------------------ xem truoc
#[derive(Serialize, Default)]
pub struct Preview {
    /// image | audio | video | text | hex | bundle
    kind: &'static str,
    text: String,
    size: u64,
    truncated: bool,
}

fn hex_dump(d: &[u8]) -> String {
    let mut s = String::new();
    for (i, row) in d.chunks(16).enumerate() {
        s.push_str(&format!("{:08x}  ", i * 16));
        for k in 0..16 {
            match row.get(k) {
                Some(b) => s.push_str(&format!("{b:02x} ")),
                None => s.push_str("   "),
            }
            if k == 7 {
                s.push(' ');
            }
        }
        s.push(' ');
        s.extend(row.iter().map(|b| if (32..127).contains(b) { *b as char } else { '.' }));
        s.push('\n');
    }
    s
}

fn text_or_hex(data: &[u8], total: u64) -> Preview {
    if assets::looks_text(data) {
        let cut = data.len().min(TEXT_MAX);
        Preview { kind: "text", text: String::from_utf8_lossy(&data[..cut]).into_owned(), size: total, truncated: (cut as u64) < total }
    } else {
        let cut = data.len().min(HEX_MAX);
        Preview { kind: "hex", text: hex_dump(&data[..cut]), size: total, truncated: (cut as u64) < total }
    }
}

#[tauri::command]
pub async fn fs_preview(path: String) -> Result<Preview, String> {
    bg(move || {
        let p = Path::new(&path);
        let size = fs::metadata(p)?.len();
        // Container detection is done by fs_classify when this specific file is selected.
        // Avoid repeating the header probe while generating its ordinary text/hex preview.
        let kind = assets::media_kind(p);
        if !kind.is_empty() {
            return Ok(Preview { kind, size, ..Default::default() });
        }
        let mut buf = vec![0u8; (size as usize).min(TEXT_MAX + 4)];
        fs::File::open(p)?.read_exact(&mut buf)?;
        Ok(text_or_hex(&buf, size))
    })
    .await
}

#[derive(Serialize)]
pub struct BundleInfo {
    unity: String,
    entries: Vec<Entry>,
}

#[tauri::command]
pub async fn bundle_list(path: String, all: bool) -> Result<BundleInfo, String> {
    bg(move || {
        let o = opened(Path::new(&path))?;
        Ok(BundleInfo { unity: o.unity.clone(), entries: o.list(all) })
    })
    .await
}

/// Xem noi dung mot doi tuong trong bundle duoi dang van ban (bang FlatBuffers -> TSV) hoac hex.
#[tauri::command]
pub async fn asset_preview(path: String, id: String) -> Result<Preview, String> {
    bg(move || {
        let (d, _) = blob(Path::new(&path), &id)?;
        Ok(text_or_hex(&d, d.len() as u64))
    })
    .await
}

// ------------------------------------------------------------------ trich xuat
/// Luu mot doi tuong (id rong = chep nguyen file) ra `dest`. Tra ve so byte da ghi.
#[tauri::command]
pub async fn asset_save(path: String, id: String, dest: String) -> Result<u64, String> {
    bg(move || {
        if id.is_empty() {
            return Ok(fs::copy(&path, &dest)?);
        }
        let (d, _) = blob(Path::new(&path), &id)?;
        fs::write(&dest, d.as_slice())?;
        Ok(d.len() as u64)
    })
    .await
}

#[tauri::command]
pub async fn extract_assets(app: AppHandle, srcs: Vec<String>, dest: String, kinds: Kinds, flat: Option<bool>) -> Result<Stats, String> {
    bg(move || {
        let log = |s: &str| {
            let _ = app.emit("log", s.to_string());
        };
        let srcs: Vec<PathBuf> = srcs.into_iter().map(PathBuf::from).collect();
        assets::extract(&srcs, Path::new(&dest), &kinds, flat.unwrap_or(false), &log)
    })
    .await
}

// ------------------------------------------------------------------ giao thuc gfs://
#[derive(Deserialize)]
struct Req {
    p: String,
    #[serde(default)]
    id: String,
    /// > 0: anh thu nho, canh dai toi da `t` px
    #[serde(default)]
    t: usize,
}

fn thumb(path: &Path, id: &str, t: usize) -> anyhow::Result<Arc<Vec<u8>>> {
    let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
    let key = format!("{}|{:?}|{id}|{t}", path.display(), mtime);
    if let Some((_, d)) = THUMBS.lock().unwrap().iter().find(|(k, _)| *k == key) {
        return Ok(d.clone());
    }
    let d = Arc::new(opened(path)?.thumb_png(id, t.clamp(16, 512))?);
    let mut g = THUMBS.lock().unwrap();
    if g.len() >= 800 {
        g.drain(..200);
    }
    g.push((key, d.clone()));
    Ok(d)
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// "bytes=a-b" -> (a, b bao gom) trong pham vi `len`.
fn parse_range(h: Option<&str>, len: u64) -> Option<(u64, u64)> {
    let r = h?.trim().strip_prefix("bytes=")?;
    let (a, b) = r.split(',').next()?.split_once('-')?;
    if len == 0 {
        return None;
    }
    if a.is_empty() {
        let n: u64 = b.trim().parse().ok()?;
        return Some((len.saturating_sub(n), len - 1));
    }
    let start: u64 = a.trim().parse().ok()?;
    let end = b.trim().parse::<u64>().ok().unwrap_or(len - 1).min(len - 1);
    (start <= end).then_some((start, end))
}

fn respond(status: StatusCode, mime: &str, body: Vec<u8>, range: Option<(u64, u64, u64)>) -> Response<Vec<u8>> {
    let mut b = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CONTENT_LENGTH, body.len());
    if let Some((s, e, total)) = range {
        b = b.header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{total}"));
    }
    b.body(body).unwrap_or_else(|_| Response::new(Vec::new()))
}

fn serve_inner(req: &Request<Vec<u8>>) -> anyhow::Result<Response<Vec<u8>>> {
    let raw = pct_decode(req.uri().path().trim_start_matches('/'));
    let r: Req = serde_json::from_str(&raw).map_err(|e| anyhow::anyhow!("yeu cau khong hop le: {e}"))?;
    let range_h = req.headers().get(header::RANGE).and_then(|v| v.to_str().ok());
    let path = Path::new(&r.p);
    if r.id.is_empty() {
        // file roi tren dia: chi doc dung doan duoc yeu cau (toi da 8 MB moi lan)
        let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let mime = assets::mime_of(&ext);
        let mut f = fs::File::open(path)?;
        let len = f.metadata()?.len();
        return Ok(match parse_range(range_h, len) {
            Some((s, e)) => {
                let e = e.min(s + (8 << 20) - 1);
                let mut buf = vec![0u8; (e - s + 1) as usize];
                f.seek(SeekFrom::Start(s))?;
                f.read_exact(&mut buf)?;
                respond(StatusCode::PARTIAL_CONTENT, mime, buf, Some((s, e, len)))
            }
            None => {
                let mut buf = Vec::with_capacity(len as usize);
                f.read_to_end(&mut buf)?;
                respond(StatusCode::OK, mime, buf, None)
            }
        });
    }
    if r.t > 0 {
        let d = thumb(path, &r.id, r.t)?;
        return Ok(respond(StatusCode::OK, "image/png", d.to_vec(), None));
    }
    let (d, ext) = blob(path, &r.id)?;
    let mime = assets::mime_of(&ext);
    let len = d.len() as u64;
    Ok(match parse_range(range_h, len) {
        Some((s, e)) => respond(StatusCode::PARTIAL_CONTENT, mime, d[s as usize..=e as usize].to_vec(), Some((s, e, len))),
        None => respond(StatusCode::OK, mime, d.to_vec(), None),
    })
}

/// Xu ly mot yeu cau gfs:// (goi tu luong rieng, khong chan giao dien).
pub fn serve(req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| serve_inner(&req))) {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => respond(StatusCode::NOT_FOUND, "text/plain; charset=utf-8", format!("{e:#}").into_bytes(), None),
        Err(_) => respond(StatusCode::INTERNAL_SERVER_ERROR, "text/plain; charset=utf-8", b"loi noi bo".to_vec(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(s: &str) -> String {
        s.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect()
    }

    fn get(p: &str, id: &str, range: Option<&str>) -> Response<Vec<u8>> {
        let json = serde_json::json!({ "p": p, "id": id }).to_string();
        let mut b = Request::builder().uri(format!("http://gfs.localhost/{}", enc(&json)));
        if let Some(r) = range {
            b = b.header(header::RANGE, r);
        }
        serve(b.body(Vec::new()).unwrap())
    }

    fn find(pat: &str) -> String {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../work/apk_install_assets/assets/yoo/GamePackage");
        fs::read_dir(d).unwrap().flatten().map(|e| e.path()).find(|p| p.to_string_lossy().contains(pat)).unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn serve_media() {
        let pct = pct_decode("%E1%BA%A3nh%20x");
        assert_eq!(pct, "ảnh x");
        // anh trong bundle
        let ui = find("ui_cards_");
        let list = Opened::open(Path::new(&ui)).unwrap().list(false);
        let img = list.iter().find(|e| e.kind == "image").unwrap();
        let r = get(&ui, &img.id, None);
        assert_eq!(r.status(), StatusCode::OK, "{}", String::from_utf8_lossy(r.body()));
        assert_eq!(r.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(&r.body()[1..4], b"PNG");
        // am thanh: Range nhu the <audio>
        let bank = find("audio_pet_bank");
        let list = Opened::open(Path::new(&bank)).unwrap().list(false);
        let a = list.iter().find(|e| e.kind == "audio").unwrap();
        let r = get(&bank, &a.id, Some("bytes=0-"));
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(&r.body()[..4], b"OggS");
        assert!(r.headers()[header::CONTENT_RANGE].to_str().unwrap().starts_with("bytes 0-"));
        // video
        let mv = find("movie_shen_502");
        let list = Opened::open(Path::new(&mv)).unwrap().list(false);
        let v = list.iter().find(|e| e.kind == "video").unwrap();
        let r = get(&mv, &v.id, Some("bytes=100-199"));
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.body().len(), 100);
        let r = get(&mv, &v.id, Some("bytes=0-11"));
        assert_eq!(&r.body()[4..8], b"ftyp");
        // anh thu nho: canh dai <= 128
        let json = serde_json::json!({ "p": ui, "id": img.id, "t": 128 }).to_string();
        let r = serve(Request::builder().uri(format!("http://gfs.localhost/{}", enc(&json))).body(Vec::new()).unwrap());
        assert_eq!(r.status(), StatusCode::OK, "{}", String::from_utf8_lossy(r.body()));
        let b = r.body();
        assert_eq!(&b[1..4], b"PNG");
        let (w, h) = (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()));
        assert!(w <= 128 && h <= 128 && w.max(h) >= 64, "thumb {w}x{h}");
        // file roi tren dia
        let png = fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("icons")).unwrap().flatten().find(|e| e.path().extension().is_some_and(|x| x == "png")).unwrap().path();
        let r = get(&png.to_string_lossy(), "", None);
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "image/png");
        // loi tra ve 404 kem ly do
        let r = get("D:\\khong_ton_tai.bundle", "0:1", None);
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }
}
