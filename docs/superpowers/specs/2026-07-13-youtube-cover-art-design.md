# YouTube Cover Art Design

## Goal

Show the YouTube cover image for a downloaded track, centered above the title and analysis details.

## Design

- Parse supported HTTPS YouTube URLs in the frontend and extract the video ID from watch, short, embed, live, and `youtu.be` forms.
- Build the public `i.ytimg.com` `hqdefault.jpg` thumbnail URL from the validated ID before starting the download.
- Render the cover centered in the track overview, followed by the title and existing Key/BPM/duration/device details.
- Local files and shell-launched files have no cover image and keep the existing centered title/details layout.
- A thumbnail load failure hides only the image; audio download, analysis, and playback remain unaffected.

## Verification

- Unit tests cover supported URL forms and reject non-YouTube or non-HTTPS URLs.
- Frontend build and the complete Vitest suite must pass.
