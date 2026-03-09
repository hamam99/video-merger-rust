# Video & Audio Merger (Rust + Iced)

A simple desktop app to merge an external audio track into a video for a selected target duration. The app loops both sources to reach the selected length, shows a live progress indicator and timer, and provides a one-click button to open the output folder after completion.

## Features

- Merge video + external audio into a single MP4 (H.264 video + AAC audio).
- Target length presets: 1h, 2h, 3h, 4h, 5h, 6h.
- Option to disable the original video’s audio before mixing.
- Progress bar with a live elapsed-time timer.
- Success message includes total time taken.
- “Open Output Folder” button after completion.
- Drag & Drop support for video and audio files.
- Works with a bundled `ffmpeg` binary (preferred) or system `ffmpeg` on PATH.

> Note: By default the app preserves the input video’s resolution. If you need to force a specific output resolution (e.g., 1080p), this can be added via an ffmpeg scale/pad filter in the processing step.

## Requirements

- Rust toolchain (stable). Install via https://rustup.rs
- FFmpeg executable
  - Preferred: Place a binary at `./bin/ffmpeg` (macOS/Linux) or alongside the final executable (also supports `./bin/ffmpeg` next to the binary).
  - Fallback: Ensure `ffmpeg` is available on your system `PATH`.

## Getting Started

1. Clone this repository:
   ```bash
   git clone <your-repo-url>
   cd vider-merge
   ```
2. Provide FFmpeg:
   - Recommended: Put the ffmpeg binary at `./bin/ffmpeg` and make it executable on macOS/Linux:
     ```bash
     chmod +x bin/ffmpeg
     ```
   - Or ensure `ffmpeg` is installed and available on your `PATH` (e.g., `brew install ffmpeg` on macOS).
3. Run in development:
   ```bash
   cargo run
   ```

To build a release binary:
```bash
cargo build --release
```
You can then place `ffmpeg` next to the built executable or under a `bin/` directory alongside it.

## Using the App

1. Launch the app (`cargo run` or run the built binary).
2. Click “Select Video” and choose your source video.
3. Click “Select Audio” and choose your audio track.
4. Choose a target length from the presets (1–6 hours).
5. (Optional) Toggle “Disable video audio” if you only want the external audio in the final output.
6. Click “Merge”.
7. Choose where to save the output file (defaults to `.mp4`). The app merges in the background:
   - A progress bar and live timer show the process.
   - On completion, you’ll see a success message with the total time taken.
   - Click “Open Output Folder” to open the location of the merged file.

### Output Details

- Container: MP4
- Video codec: H.264 (libx264)
- Audio codec: AAC
- Resolution: Preserves original video resolution by default.
- Audio handling:
  - If “Disable video audio” is ON, only the external audio is used.
  - If OFF and the video has an audio track, the app mixes the video’s audio with the external audio. If the video has no audio, it uses the external audio alone.

## Drag & Drop

You can drag a supported file from your file explorer directly onto the app window:

- Video: `.mp4`, `.avi`, `.mov`, `.mkv`
- Audio: `.mp3`, `.wav`, `.m4a`, `.aac`

The first dropped video will populate the Video selector; the first dropped audio will populate the Audio selector.

## Troubleshooting

- “FFmpeg not found” or similar:
  - Ensure `./bin/ffmpeg` exists and is executable (`chmod +x bin/ffmpeg`).
  - Or install FFmpeg and verify `ffmpeg -version` works from your terminal.
- Merge errors mentioning stream or mixing:
  - If your video has no audio and mixing is enabled, the app automatically switches to use only the external audio. If you still see an error, ensure the audio file can be decoded by your FFmpeg build.
- macOS security prompts (“cannot be opened because it is from an unidentified developer”):
  - You may need to right-click → Open the `ffmpeg` binary once to allow execution.

## Platform Notes

- macOS: Opens the output folder with `open`.
- Windows: Uses `explorer`.
- Linux: Uses `xdg-open`.

## Tech Stack

- Rust
- GUI: [`iced`](https://github.com/iced-rs/iced)
- File dialogs: [`rfd`](https://github.com/PolyMeilex/rfd)
- Media: [`ffmpeg` command-line), plus `lofty` and `mp4` crates for metadata/duration
- Async: `tokio`

## License

MIT (or your preferred license). Update this section to match your licensing choice.

## Contributing

Issues and pull requests are welcome. If you want to add options like fixed 1080p output or custom resolution selection, feel free to open a discussion or PR.
