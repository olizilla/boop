# audio codecs in tarui apps

Notes on recording audio in the webview using the mediarecorder api.

on Linux/arm64 you can record mp4/aac. you cannot record webm/opus.

On macOS you can record and play webm/opus. Notably you can't seem to play an mp4 created on linux/arm64.

You can't record wav with the mediarecorder api, on any platform.
