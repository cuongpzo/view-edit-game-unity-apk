//! Ban dong lenh cua loi (dung de kiem thu / chay tu dong).
use l10n_core as core;
use std::path::{Path, PathBuf};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let log = |s: &str| println!("{s}");
    let arg = |i: usize| a.get(i).map(|s| s.as_str());
    let r: anyhow::Result<()> = (|| {
        match (arg(0), arg(1)) {
            (Some("unpack"), Some(src)) => core::unpack(Path::new(src), Path::new(arg(2).unwrap_or("work")), &log),
            (Some("scan"), Some(work)) => {
                let rep = core::scan(Path::new(work), arg(2).unwrap_or("config"), &log)?;
                println!("Unity {} | {} | HybridCLR {} | Lua {} | YooAsset {} | FMOD {}", rep.unity, rep.scripting, rep.hybridclr, rep.lua, rep.yooasset, rep.fmod);
                println!("{} bundle; offset truoc UnityFS: {:?}", rep.bundles, rep.offsets);
                for t in rep.tables.iter().take(25) {
                    println!("  {:<38} {:>8} {:>5} {:>10}", t.name, t.rows, t.cols, t.chars);
                }
                println!("  ({} bang)", rep.tables.len());
                Ok(())
            }
            (Some("export"), Some(work)) => core::export(Path::new(work), arg(2).unwrap_or("flatbytes_languageconfig"), arg(3).unwrap_or(""), &log).map(|_| ()),
            (Some("import"), Some(work)) => {
                let files: Vec<PathBuf> = a[2..].iter().map(PathBuf::from).collect();
                core::import(Path::new(work), &files, &log).map(|_| ())
            }
            // in kieu cot + vai dong dau cua cac bang co ten khop regex
            (Some("dump"), Some(work)) => {
                let re = regex::RegexBuilder::new(arg(2).unwrap_or(".")).case_insensitive(true).build()?;
                let n: usize = arg(3).and_then(|s| s.parse().ok()).unwrap_or(5);
                for (_, rel, p) in core::bundles(Path::new(work))? {
                    if !rel.contains("flatbytes") {
                        continue;
                    }
                    let Ok(l) = core::load_bundle(&p) else { continue };
                    for t in l.texts.into_iter().filter(|t| re.is_match(&t.name)) {
                        let Some(ft) = core::flat::FlatTable::parse(t.data) else { continue };
                        println!("== {} ({} dong)", t.name, ft.nrows());
                        println!("   {}", ft.cols.iter().map(|(c, k)| format!("f{c}:{}{}", k.desc(), if k.editable() { "" } else { "(ro)" })).collect::<Vec<_>>().join(" | "));
                        for r in 0..ft.nrows().min(n) {
                            let cells: Vec<String> = (0..ft.cols.len()).map(|i| ft.text(r, i).unwrap_or_else(|| "-".into())).collect();
                            println!("   {r}: {}", cells.join(" | "));
                        }
                    }
                }
                Ok(())
            }
            // tu kiem tra: ghi lai tung o sua duoc bang chinh gia tri cua no, doc lai phai y nguyen
            (Some("check"), Some(work)) => {
                let (mut tables, mut cells, mut bad, mut qmark) = (0, 0u64, 0u64, 0u64);
                let mut kinds: std::collections::BTreeMap<&str, usize> = Default::default();
                for (_, rel, p) in core::bundles(Path::new(work))? {
                    if !rel.contains("flatbytes") {
                        continue;
                    }
                    let Ok(l) = core::load_bundle(&p) else { continue };
                    for t in l.texts {
                        let Some(ft) = core::flat::FlatTable::parse(t.data.clone()) else { continue };
                        tables += 1;
                        for (_, k) in &ft.cols {
                            *kinds.entry(k.label()).or_default() += 1;
                        }
                        let mut w = core::flat::FlatTable::parse_as(t.data, &ft.cols).unwrap();
                        let mut want = Vec::new();
                        for r in 0..ft.nrows() {
                            for (i, (c, k)) in ft.cols.iter().enumerate() {
                                let Some(s) = ft.text(r, i) else { continue };
                                if s == "?" && !matches!(k, core::flat::Kind::Str) {
                                    qmark += 1;
                                    println!("  ? {} dong {r} f{c} ({}) khong giai ma duoc", t.name, k.label());
                                    continue;
                                }
                                if !k.editable() {
                                    continue;
                                }
                                cells += 1;
                                match core::flat::canon(k, &s) {
                                    Ok(cn) if cn == s => {}
                                    other => {
                                        bad += 1;
                                        println!("  ! {} dong {r} f{c} ({}): canon {other:?} != {s:?}", t.name, k.label());
                                        continue;
                                    }
                                }
                                if let Err(e) = w.set_text(r, i, &s) {
                                    bad += 1;
                                    println!("  ! {} dong {r} f{c} ({}): {e}", t.name, k.label());
                                    continue;
                                }
                                want.push((r, i, s));
                            }
                        }
                        // doc lai sau khi da ghi het (moi o deu da tro sang doi tuong moi)
                        let back = core::flat::FlatTable::parse_as(w.buf.clone(), &ft.cols).unwrap();
                        for (r, i, s) in want {
                            if back.text(r, i).as_deref() != Some(s.as_str()) {
                                bad += 1;
                                println!("  ! {} dong {r} cot {i}: doc lai {:?} != {s:?}", t.name, back.text(r, i));
                            }
                        }
                    }
                }
                println!("{tables} bang, {cells} o ghi-doc lai, {bad} loi, {qmark} o khong giai ma duoc");
                println!("so cot theo kieu: {kinds:?}");
                Ok(())
            }
            // sua mot o: set <work> <ten bang> <dong> <f-truong> <gia tri>  (cung duong luu voi giao dien)
            (Some("set"), Some(work)) => {
                let (Some(asset), Some(r), Some(f), Some(text)) = (arg(2), arg(3).and_then(|s| s.parse::<usize>().ok()), arg(4).and_then(|s| s.trim_start_matches('f').parse::<usize>().ok()), arg(5)) else {
                    anyhow::bail!("set <work> <ten bang> <dong> <f-truong> <gia tri>");
                };
                let work = Path::new(work);
                let rep = core::scan(work, "flatbytes", &|_| {})?;
                let t = rep.tables.iter().find(|t| t.name == asset).ok_or_else(|| anyhow::anyhow!("khong thay bang {asset}"))?;
                let td = core::read_table(work, &t.split, &t.entry, asset)?;
                let i = td.cols.iter().position(|c| *c == f).ok_or_else(|| anyhow::anyhow!("bang khong co truong f{f}"))?;
                let mut vals = td.cur;
                let slot = vals.get_mut(r).ok_or_else(|| anyhow::anyhow!("khong co dong {r}"))?;
                if slot[i].is_none() {
                    anyhow::bail!("truong vang mat o dong nay");
                }
                let new = if matches!(td.kinds[i], core::flat::Kind::Str) { text.to_string() } else { core::flat::canon(&td.kinds[i], text).map_err(|e| anyhow::anyhow!(e))? };
                println!("{:?} -> {new:?}", slot[i].as_deref().unwrap_or(""));
                slot[i] = Some(new);
                core::write_table(work, &t.split, &t.entry, asset, &vals, &log).map(|_| ())
            }
            (Some("pack"), Some(work)) => core::pack(Path::new(work), arg(2).map(Path::new), &log).map(|_| ()),
            (Some("mkpak"), Some(out)) => {
                let mut files = Vec::new();
                for p in &a[2..] {
                    let name = Path::new(p).file_name().ok_or_else(|| anyhow::anyhow!("duong dan sai: {p}"))?.to_string_lossy().into_owned();
                    files.push((name, std::fs::read(p)?));
                }
                if files.is_empty() {
                    anyhow::bail!("mkpak <out.pak> <file1> [file2 ...]");
                }
                std::fs::write(out, core::pak::pack(&files)?)?;
                println!("da tao {out} tu {} file (da ma hoa)", files.len());
                Ok(())
            }
            (Some("unpak"), Some(src)) => {
                let dir = arg(2).unwrap_or(".");
                let files = core::pak::unpack(&std::fs::read(src)?)?;
                std::fs::create_dir_all(dir)?;
                for (n, d) in &files {
                    std::fs::write(Path::new(dir).join(n), d)?;
                    println!("  {n} ({} byte)", d.len());
                }
                Ok(())
            }
            _ => {
                println!("l10n-cli unpack <xapk> <work> | scan <work> [regex] | export <work> [regex] [ten,cot] | import <work> [xlsx..] | pack <work> [signer.jar] | dump <work> [regex ten bang] [so dong] | check <work> | set <work> <bang> <dong> <f-truong> <gia tri> | mkpak <out.pak> <file..> | unpak <pak> [dir]");
                Ok(())
            }
        }
    })();
    if let Err(e) = r {
        eprintln!("LOI: {e:#}");
        std::process::exit(1);
    }
}
