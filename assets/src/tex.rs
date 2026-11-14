//! Giai ma Texture2D cua Unity ra RGBA 8 bit va ghi PNG.
use anyhow::{anyhow, bail, Result};
use texture2ddecoder as t2d;

/// Anh RGBA8, dong dau tien la dong tren cung.
#[derive(Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

pub fn format_name(f: i64) -> String {
    let s = match f {
        1 => "Alpha8",
        2 => "ARGB4444",
        3 => "RGB24",
        4 => "RGBA32",
        5 => "ARGB32",
        7 => "RGB565",
        9 => "R16",
        10 => "DXT1",
        12 => "DXT5",
        13 => "RGBA4444",
        14 => "BGRA32",
        24 => "BC6H",
        25 => "BC7",
        26 => "BC4",
        27 => "BC5",
        28 => "DXT1Crunched",
        29 => "DXT5Crunched",
        30 => "PVRTC_RGB2",
        31 => "PVRTC_RGBA2",
        32 => "PVRTC_RGB4",
        33 => "PVRTC_RGBA4",
        34 => "ETC_RGB4",
        41 => "EAC_R",
        43 => "EAC_RG",
        45 => "ETC2_RGB",
        46 => "ETC2_RGBA1",
        47 => "ETC2_RGBA8",
        48 | 54 => "ASTC_4x4",
        49 | 55 => "ASTC_5x5",
        50 | 56 => "ASTC_6x6",
        51 | 57 => "ASTC_8x8",
        52 | 58 => "ASTC_10x10",
        53 | 59 => "ASTC_12x12",
        62 => "RG16",
        63 => "R8",
        64 => "ETC_RGB4Crunched",
        65 => "ETC2_RGBA8Crunched",
        _ => return format!("format {f}"),
    };
    s.to_string()
}

fn need(data: &[u8], n: usize) -> Result<&[u8]> {
    data.get(..n).ok_or_else(|| anyhow!("du lieu anh thieu ({} < {n} byte)", data.len()))
}

