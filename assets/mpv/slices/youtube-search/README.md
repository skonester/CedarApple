# YouTube Search Slice

Adapted from **Lunedor's mpv-youtube-search**:
https://github.com/Lunedor/mpv-youtube-search

The supplied basic Lua script remains the search, pagination and selection
controller. CedarApple's Lua Slice adapter supplies a native thumbnail grid,
search input and Play / Queue buttons, and forwards keyboard actions to Lua.
Playback goes through CedarApple so its queue, resume positions and titles stay
in sync. This package includes the basic signed-out search variant.

Open **Slices → YouTube Search → Open**, or press **Ctrl+S** from the player or
start screen. Type a query and press Enter. Use arrows to select, Enter to play,
Shift+Enter to queue, and N/P for the next/previous page. You can also use the
mouse and the Play / + Queue buttons. Escape closes the Slice.

CedarApple shares its existing yt-dlp/Deno preparation with Lumtub. It loads
JPEG thumbnails directly, so the Slice does not need the ffmpeg command-line
tool. Closing, cancelling or disabling a Slice aborts its pending work.

Changes from the supplied source:

- Added optional `cedarapple_slice` events for the native UI and playback.
- Preserved standalone mpv input, OSD and thumbnail code as a fallback.
- Added cancellation and stale-request protection, response validation,
  argument-safe searches, and responsive keyboard navigation.
- Kept the original search-and-page behavior and eight-result default.

Original supplied file SHA-256:
`F6374797698A180833706D467B6EB9CE81295CC9EC5382E39DFD1172A5F89D4B`
