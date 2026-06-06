use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
use tauri::Emitter;

use super::logger::log as launcher_log;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub download_url: String,
    pub installer_url: String,
    pub release_notes: String,
    pub file_size: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct UpdateProgress {
    pub stage: String,
    pub downloaded: u64,
    pub total: u64,
    pub speed_kb: u64,
    pub message: String,
}

const GITHUB_REPO: &str = "Sadoul/ramz_launcher";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ramz")
}


fn marker_path() -> PathBuf {
    data_dir().join("update_marker")
}


#[tauri::command]
pub fn check_just_updated() -> bool {
    let path = marker_path();
    if !path.exists() {
        return false;
    }

    let content = fs::read_to_string(&path).unwrap_or_default();
    let _ = fs::remove_file(&path);


    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if let Some(ts_str) = content.split(':').nth(1) {
        if let Ok(ts) = ts_str.parse::<u64>() {
            let age_secs = now.saturating_sub(ts);
            launcher_log(&format!("[updater] Found update marker, age: {}s", age_secs));
            if age_secs < 300 {
                launcher_log("[updater] Marker is fresh — skipping update check this run");
                return true;
            }
            launcher_log("[updater] Marker is stale (>5min) — ignoring, will check for updates");
            return false;
        }
    }


    launcher_log("[updater] Found old-format marker without timestamp — ignoring");
    false
}

fn update_log_path() -> PathBuf {
    std::env::temp_dir().join("ramz_update.log")
}

fn update_log(message: &str) {
    let line = format!("[{}] {}\r\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), message);
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(update_log_path())
        .and_then(|mut file| file.write_all(line.as_bytes()));
    launcher_log(message);
}

fn write_update_marker() {
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let content = format!("{}:{}", CURRENT_VERSION, ts);
    let _ = fs::write(marker_path(), content);
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect::<Vec<_>>()
    };
    parse(a).cmp(&parse(b))
}


#[tauri::command]
pub async fn check_launcher_update() -> Result<UpdateInfo, String> {
    launcher_log(&format!("[updater] Checking for updates. Current version: {}", CURRENT_VERSION));

    let client = reqwest::Client::builder()
        .user_agent("RamzLauncher/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let api_url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    launcher_log(&format!("[updater] Fetching: {}", api_url));

    let response = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| {
            let msg = format!("[updater] Network error: {}", e);
            launcher_log(&msg);
            msg
        })?;

    let status = response.status();
    launcher_log(&format!("[updater] GitHub API response: {}", status));

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let is_rate_limit = body.contains("rate limit") || body.contains("API rate limit");
        let msg = if is_rate_limit {
            format!(
                "[updater] GitHub rate limit exceeded for this IP (anonymous limit = 60/hour). Skipping update check this run. Body: {}",
                body
            )
        } else {
            format!(
                "[updater] GitHub API returned {}. Update check failed. Body: {}",
                status, body
            )
        };
        launcher_log(&msg);

        // For rate-limit / 403 we silently skip the update check rather than
        // surfacing a scary "token expired" error to the user. Return a
        // "no update available" result so the rest of the launcher works.
        if is_rate_limit || status == reqwest::StatusCode::FORBIDDEN {
            return Ok(UpdateInfo {
                current_version: CURRENT_VERSION.to_string(),
                latest_version: CURRENT_VERSION.to_string(),
                update_available: false,
                download_url: String::new(),
                installer_url: String::new(),
                release_notes: String::new(),
                file_size: 0,
            });
        }
        return Err(msg);
    }

    let release: serde_json::Value = response.json().await.map_err(|e| {
        let msg = format!("[updater] JSON parse error: {}", e);
        launcher_log(&msg);
        msg
    })?;

    let tag = release["tag_name"]
        .as_str()
        .unwrap_or(CURRENT_VERSION)
        .to_string();
    let latest_clean = tag.trim_start_matches('v').to_string();
    launcher_log(&format!("[updater] Latest release tag: {} (clean: {})", tag, latest_clean));

    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    launcher_log(&format!("[updater] Release has {} assets", assets.len()));

    let mut installer_url = String::new();
    let mut file_size: u64 = 0;

    // Ищем main launcher .exe (без NSIS). Имя как в Cargo.toml package.name:
    // project-doomsday-launcher.exe (новое) или ramz-launcher.exe (легаси).
    for asset in &assets {
        let name = asset["name"].as_str().unwrap_or("");
        let url = asset["browser_download_url"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let size = asset["size"].as_u64().unwrap_or(0);
        launcher_log(&format!("[updater] Asset: {} ({} bytes)", name, size));

        let n = name.to_lowercase();
        if (n == "pd-launcher-core.exe"
            || n == "project-doomsday-launcher.exe"
            || n == "ramz-launcher.exe")
            && !n.contains("debug")
        {
            installer_url = url;
            file_size = size;
            launcher_log(&format!("[updater] Selected main launcher exe: {}", name));
        }
    }

    let raw_notes = release["body"].as_str().unwrap_or("").trim();
    let release_notes = if raw_notes.is_empty() || raw_notes.contains("Full Changelog") || raw_notes.contains("github.com/Sadoul/ramz_launcher/compare/") {
        format!("Обновление лаунчера до версии v{}", latest_clean)
    } else {
        raw_notes.to_string()
    };
    let version_cmp = compare_versions(&latest_clean, CURRENT_VERSION);
    let update_available = !installer_url.is_empty()
        && version_cmp == std::cmp::Ordering::Greater;

    launcher_log(&format!(
        "[updater] Version comparison: {} vs {} => {:?} | installer_found={} | update_available={}",
        latest_clean, CURRENT_VERSION, version_cmp, !installer_url.is_empty(), update_available
    ));

    Ok(UpdateInfo {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_clean,
        update_available,
        download_url: installer_url.clone(),
        installer_url,
        release_notes,
        file_size,
    })
}

