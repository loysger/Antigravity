pub mod db;
pub mod local_proxy;
pub mod dns;
pub mod tunnel;
pub mod process_utils;

use std::sync::Mutex;
use tokio::sync::watch;

// Global proxy shutdown handle
static PROXY_SHUTDOWN: Mutex<Option<watch::Sender<bool>>> = Mutex::new(None);
static PROXY_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[allow(dead_code)]
static PROXY_SESSION_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn prepare_and_start_single_ide(
    token: &str,
    ide_type: &str,
    custom_exe_path: Option<&str>,
    custom_db_path: Option<&str>,
) -> Result<(), String> {
    eprintln!("[Client] Initializing proxy bridge for {}...", ide_type);
    
    // Store credentials and configure local proxy URL in IDE profile
    db::inject_real_token(
        token,
        "proxy_managed_refresh_token",
        4070908800i64,
        "http://127.0.0.1:8047/v1",
        ide_type,
        custom_db_path,
    )?;

    // Verify IDE installation and environment integrity
    verify_ide_environment(ide_type, custom_exe_path)?;

    // Launch IDE with configured proxy environment
    start_antigravity_ide(ide_type, custom_exe_path)?;
    Ok(())
}

#[tauri::command]
async fn inject_token_and_start_ide(
    token: String, 
    proxy_url: String,
    ide_type: String,
    custom_exe_path: Option<String>,
    custom_db_path: Option<String>,
) -> Result<String, String> {
    let base_url = proxy_url.trim_end_matches("/v1").to_string();

    // 1. Ensure local proxy on port 8047 is active (Singleton).
    // If it's already running, reuse it so IDE, 2.0, and CLI can all share port 8047
    if !PROXY_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
        eprintln!("[Client] Starting local proxy on :8047 -> {}", base_url);

        let config = local_proxy::ProxyConfig {
            listen_port: 8047,
            target_url: base_url.clone(),
            bearer_token: token.clone(),
        };

        let shutdown_tx = local_proxy::start_proxy(config)
            .await
            .map_err(|e| format!("Failed to start local proxy: {}", e))?;

        if let Ok(mut guard) = PROXY_SHUTDOWN.lock() {
            *guard = Some(shutdown_tx);
        }
        PROXY_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
        eprintln!("[Client] Local proxy started successfully on :8047");

        // Start reverse tunnel connection to Gateway
        eprintln!("[Client] Starting reverse tunnel connection to Gateway...");
        crate::tunnel::start_tunnel_worker(base_url.clone(), token.clone()).await;
    } else {
        eprintln!("[Client] Local proxy already running on :8047, reusing existing instance");
    }

    // 2. Launch requested tool(s)
    if ide_type == "all" {
        let _ = prepare_and_start_single_ide(&token, "Antigravity IDE", custom_exe_path.as_deref(), custom_db_path.as_deref()).await;
        let _ = prepare_and_start_single_ide(&token, "Antigravity 2.0", custom_exe_path.as_deref(), custom_db_path.as_deref()).await;
        let _ = prepare_and_start_single_ide(&token, "Antigravity CLI", custom_exe_path.as_deref(), custom_db_path.as_deref()).await;
        Ok("All tools (IDE, 2.0, CLI) launched successfully with shared proxy on :8047".to_string())
    } else {
        prepare_and_start_single_ide(&token, &ide_type, custom_exe_path.as_deref(), custom_db_path.as_deref()).await?;
        Ok(format!("{} launched successfully with shared proxy on :8047", ide_type))
    }
}

#[tauri::command]
fn stop_proxy() -> Result<String, String> {
    stop_existing_proxy();
    restore_original_state_all();
    Ok("Proxy stopped and original state restored".to_string())
}

#[tauri::command]
fn get_proxy_status() -> bool {
    PROXY_RUNNING.load(std::sync::atomic::Ordering::SeqCst)
}

fn stop_existing_proxy() {
    if let Ok(mut guard) = PROXY_SHUTDOWN.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(true);
            eprintln!("[Client] Sent proxy shutdown signal");
        }
    }
    PROXY_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
}

#[allow(dead_code)]
fn stop_proxy_for_session(session_id: u64) {
    let current = PROXY_SESSION_ID.load(std::sync::atomic::Ordering::SeqCst);
    if current == session_id {
        stop_existing_proxy();
    }
}

