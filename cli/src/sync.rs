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

    // Check for uncommitted notes changes via git (only if notes live in the sync repo)
    let notes_in_repo = dir.join("notes");
    if notes_in_repo.exists() {
        let notes_status = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["status", "--short", "notes/"])
            .output()?;

        let notes_str = String::from_utf8_lossy(&notes_status.stdout);
        let notes_lines: Vec<&str> = notes_str.lines().filter(|l| !l.is_empty()).collect();
        if notes_lines.is_empty() {
            println!("Notes are in sync");
        } else {
            println!("\nNotes have uncommitted changes:");
            for line in &notes_lines {
                println!("  {}", line);
            }
        }
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

    // Fetch so we can detect if remote is ahead
    Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["fetch", "--quiet"])
        .status()?;

    let ahead = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["log", "HEAD..@{u}", "--oneline"])
        .output()?;

    let ahead_str = String::from_utf8_lossy(&ahead.stdout);
    let ahead_lines: Vec<&str> = ahead_str.lines().filter(|l| !l.is_empty()).collect();
    if !ahead_lines.is_empty() {
        eprintln!(
            "Aborting: remote has {} unpulled commit(s). Run `bark sync restore` first.",
            ahead_lines.len()
        );
        std::process::exit(1);
    }

    let toml_content = service::export_toml_by_tag(bark.conn(), None)?;
    std::fs::write(dir.join("bark.toml"), toml_content)?;

    Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["add", "bark.toml"])
        .status()?;

    // Stage notes if the directory exists inside the sync repo
    let notes_in_repo = dir.join("notes");
    if notes_in_repo.exists() {
        Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["add", "notes/"])
            .status()?;
    }

    // Check if anything is actually staged before committing
    let staged = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["diff", "--cached", "--quiet"])
        .status()?;

    if staged.success() {
        println!("Nothing to push: no changes staged");
        return Ok(());
    }

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
