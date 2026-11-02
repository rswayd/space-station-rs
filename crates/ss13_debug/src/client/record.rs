//! Screen recording from the overlay (native only): every frame goes through Bevy's `Screenshot`
//! to a writer thread that pipes raw RGBA into `ffmpeg` (`libx264`, `.mp4` in the export dir).
//! The video runs at a fixed [`FPS`]: a slower game repeats frames, so the video keeps real time.
//! Frames are dropped, never queued without bound, when ffmpeg falls behind.
// shortcut: needs `ffmpeg` in `PATH`; the browser build has no recorder (canvas `MediaRecorder`
// would be its own path), add it if a bug only shows there.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};

/// Frames per second of the video.
pub const FPS: f64 = 30.0;
/// Captured frames waiting for the writer; more are dropped.
const QUEUE: usize = 4;
/// Screenshots requested but not captured yet; more are not requested.
const IN_FLIGHT: u32 = 2;

struct Frame {
    rgba: Vec<u8>,
    size: UVec2,
    /// Seconds since the recording started.
    at: f64,
}

struct Active {
    tx: SyncSender<Frame>,
    path: PathBuf,
    started: Instant,
    last_request: Option<Instant>,
    in_flight: u32,
    dropped: u32,
    writer: std::thread::JoinHandle<Result<u64, String>>,
}

/// The recorder's state, read by the overlay's menu.
#[derive(Resource, Default)]
pub struct Recorder {
    active: Option<Active>,
    /// The last result, shown under the button.
    pub status: String,
}

impl Recorder {
    pub fn recording(&self) -> Option<f64> {
        self.active
            .as_ref()
            .map(|a| a.started.elapsed().as_secs_f64())
    }

    pub fn start(&mut self, dir: PathBuf) {
        if self.active.is_some() {
            return;
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let path = dir.join(format!("ss13-recording-{stamp}.mp4"));
        let (tx, rx) = sync_channel(QUEUE);
        let out = path.clone();
        let writer = std::thread::spawn(move || write_video(rx, out));
        info!("recording to {}", path.display());
        self.status = format!("Запись: {}", path.display());
        self.active = Some(Active {
            tx,
            path,
            started: Instant::now(),
            last_request: None,
            in_flight: 0,
            dropped: 0,
            writer,
        });
    }

    /// Ends the recording; the writer finishes the file in the background.
    pub fn stop(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        drop(active.tx);
        let path = active.path;
        let dropped = active.dropped;
        // Finishing the encode can take a moment: wait off the frame.
        std::thread::spawn(move || match active.writer.join() {
            Ok(Ok(frames)) => info!(
                "recording saved: {} ({frames} frames, {dropped} captures dropped)",
                path.display()
            ),
            Ok(Err(error)) => error!("recording {} failed: {error}", path.display()),
            Err(_) => error!("recording {}: writer panicked", path.display()),
        });
        self.status = "Сохранено (путь в логе клиента)".into();
    }
}

fn spawn_ffmpeg(size: UVec2, path: &PathBuf) -> Result<(Child, ChildStdin), String> {
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "rawvideo", "-pixel_format", "rgba"])
        .args(["-video_size", &format!("{}x{}", size.x, size.y)])
        .args(["-framerate", &FPS.to_string(), "-i", "-"])
        // yuv420p needs even sides.
        .args(["-vf", "crop=trunc(iw/2)*2:trunc(ih/2)*2"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "20"])
        .args(["-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start ffmpeg: {e}"))?;
    let stdin = child.stdin.take().ok_or("ffmpeg has no stdin")?;
    Ok((child, stdin))
}

