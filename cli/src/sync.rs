use std::process::Command;
use std::path::PathBuf;
use std::env;

use bark_core::{service, db, Bark};

fn get_sync_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::var("BARK_SYNC_DIR")?;
    Ok(PathBuf::from(dir))
}

pub fn restore(bark: &Bark) -> Result<(), Box<dyn std::error::Error>> {
    // Delete everything first
    db::purge(bark.conn());

    let dir = get_sync_dir()?;

    Command::new("git")
        .arg("-C")
        .arg(&dir)
        .arg("pull")
        .status()?;

    let toml_filename = dir.join("bark.toml");
    let toml_content = std::fs::read_to_string(&toml_filename)?;

    service::import_toml(
        bark.conn(),
        &toml_content
    )?;

    println!("Sync restore complete");

    Ok(())
}

pub fn status(bark: &Bark) -> Result<(), Box<dyn std::error::Error>> {
    let dir = get_sync_dir()?;

    // Fetch so we can detect if remote is ahead
    Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["fetch", "--quiet"])
        .status()?;

    // Export local db to a temp file
    let tmp_path = env::temp_dir().join("bark_status_local.toml");
    let local_toml = service::export_toml_by_tag(bark.conn(), None)?;
    std::fs::write(&tmp_path, &local_toml)?;

    let remote_toml = dir.join("bark.toml");

    // Diff local db export vs last synced bark.toml
    let diff = Command::new("diff")
        .args(["--unified=2", "--label", "synced", "--label", "local"])
        .arg(&remote_toml)
        .arg(&tmp_path)
        .output()?;

    if diff.stdout.is_empty() {
        println!("Local db is in sync with {}", remote_toml.display());
    } else {
        println!("Local db differs from synced bark.toml:\n");
        print!("{}", String::from_utf8_lossy(&diff.stdout));
    }

    // Check if remote is ahead of the sync dir
    let ahead = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["log", "HEAD..@{u}", "--oneline"])
        .output()?;

    let ahead_str = String::from_utf8_lossy(&ahead.stdout);
    let ahead_lines: Vec<&str> = ahead_str.lines().collect();
    if !ahead_lines.is_empty() {
        println!("\nRemote has {} unpulled commit(s):", ahead_lines.len());
        for line in &ahead_lines {
            println!("  {}", line);
        }
    }

    std::fs::remove_file(&tmp_path).ok();
    Ok(())
}

pub fn push(bark: &Bark) -> Result<(), Box<dyn std::error::Error>> {
    let dir = get_sync_dir()?;

    let toml_content = service::export_toml_by_tag(bark.conn(), None)?;

    std::fs::write(dir.join("bark.toml"), toml_content)?;

    Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["add", "bark.toml"])
        .status()?;

    Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["commit", "-m", "bark sync"])
        .status()?;

    Command::new("git")
        .arg("-C")
        .arg(&dir)
        .arg("push")
        .status()?;

    println!("Sync push complete");

    Ok(())
}
