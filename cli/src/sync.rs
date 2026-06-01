use std::collections::HashSet;
use std::process::Command;
use std::path::PathBuf;
use std::env;

use bark_core::{service, db, Bark};

fn get_sync_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::var("BARK_SYNC_DIR")?;
    Ok(PathBuf::from(dir))
}

fn refs_dir(sync_dir: &PathBuf) -> PathBuf {
    sync_dir.join("refs")
}

fn git(sync_dir: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C").arg(sync_dir)
        .args(args)
        .output()
        .expect("git command failed")
}

fn remote_commits_ahead(sync_dir: &PathBuf) -> Vec<String> {
    let out = git(sync_dir, &["log", "HEAD..@{u}", "--oneline"]);
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

pub fn restore(bark: &Bark) -> Result<(), Box<dyn std::error::Error>> {
    db::purge(bark.conn())?;

    let dir = get_sync_dir()?;

    Command::new("git")
        .arg("-C").arg(&dir)
        .arg("pull")
        .status()?;

    let refs_dir = refs_dir(&dir);

    if refs_dir.exists() {
        for entry in std::fs::read_dir(&refs_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let content = std::fs::read_to_string(&path)?;
                service::import_ref_toml(bark.conn(), &content)?;
            }
        }
    }

    println!("Sync restore complete");
    Ok(())
}

pub fn status(bark: &Bark) -> Result<(), Box<dyn std::error::Error>> {
    let dir = get_sync_dir()?;

    git(&dir, &["fetch", "--quiet"]);

    // Export local DB refs to a temp dir
    let tmp_dir = env::temp_dir().join("bark_status_local");
    std::fs::create_dir_all(&tmp_dir)?;
    for entry in std::fs::read_dir(&tmp_dir)? {
        std::fs::remove_file(entry?.path()).ok();
    }

    let all_refs = service::list_references_and_data(bark.conn(), None)?;
    for r in &all_refs {
        let content = service::export_ref_toml(bark.conn(), &r.id)?;
        std::fs::write(tmp_dir.join(format!("{}.toml", r.id)), content)?;
    }

    let refs_dir = refs_dir(&dir);

    if !refs_dir.exists() {
        if all_refs.is_empty() {
            println!("In sync (no refs on either side)");
        } else {
            println!("Remote has no refs/; local has {} reference(s) to push", all_refs.len());
        }
        std::fs::remove_dir_all(&tmp_dir).ok();
        return Ok(());
    }

    let diff = Command::new("diff")
        .args(["--recursive", "--unified=2"])
        .arg(&refs_dir)
        .arg(&tmp_dir)
        .output()?;

    if diff.stdout.is_empty() {
        println!("Local db is in sync with synced refs");
    } else {
        println!("Local db differs from synced refs:\n");
        print!("{}", String::from_utf8_lossy(&diff.stdout));
    }

    std::fs::remove_dir_all(&tmp_dir).ok();

    // Notes
    let notes_in_repo = dir.join("notes");
    if notes_in_repo.exists() {
        let notes_status = git(&dir, &["status", "--short", "notes/"]);
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

    // Remote ahead
    let ahead = remote_commits_ahead(&dir);
    if !ahead.is_empty() {
        println!("\nRemote has {} unpulled commit(s):", ahead.len());
        for line in &ahead {
            println!("  {}", line);
        }
    }

    Ok(())
}

pub fn push(bark: &Bark) -> Result<(), Box<dyn std::error::Error>> {
    let dir = get_sync_dir()?;

    git(&dir, &["fetch", "--quiet"]);

    let ahead = remote_commits_ahead(&dir);
    if !ahead.is_empty() {
        eprintln!(
            "Aborting: remote has {} unpulled commit(s). Run `bark sync restore` first.",
            ahead.len()
        );
        std::process::exit(1);
    }

    let refs_dir = refs_dir(&dir);
    std::fs::create_dir_all(&refs_dir)?;

    let all_refs = service::list_references_and_data(bark.conn(), None)?;
    let current_ids: HashSet<String> = all_refs.iter().map(|r| r.id.clone()).collect();

    for r in &all_refs {
        let content = service::export_ref_toml(bark.conn(), &r.id)?;
        std::fs::write(refs_dir.join(format!("{}.toml", r.id)), content)?;
    }

    // Remove files for refs that no longer exist in the DB
    for entry in std::fs::read_dir(&refs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if !current_ids.contains(stem) {
                    std::fs::remove_file(&path)?;
                }
            }
        }
    }

    Command::new("git")
        .arg("-C").arg(&dir)
        .args(["add", "refs/"])
        .status()?;

    let notes_in_repo = dir.join("notes");
    if notes_in_repo.exists() {
        Command::new("git")
            .arg("-C").arg(&dir)
            .args(["add", "notes/"])
            .status()?;
    }

    let staged = Command::new("git")
        .arg("-C").arg(&dir)
        .args(["diff", "--cached", "--quiet"])
        .status()?;

    if staged.success() {
        println!("Nothing to push: no changes staged");
        return Ok(());
    }

    Command::new("git")
        .arg("-C").arg(&dir)
        .args(["commit", "-m", "bark sync"])
        .status()?;

    Command::new("git")
        .arg("-C").arg(&dir)
        .arg("push")
        .status()?;

    println!("Sync push complete");
    Ok(())
}