#[allow(dead_code)]
fn kill_running_antigravity(ide_type: &str) {
    let proc_names: &[&str] = match ide_type {
        "Antigravity 2.0" => &["antigravity", "Antigravity.exe", "antigravity.exe"],
        "Antigravity CLI" => &["agy", "agy.exe"],
        _ => &["Antigravity IDE", "Antigravity IDE.exe", "antigravity-ide"],
    };

    for name in proc_names {
        #[cfg(target_os = "windows")]
        {
            let _ = crate::process_utils::command("taskkill")
                .args(["/F", "/IM", name])
                .output();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = crate::process_utils::command("pkill")
                .args(["-f", name])
                .output();
        }
    }
}

#[allow(dead_code)]
fn get_app_bundle_path(ide_type: &str, custom_exe_path: Option<&str>) -> std::path::PathBuf {
    if let Some(path) = custom_exe_path {
        if !path.is_empty() {
            let p = std::path::PathBuf::from(path);
            if p.extension().map_or(false, |ext| ext == "app") {
                return p;
            }
            let mut current = p.as_path();
            while let Some(parent) = current.parent() {
                if parent.extension().map_or(false, |ext| ext == "app") {
                    return parent.to_path_buf();
                }
                current = parent;
            }
            return p;
        }
    }

    let default_name = if ide_type == "Antigravity 2.0" {
        "Antigravity.app"
    } else {
        "Antigravity IDE.app"
    };

    let global_app = std::path::PathBuf::from(format!("/Applications/{}", default_name));
    if global_app.exists() {
        return global_app;
    }

    if let Some(home) = dirs::home_dir() {
        let user_app = home.join(format!("Applications/{}", default_name));
        if user_app.exists() {
            return user_app;
        }
    }

    global_app
}

fn get_ide_resources_path(ide_type: &str, custom_exe_path: Option<&str>) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let app_path = get_app_bundle_path(ide_type, custom_exe_path);
        let res = app_path.join("Contents/Resources");
        if res.exists() {
            return Some(res);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(path) = custom_exe_path {
            if !path.is_empty() {
                let p = std::path::Path::new(path);
                if let Some(parent) = p.parent() {
                    let res = parent.join("resources");
                    if res.exists() {
                        return Some(res);
                    }
                }
            }
        }
        let appdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let folder = if ide_type == "Antigravity 2.0" { "antigravity" } else { "Antigravity IDE" };
        let res = std::path::PathBuf::from(appdata).join("Programs").join(folder).join("resources");
        if res.exists() {
            return Some(res);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(path) = custom_exe_path {
            if !path.is_empty() {
                let p = std::path::Path::new(path);
                if let Some(parent) = p.parent() {
                    let res = parent.join("resources");
                    if res.exists() {
                        return Some(res);
                    }
                }
            }
        }
        let folder = if ide_type == "Antigravity 2.0" { "antigravity" } else { "antigravity-ide" };
        let candidates = [
            std::path::PathBuf::from(format!("/usr/share/{}/resources", folder)),
            std::path::PathBuf::from(format!("/opt/{}/resources", folder)),
        ];
        for c in &candidates {
            if c.exists() {
                return Some(c.clone());
            }
        }
    }

    None
}

fn get_default_cli_path() -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
        let cli_path = std::path::PathBuf::from(user_profile).join(".antigravity").join("bin").join("agy.exe");
        if cli_path.exists() {
            return Ok(cli_path);
        }
        if let Ok(path) = crate::process_utils::command("where").arg("agy.exe").output() {
            if path.status.success() {
                let p = String::from_utf8_lossy(&path.stdout).lines().next().unwrap_or("").trim().to_string();
                if !p.is_empty() {
                    return Ok(std::path::PathBuf::from(p));
                }
            }
        }
        return Ok(cli_path);
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = dirs::home_dir() {
            let cli_path = home.join(".antigravity/bin/agy");
            if cli_path.exists() {
                return Ok(cli_path);
            }
        }
        if let Ok(path) = crate::process_utils::command("which").arg("agy").output() {
            if path.status.success() {
                let p = String::from_utf8_lossy(&path.stdout).lines().next().unwrap_or("").trim().to_string();
                if !p.is_empty() {
                    return Ok(std::path::PathBuf::from(p));
                }
            }
        }
        Ok(std::path::PathBuf::from("/usr/local/bin/agy"))
    }
}