#[tauri::command]
pub async fn update_launcher(app: tauri::AppHandle) -> Result<String, String> {
    let info = check_launcher_update().await?;

    if !info.update_available {
        return Ok("no_update".to_string());
    }

    let app_ref = app.clone();
    let emit = move |stage: &str, downloaded: u64, total: u64, speed: u64, msg: &str| {
        let _ = app_ref.emit(
            "update-progress",
            UpdateProgress {
                stage: stage.to_string(),
                downloaded,
                total,
                speed_kb: speed,
                message: msg.to_string(),
            },
        );
    };

    update_log(&format!("[updater] Starting in-app update {} -> {}", info.current_version, info.latest_version));
    emit("downloading", 0, info.file_size, 0, "Начало скачивания...");

    // Generous timeouts: GitHub release downloads can stall on slow links,
    // and we don't want a transient blip to kill a 10+ MB installer download.
    let client = reqwest::Client::builder()
        .user_agent("RamzLauncher/1.0")
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(15 * 60))
        .build()
        .map_err(|e| e.to_string())?;

    let temp_dir = std::env::temp_dir();
    let download_path = temp_dir.join(format!("pd-launcher-{}.exe", info.latest_version));
    update_log(&format!("[updater] Download target: {}", download_path.display()));

    // Retry loop: GitHub download can disconnect mid-stream; on flaky links we
    // try up to 3 times before giving up. Each attempt restarts from scratch.
    let mut last_err = String::new();
    let mut attempt = 0u32;
    let max_attempts = 3u32;
    let mut total: u64 = info.file_size;
    loop {
        attempt += 1;
        update_log(&format!("[updater] Download attempt {}/{}", attempt, max_attempts));

        let response = match client.get(&info.installer_url).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("Ошибка скачивания (попытка {}/{}): {}", attempt, max_attempts, e);
                update_log(&last_err);
                if attempt >= max_attempts {
                    return Err(last_err);
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        if !response.status().is_success() {
            return Err(format!("Ошибка скачивания обновления: HTTP {}", response.status()));
        }

        total = response.content_length().unwrap_or(info.file_size);

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut file = match fs::File::create(&download_path) {
            Ok(f) => f,
            Err(e) => return Err(format!("Не удалось создать файл: {}", e)),
        };
        let start_time = std::time::Instant::now();
        let mut stream_failed = false;

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    last_err = format!(
                        "Соединение прервано на {} байтах (попытка {}/{}): {}",
                        downloaded, attempt, max_attempts, e
                    );
                    update_log(&last_err);
                    stream_failed = true;
                    break;
                }
            };
            if let Err(e) = file.write_all(&chunk) {
                return Err(format!("Ошибка записи файла: {}", e));
            }
            downloaded += chunk.len() as u64;

            let elapsed = start_time.elapsed().as_secs_f64();
            let speed_kb = if elapsed > 0.1 {
                (downloaded as f64 / elapsed / 1024.0) as u64
            } else {
                0
            };

            let mb_done = downloaded as f64 / 1_048_576.0;
            let mb_total = total as f64 / 1_048_576.0;
            emit(
                "downloading",
                downloaded,
                total,
                speed_kb,
                &format!("Скачивание... {:.1}/{:.1} МБ", mb_done, mb_total),
            );
        }
        drop(file);

        if stream_failed {
            if attempt >= max_attempts {
                return Err(last_err);
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }

        update_log(&format!("[updater] Download finished: {} bytes", downloaded));
        break;
    }

    emit("applying", total, total, 0, "Установка обновления...");


    tokio::time::sleep(std::time::Duration::from_millis(800)).await;


    write_update_marker();

    apply_exe_update(app, &download_path)?;

    Ok("update_started".to_string())
}

