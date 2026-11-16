// Drives Apple's own web player from inside its page. The page plays the music: this script only
// tells its MusicKit instance what to queue and reports where playback is. It never touches a
// media key, a licence or a sample.
(() => {
  if (window.top !== window || window.soniclyPlayer) return;

  const post = (message) => {
    try {
      window.soniclyPost(JSON.stringify(message));
    } catch (_) {}
  };

  let music = null;
  // The load the page is carrying out. Everything it reports carries this, so the app can drop
  // what an earlier load still had to say.
  let serial = 0;
  let volume = 1;

  const snapshot = () => {
    if (!music) return;
    const kit = window.MusicKit;
    const item = music.nowPlayingItem;
    const preview =
      !!item &&
      (item.playbackType === (kit.PlaybackType && kit.PlaybackType.preview) ||
        music.previewOnly === true);
    post({
      t: "state",
      serial,
      state: kit.PlaybackStates[music.playbackState] || "none",
      at: music.currentPlaybackTime || 0,
      duration: music.currentPlaybackDuration || 0,
      preview,
      authorized: !!music.isAuthorized,
      volume: music.volume,
      kbps: Number(music.bitrate) || 0,
    });
  };

  const failed = (mine, error) => {
    if (mine !== serial) return;
    post({
      t: "error",
      serial: mine,
      code: String((error && (error.errorCode || error.name)) || "UNKNOWN"),
      detail: String((error && (error.message || error.description)) || error || "").slice(0, 200),
    });
  };

  const guard = (mine, work) => Promise.resolve().then(work).catch((error) => failed(mine, error));

  window.soniclyPlayer = {
    load(id, at, mine) {
      serial = mine;
      if (!music) return;
      guard(mine, async () => {
        // Silent until it stands where it should, so a resume never sounds from the start.
        music.volume = at > 0.5 ? 0 : volume;
        music.autoplayEnabled = false;
        music.repeatMode = 0;
        music.shuffleMode = 0;
        await music.setQueue({ song: id, startPlaying: false });
        if (mine !== serial) return;
        await music.play();
        if (mine !== serial) return;
        if (at > 0.5 && Math.abs((music.currentPlaybackTime || 0) - at) > 1) {
          await music.seekToTime(at);
        }
        if (mine !== serial) return;
        music.volume = volume;
        snapshot();
      });
    },
    play() {
      if (music) guard(serial, () => music.play());
    },
    pause() {
      if (music) guard(serial, () => music.pause());
    },
    seek(at) {
      const mine = serial;
      if (!music) return;
      guard(mine, async () => {
        await music.seekToTime(at);
        if (mine !== serial) return;
        post({ t: "seeked", serial: mine, at: music.currentPlaybackTime || at });
      });
    },
    volume(level) {
      volume = level;
      if (music) music.volume = level;
    },
    stop() {
      serial = 0;
      if (music) guard(0, () => music.stop());
    },
  };

  const watch = () => {
    const events = window.MusicKit.Events;
    // MusicKit counts the position in whole seconds and says so as each one begins, which is
    // the moment the figure is exact.
    for (const name of [
      events.playbackStateDidChange,
      events.playbackDurationDidChange,
      events.nowPlayingItemDidChange,
      events.playbackTimeDidChange,
    ]) {
      music.addEventListener(name, snapshot);
    }
    music.addEventListener(events.mediaPlaybackError, (error) => failed(serial, error));
  };

  const waiting = setInterval(() => {
    let found = null;
    try {
      found = window.MusicKit && window.MusicKit.getInstance();
    } catch (_) {}
    if (!found) return;
    clearInterval(waiting);
    music = found;
    watch();
    post({ t: "ready", authorized: !!music.isAuthorized, version: String(window.MusicKit.version || "") });
  }, 200);
})();
