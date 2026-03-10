use iced::widget::{
    button, checkbox, column, container, progress_bar, radio,
    row, rule, text, Row, Space,
};
use iced::{time, Alignment, Element, Length, Subscription, Task, Theme};
use lofty::{AudioFile, Probe};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::BufReader;
use tokio::process::Command;

pub fn main() -> iced::Result {
    iced::application(VideoMerger::new, VideoMerger::update, VideoMerger::view)
        .title("Video Merger")
        .theme(|_: &_| Theme::Dark)
        .subscription(VideoMerger::subscription)
        .run()
}

fn get_duration_string(path: &std::path::Path) -> String {
    // Try specialized mp4 extraction first for MP4/MOV files
    if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
        if ext == "mp4" || ext == "mov" || ext == "m4a" {
             if let Ok(file) = File::open(path) {
                let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                let reader = BufReader::new(file);
                if let Ok(mp4) = mp4::Mp4Reader::read_header(reader, size) {
                    let duration = mp4.duration();
                    let total_seconds = duration.as_secs();
                    let hours = total_seconds / 3600;
                    let minutes = (total_seconds % 3600) / 60;
                    let seconds = total_seconds % 60;
                    
                    if hours > 0 {
                        return format!("{}h {}m {}s", hours, minutes, seconds);
                    } else {
                        return format!("{}m {}s", minutes, seconds);
                    }
                }
             }
        }
    }

    // Fallback to lofty for everything else
    match Probe::open(path) {
        Ok(probe) => {
            match probe.read() {
                Ok(tagged_file) => {
                    let duration = tagged_file.properties().duration();
                    let total_seconds = duration.as_secs();
                    let hours = total_seconds / 3600;
                    let minutes = (total_seconds % 3600) / 60;
                    let seconds = total_seconds % 60;
                    
                    if hours > 0 {
                        format!("{}h {}m {}s", hours, minutes, seconds)
                    } else {
                        format!("{}m {}s", minutes, seconds)
                    }
                },
                Err(_) => "Unknown".to_string(),
            }
        },
        Err(_) => "Unknown".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoLength {
    OneHour,
    TwoHours,
    ThreeHours,
    FourHours,
    FiveHours,
    SixHours,
}

impl Default for VideoLength {
    fn default() -> Self {
        Self::OneHour
    }
}

impl VideoLength {
    fn as_seconds(&self) -> u64 {
        match self {
            Self::OneHour => 3600,
            Self::TwoHours => 7200,
            Self::ThreeHours => 10800,
            Self::FourHours => 14400,
            Self::FiveHours => 18000,
            Self::SixHours => 21600,
        }
    }
}

impl std::fmt::Display for VideoLength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OneHour => write!(f, "1 hour"),
            Self::TwoHours => write!(f, "2 hours"),
            Self::ThreeHours => write!(f, "3 hours"),
            Self::FourHours => write!(f, "4 hours"),
            Self::FiveHours => write!(f, "5 hours"),
            Self::SixHours => write!(f, "6 hours"),
        }
    }
}

struct VideoMerger {
    video_path: Option<PathBuf>,
    video_duration: String,
    disable_video_audio: bool,
    audio_path: Option<PathBuf>,
    audio_duration: String,
    video_length: VideoLength,
    merging: bool,
    progress: f32,
    output_path: Option<PathBuf>,
    error_message: Option<String>,
    success_message: Option<String>,
    merge_start_time: Option<Instant>,
}

#[derive(Debug, Clone)]
enum Message {
    SelectVideo,
    VideoSelected(Option<(PathBuf, String)>),
    ToggleDisableVideoAudio(bool),
    SelectAudio,
    AudioSelected(Option<(PathBuf, String)>),
    SetVideoLength(VideoLength),
    Merge,
    StartMerge(Option<PathBuf>),
    MergeFinished(Result<(PathBuf, String), String>),
    Tick,
    FileDropped(PathBuf),
    OpenOutputFolder,
}