/// Verify environment compatibility and runtime paths
fn verify_ide_environment(ide_type: &str, custom_exe_path: Option<&str>) -> Result<(), String> {
    if ide_type == "Antigravity CLI" {
        let agy_path = if let Some(path) = custom_exe_path {
            if !path.is_empty() {
                std::path::PathBuf::from(path)
            } else {
                get_default_cli_path()?
            }
        } else {
            get_default_cli_path()?
        };

        if !agy_path.exists() {
            eprintln!("[Client] CLI binary not found at default location: {:?}", agy_path);
        }
        return Ok(());
    }

    if let Some(res) = get_ide_resources_path(ide_type, custom_exe_path) {
        eprintln!("[Client] Validated IDE runtime resources at: {:?}", res);
    }

    Ok(())
}

/// Spawns Antigravity CLI in a standalone interactive terminal with proxy environment
fn spawn_cli_in_terminal(path: &str) -> Result<(), String> {
    eprintln!("[Client] Spawning CLI in terminal with proxy configured on 127.0.0.1:8047");

    #[cfg(target_os = "windows")]
    {
        let wt_check = crate::process_utils::command("where").arg("wt.exe").output();
        let has_wt = wt_check.map(|o| o.status.success()).unwrap_or(false);

        let ps_cmd = format!(
            "$env:HTTP_PROXY='http://127.0.0.1:8047'; \
             $env:HTTPS_PROXY='http://127.0.0.1:8047'; \
             $env:ALL_PROXY='http://127.0.0.1:8047'; \
             & '{}'",
            path.replace("'", "''")
        );

        if has_wt {
            match crate::process_utils::command("wt.exe")
                .args(["powershell.exe", "-NoExit", "-Command", &ps_cmd])
                .spawn()
            {
                Ok(_) => return Ok(()),
                Err(e) => eprintln!("[Client] Failed to spawn Windows Terminal, falling back: {}", e),
            }
        }

        crate::process_utils::command("cmd.exe")
            .args(["/c", "start", "powershell.exe", "-NoExit", "-Command", &ps_cmd])
            .spawn()
            .map_err(|e| format!("Failed to spawn PowerShell: {}", e))?;

        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let escaped_path = path.replace("\\", "\\\\").replace("\"", "\\\"");
        let apple_script = format!(
            "tell application \"Terminal\"\n\
                activate\n\
                do script \"export HTTP_PROXY=http://127.0.0.1:8047 HTTPS_PROXY=http://127.0.0.1:8047 ALL_PROXY=http://127.0.0.1:8047; \\\"{}\\\"\"\n\
            end tell",
            escaped_path
        );

        crate::process_utils::command("osascript")
            .args(["-e", &apple_script])
            .spawn()
            .map_err(|e| format!("Failed to launch Terminal via osascript: {}", e))?;

        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let term_command = format!(
            "export HTTP_PROXY=http://127.0.0.1:8047 HTTPS_PROXY=http://127.0.0.1:8047 ALL_PROXY=http://127.0.0.1:8047; \"{}\"; exec $SHELL",
            path.replace("\"", "\\\"")
        );

        let terminals = [
            ("x-terminal-emulator", vec!["-e", "bash", "-c", &term_command]),
            ("gnome-terminal", vec!["--", "bash", "-c", &term_command]),
            ("konsole", vec!["-e", "bash", "-c", &term_command]),
            ("xfce4-terminal", vec!["-e", &format!("bash -c '{}'", term_command)]),
            ("alacritty", vec!["-e", "bash", "-c", &term_command]),
            ("kitty", vec!["bash", "-c", &term_command]),
            ("xterm", vec!["-e", "bash", "-c", &term_command]),
        ];

        for (term, args) in &terminals {
            if crate::process_utils::command("which").arg(term).output().map(|o| o.status.success()).unwrap_or(false) {
                if crate::process_utils::command(term).args(args).spawn().is_ok() {
                    return Ok(());
                }
            }
        }

        Err("No supported Linux terminal emulator found".to_string())
    }
}

