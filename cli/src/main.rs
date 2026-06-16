use clap::Parser;
use bark_core::Bark;

mod cli;
use cli::{Cli, Commands};

mod sync;
mod inbox;

use directories::{BaseDirs, ProjectDirs};
use std::path::{Path, PathBuf};

fn get_data_dir() -> PathBuf {
    let proj_dirs = ProjectDirs::from("", "", "bark")
        .expect("Could not determine data directory");
    let data_dir = proj_dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&data_dir).unwrap();
    data_dir
}

fn handle_native_host(inbox_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Write};

    let mut len_buf = [0u8; 4];
    std::io::stdin().read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut msg_buf = vec![0u8; len];
    std::io::stdin().read_exact(&mut msg_buf)?;

    let response = match serde_json::from_slice::<serde_json::Value>(&msg_buf) {
        Ok(msg) => {
            let action = msg.get("action").and_then(|v| v.as_str()).unwrap_or("");
            match action {
                "preview" => {
                    let bibtex = msg.get("bibtex").and_then(|v| v.as_str());
                    let pdf_url = msg.get("pdf_url").and_then(|v| v.as_str());
                    match bibtex {
                        Some(b) => match inbox::make_toml(b, pdf_url) {
                            Ok(toml) => serde_json::json!({"toml": toml}),
                            Err(e) => serde_json::json!({"error": e.to_string()}),
                        },
                        None => serde_json::json!({"error": "missing bibtex field"}),
                    }
                }
                "import" => {
                    match msg.get("toml").and_then(|v| v.as_str()) {
                        Some(toml) => match inbox::write_raw_toml_to_inbox(inbox_dir, toml) {
                            Ok(_) => serde_json::json!({"success": true}),
                            Err(e) => serde_json::json!({"success": false, "error": e.to_string()}),
                        },
                        None => serde_json::json!({"success": false, "error": "missing toml field"}),
                    }
                }
                _ => serde_json::json!({"success": false, "error": format!("unknown action: {:?}", action)}),
            }
        }
        Err(e) => serde_json::json!({"success": false, "error": e.to_string()}),
    };

    let response_bytes = serde_json::to_vec(&response)?;
    let response_len = response_bytes.len() as u32;
    std::io::stdout().write_all(&response_len.to_le_bytes())?;
    std::io::stdout().write_all(&response_bytes)?;
    std::io::stdout().flush()?;

    Ok(())
}

fn handle_install_native_host(
    extension_id: &str,
    data_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let bark_path = std::env::current_exe()?;
    let bark_path_str = bark_path.to_str().ok_or("non-UTF8 executable path")?;

    // Chrome launches the native host binary with no arguments, so we need a
    // wrapper script that invokes `bark native-host`.
    let wrapper_path = data_dir.join("native-host");
    std::fs::write(&wrapper_path, format!("#!/bin/sh\nexec {} native-host\n", bark_path_str))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755))?;
    }

    let manifest = serde_json::json!({
        "name": "com.bark.host",
        "description": "Bark reference manager native host",
        "path": wrapper_path.to_str().ok_or("non-UTF8 wrapper path")?,
        "type": "stdio",
        "allowed_origins": [
            format!("chrome-extension://{}/", extension_id)
        ]
    });

    let manifest_json = serde_json::to_string_pretty(&manifest)?;

    let base_dirs = BaseDirs::new().ok_or("could not determine base directories")?;
    let config_dir = base_dirs.config_dir();
    let mut installed = false;

    for browser in &["google-chrome", "chromium"] {
        let browser_dir = config_dir.join(browser);
        if browser_dir.exists() {
            let host_dir = browser_dir.join("NativeMessagingHosts");
            std::fs::create_dir_all(&host_dir)?;
            let manifest_path = host_dir.join("com.bark.host.json");
            std::fs::write(&manifest_path, &manifest_json)?;
            println!("Installed: {}", manifest_path.display());
            installed = true;
        }
    }

    if !installed {
        eprintln!("No Chrome or Chromium config directory found under {}", config_dir.display());
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let data_dir = get_data_dir();
    let inbox_dir = data_dir.join("inbox");

    match &cli.command {
        Commands::NativeHost => return handle_native_host(&inbox_dir),
        Commands::InstallNativeHost { extension_id } => {
            return handle_install_native_host(extension_id, &data_dir);
        }
        _ => {}
    }

    let db_path = data_dir.join("library.db");
    let bark = Bark::new(db_path.to_str().unwrap())?;
    cli.run(&bark, &inbox_dir)?;

    Ok(())
}