impl Default for VideoMerger {
    fn default() -> Self {
        Self {
            video_path: None,
            video_duration: String::new(),
            disable_video_audio: true,
            audio_path: None,
            audio_duration: String::new(),
            video_length: VideoLength::default(),
            merging: false,
            progress: 0.0,
            output_path: None,
            error_message: None,
            success_message: None,
            merge_start_time: None,
        }
    }
}

fn get_ffmpeg_path() -> PathBuf {
    // 1. Check bin folder in current working directory (dev mode)
    let cwd_bin = std::env::current_dir().unwrap_or_default().join("bin").join("ffmpeg");
    if cwd_bin.exists() {
        return cwd_bin;
    }

    // 2. Check next to executable (release mode)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled_path = exe_dir.join("ffmpeg");
            if bundled_path.exists() {
                return bundled_path;
            }
            // Also check a "bin" subfolder next to exe
             let bundled_bin_path = exe_dir.join("bin").join("ffmpeg");
            if bundled_bin_path.exists() {
                return bundled_bin_path;
            }
        }
    }

    // 3. Fallback to system PATH
    PathBuf::from("ffmpeg")
}

async fn check_ffmpeg_installed() -> bool {
    let ffmpeg_path = get_ffmpeg_path();
    match Command::new(ffmpeg_path).arg("-version").output().await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn merge_process(
    video_path: PathBuf,
    audio_path: PathBuf,
    disable_video_audio: bool,
    target_seconds: u64,
    output_path: PathBuf,
) -> Result<(PathBuf, String), String> {
    if !check_ffmpeg_installed().await {
        return Err("FFmpeg is not installed or not in PATH. Please download FFmpeg and place it in the 'bin' folder.".to_string());
    }
    
    let ffmpeg_cmd = get_ffmpeg_path();

    // Temporary files
    let temp_dir = std::env::temp_dir();
    let temp_video = temp_dir.join("temp_processed_video.mp4");
    let temp_audio = temp_dir.join("temp_processed_audio.m4a");

    // 1. Parallel Processing: Loop Video & Audio concurrently
    let video_task: tokio::task::JoinHandle<std::io::Result<std::process::Output>> = {
        let video_path = video_path.clone();
        let temp_video = temp_video.clone();
        let ffmpeg_cmd = ffmpeg_cmd.clone();
        tokio::spawn(async move {
            let mut args = vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "error".to_string(),
                "-y".to_string(),
                "-hwaccel".to_string(),      // Enable hardware decoding
                "auto".to_string(),
                "-stream_loop".to_string(),
                "-1".to_string(),
                "-i".to_string(),
                video_path.to_string_lossy().to_string(),
                "-t".to_string(),
                target_seconds.to_string(),
                "-map".to_string(),
                "0:v".to_string(),
            ];

            // Platform-specific hardware acceleration for encoding
            #[cfg(target_os = "macos")]
            {
                args.extend_from_slice(&[
                    "-c:v".to_string(),
                    "h264_videotoolbox".to_string(), // Apple Silicon / Intel QuickSync
                    "-b:v".to_string(),
                    "6000k".to_string(),             // High bitrate for 1080p quality
                    "-allow_sw".to_string(),
                    "1".to_string(),                 // Allow software fallback if HW fails
                ]);
            }

            #[cfg(not(target_os = "macos"))]
            {
                args.extend_from_slice(&[
                    "-c:v".to_string(),
                    "libx264".to_string(),
                    "-preset".to_string(),
                    "veryfast".to_string(),          // Optimized for speed
                    "-tune".to_string(),
                    "film".to_string(),
                    "-crf".to_string(),
                    "18".to_string(),                // Maintain high quality
                ]);
            }

            args.extend_from_slice(&[
                "-threads".to_string(),
                "0".to_string(),
            ]);

            if disable_video_audio {
                // If disabling audio, we don't map audio stream
                args.push("-an".to_string());
            } else {
                // Keep audio if present (will be mixed later)
                // Note: We don't map audio here explicitly if we want to mix later, 
                // but we need it in the temp file if we want to mix.
                // However, mapping 0:a might fail if no audio stream exists.
                // Safer to just map 0:v here and handle audio in merge step if we have source.
                // Actually, if we want to mix, we need the audio stream in the temp file.
                // Let's assume input has audio if checkbox is unchecked (enabled).
                // We'll try to map 0:a?
                // ffmpeg will error if stream doesn't exist.
                // Let's rely on auto-selection for audio if enabled, but force video map.
                // If we want to guarantee audio presence, we might need to check probe results.
                // For simplicity, let's try mapping 0:a if enabled.
                 args.push("-map".to_string());
                 args.push("0:a?".to_string()); // ? makes it optional (ignore if not present)
                 args.push("-c:a".to_string());
                 args.push("aac".to_string());
            }

            args.push(temp_video.to_string_lossy().to_string());

            Command::new(ffmpeg_cmd)
                .args(&args)
                .output()
                .await
        })
    };

    let audio_task: tokio::task::JoinHandle<std::io::Result<std::process::Output>> = {
        let audio_path = audio_path.clone();
        let temp_audio = temp_audio.clone();
        let audio_path_str = audio_path.to_string_lossy().to_string();
        let temp_audio_str = temp_audio.to_string_lossy().to_string();
        let target_seconds_str = target_seconds.to_string();
        let ffmpeg_cmd = ffmpeg_cmd.clone();
        
        tokio::spawn(async move {
             let args = vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "error".to_string(),
                "-y".to_string(),
                "-stream_loop".to_string(),
                "-1".to_string(),
                "-i".to_string(),
                audio_path_str,
                "-t".to_string(),
                target_seconds_str,
                "-vn".to_string(),
                "-c:a".to_string(),
                "aac".to_string(),
                "-ac".to_string(),
                "2".to_string(),
                "-ar".to_string(),
                "44100".to_string(),
                "-f".to_string(),
                "mp4".to_string(),
                temp_audio_str,
            ];
            
            Command::new(ffmpeg_cmd)
                .args(&args)
                .output()
                .await
        })
    };

    // Wait for both processing tasks
    let (video_res, audio_res) = tokio::join!(video_task, audio_task);

    // Check results
    let video_out = video_res.map_err(|e| format!("Video processing task failed: {}", e))?
        .map_err(|e| format!("Video processing command failed: {}", e))?;
    
    if !video_out.status.success() {
        return Err(format!("Video processing error: {}", String::from_utf8_lossy(&video_out.stderr)));
    }

    let audio_out = audio_res.map_err(|e| format!("Audio processing task failed: {}", e))?
        .map_err(|e| format!("Audio processing command failed: {}", e))?;

    if !audio_out.status.success() {
         return Err(format!("Audio processing error: {}", String::from_utf8_lossy(&audio_out.stderr)));
    }

    // 2. Final Merge
    // Check if temp_video actually has audio stream (if we didn't disable it)
    let video_has_audio = if disable_video_audio {
        false
    } else {
        if let Ok(file) = File::open(&temp_video) {
            let size = file.metadata().map(|m| m.len()).unwrap_or(0);
            let reader = BufReader::new(file);
            match mp4::Mp4Reader::read_header(reader, size) {
                Ok(mp4) => {
                    mp4.tracks().values().any(|t| {
                        match t.track_type() {
                            Ok(tt) => tt == mp4::TrackType::Audio,
                            Err(_) => false,
                        }
                    })
                },
                Err(_) => false,
            }
        } else {
            false
        }
    };
    
    let mut merge_args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-y".to_string(),
        "-i".to_string(), temp_video.to_string_lossy().to_string(),
        "-i".to_string(), temp_audio.to_string_lossy().to_string(),
    ];

    if video_has_audio {
        // Mix existing video audio with new audio
        merge_args.extend_from_slice(&[
            "-filter_complex".to_string(), "[0:a][1:a]amix=inputs=2:duration=first[aout]".to_string(),
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[aout]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-ac".to_string(), "2".to_string(), // Stereo output
            "-ar".to_string(), "44100".to_string(), // Standard sample rate
        ]);
    } else {
        // Use only the new audio (video has no audio or it was disabled)
        merge_args.extend_from_slice(&[
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "1:a".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), "copy".to_string(),
        ]);
    }

    merge_args.extend_from_slice(&[
        "-f".to_string(), "mp4".to_string(),        // FORCE standard MP4 container
        "-movflags".to_string(), "+faststart".to_string(), // Optimize for web playback
    ]);
    merge_args.push(output_path.to_string_lossy().to_string());

    let merge_out = Command::new(ffmpeg_cmd)
        .args(&merge_args)
        .output()
        .await
        .map_err(|e| format!("Merge command failed: {}", e))?;

    if !merge_out.status.success() {
        return Err(format!("Merge error: {}", String::from_utf8_lossy(&merge_out.stderr)));
    }

    // Cleanup temp files (best effort)
    let _ = tokio::fs::remove_file(temp_video).await;
    let _ = tokio::fs::remove_file(temp_audio).await;

    Ok((output_path.clone(), format!("Successfully merged to: {}", output_path.file_name().unwrap_or_default().to_string_lossy())))
}

