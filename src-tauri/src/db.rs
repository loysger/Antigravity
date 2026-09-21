use rusqlite::Connection;
use std::path::PathBuf;

/// Path to local Antigravity Client configuration database
fn get_client_db_path() -> Result<PathBuf, String> {
    let base_dir = dirs::home_dir()
        .ok_or_else(|| "Failed to resolve user home directory".to_string())?
        .join(".antigravity-client");
    
    std::fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Failed to create client config directory: {}", e))?;
        
    Ok(base_dir.join("client.db"))
}

/// Initialize SQLite database schema
fn init_db() -> Result<Connection, String> {
    let db_path = get_client_db_path()?;
    let conn = Connection::open(&db_path)
        .map_err(|e| format!("Failed to open SQLite database at {:?}: {}", db_path, e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        [],
    ).map_err(|e| format!("Failed to create app_config table: {}", e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            token TEXT NOT NULL,
            refresh_token TEXT NOT NULL,
            expiry INTEGER NOT NULL,
            proxy_url TEXT NOT NULL,
            ide_type TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );",
        [],
    ).map_err(|e| format!("Failed to create sessions table: {}", e))?;

    Ok(conn)
}

/// Store active session and configure proxy credentials
pub fn inject_real_token(
    token: &str,
    refresh_token: &str,
    expiry: i64,
    proxy_url: &str,
    ide_type: &str,
    _custom_db_path: Option<&str>,
) -> Result<(), String> {
    eprintln!("[Client] Storing active session credentials for {}...", ide_type);

    let conn = init_db()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, token, refresh_token, expiry, proxy_url, ide_type, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        rusqlite::params![ide_type, token, refresh_token, expiry, proxy_url, ide_type, now],
    ).map_err(|e| format!("Failed to store session in database: {}", e))?;

    // Store in OS Keyring / Credential Manager
    if let Err(e) = write_real_token_to_keyring(token, refresh_token, expiry) {
        eprintln!("[Client] Notice: Keyring storage not available ({}), relying on session database", e);
    }

    // Configure IDE settings.json with local proxy URL
    if let Err(e) = inject_to_settings(proxy_url, ide_type) {
        eprintln!("[Client] Notice: Could not update settings.json: {}", e);
    }

    eprintln!("[Client] Session credentials configured successfully for {}", ide_type);
    Ok(())
}

/// Inject proxyBaseUrl and proxy routing into IDE settings.json
pub fn inject_to_settings(proxy_url: &str, ide_type: &str) -> Result<(), String> {
    let settings_path = get_ide_settings_path(ide_type)?;

    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        if let Some(parent) = settings_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        serde_json::json!({})
    };

    if let Some(obj) = settings.as_object_mut() {
        let final_url = if proxy_url.ends_with("/v1") {
            proxy_url.to_string()
        } else {
            format!("{}/v1", proxy_url)
        };
        
        obj.insert("antigravity.proxyBaseUrl".to_string(), serde_json::json!(final_url));
        obj.insert("http.proxy".to_string(), serde_json::json!("http://127.0.0.1:8047"));
        obj.insert("http.proxyStrictSSL".to_string(), serde_json::json!(false));
        obj.insert("telemetry.telemetryLevel".to_string(), serde_json::json!("off"));
        obj.insert("telemetry.enableCrashReporter".to_string(), serde_json::json!(false));
        obj.insert("telemetry.enableTelemetry".to_string(), serde_json::json!(false));
    }

    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&settings_path, content)
        .map_err(|e| format!("Failed to write to {:?}: {}", settings_path, e))?;

    eprintln!("[Client] Configured proxy settings in {:?}", settings_path);
    Ok(())
}

/// Resolve IDE settings.json path across platforms
fn get_ide_settings_path(ide_type: &str) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or("Failed to get home directory")?;
        let subfolder = if ide_type == "Antigravity 2.0" { "Antigravity" } else { "Antigravity IDE" };
        Ok(home.join(format!("Library/Application Support/{}/User/settings.json", subfolder)))
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .map_err(|_| "Failed to get APPDATA environment variable".to_string())?;
        let subfolder = if ide_type == "Antigravity 2.0" { "antigravity" } else { "Antigravity IDE" };
        Ok(PathBuf::from(appdata).join(format!("{}\\User\\settings.json", subfolder)))
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or("Failed to get home directory")?;
        let subfolder = if ide_type == "Antigravity 2.0" { "Antigravity" } else { "Antigravity IDE" };
        Ok(home.join(format!(".config/{}/User/settings.json", subfolder)))
    }
}

