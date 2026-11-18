mod archive;
mod media;
mod mpv;
mod vtt;
pub use media::{Media, Subtitle, Track};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
pub use vtt::Cue;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Playback engine is unavailable. Install libmpv.")]
    EngineUnavailable,
    #[error("Could not initialize the playback engine.")]
    EngineInitialization,
    #[error("Could not read this archive. It may be damaged, password-protected or unsupported.")]
    Archive,
    #[error("Could not play this audio file.")]
    Playback,
    #[error("This file is outside the selected work folder or is a symbolic link.")]
    InvalidPath,
    #[error("The work contains too many files or nested folders.")]
    ScanLimit,
    #[error("This VTT subtitle file is invalid or too large.")]
    InvalidSubtitle,
    #[error("The player control is invalid or outdated.")]
    InvalidControl,
    #[error("Could not read or save the playback history.")]
    History,
    #[error("The player is unavailable.")]
    Closed,
    #[error("Could not access the local media file.")]
    Io(#[from] std::io::Error),
}
impl Error {
    pub fn diagnostic_kind(&self) -> &'static str {
        match self {
            Self::EngineUnavailable => "engine_unavailable",
            Self::EngineInitialization => "engine_initialization",
            Self::Archive => "archive",
            Self::Playback => "playback",
            Self::InvalidPath => "invalid_path",
            Self::ScanLimit => "scan_limit",
            Self::InvalidSubtitle => "invalid_subtitle",
            Self::InvalidControl => "invalid_control",
            Self::History => "history",
            Self::Closed => "closed",
            Self::Io(_) => "io",
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub work_id: Option<String>,
    pub track_id: Option<String>,
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub loading: bool,
    pub volume: f64,
    pub speed: f64,
    pub subtitle_id: Option<String>,
    pub subtitle_offset: f64,
    pub error: Option<String>,
    pub revision: u64,
    pub durations: BTreeMap<String, f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Control {
    Play { track_id: String },
    Pause { paused: bool },
    Seek { position: f64 },
    Volume { volume: f64 },
    Speed { speed: f64 },
    Subtitle { subtitle_id: Option<String> },
    Offset { offset: f64 },
    Next { direction: i32 },
    Stop,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Bookmark {
    track_id: String,
    position: f64,
    subtitle_offset: f64,
    subtitles: BTreeMap<String, Option<String>>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct History {
    works: BTreeMap<String, Bookmark>,
    volume: Option<f64>,
    speed: Option<f64>,
}
impl History {
    fn read(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) if bytes.len() <= 8 * 1024 * 1024 => {
                serde_json::from_slice(&bytes).map_err(|_| Error::History)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            _ => Err(Error::History),
        }
    }
    fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or(Error::History)?)
            .map_err(|_| Error::History)?;
        serde_json::to_writer(&mut temp, self).map_err(|_| Error::History)?;
        temp.flush().map_err(|_| Error::History)?;
        temp.as_file().sync_all().map_err(|_| Error::History)?;
        temp.persist(path).map_err(|_| Error::History)?;
        Ok(())
    }
}

enum Request {
    Open(String, PathBuf, u64, mpsc::SyncSender<Result<Media>>),
    Probed(u64, String, f64),
    Control(String, Control, mpsc::SyncSender<Result<Snapshot>>),
    Subtitle(String, String, mpsc::SyncSender<Result<Vec<Cue>>>),
    Shutdown(mpsc::SyncSender<()>),
}
#[derive(Clone)]
pub struct Player {
    sender: mpsc::Sender<Request>,
    snapshot: Arc<Mutex<Snapshot>>,
    probe_generation: Arc<AtomicU64>,
}
impl Player {
    pub fn start(history_path: PathBuf, changed: impl Fn(Snapshot) + Send + 'static) -> Self {
        Self::start_inner(history_path, false, changed)
    }
    fn start_inner(
        history_path: PathBuf,
        null_audio: bool,
        changed: impl Fn(Snapshot) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot {
            paused: true,
            volume: 70.0,
            speed: 1.0,
            ..Snapshot::default()
        }));
        let state = snapshot.clone();
        thread::Builder::new()
            .name("dldr-player".into())
            .spawn(move || Worker::new(history_path, state, null_audio).run(receiver, changed))
            .expect("create player worker");
        Self {
            sender,
            snapshot,
            probe_generation: Arc::new(AtomicU64::new(0)),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn open(&self, work_id: String, root: PathBuf) -> Result<Media> {
        let generation = self.probe_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let root = root.canonicalize()?;
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Request::Open(work_id, root.clone(), generation, tx))
            .map_err(|_| Error::Closed)?;
        let media = rx.recv().map_err(|_| Error::Closed)??;
        // Archived durations are learned during playback. Probing all of them
        // would decompress an entire work before the listener selects a track.
        let tracks = media
            .tracks
            .iter()
            .filter(|t| !t.id.starts_with("archive:"))
            .cloned()
            .collect::<Vec<_>>();
        if tracks.is_empty() {
            return Ok(media);
        }
        let sender = self.sender.clone();
        let current = self.probe_generation.clone();
        // Metadata probing uses null output and stays separate from the playback
        // worker. A new selection cancels the old probe between events/files.
        thread::Builder::new()
            .name("dldr-audio-metadata".into())
            .spawn(move || {
                let Ok(probe) = mpv::Mpv::new(true) else {
                    return;
                };
                for track in tracks {
                    if current.load(Ordering::Relaxed) != generation {
                        break;
                    }
                    let Ok(path) = media::resolve(&root, &track.id) else {
                        continue;
                    };
                    let Some(path) = path.to_str() else { continue };
                    if probe.command(&["stop"]).is_err() {
                        break;
                    }
                    while probe.event().is_some() {}
                    if probe.command(&["loadfile", path, "replace"]).is_err() {
                        continue;
                    }
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while Instant::now() < deadline && current.load(Ordering::Relaxed) == generation
                    {
                        match probe.event() {
                            Some(mpv::PlayerEvent::Loaded) => {
                                if let Some(duration) =
                                    probe.number("duration").filter(|v| *v > 0.0)
                                {
                                    if sender
                                        .send(Request::Probed(
                                            generation,
                                            track.id.clone(),
                                            duration,
                                        ))
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                break;
                            }
                            Some(mpv::PlayerEvent::Failed) => break,
                            _ => thread::sleep(Duration::from_millis(10)),
                        }
                    }
                }
            })
            .map_err(|_| Error::Closed)?;
        Ok(media)
    }
    pub fn control(&self, work_id: String, action: Control) -> Result<Snapshot> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Request::Control(work_id, action, tx))
            .map_err(|_| Error::Closed)?;
        rx.recv().map_err(|_| Error::Closed)?
    }
    pub fn subtitle(&self, work_id: String, id: String) -> Result<Vec<Cue>> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Request::Subtitle(work_id, id, tx))
            .map_err(|_| Error::Closed)?;
        rx.recv().map_err(|_| Error::Closed)?
    }
    pub fn shutdown(&self) {
        self.probe_generation.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::sync_channel(1);
        if self.sender.send(Request::Shutdown(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(3));
        }
    }
}
struct Worker {
    root: PathBuf,
    media: Option<Media>,
    state: Snapshot,
    shared: Arc<Mutex<Snapshot>>,
    engine: Option<mpv::Mpv>,
    history: History,
    history_path: PathBuf,
    history_valid: bool,
    null_audio: bool,
    pending: Option<(f64, bool)>,
    // Keep the optimistic bookmark until mpv finishes seeking. A fixed
    // position tolerance alone can miss the target at high playback speeds.
    seek_target: Option<(f64, Instant)>,
    probe_generation: u64,
    saved: Instant,
}
impl Worker {
    fn new(history_path: PathBuf, shared: Arc<Mutex<Snapshot>>, null_audio: bool) -> Self {
        let read = History::read(&history_path);
        let valid = read.is_ok();
        let history = read.unwrap_or_default();
        let mut state = shared.lock().unwrap().clone();
        state.volume = history
            .volume
            .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
            .unwrap_or(70.0);
        state.speed = history
            .speed
            .filter(|v| v.is_finite() && (0.5..=3.0).contains(v))
            .unwrap_or(1.0);
        if !valid {
            state.error = Some(Error::History.to_string());
        }
        Self {
            root: PathBuf::new(),
            media: None,
            state,
            shared,
            engine: None,
            history,
            history_path,
            history_valid: valid,
            null_audio,
            pending: None,
            seek_target: None,
            probe_generation: 0,
            saved: Instant::now(),
        }
    }
    fn publish(&mut self, changed: &impl Fn(Snapshot)) {
        if !self.history_valid && self.state.error.is_none() {
            self.state.error = Some(Error::History.to_string());
        }
        self.state.revision = self.state.revision.saturating_add(1);
        *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = self.state.clone();
        changed(self.state.clone());
    }
    fn run(&mut self, receiver: mpsc::Receiver<Request>, changed: impl Fn(Snapshot)) {
        loop {
            self.tick();
            match receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(Request::Probed(generation, id, duration)) => {
                    if generation == self.probe_generation {
                        self.state.durations.insert(id, duration);
                        self.publish(&changed);
                    }
                }
                Ok(Request::Open(id, root, generation, reply)) => {
                    self.probe_generation = generation;
                    let result = self.open(id, root);
                    if let Err(error) = &result {
                        self.state.error = Some(error.to_string());
                    }
                    self.publish(&changed);
                    let _ = reply.send(result);
                }
                Ok(Request::Control(work, action, reply)) => {
                    let result = self.control(&work, action);
                    if let Err(error) = &result {
                        self.state.error = Some(error.to_string());
                    }
                    self.publish(&changed);
                    let _ = reply.send(result.map(|_| self.state.clone()));
                }
                Ok(Request::Subtitle(work, id, reply)) => {
                    let _ = reply.send(self.read_subtitle(&work, &id));
                }
                Ok(Request::Shutdown(reply)) => {
                    self.save();
                    self.engine.take();
                    let _ = reply.send(());
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.save();
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.state.track_id.is_some() {
                        self.publish(&changed);
                    }
                }
            }
            if self.saved.elapsed() >= Duration::from_secs(5)
                && self.state.track_id.is_some()
                && !self.state.paused
            {
                self.save();
            }
        }
    }
    fn save(&mut self) {
        if !self.history_valid {
            return;
        } // Preserve unreadable/corrupt history for recovery.
        if let (Some(work), Some(track)) = (&self.state.work_id, &self.state.track_id) {
            let bookmark = self.history.works.entry(work.clone()).or_default();
            bookmark.track_id = track.clone();
            bookmark.position = self.state.position;
            bookmark.subtitle_offset = self.state.subtitle_offset;
        }
        self.history.volume = Some(self.state.volume);
        self.history.speed = Some(self.state.speed);
        if self.history.save(&self.history_path).is_err() {
            self.state.error = Some(Error::History.to_string());
        }
        self.saved = Instant::now();
    }
    fn open(&mut self, id: String, root: PathBuf) -> Result<Media> {
        let root = root.canonicalize()?;
        let media = media::scan(id.clone(), &root)?;
        if self.state.work_id.as_ref() == Some(&id) && self.root == root {
            self.media = Some(media.clone());
            return Ok(media);
        }
        self.save();
        if let Some(engine) = &self.engine {
            engine.command(&["stop"])?;
            while engine.event().is_some() {}
        }
        self.root = root;
        self.media = Some(media.clone());
        self.pending = None;
        self.seek_target = None;
        self.state.work_id = Some(id.clone());
        self.state.durations.clear();
        self.state.track_id = None;
        self.state.position = 0.0;
        self.state.duration = 0.0;
        self.state.paused = true;
        self.state.loading = false;
        self.state.subtitle_id = None;
        self.state.error = None;
        let bookmark = self.history.works.get(&id).cloned().unwrap_or_default();
        self.state.subtitle_offset = if bookmark.subtitle_offset.is_finite() {
            bookmark.subtitle_offset.clamp(-600.0, 600.0)
        } else {
            0.0
        };
        if media.tracks.iter().any(|t| t.id == bookmark.track_id) {
            self.state.track_id = Some(bookmark.track_id.clone());
            self.state.position = if bookmark.position.is_finite() {
                bookmark.position.max(0.0)
            } else {
                0.0
            };
            self.select_subtitle(&bookmark.track_id);
        }
        Ok(media)
    }
    fn select_subtitle(&mut self, track: &str) {
        let Some(media) = &self.media else { return };
        self.state.subtitle_id = match self
            .history
            .works
            .get(&media.work_id)
            .and_then(|b| b.subtitles.get(track))
        {
            Some(None) => None,
            Some(Some(id)) if media.subtitles.iter().any(|s| &s.id == id) => Some(id.clone()),
            _ => media
                .tracks
                .iter()
                .find(|t| t.id == track)
                .and_then(|t| media::matched_subtitle(t, &media.subtitles)),
        };
    }
    fn play(&mut self, track: &str, position: f64, autoplay: bool) -> Result<()> {
        if !self
            .media
            .as_ref()
            .is_some_and(|m| m.tracks.iter().any(|t| t.id == track))
        {
            return Err(Error::InvalidControl);
        }
        let path = media::playback_source(&self.root, track)?;
        if self.engine.is_none() {
            self.engine = Some(mpv::Mpv::new(self.null_audio)?);
        }
        let engine = self.engine.as_ref().unwrap();
        engine.command(&["stop"])?;
        while engine.event().is_some() {}
        engine.pause(true)?;
        engine.set_number("volume", self.state.volume)?;
        engine.set_number("speed", self.state.speed)?;
        engine.command(&["loadfile", &path, "replace"])?;
        self.state.track_id = Some(track.into());
        self.state.position = position;
        self.state.duration = 0.0;
        self.state.paused = !autoplay;
        self.state.loading = true;
        self.state.error = None;
        self.pending = Some((position, autoplay));
        self.seek_target = None;
        self.select_subtitle(track);
        Ok(())
    }
    fn control(&mut self, work: &str, action: Control) -> Result<()> {
        if self.state.work_id.as_deref() != Some(work) {
            return Err(Error::InvalidControl);
        }
        self.state.error = None;
        match action {
            Control::Play { track_id } => {
                let position = if self.state.track_id.as_ref() == Some(&track_id) {
                    if self.state.duration > 0.0 && self.state.position >= self.state.duration - 0.1
                    {
                        0.0
                    } else {
                        self.state.position
                    }
                } else {
                    self.save();
                    0.0
                };
                self.play(&track_id, position, true)?;
            }
            Control::Pause { paused } => {
                if self.state.track_id.is_none() {
                    return Err(Error::InvalidControl);
                }
                if let Some((_, autoplay)) = self.pending.as_mut() {
                    *autoplay = !paused;
                } else if !paused
                    && self.state.duration > 0.0
                    && self.state.position >= self.state.duration - 0.1
                {
                    let track = self.state.track_id.clone().unwrap();
                    self.play(&track, 0.0, true)?;
                } else if self.state.duration > 0.0 {
                    self.engine.as_ref().ok_or(Error::Playback)?.pause(paused)?;
                } else if !paused {
                    let track = self.state.track_id.clone().unwrap();
                    self.play(&track, self.state.position, true)?;
                }
                self.state.paused = paused;
                self.save();
            }
            Control::Seek { position } => {
                if !position.is_finite() || position < 0.0 || self.state.track_id.is_none() {
                    return Err(Error::InvalidControl);
                }
                let position = if self.state.duration > 0.0 {
                    position.min(self.state.duration)
                } else {
                    position
                };
                if let Some((start, _)) = self.pending.as_mut() {
                    *start = position;
                } else if self.state.duration > 0.0 {
                    self.engine.as_ref().ok_or(Error::Playback)?.command(&[
                        "seek",
                        &position.to_string(),
                        "absolute+exact",
                    ])?;
                }
                self.state.position = position;
                self.seek_target = Some((position, Instant::now()));
                self.save();
            }
            Control::Volume { volume } => {
                if !volume.is_finite() || !(0.0..=100.0).contains(&volume) {
                    return Err(Error::InvalidControl);
                }
                if let Some(engine) = &self.engine {
                    engine.set_number("volume", volume)?;
                }
                self.state.volume = volume;
                self.save();
            }
            Control::Speed { speed } => {
                if !speed.is_finite() || !(0.5..=3.0).contains(&speed) {
                    return Err(Error::InvalidControl);
                }
                if let Some(engine) = &self.engine {
                    engine.set_number("speed", speed)?;
                }
                self.state.speed = speed;
                self.save();
            }
            Control::Subtitle { subtitle_id } => {
                let track = self.state.track_id.clone().ok_or(Error::InvalidControl)?;
                let media = self.media.as_ref().ok_or(Error::InvalidControl)?;
                if subtitle_id
                    .as_ref()
                    .is_some_and(|id| !media.subtitles.iter().any(|s| &s.id == id))
                {
                    return Err(Error::InvalidControl);
                }
                if let Some(id) = &subtitle_id {
                    self.read_subtitle(work, id)?;
                }
                self.history
                    .works
                    .entry(work.into())
                    .or_default()
                    .subtitles
                    .insert(track, subtitle_id.clone());
                self.state.subtitle_id = subtitle_id;
                self.save();
            }
            Control::Offset { offset } => {
                if !offset.is_finite() || !(-600.0..=600.0).contains(&offset) {
                    return Err(Error::InvalidControl);
                }
                self.state.subtitle_offset = offset;
                self.save();
            }
            Control::Next { direction } => {
                if ![-1, 1].contains(&direction) {
                    return Err(Error::InvalidControl);
                }
                self.advance(direction)?;
            }
            Control::Stop => {
                self.save();
                if let Some(engine) = &self.engine {
                    engine.command(&["stop"])?;
                }
                self.pending = None;
                self.seek_target = None;
                self.state.loading = false;
                self.state.paused = true;
                self.state.track_id = None;
                self.state.position = 0.0;
                self.state.duration = 0.0;
                self.state.subtitle_id = None;
            }
        }
        Ok(())
    }
    fn advance(&mut self, direction: i32) -> Result<()> {
        let media = self.media.as_ref().ok_or(Error::InvalidControl)?;
        let index = media
            .tracks
            .iter()
            .position(|t| Some(&t.id) == self.state.track_id.as_ref())
            .ok_or(Error::InvalidControl)?;
        // Never autoplay an alternative WAV/MP3 version in another folder.
        let group = &media.tracks[index].group;
        let next = if direction > 0 {
            media.tracks.get(index + 1)
        } else {
            index.checked_sub(1).and_then(|i| media.tracks.get(i))
        };
        if let Some(next) = next.filter(|t| &t.group == group) {
            let id = next.id.clone();
            self.save();
            self.play(&id, 0.0, true)?;
        } else {
            self.state.paused = true;
            if let Some(engine) = &self.engine {
                engine.pause(true)?;
            }
        }
        Ok(())
    }
    fn tick(&mut self) {
        for _ in 0..128 {
            let Some(event) = self.engine.as_ref().and_then(|p| p.event()) else {
                break;
            };
            match event {
                mpv::PlayerEvent::Loaded => {
                    if let Some((position, autoplay)) = self.pending.take() {
                        let engine = self.engine.as_ref().unwrap();
                        self.state.duration = engine.number("duration").unwrap_or(0.0).max(0.0);
                        if self.state.duration > 0.0 {
                            if let Some(id) = &self.state.track_id {
                                self.state.durations.insert(id.clone(), self.state.duration);
                            }
                        }
                        let position = if self.state.duration > 0.0 {
                            position.min((self.state.duration - 0.05).max(0.0))
                        } else {
                            position
                        };
                        let result = if position > 0.0 {
                            engine.command(&["seek", &position.to_string(), "absolute+exact"])
                        } else {
                            Ok(())
                        }
                        .and_then(|_| engine.pause(!autoplay));
                        self.seek_target = (position > 0.0).then(|| (position, Instant::now()));
                        self.state.position = position;
                        self.state.loading = false;
                        if let Err(error) = result {
                            self.state.error = Some(error.to_string());
                            self.state.paused = true;
                        }
                    }
                }
                mpv::PlayerEvent::Eof if self.pending.is_none() => {
                    self.seek_target = None;
                    if self.state.duration > 0.0 {
                        self.state.position = self.state.duration;
                    }
                    if let Err(error) = self.advance(1) {
                        self.state.error = Some(error.to_string());
                        self.state.paused = true;
                    }
                    self.save();
                }
                mpv::PlayerEvent::Failed => {
                    self.seek_target = None;
                    self.pending = None;
                    self.state.loading = false;
                    self.state.paused = true;
                    self.state.error = Some(Error::Playback.to_string());
                }
                _ => {}
            }
        }
        if self.pending.is_none() && self.state.track_id.is_some() {
            if let Some(engine) = &self.engine {
                if let Some(position) = engine.number("time-pos") {
                    // Paused audio can report the demuxer's pre-roll timestamp
                    // until decoding resumes. Keep the user's requested position
                    // rather than replacing it with that earlier timestamp.
                    if !(self.state.paused && self.seek_target.is_some())
                        && self.seek_target.is_none_or(|(target, requested)| {
                            // At high speed the clock can advance past the target
                            // between ticks. Account for elapsed playback time, but
                            // do not mistake the pre-seek position for a paused seek.
                            let drift = if self.state.paused {
                                0.2
                            } else {
                                requested.elapsed().as_secs_f64() * self.state.speed + 0.5
                            };
                            !engine.seeking()
                                && ((position >= target - 0.2 && position <= target + drift)
                                    || requested.elapsed() >= Duration::from_secs(5))
                        })
                    {
                        self.seek_target = None;
                        self.state.position = position.max(0.0);
                    }
                }
                if let Some(duration) = engine.number("duration") {
                    self.state.duration = duration.max(0.0);
                }
            }
        }
    }
    fn read_subtitle(&self, work: &str, id: &str) -> Result<Vec<Cue>> {
        if !self
            .media
            .as_ref()
            .is_some_and(|m| m.work_id == work && m.subtitles.iter().any(|s| s.id == id))
        {
            return Err(Error::InvalidControl);
        }
        let bytes = media::subtitle_bytes(&self.root, id)?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(Error::InvalidSubtitle);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::InvalidSubtitle)?;
        vtt::parse(text)
    }
}

#[cfg(test)]
mod tests;
