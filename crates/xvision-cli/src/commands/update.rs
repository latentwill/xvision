//! `xvn update` — self-update from GitHub Releases.
//!
//! Checks the latest release, downloads the platform-appropriate binary,
//! verifies SHA256, and replaces the running binary in-place.
//! Use `--check` to only report availability.

use clap::Args;

/// Platform identifier in GitHub Release asset names.
fn platform_artifact() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "xvn-aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "xvn-x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "xvn-x86_64-linux-musl"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "xvn-x86_64-windows-msvc"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "xvn-unsupported"
    }
}

#[derive(Args, Debug, Clone)]
pub struct UpdateCmd {
    /// Only check whether an update is available (exit 0 if current, 1 if update available).
    #[arg(long)]
    pub check: bool,
    /// Install a specific version (e.g., "v0.36.0").
    #[arg(long)]
    pub version: Option<String>,
}

pub async fn run(cmd: UpdateCmd) -> anyhow::Result<()> {
    let artifact = platform_artifact();
    if artifact == "xvn-unsupported" {
        anyhow::bail!(
            "xvn update is not supported on this platform/arch. Build from source: \
             https://github.com/latentwill/xvision"
        );
    }

    let current_version = env!("CARGO_PKG_VERSION");
    let current = format!("v{current_version}");

    let target_tag = if let Some(tag) = &cmd.version {
        tag.clone()
    } else {
        let tag = fetch_latest_tag().await?;
        if tag == current {
            println!("Already up to date ({current})");
            if cmd.check {
                std::process::exit(0);
            }
            return Ok(());
        }
        tag
    };

    if cmd.check {
        println!("Update available: {target_tag} (current: {current})");
        std::process::exit(1);
    }

    println!("Updating from {current} to {target_tag}...");

    let ext = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    };
    let download_url =
        format!("https://github.com/latentwill/xvision/releases/download/{target_tag}/{artifact}.{ext}");
    let checksum_url = format!("{download_url}.sha256");

    let tmp = tempfile::tempdir()?;
    let archive_path = tmp.path().join(format!("xvn.{ext}"));
    let binary_name = if cfg!(target_os = "windows") {
        "xvn.exe"
    } else {
        "xvn"
    };
    let extracted = tmp.path().join(binary_name);

    // Download
    let body = reqwest::get(&download_url).await?.bytes().await?;
    tokio::fs::write(&archive_path, &body).await?;

    // Verify SHA256
    {
        let expected_sha = reqwest::get(&checksum_url).await?.text().await?;
        let expected = expected_sha.split_whitespace().next().unwrap_or("");
        let actual = sha256_of_file(&archive_path)?;
        if expected.is_empty() || expected != actual {
            anyhow::bail!("SHA256 mismatch!\n  expected: {expected}\n  got:      {actual}");
        }
        println!("SHA256 verified");
    }

    // Extract
    extract_binary(&archive_path, &ext, tmp.path(), &extracted)?;

    // Self-replace
    let current_exe = std::env::current_exe()?;
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::copy(&extracted, &current_exe)?;
        println!("Updated to {target_tag}. Restart to use the new binary.");
    }
    #[cfg(target_os = "windows")]
    {
        let backup = current_exe.with_extension("exe.old");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&current_exe, &backup)?;
        std::fs::copy(&extracted, &current_exe)?;
        println!(
            "Updated to {target_tag}. Old binary backed up to {}",
            backup.display()
        );
    }
    Ok(())
}

async fn fetch_latest_tag() -> anyhow::Result<String> {
    let resp: serde_json::Value = reqwest::Client::new()
        .get("https://api.github.com/repos/latentwill/xvision/releases/latest")
        .header("User-Agent", "xvn-update")
        .send()
        .await?
        .json()
        .await?;
    let tag = resp["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("could not parse tag_name from GitHub releases API"))?;
    Ok(tag.to_string())
}

fn sha256_of_file(path: &std::path::Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_binary(
    archive: &std::path::Path,
    ext: &str,
    dest: &std::path::Path,
    _binary_name: &std::path::Path,
) -> anyhow::Result<()> {
    if ext == "zip" {
        let status = std::process::Command::new("unzip")
            .arg("-o")
            .arg(archive)
            .arg("-d")
            .arg(dest)
            .status()
            .map_err(|e| anyhow::anyhow!("unzip not found: {e}. Install unzip or extract manually."))?;
        if !status.success() {
            anyhow::bail!("unzip failed with exit code {}", status);
        }
    } else {
        let status = std::process::Command::new("tar")
            .arg("xzf")
            .arg(archive)
            .arg("-C")
            .arg(dest)
            .status()
            .map_err(|e| anyhow::anyhow!("tar not found: {e}. Install tar or extract manually."))?;
        if !status.success() {
            anyhow::bail!("tar failed with exit code {}", status);
        }
    }
    Ok(())
}
