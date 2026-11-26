# Media

`director.mjs` records the walkthrough video: `node media/director.mjs` writes
`media/out/pulsefold-walkthrough-1080p60.mp4` (1920×1080, 60 fps, H.264 High, with chapters).

The page runs on a virtual clock (`clock.js`): `performance.now`, `Date.now`, timers,
`requestAnimationFrame`, CSS animations and transitions, `Math.random` (seeded) and network
and WebAssembly loads all advance one frame at a time, and each frame is a lossless
screenshot. The video is therefore a perfect 60 fps however loaded the machine is, and any
Chrome instance renders any frame identically, so the capture is split across workers
(`--workers N`, default 8) and joined losslessly. `kit.js` adds the on-screen cursor and
the section cards; neither ships in `dist/`.

Needs `google-chrome`, `ffmpeg` with libx264 and `python3`. `--smoke` renders a few seconds.
