# GUI Interactive Testing Guide (Windows)

## Objective
Validate end-to-end behavior of `superview-gui.exe` on Windows with a real local video.

## Prerequisites
```powershell
ffmpeg -version
ffprobe -version
```

Build the GUI binary:
```powershell
go build -ldflags="-H=windowsgui" -o superview-gui.exe superview-gui.go
```

## Manual Test Flow
1. Launch `superview-gui.exe`.
2. Click `1) Choose input file` and pick a local video.
3. (Optional) Select an output codec.
4. Click `2) Choose output file` and select destination `.mp4`.
5. Click `3) Start Superview transform`.
6. Confirm progress updates and completion dialog.

## Recommended Test Cases
- Basic transform with default codec.
- Transform with explicit codec selection.
- Cancel input file dialog (no error popup expected).
- Cancel output file dialog (no error popup expected).

## Expected Results
- No unexpected console popup during file dialog usage.
- GUI remains responsive during encoding.
- Output file is generated and readable by `ffprobe`.

## Output Validation
```powershell
ffprobe -show_entries format=duration,size -of default=noprint_wrappers=1 "C:\path\to\output.mp4"
```

## Status
Guide aligned with GUI-only architecture.