/// Write OAuth token to system keyring as JSON
fn write_real_token_to_keyring(access_token: &str, refresh_token: &str, expiry: i64) -> Result<(), String> {
    let expiry_str = format_timestamp_rfc3339(expiry);

    let payload = serde_json::json!({
        "token": {
            "access_token": access_token,
            "token_type": "Bearer",
            "refresh_token": refresh_token,
            "expiry": expiry_str
        },
        "auth_method": "consumer"
    });

    let payload_json = serde_json::to_string(&payload)
        .map_err(|e| format!("Failed to serialize keyring JSON: {}", e))?;

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;

        let dbus_env = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| {
            let uid = std::env::var("UID").unwrap_or_else(|_| "1000".to_string());
            format!("unix:path=/run/user/{}/bus", uid)
        });

        let _ = crate::process_utils::command("secret-tool")
            .args(["clear", "service", "gemini", "username", "antigravity"])
            .env("DBUS_SESSION_BUS_ADDRESS", &dbus_env)
            .output();

        let mut child = crate::process_utils::command("secret-tool")
            .args(["store", "--label=gemini", "service", "gemini", "username", "antigravity"])
            .env("DBUS_SESSION_BUS_ADDRESS", &dbus_env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn secret-tool: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(payload_json.as_bytes())
                .map_err(|e| format!("Failed to write to secret-tool: {}", e))?;
        }

        let output = child.wait_with_output()
            .map_err(|e| format!("Failed to wait for secret-tool: {}", e))?;

        if !output.status.success() {
            return Err("secret-tool execution returned non-zero status".to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let _ = crate::process_utils::command("cmdkey.exe")
            .args(["/delete:gemini:antigravity"])
            .output();
        let _ = crate::process_utils::command("cmdkey.exe")
            .args(["/generic:gemini:antigravity", "/user:antigravity", &format!("/pass:{}", payload_json)])
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = crate::process_utils::command("security")
            .args(["delete-generic-password", "-s", "gemini", "-a", "antigravity"])
            .output();
        let _ = crate::process_utils::command("security")
            .args(["add-generic-password", "-s", "gemini", "-a", "antigravity", "-w", &payload_json, "-U"])
            .output();
    }

    Ok(())
}

/// Format timestamp as RFC3339 string
fn format_timestamp_rfc3339(timestamp: i64) -> String {
    let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64);
    let duration = dt.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let total_secs = duration.as_secs();
    let days = total_secs / 86400;
    let rem_secs = total_secs % 86400;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;
    let secs = rem_secs % 60;

    let mut year = 1970;
    let mut d = days;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        year += 1;
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for &dim in &days_in_months {
        if d < dim {
            break;
        }
        d -= dim;
        month += 1;
    }
    let day = d + 1;

    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hours, mins, secs)
}

/// Clear proxy settings from settings.json and remove active sessions
pub fn clear_proxy_settings(ide_type: &str) -> Result<(), String> {
    if let Ok(settings_path) = get_ide_settings_path(ide_type) {
        if settings_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&settings_path) {
                if let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = settings.as_object_mut() {
                        obj.remove("antigravity.proxyBaseUrl");
                        obj.remove("http.proxy");
                        obj.remove("http.proxyStrictSSL");
                    }
                    if let Ok(new_content) = serde_json::to_string_pretty(&settings) {
                        let _ = std::fs::write(&settings_path, new_content);
                    }
                }
            }
        }
    }

    if let Ok(conn) = init_db() {
        let _ = conn.execute("DELETE FROM sessions WHERE ide_type = ?", rusqlite::params![ide_type]);
    }

    eprintln!("[Client] Proxy configuration cleared for {}", ide_type);
    Ok(())
}

/// Clear credentials from system keyring
pub fn clear_keyring_credentials() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let dbus_env = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_else(|_| {
            let uid = std::env::var("UID").unwrap_or_else(|_| "1000".to_string());
            format!("unix:path=/run/user/{}/bus", uid)
        });

        let _ = crate::process_utils::command("secret-tool")
            .args(["clear", "service", "gemini", "username", "antigravity"])
            .env("DBUS_SESSION_BUS_ADDRESS", &dbus_env)
            .output();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = crate::process_utils::command("cmdkey.exe")
            .args(["/delete:gemini:antigravity"])
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = crate::process_utils::command("security")
            .args(["delete-generic-password", "-s", "gemini", "-a", "antigravity"])
            .output();
    }

    Ok(())
}