impl VideoMerger {
    fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectVideo => {
                self.error_message = None;
                self.success_message = None;
                Task::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Video", &["mp4", "avi", "mov", "mkv"])
                            .pick_file()
                            .await;
                        
                        if let Some(handle) = handle {
                            let path = handle.path().to_path_buf();
                            let duration = get_duration_string(&path);
                            Some((path, duration))
                        } else {
                            None
                        }
                    },
                    Message::VideoSelected,
                )
            }
            Message::VideoSelected(data) => {
                if let Some((path, duration)) = data {
                    self.video_path = Some(path);
                    self.video_duration = duration;
                }
                Task::none()
            }
            Message::ToggleDisableVideoAudio(disabled) => {
                self.disable_video_audio = disabled;
                Task::none()
            }
            Message::SelectAudio => {
                self.error_message = None;
                self.success_message = None;
                Task::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .add_filter("Audio", &["mp3", "wav", "m4a", "aac"])
                            .pick_file()
                            .await;
                        
                        if let Some(handle) = handle {
                            let path = handle.path().to_path_buf();
                            let duration = get_duration_string(&path);
                            Some((path, duration))
                        } else {
                            None
                        }
                    },
                    Message::AudioSelected,
                )
            }
            Message::AudioSelected(data) => {
                if let Some((path, duration)) = data {
                    self.audio_path = Some(path);
                    self.audio_duration = duration;
                }
                Task::none()
            }
            Message::SetVideoLength(length) => {
                self.video_length = length;
                Task::none()
            }
            Message::Merge => {
                if self.video_path.is_none() || self.audio_path.is_none() {
                    self.error_message = Some("Please select both video and audio files.".to_string());
                    return Task::none();
                }

                // First, select output file
                Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("MP4 Video", &["mp4"])
                            .set_file_name("merged_output.mp4")
                            .save_file()
                            .await
                            .map(|h| {
                                let mut path = h.path().to_path_buf();
                                let is_mp4 = path
                                    .extension()
                                    .and_then(|ext| ext.to_str())
                                    .map(|ext| ext.eq_ignore_ascii_case("mp4"))
                                    .unwrap_or(false);
                                if !is_mp4 {
                                    path.set_extension("mp4");
                                }
                                path
                            })
                    },
                    Message::StartMerge,
                )
            }
            Message::StartMerge(output_path) => {
                if let Some(output_path) = output_path {
                    self.merging = true;
                    self.progress = 0.0;
                    self.error_message = None;
                    self.success_message = None;
                    self.merge_start_time = Some(Instant::now());

                    let video_path = self.video_path.clone().unwrap();
                    let audio_path = self.audio_path.clone().unwrap();
                    let disable_video_audio = self.disable_video_audio;
                    let target_seconds = self.video_length.as_seconds();

                    Task::perform(
                        async move {
                            merge_process(video_path, audio_path, disable_video_audio, target_seconds, output_path).await
                        },
                        Message::MergeFinished,
                    )
                } else {
                    Task::none()
                }
            }
            Message::MergeFinished(result) => {
                self.merging = false;
                self.progress = 1.0;
                
                let time_str = if let Some(start) = self.merge_start_time {
                    let elapsed = start.elapsed().as_secs();
                    if elapsed > 60 {
                        format!("{}m {}s", elapsed / 60, elapsed % 60)
                    } else {
                        format!("{}s", elapsed)
                    }
                } else {
                    "".to_string()
                };
                self.merge_start_time = None;

                match result {
                    Ok((path, msg)) => {
                        self.success_message = Some(format!("{}\nTime taken: {}", msg, time_str));
                        self.output_path = Some(path);
                    }
                    Err(e) => self.error_message = Some(e),
                }
                Task::none()
            }
            Message::OpenOutputFolder => {
                if let Some(path) = &self.output_path {
                    if let Some(parent) = path.parent() {
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(parent).spawn();
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("explorer").arg(parent).spawn();
                        #[cfg(target_os = "linux")]
                        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
                    }
                }
                Task::none()
            }
            Message::Tick => {
                if self.merging {
                    if self.progress < 0.95 {
                        // Decaying growth for indeterminate progress
                        let remaining = 0.95 - self.progress;
                        self.progress += remaining * 0.02;
                        if self.progress < 0.1 { self.progress += 0.01; }
                    }
                }
                Task::none()
            }
            Message::FileDropped(path) => {
                self.error_message = None;
                self.success_message = None;
                if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                    let ext = extension.to_lowercase();
                    match ext.as_str() {
                        "mp4" | "avi" | "mov" | "mkv" => {
                            self.video_duration = get_duration_string(&path);
                            self.video_path = Some(path);
                        }
                        "mp3" | "wav" | "m4a" | "aac" => {
                            self.audio_duration = get_duration_string(&path);
                            self.audio_path = Some(path);
                        }
                        _ => {
                            self.error_message = Some(format!("Unsupported file format: .{}", ext));
                        }
                    }
                }
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let events = iced::event::listen_with(|event, status, _| match (event, status) {
            (iced::Event::Window(iced::window::Event::FileDropped(path)), _) => {
                Some(Message::FileDropped(path))
            }
            _ => None,
        });

        if self.merging {
            Subscription::batch(vec![
                events,
                time::every(Duration::from_millis(50)).map(|_| Message::Tick),
            ])
        } else {
            events
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let title = text("Video & Audio Merger")
            .size(30)
            .width(Length::Fill)
            .align_x(Alignment::Center);

        // Video Section
        let video_section = {
            let select_btn = button("Select Video")
                .on_press_maybe((!self.merging).then_some(Message::SelectVideo))
                .width(Length::Fill)
                .padding(10);

            let file_info = if let Some(path) = &self.video_path {
                        column![
                            text(format!("Selected: {}", path.file_name().unwrap_or_default().to_string_lossy())).size(14),
                            text(format!("Duration: {}", self.video_duration)).size(12).color([0.7, 0.7, 0.7]),
                        ]
                    } else {
                column![
                    text("No video selected").size(14).color([0.5, 0.5, 0.5]),
                ]
            };

            let audio_toggle = row![
                checkbox(self.disable_video_audio).on_toggle(Message::ToggleDisableVideoAudio),
                text("Disable Audio")
            ]
            .spacing(10);

            column![
                text("1. Video Selection").size(18),
                select_btn,
                file_info,
                audio_toggle
            ]
            .spacing(10)
            .padding(15)
            .width(Length::Fill)
        };

        // Audio Section
        let audio_section = {
            let select_btn = button("Select Audio")
                .on_press_maybe((!self.merging).then_some(Message::SelectAudio))
                .width(Length::Fill)
                .padding(10);
            
            let file_info = if let Some(path) = &self.audio_path {
                        column![
                            text(format!("Selected: {}", path.file_name().unwrap_or_default().to_string_lossy())).size(14),
                            text(format!("Duration: {}", self.audio_duration)).size(12).color([0.7, 0.7, 0.7]),
                        ]
                    } else {
                column![
                    text("No audio selected").size(14).color([0.5, 0.5, 0.5]),
                ]
            };

            column![
                text("2. Audio Selection").size(18),
                select_btn,
                file_info
            ]
            .spacing(10)
            .padding(15)
            .width(Length::Fill)
        };

        // Video Length Section
        let length_section = {
            let options = [
                VideoLength::OneHour,
                VideoLength::TwoHours,
                VideoLength::ThreeHours,
                VideoLength::FourHours,
                VideoLength::FiveHours,
                VideoLength::SixHours,
            ];

            let radios = options.iter().fold(Row::new().spacing(20), |row, &len| {
                row.push(radio(
                    len.to_string(),
                    len,
                    Some(self.video_length),
                    Message::SetVideoLength,
                ))
            });

            column![
                text("3. Target Video Length").size(18),
                radios
            ]
            .spacing(10)
            .padding(15)
            .width(Length::Fill)
        };

        // Merge Section
        let merge_section = {
            let can_merge = self.video_path.is_some() && self.audio_path.is_some() && !self.merging;
            
            let merge_btn = button("Merge")
                .on_press_maybe(can_merge.then_some(Message::Merge))
                .width(Length::Fill)
                .padding(15);

            let progress = if self.merging {
                let elapsed_str = if let Some(start) = self.merge_start_time {
                    let elapsed = start.elapsed().as_secs();
                    if elapsed > 60 {
                        format!("{}m {}s", elapsed / 60, elapsed % 60)
                    } else {
                        format!("{}s", elapsed)
                    }
                } else {
                    "0s".to_string()
                };

                column![
                    text("Merging...").size(16),
                    progress_bar(0.0..=1.0, self.progress),
                    text(format!("Duration: {}", elapsed_str)).size(14),
                ]
                .spacing(10)
                .align_x(Alignment::Center)
            } else {
                column![]
            };

            column![
                merge_btn,
                progress
            ]
            .spacing(10)
            .padding(15)
            .width(Length::Fill)
        };

        // Status Messages
        let status_section = if let Some(error) = &self.error_message {
            container(text(error).color([0.9, 0.2, 0.2]))
                .padding(10)
                .style(style_box)
        } else if let Some(success) = &self.success_message {
            let content = column![
                text(success).color([0.2, 0.8, 0.2]),
                button("Open Output Folder")
                    .on_press(Message::OpenOutputFolder)
                    .padding(5)
            ]
            .spacing(10)
            .align_x(Alignment::Center);

            container(content)
                .padding(10)
                .style(style_box)
        } else {
            container(column![])
        };

        container(
                column![
                    title,
                    rule::horizontal(1),
                    row![
                        container(video_section).width(Length::FillPortion(1)).style(style_box),
                        Space::new().width(Length::Fixed(10.0)),
                        container(audio_section).width(Length::FillPortion(1)).style(style_box),
                    ],
                    length_section,
                    Space::new().height(Length::Fixed(20.0)),
                    merge_section,
                    status_section,
                ]
                .spacing(15)
            .padding(20)
            .max_width(800)
            .align_x(Alignment::Center)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}

fn style_box(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced::border::Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}