/// Start Antigravity IDE with proxy arguments and environment variables
fn start_antigravity_ide(ide_type: &str, custom_exe_path: Option<&str>) -> Result<(), String> {
    if ide_type == "Antigravity CLI" {
        let path = if let Some(p) = custom_exe_path {
            if !p.is_empty() {
                p.to_string()
            } else {
                get_default_cli_path()?.to_string_lossy().to_string()
            }
        } else {
            get_default_cli_path()?.to_string_lossy().to_string()
        };
        let p_check = std::path::Path::new(&path);
        if !p_check.exists() {
            return Err(format!("Antigravity CLI binary not found at: {}", path));
        }
        return spawn_cli_in_terminal(&path);
    }

    // Prepare proxy environment flags
    let proxy_addr = "http://127.0.0.1:8047";
    let proxy_args = ["--proxy-server=http://127.0.0.1:8047", "--proxy-bypass-list=<-loopback>"];

    #[cfg(target_os = "windows")]
    {
        let exe_path = if let Some(path) = custom_exe_path {
            if !path.is_empty() {
                path.to_string()
            } else {
                get_default_windows_ide_path(ide_type)?
            }
        } else {
            get_default_windows_ide_path(ide_type)?
        };

        let p = std::path::Path::new(&exe_path);
        if !p.exists() {
            return Err(format!("Executable not found at path: {}", exe_path));
        }

        crate::process_utils::command(&exe_path)
            .args(proxy_args)
            .env("HTTP_PROXY", proxy_addr)
            .env("HTTPS_PROXY", proxy_addr)
            .env("ALL_PROXY", proxy_addr)
            .env("NO_PROXY", "localhost,127.0.0.1")
            .spawn()
            .map_err(|e| format!("Failed to spawn executable {}: {}", exe_path, e))?;

        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let app_path = get_app_bundle_path(ide_type, custom_exe_path);
        if !app_path.exists() {
            return Err(format!("Application bundle not found at: {:?}", app_path));
        }

        crate::process_utils::command("open")
            .args(["-n", &app_path.to_string_lossy(), "--args", "--proxy-server=http://127.0.0.1:8047"])
            .env("HTTP_PROXY", proxy_addr)
            .env("HTTPS_PROXY", proxy_addr)
            .spawn()
            .map_err(|e| format!("Failed to launch macOS application: {}", e))?;

        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        // Inject settings before launching
        let _ = crate::db::inject_to_settings("http://127.0.0.1:8047/v1", ide_type);

        let exe_path = if let Some(path) = custom_exe_path {
            if !path.is_empty() {
                path.to_string()
            } else {
                get_default_linux_ide_path(ide_type)?
            }
        } else {
            get_default_linux_ide_path(ide_type)?
        };

        crate::process_utils::command(&exe_path)
            .args(proxy_args)
            .env("HTTP_PROXY", proxy_addr)
            .env("HTTPS_PROXY", proxy_addr)
            .env("ALL_PROXY", proxy_addr)
            .spawn()
            .map_err(|e| format!("Failed to spawn Linux binary: {}", e))?;

        return Ok(());
    }
}

#[cfg(target_os = "windows")]
fn get_default_windows_ide_path(ide_type: &str) -> Result<String, String> {
    let appdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let path = if ide_type == "Antigravity 2.0" {
        format!(r"{}\Programs\antigravity\Antigravity.exe", appdata)
    } else {
        format!(r"{}\Programs\Antigravity IDE\Antigravity IDE.exe", appdata)
    };
    Ok(path)
}

#[cfg(target_os = "linux")]
fn get_default_linux_ide_path(ide_type: &str) -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if ide_type == "Antigravity 2.0" {
        candidates.push(home.join(".local/bin/antigravity"));
        candidates.push(std::path::PathBuf::from("/usr/bin/antigravity"));
        candidates.push(std::path::PathBuf::from("/usr/local/bin/antigravity"));
    } else {
        candidates.push(home.join(".local/bin/antigravity-ide"));
        candidates.push(std::path::PathBuf::from("/usr/bin/antigravity-ide"));
        candidates.push(std::path::PathBuf::from("/usr/local/bin/antigravity-ide"));
    }

    for c in &candidates {
        if c.exists() {
            return Ok(c.to_string_lossy().to_string());
        }
    }

    if let Some(first) = candidates.first() {
        return Ok(first.to_string_lossy().to_string());
    }

    Err("Antigravity executable not found on Linux".to_string())
}

fn restore_files(ide_type: &str) -> Result<(), String> {
    eprintln!("[Client] Resetting proxy configuration for {}", ide_type);
    db::clear_proxy_settings(ide_type)?;
    Ok(())
}

