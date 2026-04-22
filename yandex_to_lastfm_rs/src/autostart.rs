use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "YaMusicLastFmPopup";

fn current_exe_quoted() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe error: {e}"))?;
    Ok(format!("\"{}\"", exe.to_string_lossy()))
}

pub fn enable_autostart() -> Result<(), String> {
    let exe = current_exe_quoted()?;

    let status = Command::new("reg")
        .args([
            "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &exe, "/f",
        ])
        .status()
        .map_err(|e| format!("failed to run reg add: {e}"))?;

    if !status.success() {
        return Err(format!("reg add failed with status: {status}"));
    }

    Ok(())
}

pub fn disable_autostart() -> Result<(), String> {
    let output = Command::new("reg")
        .args(["query", RUN_KEY, "/v", VALUE_NAME])
        .output()
        .map_err(|e| format!("failed to run reg query: {e}"))?;

    if !output.status.success() {
        return Ok(());
    }

    let status = Command::new("reg")
        .args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
        .status()
        .map_err(|e| format!("failed to run reg delete: {e}"))?;

    if !status.success() {
        return Err(format!("reg delete failed with status: {status}"));
    }

    Ok(())
}

pub fn sync_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        enable_autostart()
    } else {
        disable_autostart()
    }
}