/// Giai ma mip 0. `data` la "image data" (hoac vung .resS tuong ung).
pub fn decode(fmt: i64, w: usize, h: usize, data: &[u8]) -> Result<Image> {
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        bail!("kich thuoc anh khong hop le: {w}x{h}");
    }
    let px = w * h;
    let mut rgba = vec![0u8; px * 4];
    // dinh dang tho
    let raw = |bpp: usize, f: &dyn Fn(&[u8], &mut [u8])| -> Result<Vec<u8>> {
        let src = need(data, px * bpp)?;
        let mut out = vec![0u8; px * 4];
        for (s, d) in src.chunks_exact(bpp).zip(out.chunks_exact_mut(4)) {
            f(s, d);
        }
        Ok(out)
    };
    let x4 = |v: u16, sh: u32| (((v >> sh) & 0xf) * 17) as u8;
    match fmt {
        1 => rgba = raw(1, &|s, d| d.copy_from_slice(&[255, 255, 255, s[0]]))?,
        2 => {
            rgba = raw(2, &|s, d| {
                let v = u16::from_le_bytes([s[0], s[1]]);
                d.copy_from_slice(&[x4(v, 8), x4(v, 4), x4(v, 0), x4(v, 12)]);
            })?
        }
        3 => rgba = raw(3, &|s, d| d.copy_from_slice(&[s[0], s[1], s[2], 255]))?,
        4 => rgba = need(data, px * 4)?.to_vec(),
        5 => rgba = raw(4, &|s, d| d.copy_from_slice(&[s[1], s[2], s[3], s[0]]))?,
        7 => {
            rgba = raw(2, &|s, d| {
                let v = u16::from_le_bytes([s[0], s[1]]);
                let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                d.copy_from_slice(&[((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8, 255]);
            })?
        }
        9 => rgba = raw(2, &|s, d| d.copy_from_slice(&[s[1], 0, 0, 255]))?,
        13 => {
            rgba = raw(2, &|s, d| {
                let v = u16::from_le_bytes([s[0], s[1]]);
                d.copy_from_slice(&[x4(v, 12), x4(v, 8), x4(v, 4), x4(v, 0)]);
            })?
        }
        14 => rgba = raw(4, &|s, d| d.copy_from_slice(&[s[2], s[1], s[0], s[3]]))?,
        62 => rgba = raw(2, &|s, d| d.copy_from_slice(&[s[0], s[1], 0, 255]))?,
        63 => rgba = raw(1, &|s, d| d.copy_from_slice(&[s[0], 0, 0, 255]))?,
        _ => {
            let mut img = vec![0u32; px];
            let r = match fmt {
                10 => t2d::decode_bc1(data, w, h, &mut img),
                12 => t2d::decode_bc3(data, w, h, &mut img),
                24 => t2d::decode_bc6_unsigned(data, w, h, &mut img),
                25 => t2d::decode_bc7(data, w, h, &mut img),
                26 => t2d::decode_bc4(data, w, h, &mut img),
                27 => t2d::decode_bc5(data, w, h, &mut img),
                28 | 29 | 64 | 65 => t2d::decode_unity_crunch(data, w, h, &mut img),
                30 | 31 => t2d::decode_pvrtc_2bpp(data, w, h, &mut img),
                32 | 33 => t2d::decode_pvrtc_4bpp(data, w, h, &mut img),
                34 => t2d::decode_etc1(data, w, h, &mut img),
                41 => t2d::decode_eacr(data, w, h, &mut img),
                43 => t2d::decode_eacrg(data, w, h, &mut img),
                45 => t2d::decode_etc2_rgb(data, w, h, &mut img),
                46 => t2d::decode_etc2_rgba1(data, w, h, &mut img),
                47 => t2d::decode_etc2_rgba8(data, w, h, &mut img),
                48..=59 => {
                    let b = [4, 5, 6, 8, 10, 12][((fmt - 48) % 6) as usize];
                    let blocks = w.div_ceil(b) * h.div_ceil(b);
                    need(data, blocks * 16)?;
                    t2d::decode_astc(data, w, h, b, b, &mut img)
                }
                _ => bail!("chua ho tro {}", format_name(fmt)),
            };
            r.map_err(|e| anyhow!("giai ma {} loi: {e}", format_name(fmt)))?;
            // texture2ddecoder tra ve BGRA (u32 little-endian)
            for (p, d) in img.iter().zip(rgba.chunks_exact_mut(4)) {
                let [b, g, r, a] = p.to_le_bytes();
                d.copy_from_slice(&[r, g, b, a]);
            }
        }
    }
    // Unity luu anh tu duoi len
    let row = w * 4;
    let mut flipped = vec![0u8; rgba.len()];
    for y in 0..h {
        flipped[y * row..(y + 1) * row].copy_from_slice(&rgba[(h - 1 - y) * row..(h - y) * row]);
    }
    Ok(Image { w, h, rgba: flipped })
}

impl Image {
    /// Cat vung (x, y tinh tu goc DUOI-trai nhu Unity).
    pub fn crop_unity(&self, x: f64, y: f64, w: f64, h: f64) -> Result<Image> {
        let x0 = x.round().max(0.0) as usize;
        let cw = (w.round() as usize).min(self.w.saturating_sub(x0));
        let ch = (h.round() as usize).min(self.h);
        let top = self.h.saturating_sub((y.round().max(0.0) as usize) + ch);
        if cw == 0 || ch == 0 {
            bail!("vung cat rong");
        }
        let mut rgba = Vec::with_capacity(cw * ch * 4);
        for r in top..top + ch {
            let s = (r * self.w + x0) * 4;
            rgba.extend_from_slice(&self.rgba[s..s + cw * 4]);
        }
        Ok(Image { w: cw, h: ch, rgba })
    }

    /// Thu nho de canh dai nhat <= `max` (trung binh vung, co trong so alpha de khong vien toi).
    pub fn thumb(&self, max: usize) -> Image {
        if self.w <= max && self.h <= max {
            return self.clone();
        }
        let s = max as f64 / self.w.max(self.h) as f64;
        let nw = ((self.w as f64 * s).round() as usize).max(1);
        let nh = ((self.h as f64 * s).round() as usize).max(1);
        let mut rgba = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            let (y0, y1) = (y * self.h / nh, ((y + 1) * self.h / nh).max(y * self.h / nh + 1));
            for x in 0..nw {
                let (x0, x1) = (x * self.w / nw, ((x + 1) * self.w / nw).max(x * self.w / nw + 1));
                let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let p = &self.rgba[(sy * self.w + sx) * 4..][..4];
                        let al = p[3] as u64;
                        r += p[0] as u64 * al;
                        g += p[1] as u64 * al;
                        b += p[2] as u64 * al;
                        a += al;
                        n += 1;
                    }
                }
                let d = &mut rgba[(y * nw + x) * 4..][..4];
                if a > 0 {
                    d.copy_from_slice(&[(r / a) as u8, (g / a) as u8, (b / a) as u8, (a / n) as u8]);
                }
            }
        }
        Image { w: nw, h: nh, rgba }
    }

    pub fn png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.w as u32, self.h as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            let mut w = enc.write_header()?;
            w.write_image_data(&self.rgba)?;
        }
        Ok(out)
    }
}
