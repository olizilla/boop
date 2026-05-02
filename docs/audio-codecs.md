# audio in boop

First up, I want boops to be captured in a standard audio format that all platforms can record and play. As they are intended for small voice notes, they should be compressed to save bandwidth.

Where we endeded up:

- Record: wav/pcm mono 16khz 16bit (in webview)
- Transcode: wav -> flac (in rust)
- Playback: flac via rodio (in rust)


## Why do that?

I wanted to standarise on webm/opus everywhere but I also want it to work on linux, and I am not yet ready to give up on tarui.

In Tarui v2 on linux the mediaRecorder api in the webview can only record mp4/aac. This would be okay, but the webview on mac cannot playback those files. 

webm/opus recorded on a mac does play fine on linux which makes it frustrating that we can't record it in the webview on linux. We tried forcing the webview on linux to support webm/opus by adding gstreamer codecs, and other sacrifices, but it would not.

So we record in .wav/pcm in the webview and send that to the rust process. Mac and linkux can play and record wav/pcm, but we transcode to FLAC to save bandwidth. FLAC is chosen as we can encode it in rust, and it plays back in rodio (rust audio).


## Future

- Switch to webm/opus when we can record/play it everywhere.
- Might be worth testing recording from the mic in rust, so the core is self contained, and we dont have to rely on the webview or send audio over the webview->rust boundary. But recording works currently, so this is not a priority.