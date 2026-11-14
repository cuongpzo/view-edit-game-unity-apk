#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod files;
mod store;
mod tables;

use l10n_core as core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use store::Store;
use tables::{Opened, Page, Session, SetResult, Shared};
use tauri::{AppHandle, Emitter, Manager, State};

/// Chay viec nang o luong rieng; moi dong log duoc day ve giao dien qua su kien "log".
async fn blocking<T, F>(app: AppHandle, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(core::Log) -> anyhow::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let log = |s: &str| {
            let _ = app.emit("log", s.to_string());
        };
        f(&log).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Tach duong dan mod bundle (work\mod\<split>\<entry>) thanh (split, entry).
fn split_entry(work: &str, out: &str) -> Option<(String, String)> {
    let modp = Path::new(work).join("mod");
    let rel = Path::new(out).strip_prefix(&modp).ok()?;
    let mut comps = rel.components();
    let split = comps.next()?.as_os_str().to_string_lossy().into_owned();
    let entry = comps.as_path().to_string_lossy().replace('\\', "/");
    Some((split, entry))
}

#[tauri::command]
async fn unpack(app: AppHandle, src: String, work: String) -> Result<(), String> {
    blocking(app, move |log| core::unpack(Path::new(&src), Path::new(&work), log)).await
}

#[tauri::command]
async fn scan(app: AppHandle, work: String, pattern: String) -> Result<core::ScanReport, String> {
    blocking(app, move |log| core::scan(Path::new(&work), &pattern, log)).await
}

#[tauri::command]
async fn export_text(app: AppHandle, work: String, pattern: String, names: String) -> Result<Vec<String>, String> {
    blocking(app, move |log| core::export(Path::new(&work), &pattern, &names, log)).await
}

#[tauri::command]
async fn import_text(app: AppHandle, work: String) -> Result<Vec<core::ImportResult>, String> {
    let app2 = app.clone();
    let work2 = work.clone();
    let res = blocking(app, move |log| core::import(Path::new(&work2), &[], log)).await?;
    let store = app2.state::<Store>();
    for r in &res {
        if r.out.is_empty() {
            continue;
        }
        if let Some((split, entry)) = split_entry(&work, &r.out) {
            let _ = store.snapshot(&work, &split, &entry, &r.asset, Path::new(&r.out));
            let _ = store.append_history(json!({ "type": "import", "work": work, "asset": r.asset, "entry": entry, "cells": r.changed }));
        }
    }
    Ok(res)
}

#[tauri::command]
async fn pack(app: AppHandle, work: String, signer: String) -> Result<core::PackResult, String> {
    let app2 = app.clone();
    let work2 = work.clone();
    let r = blocking(app, move |log| {
        let s = PathBuf::from(signer.trim());
        core::pack(Path::new(&work2), if signer.trim().is_empty() { None } else { Some(s.as_path()) }, log)
    })
    .await?;
    let _ = app2.state::<Store>().append_history(json!({ "type": "pack", "work": work, "signed": r.signed, "apks": r.apks.len(), "replaced": r.replaced }));
    Ok(r)
}

#[tauri::command]
fn find_signer(work: String) -> Option<String> {
    core::find_signer(Path::new(&work)).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn signer_info(work: String) -> Option<String> {
    core::signer_info(Path::new(&work))
}

/// Mo file/thu muc bang ung dung mac dinh cua Windows (Excel, Explorer).
#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("khong ton tai: {path}"));
    }
    std::process::Command::new("explorer").arg(p).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn modified_entries(work: String) -> Vec<String> {
    core::modified_entries(Path::new(&work))
}

#[tauri::command]
fn revert_bundle(app: AppHandle, work: String, split: String, entry: String) -> Result<bool, String> {
    let r = core::revert(Path::new(&work), &split, &entry).map_err(|e| format!("{e:#}"))?;
    if r {
        let _ = app.state::<Store>().append_history(json!({ "type": "revert", "work": work, "entry": entry }));
    }
    Ok(r)
}

// ------------------------------------------------------------------ cau hinh / lich su / rollback
#[tauri::command]
fn cfg_load(store: State<Store>) -> Value {
    store.load_config()
}

#[tauri::command]
fn cfg_save(store: State<Store>, cfg: Value) -> Result<(), String> {
    store.save_config(&cfg).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn history_list(store: State<Store>, work: Option<String>, limit: Option<usize>) -> Vec<Value> {
    store.list_history(work.as_deref(), limit.unwrap_or(500))
}

#[tauri::command]
fn log_event(store: State<Store>, event: Value) -> Result<(), String> {
    store.append_history(event).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn snapshots(store: State<Store>, work: String, split: String, entry: String) -> Vec<Value> {
    store.list_snapshots(&work, &split, &entry)
}

#[tauri::command]
fn rollback(store: State<Store>, work: String, split: String, entry: String, ts: i64) -> Result<(), String> {
    let dest = core::mod_path(Path::new(&work), &split, &entry);
    store.restore(&work, &split, &entry, ts, &dest).map_err(|e| format!("{e:#}"))?;
    let _ = store.append_history(json!({ "type": "rollback", "work": work, "entry": entry, "ts": ts }));
    Ok(())
}

#[tauri::command]
fn data_dir(store: State<Store>) -> String {
    store.data_dir()
}

#[tauri::command]
fn cleanup_now(store: State<Store>) -> Result<usize, String> {
    let d = store.retention_days();
    store.cleanup(d).map_err(|e| format!("{e:#}"))
}

// ------------------------------------------------------------------ trinh soan thao bang
fn with<T>(st: &Shared, id: u32, f: impl FnOnce(&mut Session) -> Result<T, String>) -> Result<T, String> {
    let mut g = st.lock().map_err(|_| "loi khoa trang thai".to_string())?;
    let s = g.map.get_mut(&id).ok_or("tab da dong")?;
    f(s)
}

#[tauri::command]
async fn table_open(app: AppHandle, st: State<'_, Shared>, work: String, split: String, entry: String, asset: String) -> Result<Opened, String> {
    let st = st.inner().clone();
    let app2 = app.clone();
    let (w, sp, en, az) = (work.clone(), split.clone(), entry.clone(), asset.clone());
    let o = blocking(app, move |log| {
        let td = core::read_table(Path::new(&w), &sp, &en, &az)?;
        log(&format!("mo {az}: {} dong x {} cot", td.orig.len(), td.cols.len()));
        let s = Session { work: w.into(), split: sp, entry: en, asset: az, cols: td.cols, kinds: td.kinds, saved: td.cur.clone(), cur: td.cur, orig: td.orig };
        let mut g = st.lock().map_err(|_| anyhow::anyhow!("loi khoa trang thai"))?;
        let id = g.insert(s);
        Ok(Opened::of(id, &g.map[&id]))
    })
    .await?;
    let _ = app2.state::<Store>().append_history(json!({ "type": "open", "work": work, "asset": asset, "entry": entry }));
    Ok(o)
}

#[tauri::command]
async fn table_rows(st: State<'_, Shared>, id: u32, offset: usize, limit: usize, query: String, col: i64, mode: String) -> Result<Page, String> {
    with(st.inner(), id, |s| Ok(tables::page(s, offset, limit.min(1000), &query, col, &mode)))
}

#[tauri::command]
fn table_set(st: State<Shared>, id: u32, r: usize, i: usize, value: String) -> Result<SetResult, String> {
    with(st.inner(), id, |s| tables::set(s, r, i, value))
}

#[tauri::command]
async fn table_save(app: AppHandle, st: State<'_, Shared>, id: u32) -> Result<core::WriteResult, String> {
    let st = st.inner().clone();
    let (work, split, entry, asset, cur) = with(&st, id, |s| Ok((s.work.clone(), s.split.clone(), s.entry.clone(), s.asset.clone(), s.cur.clone())))?;
    let (w, sp, en, az) = (work.clone(), split.clone(), entry.clone(), asset.clone());
    let app2 = app.clone();
    let out = blocking(app, move |log| core::write_table(&w, &sp, &en, &az, &cur, log)).await?;
    let store = app2.state::<Store>();
    let work_s = work.to_string_lossy().into_owned();
    if out.path.is_empty() {
        let _ = store.append_history(json!({ "type": "save_clean", "work": work_s, "asset": asset }));
    } else {
        let _ = store.snapshot(&work_s, &split, &entry, &asset, Path::new(&out.path));
        let _ = store.append_history(json!({ "type": "save", "work": work_s, "asset": asset, "entry": entry, "cells": out.diff }));
    }
    // chi danh dau "da luu" phan da ghi (nguoi dung co the sua tiep trong luc dang luu)
    with(&st, id, |s| {
        s.saved = s.cur.clone();
        Ok(())
    })?;
    Ok(out)
}

#[tauri::command]
fn table_discard(st: State<Shared>, id: u32) -> Result<(), String> {
    with(st.inner(), id, |s| {
        s.cur = s.saved.clone();
        Ok(())
    })
}

#[tauri::command]
fn table_close(st: State<Shared>, id: u32) -> Result<(), String> {
    st.lock().map_err(|_| "loi khoa trang thai".to_string())?.map.remove(&id);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Shared::default())
        .setup(|app| {
            let base = app.path().document_dir().unwrap_or_else(|_| std::env::temp_dir()).join("UnityL10nTool");
            let store = Store::new(base);
            let _ = store.cleanup(store.retention_days());
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            unpack,
            scan,
            export_text,
            import_text,
            pack,
            find_signer,
            signer_info,
            open_path,
            modified_entries,
            revert_bundle,
            cfg_load,
            cfg_save,
            history_list,
            log_event,
            snapshots,
            rollback,
            data_dir,
            cleanup_now,
            table_open,
            table_rows,
            table_set,
            table_save,
            table_discard,
            table_close,
            files::fs_list,
            files::fs_search,
            files::fs_classify,
            files::fs_preview,
            files::bundle_list,
            files::asset_preview,
            files::asset_save,
            files::extract_assets
        ])
        .register_asynchronous_uri_scheme_protocol("gfs", |_ctx, req, responder| {
            std::thread::spawn(move || responder.respond(files::serve(req)));
        })
        .run(tauri::generate_context!())
        .expect("khong khoi dong duoc ung dung");
}
