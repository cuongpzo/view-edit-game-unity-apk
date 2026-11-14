//! libra.pak - goi nhieu file phu thuoc vao MOT file, nen (LZ4) roi ma hoa AES-256-GCM.
//!
//! Khoa duoc suy ra tu mat khau co dinh "cuongpzo" bang PBKDF2-HMAC-SHA256 (muoi ngau nhien).
//! File ghi ra chi gom byte ngau nhien ngoai 9 byte chu ky: mo bang hex khong thay noi dung,
//! khong chuong trinh nao giai nen duoc neu khong co dung mat khau. GCM co the xac thuc, nen
//! sai mat khau hay sua 1 byte deu lam viec giai ma that bai (khong tra ve du lieu rac).
//!
//! LUU Y THUC TE: mat khau nam trong file .exe de chuong trinh tu giai ma, nen nguoi dich nguoc
//! .exe van co the moi ra. Day la lop bao ve chong doc/sua truc tiep file .pak, khong phai DRM tuyet doi.
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{anyhow, bail, Result};
use sha2::Sha256;

const MAGIC: &[u8; 9] = b"LIBRAPAK\x01";
const PASS: &[u8] = b"cuongpzo";
const ITERS: u32 = 200_000;
const SALT: usize = 16;
const NONCE: usize = 12;

fn derive(salt: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(PASS, salt, ITERS, &mut k);
    k
}

/// Nen + ma hoa danh sach (ten, noi dung) thanh byte cua file libra.pak.
pub fn pack(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut inner = Vec::new();
    inner.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (n, d) in files {
        let nb = n.as_bytes();
        if nb.len() > u16::MAX as usize {
            bail!("ten file qua dai: {n}");
        }
        inner.extend_from_slice(&(nb.len() as u16).to_le_bytes());
        inner.extend_from_slice(nb);
        inner.extend_from_slice(&(d.len() as u32).to_le_bytes());
        inner.extend_from_slice(d);
    }
    let orig = inner.len() as u32;
    let comp = lz4_flex::block::compress(&inner);

    let mut salt = [0u8; SALT];
    let mut nonce = [0u8; NONCE];
    getrandom::getrandom(&mut salt).map_err(|e| anyhow!("khong lay duoc so ngau nhien: {e}"))?;
    getrandom::getrandom(&mut nonce).map_err(|e| anyhow!("khong lay duoc so ngau nhien: {e}"))?;
    let key = derive(&salt);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: &comp, aad: MAGIC })
        .map_err(|_| anyhow!("ma hoa that bai"))?;

    let mut out = Vec::with_capacity(9 + SALT + NONCE + 4 + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&orig.to_le_bytes());
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Giai ma + giai nen file libra.pak thanh danh sach (ten, noi dung).
pub fn unpack(raw: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let head = 9 + SALT + NONCE + 4;
    if raw.len() < head + 16 || &raw[..9] != MAGIC {
        bail!("khong phai file libra.pak hop le");
    }
    let salt = &raw[9..9 + SALT];
    let nonce = &raw[9 + SALT..9 + SALT + NONCE];
    let orig = u32::from_le_bytes(raw[9 + SALT + NONCE..head].try_into().unwrap()) as usize;
    let ct = &raw[head..];
    let key = derive(salt);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let comp = cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: MAGIC })
        .map_err(|_| anyhow!("giai ma that bai (sai khoa hoac file da hong)"))?;
    let inner = lz4_flex::block::decompress(&comp, orig).map_err(|e| anyhow!("giai nen loi: {e}"))?;

    let mut p = 0usize;
    let rd_u16 = |b: &[u8], p: &mut usize| -> Result<usize> {
        let v = u16::from_le_bytes(b.get(*p..*p + 2).ok_or_else(|| anyhow!("pak hong"))?.try_into().unwrap());
        *p += 2;
        Ok(v as usize)
    };
    let rd_u32 = |b: &[u8], p: &mut usize| -> Result<usize> {
        let v = u32::from_le_bytes(b.get(*p..*p + 4).ok_or_else(|| anyhow!("pak hong"))?.try_into().unwrap());
        *p += 4;
        Ok(v as usize)
    };
    let count = rd_u32(&inner, &mut p)?;
    let mut files = Vec::with_capacity(count);
    for _ in 0..count {
        let nl = rd_u16(&inner, &mut p)?;
        let name = String::from_utf8(inner.get(p..p + nl).ok_or_else(|| anyhow!("pak hong"))?.to_vec())?;
        p += nl;
        let dl = rd_u32(&inner, &mut p)?;
        let data = inner.get(p..p + dl).ok_or_else(|| anyhow!("pak hong"))?.to_vec();
        p += dl;
        files.push((name, data));
    }
    Ok(files)
}
