use crate::{
    app::AppEvent,
    cliphist,
    config::ImageConfig,
    model::{ClipboardItem, ClipboardKind},
};
use anyhow::{Context, Result};
use async_channel::{Receiver, Sender};
use image::{imageops::FilterType, RgbaImage};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    thread,
};
use tracing::{debug, warn};

#[derive(Clone)]
pub struct Thumbnailer {
    sender: Sender<ThumbnailJob>,
}

#[derive(Clone, Debug)]
struct ThumbnailJob {
    id: String,
    raw_line: String,
}

impl Thumbnailer {
    pub fn new(config: ImageConfig, cache_dir: PathBuf, event_sender: Sender<AppEvent>) -> Self {
        let (sender, receiver) = async_channel::bounded::<ThumbnailJob>(128);
        let workers = config.concurrent_jobs.clamp(1, 8);

        for worker_id in 0..workers {
            let receiver = receiver.clone();
            let event_sender = event_sender.clone();
            let config = config.clone();
            let cache_dir = cache_dir.clone();

            thread::spawn(move || {
                worker_loop(worker_id, receiver, event_sender, config, cache_dir);
            });
        }

        Self { sender }
    }

    pub fn request(&self, item: &ClipboardItem) -> bool {
        self.sender
            .try_send(ThumbnailJob {
                id: item.id.clone(),
                raw_line: item.raw_line.clone(),
            })
            .is_ok()
    }
}

fn worker_loop(
    worker_id: usize,
    receiver: Receiver<ThumbnailJob>,
    event_sender: Sender<AppEvent>,
    config: ImageConfig,
    cache_dir: PathBuf,
) {
    while let Ok(job) = receiver.recv_blocking() {
        debug!(worker_id, id = %job.id, "thumbnail job started");
        match thumbnail_path(&job, &config, &cache_dir) {
            Ok(path) => {
                let _ = event_sender.send_blocking(AppEvent::ThumbnailReady { id: job.id, path });
            }
            Err(err) => {
                warn!(worker_id, id = %job.id, error = %err, "thumbnail job failed");
                let _ = event_sender.send_blocking(AppEvent::ThumbnailFailed {
                    id: job.id,
                    error: format!("{err:#}"),
                });
            }
        }
    }
}

fn thumbnail_path(
    job: &ThumbnailJob,
    config: &ImageConfig,
    cache_dir: &PathBuf,
) -> Result<PathBuf> {
    fs::create_dir_all(cache_dir).with_context(|| {
        format!(
            "failed to create thumbnail cache directory {}",
            cache_dir.display()
        )
    })?;

    let path = cache_dir.join(cache_filename(&job.id, config));
    if path.exists() {
        return Ok(path);
    }

    let decoded = cliphist::decode_entry(&job.raw_line)?;
    let image = image::load_from_memory(&decoded).context("failed to decode clipboard image")?;
    drop(decoded);

    let width = config.width.max(1);
    let height = config.height.max(1);
    let thumbnail = if config.preserve_aspect_ratio {
        image.thumbnail(width, height)
    } else {
        image.resize_exact(width, height, FilterType::Lanczos3)
    };

    let temporary_path = path.with_extension("tmp.png");
    if config.rounded_corners {
        rounded_corners(thumbnail.to_rgba8())
            .save_with_format(&temporary_path, image::ImageFormat::Png)
            .context("failed to save rounded thumbnail")?;
    } else {
        thumbnail
            .save_with_format(&temporary_path, image::ImageFormat::Png)
            .context("failed to save thumbnail")?;
    }
    fs::rename(&temporary_path, &path).context("failed to move thumbnail into cache")?;

    Ok(path)
}

pub fn prune_cache(
    items: &[ClipboardItem],
    config: &ImageConfig,
    cache_dir: &Path,
) -> Result<usize> {
    let mut expected = HashSet::new();
    for item in items {
        if item.kind == ClipboardKind::Image {
            expected.insert(cache_filename(&item.id, config));
        }
    }

    let entries = match fs::read_dir(cache_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to read thumbnail cache directory {}",
                    cache_dir.display()
                )
            });
        }
    };

    let mut removed = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warn!(error = %err, "failed to inspect thumbnail cache entry");
                continue;
            }
        };

        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("png") {
            continue;
        }

        let Some(file_name) = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };

        if expected.contains(&file_name) {
            continue;
        }

        match fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => {
                warn!(path = %path.display(), error = %err, "failed to remove stale thumbnail")
            }
        }
    }

    Ok(removed)
}

fn cache_filename(id: &str, config: &ImageConfig) -> String {
    format!(
        "{}-{}x{}-aspect{}-round{}.png",
        sanitize_id(id),
        config.width.max(1),
        config.height.max(1),
        config.preserve_aspect_ratio as u8,
        config.rounded_corners as u8
    )
}

fn rounded_corners(mut image: RgbaImage) -> RgbaImage {
    let width = image.width();
    let height = image.height();
    let radius = 8_u32.min(width / 2).min(height / 2);
    if radius == 0 {
        return image;
    }

    let radius_squared = (radius * radius) as i64;
    for y in 0..height {
        for x in 0..width {
            let corner_center_x = if x < radius {
                radius - 1
            } else if x >= width - radius {
                width - radius
            } else {
                continue;
            };
            let corner_center_y = if y < radius {
                radius - 1
            } else if y >= height - radius {
                height - radius
            } else {
                continue;
            };

            let dx = x as i64 - corner_center_x as i64;
            let dy = y as i64 - corner_center_y as i64;
            if dx * dx + dy * dy > radius_squared {
                image.get_pixel_mut(x, y).0[3] = 0;
            }
        }
    }

    image
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}