/// The writer thread: frames in, a video out. Returns the number of video frames written.
fn write_video(rx: Receiver<Frame>, path: PathBuf) -> Result<u64, String> {
    let mut encoder: Option<(Child, ChildStdin, UVec2)> = None;
    let mut written: u64 = 0;
    let mut resized = false;
    for frame in rx {
        let (_, stdin, size) = match &mut encoder {
            Some(e) => e,
            None => {
                let (child, stdin) = spawn_ffmpeg(frame.size, &path)?;
                encoder.insert((child, stdin, frame.size))
            }
        };
        if frame.size != *size {
            if !resized {
                warn!("recording: the window was resized, frames of the new size are skipped");
                resized = true;
            }
            continue;
        }
        // Frames that should exist by now at the fixed rate (the first one at 0 s).
        let due = (frame.at * FPS).floor() as u64 + 1;
        while written < due {
            stdin
                .write_all(&frame.rgba)
                .map_err(|e| format!("ffmpeg stopped taking frames: {e}"))?;
            written += 1;
        }
    }
    if let Some((mut child, stdin, _)) = encoder {
        drop(stdin);
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("ffmpeg exited with {status}"));
        }
    }
    Ok(written)
}

/// Requests a screenshot per video frame while recording.
pub(super) fn capture(mut recorder: ResMut<Recorder>, mut commands: Commands) {
    let Some(active) = recorder.active.as_mut() else {
        return;
    };
    let now = Instant::now();
    let due = active
        .last_request
        .is_none_or(|last| (now - last).as_secs_f64() >= 1.0 / FPS);
    if !due || active.in_flight >= IN_FLIGHT {
        return;
    }
    active.last_request = Some(now);
    active.in_flight += 1;
    let at = (now - active.started).as_secs_f64();
    commands.spawn(Screenshot::primary_window()).observe(
        move |shot: On<ScreenshotCaptured>, mut recorder: ResMut<Recorder>| {
            let Some(active) = recorder.active.as_mut() else {
                return;
            };
            active.in_flight = active.in_flight.saturating_sub(1);
            let Ok(image) = shot.image.clone().try_into_dynamic() else {
                active.dropped += 1;
                return;
            };
            let rgba = image.to_rgba8();
            let frame = Frame {
                size: UVec2::new(rgba.width(), rgba.height()),
                rgba: rgba.into_raw(),
                at,
            };
            match active.tx.try_send(frame) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => active.dropped += 1,
                Err(TrySendError::Disconnected(_)) => {
                    // The writer gave up (no ffmpeg): stop and say why in the log.
                    recorder.stop();
                    recorder.status = "Ошибка записи (см. лог клиента)".into();
                }
            }
        },
    );
}

/// The menu row: one button that starts and stops.
pub(super) fn button(ui: &mut bevy_egui::egui::Ui, recorder: &mut Recorder, dir: PathBuf) {
    ui.horizontal(|ui| match recorder.recording() {
        Some(secs) => {
            let label = format!("⏹ Стоп ({:02}:{:02})", secs as u64 / 60, secs as u64 % 60);
            if ui.button(label).clicked() {
                recorder.stop();
            }
        }
        None => {
            if ui.button("⏺ Записать видео").clicked() {
                recorder.start(dir);
            }
        }
    });
    if !recorder.status.is_empty() {
        ui.small(&recorder.status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gaps between captures are filled, so 0.2 s of game is 0.2 s of video.
    #[test]
    fn writer_keeps_real_time() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("no ffmpeg, skipped");
            return;
        }
        let path = std::env::temp_dir().join(format!("ss13-rec-test-{}.mp4", std::process::id()));
        let (tx, rx) = sync_channel(QUEUE);
        let out = path.clone();
        let writer = std::thread::spawn(move || write_video(rx, out));
        let size = UVec2::new(6, 4);
        for (i, at) in [0.0, 0.1, 0.2].into_iter().enumerate() {
            let rgba = vec![(i * 100) as u8; (size.x * size.y * 4) as usize];
            tx.send(Frame { rgba, size, at }).unwrap();
        }
        // A frame of another size (a resized window) is skipped.
        tx.send(Frame {
            rgba: vec![0; 8 * 8 * 4],
            size: UVec2::splat(8),
            at: 0.3,
        })
        .unwrap();
        drop(tx);
        assert_eq!(writer.join().unwrap(), Ok(7));
        assert!(std::fs::metadata(&path).is_ok_and(|m| m.len() > 0));
        let _ = std::fs::remove_file(path);
    }
}
