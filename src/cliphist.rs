use crate::model::{parse_list, ClipboardItem};
use anyhow::{anyhow, Context, Result};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub fn list_entries(max_preview_chars: usize) -> Result<Vec<ClipboardItem>> {
    let output = Command::new("cliphist")
        .arg("list")
        .output()
        .context("failed to run `cliphist list`; is cliphist installed?")?;

    if !output.status.success() {
        return Err(anyhow!(
            "cliphist list failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_list(&stdout, max_preview_chars))
}

pub fn decode_entry(raw_line: &str) -> Result<Vec<u8>> {
    let mut child = Command::new("cliphist")
        .arg("decode")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `cliphist decode`; is cliphist installed?")?;

    write_stdin_line(&mut child, raw_line)?;
    let output = child
        .wait_with_output()
        .context("failed to wait for `cliphist decode`")?;

    if !output.status.success() {
        return Err(anyhow!(
            "cliphist decode failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(output.stdout)
}

pub fn copy_to_clipboard(item: &ClipboardItem) -> Result<()> {
    let decoded = decode_entry(&item.raw_line)?;
    wl_copy(&decoded, item.mime_type())
}

pub fn delete_entry(raw_line: &str) -> Result<()> {
    let mut child = Command::new("cliphist")
        .arg("delete")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `cliphist delete`; is cliphist installed?")?;

    write_stdin_line(&mut child, raw_line)?;
    let output = child
        .wait_with_output()
        .context("failed to wait for `cliphist delete`")?;

    if !output.status.success() {
        return Err(anyhow!(
            "cliphist delete failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

pub fn wipe_history() -> Result<()> {
    let output = Command::new("cliphist")
        .arg("wipe")
        .output()
        .context("failed to run `cliphist wipe`; is cliphist installed?")?;

    if !output.status.success() {
        return Err(anyhow!(
            "cliphist wipe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

pub fn clear_clipboard_if_available() -> Result<()> {
    let output = Command::new("wl-copy")
        .arg("--clear")
        .output()
        .context("failed to run `wl-copy --clear`; is wl-copy installed?")?;

    if !output.status.success() {
        return Err(anyhow!(
            "wl-copy --clear failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

fn wl_copy(bytes: &[u8], mime_type: Option<&str>) -> Result<()> {
    let mut command = Command::new("wl-copy");
    if let Some(mime_type) = mime_type {
        command.arg("--type").arg(mime_type);
    }

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `wl-copy`; is wl-copy installed?")?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(bytes)
            .context("failed to write decoded clipboard data to wl-copy")?;
    }

    let output = child
        .wait_with_output()
        .context("failed to wait for `wl-copy`")?;

    if !output.status.success() {
        return Err(anyhow!(
            "wl-copy failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

fn write_stdin_line(child: &mut std::process::Child, raw_line: &str) -> Result<()> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("failed to open child stdin"))?;
    stdin
        .write_all(raw_line.as_bytes())
        .context("failed to write cliphist entry to stdin")?;
    stdin
        .write_all(b"\n")
        .context("failed to finish cliphist entry input")?;
    drop(stdin);
    Ok(())
}
