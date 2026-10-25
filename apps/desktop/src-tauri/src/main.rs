#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
mod power;

use base64::Engine;
use recovery_core::{Entry, Partition, Progress, ScanOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};

type ApiResult<T> = Result<T, String>;
#[derive(Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub kind: String,
    pub accessible: bool,
    pub detail: String,
}
#[derive(Clone)]
struct RegisteredSource {
    source: Source,
    stamp: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct FileReport {
    name: String,
    status: String,
    sha256: String,
    error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct RecoveryReport {
    recovered: usize,
    failed: usize,
    bytes: u64,
    destination: String,
    files: Vec<FileReport>,
}
#[derive(Clone, Serialize)]
struct JobStatus {
    id: String,
    mode: String,
    #[serde(rename = "sourceName")]
    source_name: String,
    #[serde(rename = "sourceId")]
    source_id: String,
    status: String,
    progress: Progress,
    stage: String,
    #[serde(rename = "elapsedSeconds")]
    elapsed_seconds: f64,
    #[serde(rename = "progressDeterminate")]
    progress_determinate: bool,
    #[serde(rename = "sleepInhibited")]
    sleep_inhibited: bool,
    #[serde(rename = "sleepInhibitionError")]
    sleep_inhibition_error: Option<String>,
    entries: Vec<Entry>,
    warnings: Vec<String>,
    partitions: Vec<Partition>,
    #[serde(rename = "methodNotes")]
    method_notes: Vec<String>,
    error: Option<String>,
    report: Option<RecoveryReport>,
}
struct Job {
    view: JobStatus,
    source: RegisteredSource,
    options: ScanOptions,
    cancel: Arc<AtomicBool>,
    started: Instant,
}
#[derive(Clone, Default)]
struct AppState {
    sources: Arc<Mutex<BTreeMap<String, RegisteredSource>>>,
    jobs: Arc<Mutex<BTreeMap<String, Job>>>,
    destinations: Arc<Mutex<BTreeSet<PathBuf>>>,
    image_destinations: Arc<Mutex<BTreeSet<PathBuf>>>,
    busy: Arc<AtomicBool>,
    sessions: Arc<Mutex<PathBuf>>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    source: Source,
    stamp: String,
    options: ScanOptions,
    count: usize,
    date: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    id: String,
    source_name: String,
    date: String,
    count: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    hex: String,
    text: Option<String>,
    data_url: Option<String>,
}
fn id() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}
fn lock<T>(m: &Mutex<T>) -> ApiResult<std::sync::MutexGuard<'_, T>> {
    m.lock()
        .map_err(|_| "Application state unavailable; restart the application".into())
}
fn fingerprint(source: &Source) -> ApiResult<String> {
    let mut reader = recovery_core::SourceReader::open(Path::new(&source.path), Some(source.size))
        .map_err(|e| format!("Cannot open source: {e}"))?;
    if reader.len() != source.size {
        return Err(
            "Source capacity changed since selection. Refresh the source list and rescan.".into(),
        );
    }
    let mut h = Sha256::new();
    h.update(source.size.to_le_bytes());
    if source.kind == "image" {
        let meta = fs::metadata(&source.path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() != source.size {
            return Err("Source size or type changed".into());
        }
        h.update(
            meta.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0)
                .to_le_bytes(),
        );
    }
    let mut buf = vec![0; source.size.min(65536) as usize];
    reader
        .read_at(0, &mut buf)
        .map_err(|e| format!("Cannot read source fingerprint: {e}"))?;
    h.update(&buf);
    if source.size > 65536 {
        reader
            .read_at(source.size - 65536, &mut buf)
            .map_err(|e| format!("Cannot read source fingerprint: {e}"))?;
        h.update(&buf);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn verify_source(source: &RegisteredSource) -> ApiResult<()> {
    if fingerprint(&source.source)? != source.stamp {
        return Err(
            "Source changed since selection. Select it again and rescan before recovery.".into(),
        );
    }
    Ok(())
}
fn register(state: &AppState, source: Source) -> ApiResult<Source> {
    let stamp = if source.accessible {
        fingerprint(&source)?
    } else {
        String::new()
    };
    lock(&state.sources)?.insert(
        source.id.clone(),
        RegisteredSource {
            source: source.clone(),
            stamp,
        },
    );
    Ok(source)
}
fn image_source(path: PathBuf) -> ApiResult<Source> {
    let p = path.canonicalize().map_err(|e| e.to_string())?;
    let m = p.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() {
        return Err("Choose a regular raw disk-image file".into());
    }
    Ok(Source {
        id: format!("image:{}", id()),
        name: p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: p.to_string_lossy().into_owned(),
        size: m.len(),
        kind: "image".into(),
        accessible: true,
        detail: "Raw disk image · opened read-only · original device is not accessed".into(),
    })
}
#[tauri::command]
async fn list_sources(state: State<'_, AppState>) -> ApiResult<Vec<Source>> {
    let mut sources = platform::sources()?;
    for source in &sources {
        lock(&state.sources)?.insert(
            source.id.clone(),
            RegisteredSource {
                source: source.clone(),
                stamp: String::new(),
            },
        );
    }
    sources.extend(
        lock(&state.sources)?
            .values()
            .filter(|s| s.source.kind == "image")
            .map(|s| s.source.clone()),
    );
    Ok(sources)
}
#[tauri::command]
async fn pick_source(state: State<'_, AppState>) -> ApiResult<Option<Source>> {
    match rfd::AsyncFileDialog::new()
        .set_title("Open raw disk image / Открыть образ диска")
        .pick_file()
        .await
    {
        Some(f) => Ok(Some(register(&state, image_source(f.path().to_owned())?)?)),
        None => Ok(None),
    }
}
fn begin(
    state: &AppState,
    source: RegisteredSource,
    options: ScanOptions,
    total: u64,
) -> ApiResult<(String, Arc<AtomicBool>)> {
    if state
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Another operation is running. Cancel it or wait for completion.".into());
    }
    let key = id();
    let cancel = Arc::new(AtomicBool::new(false));
    let result = lock(&state.jobs).map(|mut jobs| {
        jobs.insert(
            key.clone(),
            Job {
                view: JobStatus {
                    id: key.clone(),
                    mode: options.mode.clone(),
                    source_name: source.source.name.clone(),
                    source_id: source.source.id.clone(),
                    status: "running".into(),
                    progress: Progress {
                        scanned: 0,
                        total,
                        found: 0,
                        ..Default::default()
                    },
                    stage: "preparing".into(),
                    elapsed_seconds: 0.0,
                    progress_determinate: false,
                    sleep_inhibited: false,
                    sleep_inhibition_error: None,
                    entries: Vec::new(),
                    warnings: Vec::new(),
                    partitions: Vec::new(),
                    method_notes: Vec::new(),
                    error: None,
                    report: None,
                },
                source,
                options,
                started: Instant::now(),
                cancel: cancel.clone(),
            },
        )
    });
    if result.is_err() {
        state.busy.store(false, Ordering::SeqCst);
        result?;
    }
    Ok((key, cancel))
}
fn finish(state: &AppState, key: &str, result: ApiResult<()>, cancel: &AtomicBool) {
    if let Ok(mut jobs) = lock(&state.jobs) {
        if let Some(job) = jobs.get_mut(key) {
            job.view.status = if cancel.load(Ordering::Relaxed) {
                "cancelled"
            } else if result.is_ok() {
                "completed"
            } else {
                "failed"
            }
            .into();
            job.view.error = result.err();
            job.view.elapsed_seconds = job.started.elapsed().as_secs_f64();
            job.view.sleep_inhibited = false;
            if job.view.status == "completed" {
                job.view.stage = "finished".into();
            }
        }
    }
    state.busy.store(false, Ordering::SeqCst);
}
fn update(state: &AppState, key: &str, p: Progress) {
    if let Ok(mut jobs) = lock(&state.jobs) {
        if let Some(job) = jobs.get_mut(key) {
            if !p.stage.is_empty() {
                job.view.stage = p.stage.clone();
                job.view.progress_determinate = !p.indeterminate && p.total > 0;
            }
            job.view.progress = p;
        }
    }
}
fn set_stage(state: &AppState, key: &str, stage: &str, determinate: bool) {
    if let Ok(mut jobs) = lock(&state.jobs) {
        if let Some(job) = jobs.get_mut(key) {
            job.view.stage = stage.into();
            job.view.progress_determinate = determinate;
        }
    }
}
fn spawn_job(
    state: AppState,
    key: String,
    cancel: Arc<AtomicBool>,
    work: impl FnOnce(&AppState, &str, &AtomicBool) -> ApiResult<()> + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let protection = power::SleepGuard::acquire();
        if let Ok(mut jobs) = lock(&state.jobs) {
            if let Some(job) = jobs.get_mut(&key) {
                job.view.sleep_inhibited = protection.is_ok();
                job.view.sleep_inhibition_error = protection.as_ref().err().cloned();
            }
        }
        // Always release power protection and the busy slot, including a parser
        // panic. A native device read can still wait for the OS driver's timeout.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if cancel.load(Ordering::Relaxed) {
                return Err("Operation cancelled".into());
            }
            work(&state, &key, &cancel)
        })).unwrap_or_else(|_| Err("The operation stopped after an internal error. Restart the application and report this issue.".into()));
        drop(protection);
        finish(&state, &key, result, &cancel);
    })
}
#[tauri::command]
async fn start_scan(
    state: State<'_, AppState>,
    source_id: String,
    mode: String,
    start: Option<u64>,
    end: Option<u64>,
    types: Vec<String>,
) -> ApiResult<String> {
    scan_start(
        state.inner().clone(),
        source_id,
        ScanOptions {
            mode,
            start,
            end,
            types,
        },
    )
}
fn scan_start(state: AppState, source_id: String, mut options: ScanOptions) -> ApiResult<String> {
    if !["quick", "deep", "advanced"].contains(&options.mode.as_str()) {
        return Err("Unknown scan mode".into());
    }
    let mut source = lock(&state.sources)?
        .get(&source_id)
        .cloned()
        .ok_or("Select a source first")?;
    if !source.source.accessible {
        return Err("The OS denied source access. Create an image with an authorized tool or select another source.".into());
    }
    let start = options.start.unwrap_or(0);
    let end = options.end.unwrap_or(source.source.size);
    if start >= end || end > source.source.size {
        return Err("Scan range must be within the source and non-empty".into());
    }
    options.end = Some(end);
    let (key, cancel) = begin(&state, source.clone(), options.clone(), end - start)?;
    let job_id = key.clone();
    spawn_job(state, key, cancel, move |state, key, cancel| {
        set_stage(state, key, "opening-source", false);
        source.stamp = fingerprint(&source.source)?;
        lock(&state.jobs)?
            .get_mut(key)
            .ok_or("Job lost")?
            .source
            .stamp = source.stamp.clone();
        let result = recovery_core::scan_with_size(
            Path::new(&source.source.path),
            &options,
            Some(source.source.size),
            cancel,
            |p| update(state, key, p),
        )
        .map_err(|e| e.to_string())?;
        set_stage(state, key, "verifying", false);
        verify_source(&source)?;
        let mut jobs = lock(&state.jobs)?;
        let job = jobs.get_mut(key).ok_or("Job lost")?;
        job.view.entries = result.entries;
        job.view.warnings = result.warnings;
        job.view.partitions = result.partitions;
        job.view.method_notes = result.method_notes;
        job.view.progress.scanned = result.bytes_scanned;
        job.view.progress.found = job.view.entries.len();
        Ok(())
    });
    Ok(job_id)
}
#[tauri::command]
fn job_status(state: State<'_, AppState>, job_id: String) -> ApiResult<JobStatus> {
    lock(&state.jobs)?
        .get(&job_id)
        .map(|j| {
            let mut view = j.view.clone();
            if view.status == "running" {
                view.elapsed_seconds = j.started.elapsed().as_secs_f64();
                if j.cancel.load(Ordering::Relaxed) {
                    view.stage = "cancelling".into();
                    view.progress_determinate = false;
                }
            }
            view
        })
        .ok_or("Unknown job".into())
}
#[tauri::command]
fn cancel_job(state: State<'_, AppState>, job_id: String) -> ApiResult<()> {
    lock(&state.jobs)?
        .get(&job_id)
        .ok_or("Unknown job")?
        .cancel
        .store(true, Ordering::Relaxed);
    Ok(())
}
fn entry_for(
    state: &AppState,
    job_id: &str,
    entry_id: &str,
) -> ApiResult<(RegisteredSource, Entry)> {
    let jobs = lock(&state.jobs)?;
    let job = jobs.get(job_id).ok_or("Unknown scan")?;
    if job.view.status != "completed" {
        return Err("Wait for a completed scan".into());
    }
    let entry = job
        .view
        .entries
        .iter()
        .find(|e| e.id == entry_id)
        .cloned()
        .ok_or("Unknown result")?;
    Ok((job.source.clone(), entry))
}
#[tauri::command]
async fn preview(
    state: State<'_, AppState>,
    job_id: String,
    entry_id: String,
) -> ApiResult<Preview> {
    let (source, entry) = entry_for(&state, &job_id, &entry_id)?;
    verify_source(&source)?;
    let bytes = recovery_core::read_entry_with_size(
        Path::new(&source.source.path),
        &entry,
        1024 * 1024,
        Some(source.source.size),
    )
    .map_err(|e| e.to_string())?;
    let hex = bytes
        .iter()
        .take(1024)
        .copied()
        .collect::<Vec<_>>()
        .chunks(16)
        .enumerate()
        .map(|(i, row)| {
            format!(
                "{:08x}  {}",
                i * 16,
                row.iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = std::str::from_utf8(&bytes[..bytes.len().min(65536)])
        .ok()
        .filter(|t| !t.chars().any(|c| c == '\0'))
        .map(str::to_owned);
    // Raster-only preview: SVG/HTML/PDF and recovered executables are never rendered or executed.
    let mime = match entry.kind.to_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        _ => None,
    };
    let data_url = if entry.size <= 1024 * 1024 {
        mime.map(|m| {
            format!(
                "data:{m};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            )
        })
    } else {
        None
    };
    Ok(Preview {
        hex,
        text,
        data_url,
    })
}
#[tauri::command]
async fn choose_destination(state: State<'_, AppState>) -> ApiResult<Option<String>> {
    match rfd::AsyncFileDialog::new()
        .set_title("Choose healthy destination storage / Папка восстановления")
        .pick_folder()
        .await
    {
        Some(p) => {
            let path = p.path().canonicalize().map_err(|e| e.to_string())?;
            lock(&state.destinations)?.insert(path.clone());
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}
fn safe_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(100)
        .collect();
    s = s.trim_matches([' ', '.']).to_owned();
    while s.len() > 180 {
        s.pop();
    }
    s = s.trim_end_matches([' ', '.']).to_owned();
    if s.is_empty() {
        s = "recovered".into()
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        s.insert(0, '_')
    }
    s
}
fn private_directory(path: &Path) -> ApiResult<()> {
    fs::create_dir(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
struct HashWriter<'a> {
    file: &'a mut File,
    hash: Sha256,
}
impl Write for HashWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.hash.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
fn hash_file_with_progress(
    path: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> ApiResult<String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let total = f.metadata().map_err(|e| e.to_string())?.len();
    let mut read = 0;
    let mut h = Sha256::new();
    let mut buf = vec![0; 1024 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        read += n as u64;
        progress(read, total);
    }
    Ok(format!("{:x}", h.finalize()))
}
#[cfg(test)]
fn copy_entry(
    source: &Source,
    entry: &Entry,
    dir: &Path,
    index: usize,
    cancel: &AtomicBool,
) -> ApiResult<(String, u64, String)> {
    copy_entry_with_progress(source, entry, dir, index, cancel, |_, _, _| {})
}
fn copy_entry_with_progress(
    source: &Source,
    entry: &Entry,
    dir: &Path,
    index: usize,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&str, u64, u64),
) -> ApiResult<(String, u64, String)> {
    let name = format!("{:06}_{}", index + 1, safe_name(&entry.name));
    let tmp = dir.join(format!(".{index}.partial"));
    let target = dir.join(&name);
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        let mut writer = HashWriter {
            file: &mut f,
            hash: Sha256::new(),
        };
        let n = recovery_core::recover_entry_with_progress_and_size(
            Path::new(&source.path),
            entry,
            &mut writer,
            cancel,
            Some(source.size),
            |n, total| progress("recovering", n, total),
        )
        .map_err(|e| e.to_string())?;
        let expected = format!("{:x}", writer.hash.finalize());
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        if n != entry.size {
            return Err("Incomplete reconstruction".into());
        }
        progress("verifying", 0, entry.size);
        if hash_file_with_progress(&tmp, cancel, |n, total| progress("verifying", n, total))?
            != expected
        {
            return Err("Read-back SHA-256 verification failed".into());
        }
        fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
        Ok((name, n, expected))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
#[tauri::command]
async fn recover(
    state: State<'_, AppState>,
    job_id: String,
    entry_ids: Vec<String>,
    destination: String,
) -> ApiResult<String> {
    let (source, options, entries) = {
        let jobs = lock(&state.jobs)?;
        let job = jobs.get(&job_id).ok_or("Unknown scan")?;
        if job.view.status != "completed" {
            return Err("A completed scan is required".into());
        }
        let ids: BTreeSet<_> = entry_ids.into_iter().collect();
        if ids.is_empty() {
            return Err("Select at least one result".into());
        }
        let entries: Vec<_> = job
            .view
            .entries
            .iter()
            .filter(|e| ids.contains(&e.id))
            .cloned()
            .collect();
        if entries.len() != ids.len() {
            return Err("Invalid selection".into());
        }
        (job.source.clone(), job.options.clone(), entries)
    };
    let dest = PathBuf::from(destination)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !lock(&state.destinations)?.contains(&dest) {
        return Err("Choose the destination through the native folder dialog".into());
    }
    let total = entries
        .iter()
        .try_fold(0u64, |sum, e| sum.checked_add(e.size))
        .ok_or("Selected size overflows")?;
    let (key, cancel) = begin(
        &state,
        source.clone(),
        ScanOptions {
            mode: "recover".into(),
            ..options
        },
        total,
    )?;
    let job_id = key.clone();
    let state = state.inner().clone();
    spawn_job(state, key, cancel, move |state, key, cancel| {
        set_stage(state, key, "opening-source", false);
        verify_source(&source)?;
        platform::ensure_separate_device(&source.source, &dest)?;
        platform::check_free_space(&dest, total)?;
        let dir = dest.join(format!("Memory-Regainer-{key}"));
        private_directory(&dir)?;
        let mut report = RecoveryReport {
            recovered: 0,
            failed: 0,
            bytes: 0,
            destination: dir.to_string_lossy().into_owned(),
            files: Vec::new(),
        };
        let mut processed = 0;
        for (index, entry) in entries.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if let Err(error) = verify_source(&source) {
                for pending in &entries[index..] {
                    report.failed += 1;
                    report.files.push(FileReport {
                        name: pending.name.clone(),
                        status: "not-recovered".into(),
                        sha256: String::new(),
                        error: Some(error.clone()),
                    });
                }
                break;
            }
            match copy_entry_with_progress(
                &source.source,
                entry,
                &dir,
                index,
                cancel,
                |stage, n, _| {
                    update(
                        state,
                        key,
                        Progress {
                            scanned: processed + if stage == "recovering" { n } else { entry.size },
                            total,
                            found: report.recovered,
                            stage: stage.into(),
                            indeterminate: stage == "verifying",
                            ..Default::default()
                        },
                    );
                },
            ) {
                Ok((name, n, sha256)) => {
                    report.recovered += 1;
                    report.bytes += n;
                    report.files.push(FileReport {
                        name,
                        status: "verified-copy".into(),
                        sha256,
                        error: None,
                    });
                }
                Err(e) => {
                    report.failed += 1;
                    report.files.push(FileReport {
                        name: entry.name.clone(),
                        status: "failed".into(),
                        sha256: String::new(),
                        error: Some(e),
                    });
                }
            }
            processed += entry.size;
            update(
                state,
                key,
                Progress {
                    scanned: processed,
                    total,
                    found: report.recovered,
                    stage: "recovering".into(),
                    indeterminate: false,
                    ..Default::default()
                },
            );
        }
        let unattempted = entries.len() - report.files.len();
        if unattempted > 0 {
            report.failed += unattempted;
        }
        set_stage(state, key, "writing-report", false);
        let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        let failed = report.failed;
        lock(&state.jobs)?
            .get_mut(key)
            .ok_or("Job lost")?
            .view
            .report = Some(report);
        fs::write(dir.join("recovery-report.json"), bytes)
            .map_err(|e| format!("Files processed but report could not be saved: {e}"))?;
        if failed > 0 && !cancel.load(Ordering::Relaxed) {
            return Err(format!(
                "{failed} files could not be recovered; see the report"
            ));
        }
        Ok(())
    });
    Ok(job_id)
}
#[tauri::command]
async fn pick_image_destination(state: State<'_, AppState>) -> ApiResult<Option<String>> {
    match rfd::AsyncFileDialog::new()
        .set_title("New raw image / Новый образ")
        .set_file_name("recovery-image.img")
        .save_file()
        .await
    {
        Some(p) => {
            let parent = p
                .path()
                .parent()
                .ok_or("Invalid destination")?
                .canonicalize()
                .map_err(|e| e.to_string())?;
            let path = parent.join(p.path().file_name().ok_or("Invalid filename")?);
            if path.exists() {
                return Err("Image destination must be a new file".into());
            }
            lock(&state.image_destinations)?.insert(path.clone());
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}
#[tauri::command]
async fn create_image(
    state: State<'_, AppState>,
    source_id: String,
    destination: String,
) -> ApiResult<String> {
    let mut source = lock(&state.sources)?
        .get(&source_id)
        .cloned()
        .ok_or("Select a source")?;
    let dest = PathBuf::from(destination);
    if !lock(&state.image_destinations)?.contains(&dest) {
        return Err("Choose a new file using the save dialog".into());
    }
    let options = ScanOptions {
        mode: "image".into(),
        start: None,
        end: None,
        types: Vec::new(),
    };
    let total = source.source.size;
    let (key, cancel) = begin(&state, source.clone(), options, total)?;
    let job_id = key.clone();
    let state = state.inner().clone();
    spawn_job(state, key, cancel, move |state, key, cancel| {
        set_stage(state, key, "opening-source", false);
        source.stamp = fingerprint(&source.source)?;
        lock(&state.jobs)?
            .get_mut(key)
            .ok_or("Job lost")?
            .source
            .stamp = source.stamp.clone();
        platform::ensure_separate_device(
            &source.source,
            dest.parent().ok_or("No parent directory")?,
        )?;
        platform::check_free_space(dest.parent().ok_or("No destination parent")?, total)?;
        let mut input =
            recovery_core::SourceReader::open(Path::new(&source.source.path), Some(total))
                .map_err(|e| e.to_string())?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0; 1024 * 1024];
        let mut hash = Sha256::new();
        let mut copied = 0;
        let acquisition = (|| {
            while copied < total {
                if cancel.load(Ordering::Relaxed) {
                    return Err(
                        "Acquisition cancelled. Partial image retained with failure report."
                            .to_owned(),
                    );
                }
                let count = (total - copied).min(buf.len() as u64) as usize;
                input.read_at(copied, &mut buf[..count]).map_err(|e|format!("Read failed at byte {copied}: {e}. Acquisition stopped without retries; consider professional help for failing hardware."))?;
                output.write_all(&buf[..count]).map_err(|e| e.to_string())?;
                hash.update(&buf[..count]);
                copied += count as u64;
                update(
                    state,
                    key,
                    Progress {
                        scanned: copied,
                        total,
                        found: 0,
                        stage: "imaging".into(),
                        indeterminate: false,
                        ..Default::default()
                    },
                );
            }
            output.sync_all().map_err(|e| e.to_string())?;
            set_stage(state, key, "verifying", false);
            verify_source(&source)?;
            Ok(())
        })();
        drop(output);
        let expected = format!("{:x}", hash.finalize());
        let verification = if acquisition.is_ok() {
            hash_file_with_progress(&dest, cancel, |scanned, total| {
                update(
                    state,
                    key,
                    Progress {
                        scanned,
                        total,
                        found: 0,
                        stage: "verifying".into(),
                        indeterminate: false,
                        ..Default::default()
                    },
                );
            })
        } else {
            Err("Acquisition incomplete".into())
        };
        let verified = verification
            .as_ref()
            .is_ok_and(|actual| actual == &expected);
        let error = acquisition
            .err()
            .or_else(|| verification.err())
            .or_else(|| {
                if verified {
                    None
                } else {
                    Some("Image hash verification failed".into())
                }
            });
        let report = RecoveryReport {
            recovered: usize::from(verified),
            failed: usize::from(!verified),
            bytes: copied,
            destination: dest.to_string_lossy().into_owned(),
            files: vec![FileReport {
                name: dest
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                status: if verified {
                    "verified-image"
                } else {
                    "partial-image"
                }
                .into(),
                sha256: expected,
                error: error.clone(),
            }],
        };
        set_stage(state, key, "writing-report", false);
        let report_path = dest.with_file_name(format!(
            "{}.{}.report.json",
            dest.file_name().unwrap_or_default().to_string_lossy(),
            key
        ));
        let mut report_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(report_path)
            .map_err(|e| e.to_string())?;
        report_file
            .write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        lock(&state.jobs)?
            .get_mut(key)
            .ok_or("Job lost")?
            .view
            .report = Some(report);
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    });
    Ok(job_id)
}
fn manifest(state: &AppState, key: &str) -> ApiResult<Manifest> {
    let jobs = lock(&state.jobs)?;
    let job = jobs.get(key).ok_or("Unknown job")?;
    if job.view.status != "completed" || job.options.mode == "image" {
        return Err("Save a completed scan".into());
    }
    Ok(Manifest {
        version: 1,
        source: job.source.source.clone(),
        stamp: job.source.stamp.clone(),
        options: job.options.clone(),
        count: job.view.entries.len(),
        date: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    })
}
#[tauri::command]
async fn save_manifest(state: State<'_, AppState>, job_id: String) -> ApiResult<Option<String>> {
    let m = manifest(&state, &job_id)?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_file_name("memory-regainer-scan.json")
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let data = serde_json::to_vec_pretty(&m).map_err(|e| e.to_string())?;
    let path = file.path();
    let canon_parent = path
        .parent()
        .ok_or("Invalid path")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    platform::ensure_separate_device(&m.source, &canon_parent)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Choose a new manifest filename: {e}"))?;
    output.write_all(&data).map_err(|e| e.to_string())?;
    // Never store a device session on the application's potentially affected system disk.
    if m.source.kind == "image" {
        let dir = lock(&state.sessions)?.clone();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join(format!("{}.json", id())), data).map_err(|e| e.to_string())?;
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}
#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> ApiResult<Vec<Session>> {
    let dir = lock(&state.sessions)?.clone();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        if entry
            .metadata()
            .map(|m| m.len() > 1024 * 1024)
            .unwrap_or(true)
        {
            continue;
        }
        if let Ok(m) = fs::read(entry.path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
            .ok_or(())
        {
            sessions.push(Session {
                id: entry.file_name().to_string_lossy().into_owned(),
                source_name: m.source.name,
                date: m.date,
                count: m.count,
            });
        }
    }
    sessions.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(sessions)
}
#[tauri::command]
async fn load_session(state: State<'_, AppState>, id: String) -> ApiResult<String> {
    if !id.chars().all(|c| {
        c.is_ascii_digit() || c == '-' || c == '.' || c == 'j' || c == 's' || c == 'o' || c == 'n'
    }) || !id.ends_with(".json")
        || id.contains("..")
    {
        return Err("Invalid session ID".into());
    }
    let path = lock(&state.sessions)?.join(id);
    let metadata = path.metadata().map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 {
        return Err("Invalid manifest size".into());
    }
    let m: Manifest = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if m.version != 1 || m.source.kind != "image" {
        return Err(
            "Only disk-image sessions can be reloaded. Select physical devices again.".into(),
        );
    }
    let source = image_source(PathBuf::from(&m.source.path))?;
    if fingerprint(&source)? != m.stamp {
        return Err("Source image changed since this session; open it and scan again".into());
    }
    let source = register(&state, source)?;
    scan_start(state.inner().clone(), source.id, m.options)
}
fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            let state = app.state::<AppState>();
            *state.sessions.lock().map_err(|_| "State unavailable")? =
                app.path().app_data_dir()?.join("sessions");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_sources,
            pick_source,
            start_scan,
            job_status,
            cancel_job,
            preview,
            choose_destination,
            recover,
            pick_image_destination,
            create_image,
            save_manifest,
            list_sessions,
            load_session
        ])
        .run(tauri::generate_context!())
        .expect("Unable to launch Memory Regainer");
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_source() -> RegisteredSource {
        RegisteredSource {
            source: Source {
                id: "test".into(),
                name: "Synthetic test".into(),
                path: "missing-test-image.img".into(),
                size: 1024,
                kind: "image".into(),
                accessible: true,
                detail: String::new(),
            },
            stamp: String::new(),
        }
    }
    #[test]
    fn job_cleanup_covers_success_failure_cancellation_and_panic() {
        for outcome in ["completed", "failed", "cancelled", "panic"] {
            let state = AppState::default();
            let (key, cancel) = begin(&state, test_source(), ScanOptions::default(), 1024).unwrap();
            assert!(begin(&state, test_source(), ScanOptions::default(), 1024).is_err());
            let observer = state.clone();
            spawn_job(state, key.clone(), cancel, move |state, key, cancel| {
                let jobs = lock(&state.jobs)?;
                let view = &jobs.get(key).unwrap().view;
                assert!(view.sleep_inhibited || view.sleep_inhibition_error.is_some());
                drop(jobs);
                match outcome {
                    "failed" => Err("synthetic read failure".into()),
                    "cancelled" => {
                        cancel.store(true, Ordering::Relaxed);
                        Ok(())
                    }
                    "panic" => panic!("synthetic worker panic"),
                    _ => Ok(()),
                }
            })
            .join()
            .unwrap();
            let jobs = observer.jobs.lock().unwrap();
            let view = &jobs.get(&key).unwrap().view;
            assert_eq!(
                view.status,
                if outcome == "panic" {
                    "failed"
                } else {
                    outcome
                }
            );
            assert!(!view.sleep_inhibited);
            assert!(view.elapsed_seconds > 0.0);
            assert!(!observer.busy.load(Ordering::SeqCst));
        }
    }
    #[test]
    fn scan_returns_a_job_before_opening_the_source() {
        let state = AppState::default();
        let source = test_source();
        state
            .sources
            .lock()
            .unwrap()
            .insert(source.source.id.clone(), source);
        let key = scan_start(state.clone(), "test".into(), ScanOptions::default())
            .expect("source opening belongs to the worker");
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while state.busy.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let jobs = state.jobs.lock().unwrap();
        let view = &jobs.get(&key).unwrap().view;
        assert_eq!(view.status, "failed");
        assert!(view.error.as_ref().unwrap().contains("Cannot open source"));
        assert!(!view.sleep_inhibited);
    }
    #[test]
    fn names_cannot_escape_output_directory() {
        for name in [
            "../../lost.txt",
            "C:\\Windows\\a.exe",
            "/tmp/a",
            "..",
            "CON",
            "NUL.txt",
            "a\0b",
        ] {
            let n = safe_name(name);
            assert!(!n.contains('/'));
            assert!(!n.contains('\\'));
            assert!(!n.contains('\0'));
            assert_ne!(n, "..");
            assert_ne!(n, "CON");
            assert_ne!(n, "NUL.txt");
        }
        assert!(safe_name(&"фотография📷".repeat(100)).len() <= 180);
    }
    #[test]
    fn detects_changed_image() {
        let p = std::env::temp_dir().join(format!("mr-stamp-{}", id()));
        fs::write(&p, b"test image").unwrap();
        let s = image_source(p.clone()).unwrap();
        let r = RegisteredSource {
            stamp: fingerprint(&s).unwrap(),
            source: s,
        };
        assert!(verify_source(&r).is_ok());
        fs::write(&p, b"other data").unwrap();
        assert!(verify_source(&r).is_err());
        fs::remove_file(p).unwrap();
    }
    #[test]
    fn recovery_checks_hash_and_preserves_source_with_duplicate_names() {
        let root = std::env::temp_dir().join(format!("mr-copy-{}", id()));
        private_directory(&root).unwrap();
        let p = root.join("source.img");
        let payload = b"original bytes of a deleted sample";
        fs::write(&p, payload).unwrap();
        let source = image_source(p.clone()).unwrap();
        let out = root.join("out");
        private_directory(&out).unwrap();
        let e = Entry {
            id: "test".into(),
            name: "../NUL.txt".into(),
            kind: "txt".into(),
            size: payload.len() as u64,
            offset: 0,
            confidence: "low".into(),
            method: "test".into(),
            notes: vec![],
            extents: vec![recovery_core::Extent {
                offset: 0,
                length: payload.len() as u64,
            }],
        };
        let cancel = AtomicBool::new(false);
        let (n1, count, hash) = copy_entry(&source, &e, &out, 0, &cancel).unwrap();
        let (n2, _, _) = copy_entry(&source, &e, &out, 1, &cancel).unwrap();
        assert_ne!(n1, n2);
        assert_eq!(count, payload.len() as u64);
        assert_eq!(fs::read(out.join(n1)).unwrap(), payload);
        assert_eq!(hash, format!("{:x}", Sha256::digest(payload)));
        assert_eq!(fs::read(&p).unwrap(), payload);
        cancel.store(true, Ordering::Relaxed);
        assert!(copy_entry(&source, &e, &out, 2, &cancel).is_err());
        assert!(!out.join(".2.partial").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unknown_device_mapping_fails_closed_and_space_is_checked() {
        let source = Source {
            id: "invalid".into(),
            name: "invalid".into(),
            path: "/definitely-not-a-device".into(),
            size: 1,
            kind: "device".into(),
            accessible: false,
            detail: String::new(),
        };
        assert!(platform::ensure_separate_device(&source, &std::env::temp_dir()).is_err());
        assert!(platform::check_free_space(&std::env::temp_dir(), u64::MAX).is_err());
        let canonical = std::env::temp_dir().canonicalize().unwrap();
        platform::check_free_space(&canonical, 1).expect("A healthy test directory must accept a small output, including Windows canonical paths");
    }
}
