//! Cong cu thu nho cho crate l10n-assets:
//!   dump list <file> [all]
//!   dump get <file> <id> <out>
//!   dump extract <src> <dest> <kinds: text,image,sprite,audio,video,font,data>
use l10n_assets::{extract, Kinds, Opened};
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("list") => {
            let op = Opened::open(Path::new(&a[1]))?;
            println!("unity {}", op.unity);
            for e in op.list(a.get(2).is_some()) {
                println!("{:<22} {:<8} {:<12} {:>9}  {}  [{}] .{}", e.id, e.kind, e.class, e.size, e.name, e.info, e.ext);
            }
        }
        Some("get") => {
            let op = Opened::open(Path::new(&a[1]))?;
            let b = op.render(&a[2], true)?;
            std::fs::write(&a[3], &b.data)?;
            println!("{} byte .{}", b.data.len(), b.ext);
        }
        Some("extract") => {
            let k = &a[3];
            let kinds = Kinds {
                text: k.contains("text"),
                image: k.contains("image"),
                sprite: k.contains("sprite"),
                audio: k.contains("audio"),
                video: k.contains("video"),
                font: k.contains("font"),
                data: k.contains("data"),
            };
            let t = std::time::Instant::now();
            let flat = a.get(4).is_some_and(|s| s == "flat");
            let st = extract(&[PathBuf::from(&a[1])], Path::new(&a[2]), &kinds, flat, &|s| println!("{s}"))?;
            println!("{st:?} in {:.1}s", t.elapsed().as_secs_f64());
        }
        _ => eprintln!("dump list|get|extract ..."),
    }
    Ok(())
}