fn restore_original_state_all() {
    eprintln!("[Client] Restoring default environment state...");
    for ide in &["Antigravity IDE", "Antigravity 2.0", "Antigravity CLI"] {
        if let Err(e) = db::clear_proxy_settings(ide) {
            eprintln!("[Client] Warning: Failed to clear proxy settings for {}: {}", ide, e);
        }
        if let Err(e) = restore_files(ide) {
            eprintln!("[Client] Warning: Failed to reset state for {}: {}", ide, e);
        }
    }
}

#[tauri::command]
async fn install_client_update(app_handle: tauri::AppHandle, download_url: String) -> Result<(), String> {
    eprintln!("[Client] Starting update installation. Download URL: {}", download_url);
    
    let temp_dir = std::env::temp_dir();
    #[cfg(target_os = "windows")]
    let msi_path = temp_dir.join("AntigravityClient_setup.msi");
    #[cfg(target_os = "macos")]
    let msi_path = temp_dir.join("AntigravityClient_setup.dmg");
    #[cfg(target_os = "linux")]
    let msi_path = temp_dir.join("AntigravityClient_setup.AppImage");
    
    let client = reqwest::Client::new();
    let resp = client.get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to send download request: {}", e))?;
        
    if !resp.status().is_success() {
        return Err(format!("Download request failed with status: {}", resp.status()));
    }
    
    let bytes = resp.bytes().await.map_err(|e| format!("Failed to read response bytes: {}", e))?;
    std::fs::write(&msi_path, bytes).map_err(|e| format!("Failed to write setup file: {}", e))?;
    
    eprintln!("[Client] Setup file downloaded to {:?}", msi_path);
    
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to get current executable path: {}", e))?;
        
    #[cfg(target_os = "windows")]
    {
        let ps_command = format!(
            "Start-Sleep -Seconds 2; Start-Process msiexec.exe -ArgumentList '/i', '{}', '/passive', '/norestart' -Wait; Start-Process '{}'",
            msi_path.to_string_lossy(),
            current_exe.to_string_lossy()
        );
        
        crate::process_utils::command("powershell")
            .arg("-NoProfile")
            .arg("-WindowStyle")
            .arg("Hidden")
            .arg("-Command")
            .arg(ps_command)
            .spawn()
            .map_err(|e| format!("Failed to spawn PowerShell installer script: {}", e))?;
            
        app_handle.exit(0);
    }
    
    #[cfg(target_os = "macos")]
    {
        let app_bundle = current_exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .ok_or("Failed to find .app bundle path")?;
        let install_dir = app_bundle
            .parent()
            .ok_or("Failed to find install directory")?;
        let app_name = app_bundle
            .file_name()
            .ok_or("Failed to get .app bundle name")?
            .to_string_lossy()
            .to_string();

        let sh_command = format!(
            "sleep 2; \
             hdiutil attach -nobrowse -mountpoint /tmp/ag_mount '{}'; \
             cp -R '/tmp/ag_mount/{}' '{}/'; \
             hdiutil detach /tmp/ag_mount; \
             xattr -cr '{}/{}'; \
             open -n '{}/{}'",
            msi_path.to_string_lossy(),
            app_name,
            install_dir.to_string_lossy(),
            install_dir.to_string_lossy(), app_name,
            install_dir.to_string_lossy(), app_name,
        );
        
        crate::process_utils::command("sh")
            .arg("-c")
            .arg(sh_command)
            .spawn()
            .map_err(|e| format!("Failed to spawn macOS shell installer: {}", e))?;
            
        app_handle.exit(0);
    }
    
    #[cfg(target_os = "linux")]
    {
        let sh_command = format!(
            "sleep 2; chmod +x '{}'; mv '{}' '{}'; '{}' &",
            msi_path.to_string_lossy(),
            msi_path.to_string_lossy(),
            current_exe.to_string_lossy(),
            current_exe.to_string_lossy()
        );
        
        crate::process_utils::command("sh")
            .arg("-c")
            .arg(sh_command)
            .spawn()
            .map_err(|e| format!("Failed to spawn Linux shell installer: {}", e))?;
            
        app_handle.exit(0);
    }
    
    Ok(())
}