// Без NSIS: подменяем main launcher .exe напрямую.
//   1. Текущий процесс держит занятым свой .exe — переименовываем его в .old.
//   2. Кладём свежескачанный .exe на место текущего.
//   3. PowerShell ждёт пока процесс умрёт, запускает новый .exe, чистит .old.
fn apply_exe_update(app: tauri::AppHandle, new_exe: &PathBuf) -> Result<(), String> {
    update_log(&format!("[updater] Applying .exe update: {}", new_exe.display()));

    #[cfg(windows)]
    {
        let exe = std::env::current_exe()
            .map_err(|e| format!("Не удалось получить путь лаунчера: {e}"))?;

        let exe_str       = exe.to_string_lossy().replace('\'', "''");
        let new_exe_str   = new_exe.to_string_lossy().replace('\'', "''");
        let old_exe_str   = format!("{}.old", exe_str);
        let script_path   = std::env::temp_dir().join("pd_update.ps1");
        let script_str    = script_path.to_string_lossy().replace('\'', "''");

        // Скрипт ждёт пока текущий процесс отпустит свой .exe, переименовывает
        // его в .exe.old, копирует новый .exe на место, запускает его и удаляет
        // .exe.old. Если что-то падает — оставляем .exe.old, при следующем
        // запуске можно вручную восстановить.
        let ps1 = format!(
            "\
$exe = '{exe}'
$new = '{new_exe}'
$old = '{old}'
# Wait until the launcher releases its own .exe (Windows file lock).
for ($i=0; $i -lt 60; $i++) {{
    try {{
        if (Test-Path $old) {{ Remove-Item $old -Force -ErrorAction SilentlyContinue }}
        Rename-Item -Path $exe -NewName ([System.IO.Path]::GetFileName($old)) -ErrorAction Stop
        break
    }} catch {{
        Start-Sleep -Milliseconds 500
    }}
}}
Copy-Item -Path $new -Destination $exe -Force
Start-Sleep -Seconds 1
Start-Process -FilePath $exe
Start-Sleep -Seconds 2
Remove-Item $old -Force -ErrorAction SilentlyContinue
Remove-Item $new -Force -ErrorAction SilentlyContinue
Remove-Item '{script}' -Force -ErrorAction SilentlyContinue
",
            exe = exe_str,
            new_exe = new_exe_str,
            old = old_exe_str,
            script = script_str,
        );
        std::fs::write(&script_path, ps1.as_bytes())
            .map_err(|e| format!("Не удалось записать скрипт обновления: {e}"))?;
        update_log(&format!("[updater] Script written to {}", script_path.display()));

        // Запускаем скрипт через Start-Process чтобы он жил вне Job Object'а
        // текущего процесса и выжил после app.exit().
        let outer = format!(
            "Start-Process -FilePath powershell -ArgumentList @('-NoProfile','-NonInteractive','-WindowStyle','Hidden','-ExecutionPolicy','Bypass','-File','{}') -WindowStyle Hidden",
            script_str
        );
        let result = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &outer])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match result {
            Ok(_)  => update_log("[updater] Update watcher launched"),
            Err(e) => update_log(&format!("[updater] Failed to launch watcher: {e}")),
        }
    }

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        app.exit(0);
    });

    Ok(())
}