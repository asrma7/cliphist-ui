use crate::{app::AppEvent, cliphist, config::ImageConfig, model::ClipboardItem, thumbnails};
use anyhow::{anyhow, Context, Result};
use async_channel::Sender;
use std::{path::PathBuf, thread};
use tracing::warn;

pub fn reload(
    sender: Sender<AppEvent>,
    generation: u64,
    max_preview_chars: usize,
    image_config: ImageConfig,
    cache_dir: PathBuf,
) {
    thread::spawn(move || {
        let result = cliphist::list_entries(max_preview_chars)
            .inspect(|items| {
                if let Err(err) = thumbnails::prune_cache(items, &image_config, &cache_dir) {
                    warn!(error = %err, "failed to prune thumbnail cache");
                }
            })
            .map_err(error_string);
        let _ = sender.send_blocking(AppEvent::HistoryLoaded { generation, result });
    });
}

pub fn copy(sender: Sender<AppEvent>, item: ClipboardItem, max_preview_chars: usize) {
    thread::spawn(move || {
        let result = copy_with_stale_retry(&item, max_preview_chars).map_err(error_string);
        let _ = sender.send_blocking(AppEvent::Copied { result });
    });
}

pub fn delete(sender: Sender<AppEvent>, id: String, raw_line: String) {
    thread::spawn(move || {
        let result = cliphist::delete_entry(&raw_line).map_err(error_string);
        let _ = sender.send_blocking(AppEvent::Deleted { id, result });
    });
}

pub fn clear_all(sender: Sender<AppEvent>) {
    thread::spawn(move || {
        let result = cliphist::wipe_history().map_err(error_string).map(|_| {
            if let Err(err) = cliphist::clear_clipboard_if_available() {
                warn!(error = %err, "failed to clear current Wayland clipboard after wipe");
            }
        });

        let _ = sender.send_blocking(AppEvent::Cleared { result });
    });
}

fn error_string(err: anyhow::Error) -> String {
    format!("{err:#}")
}

fn copy_with_stale_retry(item: &ClipboardItem, max_preview_chars: usize) -> Result<()> {
    match cliphist::copy_to_clipboard(item) {
        Ok(()) => Ok(()),
        Err(first_err) => {
            let first_error = format!("{first_err:#}");
            let entries = cliphist::list_entries(max_preview_chars).with_context(|| {
                format!("copy failed and failed to refresh history; original error: {first_error}")
            })?;

            let Some(fresh_item) = entries
                .into_iter()
                .find(|candidate| candidate.kind == item.kind && candidate.preview == item.preview)
            else {
                return Err(anyhow!(
                    "copy failed: {first_error}; refreshed history did not contain a matching entry"
                ));
            };

            cliphist::copy_to_clipboard(&fresh_item).with_context(|| {
                format!("copy failed after refreshing stale history entry; original error: {first_error}")
            })
        }
    }
}