#[tauri::command]
fn get_platform_info() -> String {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "mac-intel".to_string();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "mac-arm".to_string();
    #[cfg(target_os = "linux")]
    return "linux".to_string();
    #[cfg(target_os = "windows")]
    return "windows".to_string();
}

#[tauri::command]
async fn request_server(
    method: String,
    url: String,
    headers: std::collections::HashMap<String, String>,
    body: Option<String>,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let mut req = match method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        _ => client.get(&url),
    };
    for (k, v) in headers {
        req = req.header(k, v);
    }
    if let Some(b) = body {
        req = req.body(b);
    }
    req = req.timeout(std::time::Duration::from_secs(10));
    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;
    if status.is_success() {
        Ok(text)
    } else {
        Err(format!("HTTP {} : {}", status.as_u16(), text))
    }
}

#[tauri::command]
async fn browse_executable() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let ps_script = r#"
            Add-Type -AssemblyName System.Windows.Forms
            $f = New-Object System.Windows.Forms.OpenFileDialog
            $f.Filter = "Executable Files (*.exe)|*.exe|All Files (*.*)|*.*"
            $f.Title = "Select Antigravity Executable"
            $f.ShowHelp = $true
            if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                Write-Output $f.FileName
            }
        "#;

        let output = crate::process_utils::command("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
            .output()
            .map_err(|e| format!("Failed to run file dialog: {}", e))?;

        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path_str.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path_str))
            }
        } else {
            let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(format!("File dialog error: {}", err))
        }
    }

    #[cfg(target_os = "macos")]
    {
        let output = crate::process_utils::command("osascript")
            .args(["-e", "POSIX path of (choose file with prompt \"Select Antigravity Executable\")"])
            .output()
            .map_err(|e| format!("Failed to run file dialog: {}", e))?;

        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path_str.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path_str))
            }
        } else {
            Ok(None)
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("File dialog is only supported on Windows and macOS".to_string())
    }
}

#[tauri::command]
fn save_token(token: String) -> Result<(), String> {
    if let Some(home) = dirs::home_dir() {
        let config_dir = home.join(".antigravity-client");
        let _ = std::fs::create_dir_all(&config_dir);
        let token_file = config_dir.join("saved_token.txt");
        let _ = std::fs::write(&token_file, token.trim());
    }
    Ok(())
}

#[tauri::command]
fn get_saved_token() -> Result<String, String> {
    if let Some(home) = dirs::home_dir() {
        let token_file = home.join(".antigravity-client").join("saved_token.txt");
        if token_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&token_file) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(trimmed);
                }
            }
        }
    }
    Ok(String::new())
}

#[tauri::command]
fn clear_saved_token() -> Result<(), String> {
    if let Some(home) = dirs::home_dir() {
        let token_file = home.join(".antigravity-client").join("saved_token.txt");
        if token_file.exists() {
            let _ = std::fs::remove_file(&token_file);
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            eprintln!("[Client] Application startup. Initializing environment state...");
            restore_original_state_all();
            
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            
            if let Some(icon) = app.default_window_icon() {
                let show_i = MenuItem::with_id(app, "show", "Развернуть (Show)", true, None::<&str>)?;
                let quit_i = MenuItem::with_id(app, "quit", "Выход (Quit)", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

                let _tray = TrayIconBuilder::new()
                    .icon(icon.clone())
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => {
                            std::process::exit(0);
                        }
                        "show" => {
                            use tauri::Manager;
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        } = event {
                            use tauri::Manager;
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    })
                    .build(app)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            inject_token_and_start_ide,
            stop_proxy,
            get_proxy_status,
            install_client_update,
            get_platform_info,
            request_server,
            browse_executable,
            save_token,
            get_saved_token,
            clear_saved_token
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let window_clone = window.clone();
                use tauri::Manager;
                let app_handle = window.app_handle().clone();
                
                use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
                use tauri_plugin_dialog::MessageDialogKind;
                
                app_handle.dialog().message("Что сделать с клиентом Antigravity?")
                    .title("Закрытие")
                    .kind(MessageDialogKind::Info)
                    .buttons(MessageDialogButtons::OkCancelCustom("Свернуть в трей".to_string(), "Закрыть полностью".to_string()))
                    .show(move |result| {
                        if result {
                            let _ = window_clone.hide();
                        } else {
                            std::process::exit(0);
                        }
                    });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
