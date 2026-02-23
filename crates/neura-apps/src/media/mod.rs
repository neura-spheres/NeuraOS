// NeuraMedia — Professional TUI Music Player
// Features: real audio playback (rodio), metadata reading (lofty),
//           local-path import with progress, queue, shuffle/repeat,
//           albums/artists views, search, playlists, visualizer, favorites.

use std::any::Any;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use chrono::Utc;
use neura_app_framework::app_trait::App;
use neura_storage::vfs::Vfs;

// ── Tokyo Night palette (imported from neura_app_framework) ──────────────────
use neura_app_framework::palette::*;

const BAR_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub year: u32,
    pub track_number: u32,
    pub duration_secs: u32,
    pub path: String,
    pub file_size_bytes: u64,
    pub added_at: String,
    pub play_count: u32,
    pub is_favorite: bool,
}

impl MediaTrack {
    pub fn duration_str(&self) -> String {
        let m = self.duration_secs / 60;
        let s = self.duration_secs % 60;
        format!("{:02}:{:02}", m, s)
    }
    pub fn size_str(&self) -> String {
        let mb = self.file_size_bytes as f64 / 1_048_576.0;
        if mb >= 1.0 { format!("{:.1} MB", mb) } else { format!("{:.0} KB", mb * 1024.0) }
    }
    pub fn display_artist(&self) -> &str {
        if !self.artist.is_empty() { &self.artist }
        else if !self.album_artist.is_empty() { &self.album_artist }
        else { "Unknown Artist" }
    }
    pub fn display_album(&self) -> &str {
        if !self.album.is_empty() { &self.album } else { "Unknown Album" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub track_ids: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepeatMode { Off, All, One }
impl RepeatMode {
    fn cycle(&self) -> Self { match self { Self::Off => Self::All, Self::All => Self::One, Self::One => Self::Off } }
    fn label(&self) -> &str { match self { Self::Off => "Off", Self::All => "All", Self::One => "One" } }
    fn icon(&self) -> &str { match self { Self::Off => "⇄", Self::All => "↻", Self::One => "↺" } }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortBy { Title, Artist, Album, Year, Duration, PlayCount, DateAdded }
impl SortBy {
    fn label(&self) -> &str {
        match self { Self::Title=>"Title", Self::Artist=>"Artist", Self::Album=>"Album",
            Self::Year=>"Year", Self::Duration=>"Duration", Self::PlayCount=>"Plays", Self::DateAdded=>"Added" }
    }
}

// ── Audio thread protocol ─────────────────────────────────────────────────────

#[derive(Debug)]
pub enum AudioCmd {
    Play(String),   // file path
    Pause, Resume, TogglePause, Stop,
    SetVolume(f32),
    Quit,
}

#[derive(Debug, Clone, Default)]
pub struct AudioStatus {
    pub is_playing: bool,
    pub is_paused: bool,
    pub elapsed_secs: f64,
    pub track_finished: bool,
    pub audio_ok: bool,
}

// ── View state ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum View { Library, Albums, Artists, Playlists, Queue, NowPlaying, Import, Search }

#[derive(Debug, Clone)]
enum ImportProgress { Idle, Scanning(String), Done(usize, usize), Error(String) }

#[derive(Debug, Clone)]
struct AlbumEntry { name: String, artist: String, year: u32, track_ids: Vec<String> }
#[derive(Debug, Clone)]
struct ArtistEntry { name: String, track_ids: Vec<String>, album_names: Vec<String> }

// ── The App ───────────────────────────────────────────────────────────────────

pub struct MediaApp {
    vfs: Arc<Vfs>,
    username: String,

    // Data
    library: Vec<MediaTrack>,
    playlists: Vec<Playlist>,

    // Playback
    queue: Vec<String>,         // track IDs in playback order
    queue_pos: usize,
    current_track_id: Option<String>,
    volume: f32,
    is_muted: bool,
    shuffle: bool,
    repeat: RepeatMode,
    track_duration_secs: u32,

    // Audio backend (channel-based, OutputStream lives in thread)
    audio_tx: mpsc::SyncSender<AudioCmd>,
    audio_status: Arc<Mutex<AudioStatus>>,

    // Visualizer
    viz_tick: u64,

    // View
    view: View,
    prev_view: View,

    // Library view
    lib_sel: usize,
    lib_scroll: usize,
    lib_sort: SortBy,
    lib_sort_asc: bool,
    lib_filter: String,
    lib_filter_cursor: usize,
    lib_filtering: bool,
    lib_ids: Vec<String>,          // filtered+sorted track IDs for display

    // Album / Artist indexes
    albums: Vec<AlbumEntry>,
    album_sel: usize,
    album_scroll: usize,
    album_open: bool,
    album_track_sel: usize,

    artists: Vec<ArtistEntry>,
    artist_sel: usize,
    artist_scroll: usize,
    artist_open: bool,
    artist_track_sel: usize,

    // Playlist view
    pl_sel: usize,
    pl_track_sel: usize,
    pl_open: bool,
    new_pl_name: String,
    new_pl_cursor: usize,
    new_pl_dialog: bool,
    add_to_pl_track: Option<String>,
    add_to_pl_sel: usize,
    add_to_pl_dialog: bool,

    // Queue view
    queue_sel: usize,

    // Now Playing view
    np_scroll: usize,

    // Import view
    import_path: String,
    import_cursor: usize,
    import_progress: ImportProgress,

    // Search view
    search_query: String,
    search_cursor: usize,
    search_results: Vec<String>,
    search_sel: usize,

    // Async signals (read by main.rs)
    pub pending_import: Option<String>,
    pub needs_rebuild: bool,
    pub needs_save: bool,

    // Agent control slots (AI agent writes, tick() reads and populates)
    agent_cmd: Option<crate::agent_tools::MediaCmdSlot>,
    now_playing_slot: Option<crate::agent_tools::NowPlayingSlot>,
}

impl MediaApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        let (tx, rx) = mpsc::sync_channel::<AudioCmd>(32);
        let status = Arc::new(Mutex::new(AudioStatus::default()));
        spawn_audio_thread(rx, status.clone());

        let mut app = Self {
            vfs,
            username: username.to_string(),
            library: Vec::new(),
            playlists: Vec::new(),
            queue: Vec::new(),
            queue_pos: 0,
            current_track_id: None,
            volume: 0.8,
            is_muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            track_duration_secs: 0,
            audio_tx: tx,
            audio_status: status,
            viz_tick: 0,
            view: View::Library,
            prev_view: View::Library,
            lib_sel: 0,
            lib_scroll: 0,
            lib_sort: SortBy::Artist,
            lib_sort_asc: true,
            lib_filter: String::new(),
            lib_filter_cursor: 0,
            lib_filtering: false,
            lib_ids: Vec::new(),
            albums: Vec::new(),
            album_sel: 0,
            album_scroll: 0,
            album_open: false,
            album_track_sel: 0,
            artists: Vec::new(),
            artist_sel: 0,
            artist_scroll: 0,
            artist_open: false,
            artist_track_sel: 0,
            pl_sel: 0,
            pl_track_sel: 0,
            pl_open: false,
            new_pl_name: String::new(),
            new_pl_cursor: 0,
            new_pl_dialog: false,
            add_to_pl_track: None,
            add_to_pl_sel: 0,
            add_to_pl_dialog: false,
            queue_sel: 0,
            np_scroll: 0,
            import_path: String::new(),
            import_cursor: 0,
            import_progress: ImportProgress::Idle,
            search_query: String::new(),
            search_cursor: 0,
            search_results: Vec::new(),
            search_sel: 0,
            pending_import: None,
            needs_rebuild: false,
            needs_save: false,
            agent_cmd: None,
            now_playing_slot: None,
        };
        app.rebuild_indexes();
        app
    }

    // ── Public signals (checked by main.rs) ──────────────────────────────────

    pub fn needs_import(&self) -> bool { self.pending_import.is_some() }

    /// Give the AI agent a shared slot to send media commands into.
    pub fn set_agent_cmd_slot(&mut self, slot: crate::agent_tools::MediaCmdSlot) {
        self.agent_cmd = Some(slot);
    }

    /// Give the AI agent a shared slot to read the current now-playing snapshot from.
    pub fn set_now_playing_slot(&mut self, slot: crate::agent_tools::NowPlayingSlot) {
        self.now_playing_slot = Some(slot);
    }

    /// Execute a command queued by the AI agent.
    fn execute_agent_cmd(&mut self, cmd: crate::agent_tools::MediaAgentCmd) {
        use crate::agent_tools::MediaAgentCmd;
        match cmd {
            MediaAgentCmd::PlayQuery(query) => {
                let q = query.to_lowercase();
                let found = self.library.iter()
                    .find(|t| {
                        t.title.to_lowercase().contains(&q)
                            || t.artist.to_lowercase().contains(&q)
                            || t.album.to_lowercase().contains(&q)
                    })
                    .map(|t| t.id.clone());
                if let Some(id) = found {
                    self.build_queue_and_play(&id);
                }
            }
            MediaAgentCmd::Stop => {
                self.send_audio(AudioCmd::Stop);
                self.current_track_id = None;
            }
            MediaAgentCmd::Pause   => self.send_audio(AudioCmd::Pause),
            MediaAgentCmd::Resume  => self.send_audio(AudioCmd::Resume),
            MediaAgentCmd::Next    => self.play_next(),
            MediaAgentCmd::Previous => self.play_prev(),
            MediaAgentCmd::SetVolume(v) => self.set_volume(v),
        }
    }

    pub fn tick(&mut self) {
        self.viz_tick = self.viz_tick.wrapping_add(1);

        // Process incoming agent command (AI agent writes, we read and clear)
        let agent_cmd = self.agent_cmd.as_ref()
            .and_then(|slot| slot.lock().ok())
            .and_then(|mut g| g.take());
        if let Some(cmd) = agent_cmd {
            self.execute_agent_cmd(cmd);
        }

        // Update now-playing snapshot so AI tools can read current state
        let nps = self.now_playing_slot.clone();
        if let Some(slot) = nps {
            if let Ok(mut snap) = slot.lock() {
                let (title, artist, album) = if let Some(ref id) = self.current_track_id {
                    self.library.iter().find(|t| &t.id == id)
                        .map(|t| (
                            t.title.clone(),
                            t.display_artist().to_string(),
                            t.display_album().to_string(),
                        ))
                        .unwrap_or_default()
                } else {
                    (String::new(), String::new(), String::new())
                };
                let (is_playing, is_paused, elapsed_secs) = self.audio_status.lock()
                    .map(|s| (s.is_playing, s.is_paused, s.elapsed_secs))
                    .unwrap_or((false, false, 0.0));
                snap.title        = title;
                snap.artist       = artist;
                snap.album        = album;
                snap.is_playing   = is_playing;
                snap.is_paused    = is_paused;
                snap.elapsed_secs = elapsed_secs;
                snap.duration_secs = self.track_duration_secs;
                snap.volume       = self.volume;
            }
        }

        // Check if track finished → auto advance
        let finished = self.audio_status.lock()
            .map(|s| { let f = s.track_finished; f })
            .unwrap_or(false);
        if finished {
            if let Ok(mut s) = self.audio_status.lock() { s.track_finished = false; }
            self.advance_queue();
        }
    }

    // ── Playback control ─────────────────────────────────────────────────────

    fn send_audio(&self, cmd: AudioCmd) {
        let _ = self.audio_tx.try_send(cmd);
    }

    fn play_track(&mut self, track_id: &str) {
        if let Some(track) = self.library.iter().find(|t| t.id == track_id) {
            let path = track.path.clone();
            let dur = track.duration_secs;
            self.current_track_id = Some(track_id.to_string());
            self.track_duration_secs = dur;
            self.send_audio(AudioCmd::Play(path));
            if !self.is_muted {
                self.send_audio(AudioCmd::SetVolume(self.volume));
            } else {
                self.send_audio(AudioCmd::SetVolume(0.0));
            }
            // Increment play count
            if let Some(t) = self.library.iter_mut().find(|t| t.id == track_id) {
                t.play_count += 1;
            }
            self.needs_save = true;
        }
    }

    fn play_queue_at(&mut self, pos: usize) {
        if pos < self.queue.len() {
            self.queue_pos = pos;
            let id = self.queue[pos].clone();
            self.play_track(&id);
        }
    }

    fn advance_queue(&mut self) {
        match self.repeat {
            RepeatMode::One => {
                // Replay current
                if let Some(ref id) = self.current_track_id.clone() {
                    let path = self.library.iter().find(|t| &t.id == id).map(|t| t.path.clone());
                    if let Some(p) = path { self.send_audio(AudioCmd::Play(p)); }
                }
            }
            RepeatMode::All => {
                let next = (self.queue_pos + 1) % self.queue.len().max(1);
                self.play_queue_at(next);
            }
            RepeatMode::Off => {
                let next = self.queue_pos + 1;
                if next < self.queue.len() {
                    self.play_queue_at(next);
                } else {
                    self.current_track_id = None;
                    self.send_audio(AudioCmd::Stop);
                }
            }
        }
    }

    fn play_next(&mut self) {
        let next = (self.queue_pos + 1) % self.queue.len().max(1);
        if !self.queue.is_empty() { self.play_queue_at(next); }
    }

    fn play_prev(&mut self) {
        // If more than 3s in: restart, else go previous
        let elapsed = self.audio_status.lock().map(|s| s.elapsed_secs).unwrap_or(0.0);
        if elapsed > 3.0 {
            if let Some(ref id) = self.current_track_id.clone() {
                let path = self.library.iter().find(|t| &t.id == id).map(|t| t.path.clone());
                if let Some(p) = path { self.send_audio(AudioCmd::Play(p)); }
            }
        } else if self.queue_pos > 0 {
            let prev = self.queue_pos - 1;
            self.play_queue_at(prev);
        }
    }

    fn toggle_pause(&mut self) { self.send_audio(AudioCmd::TogglePause); }

    fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if !self.is_muted { self.send_audio(AudioCmd::SetVolume(self.volume)); }
    }

    fn toggle_mute(&mut self) {
        self.is_muted = !self.is_muted;
        self.send_audio(AudioCmd::SetVolume(if self.is_muted { 0.0 } else { self.volume }));
    }

    fn toggle_favorite_current(&mut self) {
        if let Some(ref id) = self.current_track_id.clone() {
            if let Some(t) = self.library.iter_mut().find(|t| &t.id == id) {
                t.is_favorite = !t.is_favorite;
                self.needs_save = true;
            }
        }
    }

    fn toggle_favorite_selected(&mut self) {
        if let Some(id) = self.selected_track_id() {
            if let Some(t) = self.library.iter_mut().find(|t| t.id == id) {
                t.is_favorite = !t.is_favorite;
                self.needs_save = true;
            }
        }
    }

    /// Build queue from lib_ids starting at the selected track, optionally shuffled.
    fn build_queue_and_play(&mut self, start_id: &str) {
        let mut ids = self.lib_ids.clone();
        let start_pos;
        if self.shuffle {
            // Fisher-Yates shuffle with selected at front
            let sel_pos = ids.iter().position(|id| id == start_id).unwrap_or(0);
            ids.swap(0, sel_pos);
            let n = ids.len();
            for i in 1..n {
                let j = i + (self.viz_tick as usize + i * 7919) % (n - i);
                ids.swap(i, j);
            }
            start_pos = 0;
        } else {
            start_pos = ids.iter().position(|id| id == start_id).unwrap_or(0);
        }
        self.queue = ids;
        self.play_queue_at(start_pos);
    }

    fn add_to_queue(&mut self, track_id: &str) {
        self.queue.push(track_id.to_string());
    }

    // ── Index building ────────────────────────────────────────────────────────

    pub fn rebuild_indexes(&mut self) {
        self.rebuild_lib_ids();
        self.rebuild_albums();
        self.rebuild_artists();
    }

    fn rebuild_lib_ids(&mut self) {
        let filter = self.lib_filter.to_lowercase();
        let mut ids: Vec<String> = self.library.iter()
            .filter(|t| {
                filter.is_empty()
                    || t.title.to_lowercase().contains(&filter)
                    || t.artist.to_lowercase().contains(&filter)
                    || t.album.to_lowercase().contains(&filter)
                    || t.genre.to_lowercase().contains(&filter)
            })
            .map(|t| t.id.clone())
            .collect();

        let lib = &self.library;
        ids.sort_by(|a, b| {
            let ta = lib.iter().find(|t| &t.id == a);
            let tb = lib.iter().find(|t| &t.id == b);
            if let (Some(ta), Some(tb)) = (ta, tb) {
                let ord = match self.lib_sort {
                    SortBy::Title    => ta.title.cmp(&tb.title),
                    SortBy::Artist   => ta.display_artist().cmp(tb.display_artist())
                                          .then(ta.album.cmp(&tb.album))
                                          .then(ta.track_number.cmp(&tb.track_number)),
                    SortBy::Album    => ta.album.cmp(&tb.album).then(ta.track_number.cmp(&tb.track_number)),
                    SortBy::Year     => ta.year.cmp(&tb.year),
                    SortBy::Duration => ta.duration_secs.cmp(&tb.duration_secs),
                    SortBy::PlayCount => ta.play_count.cmp(&tb.play_count),
                    SortBy::DateAdded => ta.added_at.cmp(&tb.added_at),
                };
                if self.lib_sort_asc { ord } else { ord.reverse() }
            } else {
                std::cmp::Ordering::Equal
            }
        });
        self.lib_ids = ids;
    }

    fn rebuild_albums(&mut self) {
        let mut map: std::collections::HashMap<String, AlbumEntry> = std::collections::HashMap::new();
        for t in &self.library {
            let key = format!("{}|{}", t.display_album(), t.display_artist());
            let entry = map.entry(key).or_insert_with(|| AlbumEntry {
                name: t.display_album().to_string(),
                artist: t.display_artist().to_string(),
                year: t.year,
                track_ids: Vec::new(),
            });
            entry.track_ids.push(t.id.clone());
            if t.year > 0 && t.year < entry.year { entry.year = t.year; }
        }
        let mut albums: Vec<AlbumEntry> = map.into_values().collect();
        albums.sort_by(|a, b| a.name.cmp(&b.name));
        self.albums = albums;
        if self.album_sel >= self.albums.len() { self.album_sel = 0; }
    }

    fn rebuild_artists(&mut self) {
        let mut map: std::collections::HashMap<String, ArtistEntry> = std::collections::HashMap::new();
        for t in &self.library {
            let name = t.display_artist().to_string();
            let entry = map.entry(name.clone()).or_insert_with(|| ArtistEntry {
                name: name.clone(),
                track_ids: Vec::new(),
                album_names: Vec::new(),
            });
            entry.track_ids.push(t.id.clone());
            let alb = t.display_album().to_string();
            if !entry.album_names.contains(&alb) { entry.album_names.push(alb); }
        }
        let mut artists: Vec<ArtistEntry> = map.into_values().collect();
        artists.sort_by(|a, b| a.name.cmp(&b.name));
        self.artists = artists;
        if self.artist_sel >= self.artists.len() { self.artist_sel = 0; }
    }

    fn run_search(&mut self) {
        let q = self.search_query.to_lowercase();
        self.search_results = if q.is_empty() {
            Vec::new()
        } else {
            self.library.iter()
                .filter(|t| {
                    t.title.to_lowercase().contains(&q)
                        || t.artist.to_lowercase().contains(&q)
                        || t.album.to_lowercase().contains(&q)
                        || t.genre.to_lowercase().contains(&q)
                })
                .map(|t| t.id.clone())
                .collect()
        };
        self.search_sel = 0;
    }

    // ── Save / Load ───────────────────────────────────────────────────────────

    pub async fn async_save(&mut self) {
        self.needs_save = false;
        let state = serde_json::json!({
            "library": &self.library,
            "playlists": &self.playlists,
        });
        if let Ok(data) = serde_json::to_vec_pretty(&state) {
            let path = format!("/home/{}/media.json", self.username);
            let _ = self.vfs.write_file(&path, data, &self.username).await;
        }
    }

    // ── Import from local path ────────────────────────────────────────────────

    pub async fn async_import(&mut self) {
        let path = match self.pending_import.take() { Some(p) => p, None => return };

        // Collect existing paths to avoid duplicates
        let existing: std::collections::HashSet<String> =
            self.library.iter().map(|t| t.path.clone()).collect();

        let result = tokio::task::spawn_blocking(move || {
            let p = std::path::Path::new(&path);
            let mut files: Vec<std::path::PathBuf> = Vec::new();
            if p.is_file() {
                if is_audio_file(p) { files.push(p.to_path_buf()); }
                else { return Err(format!("Not an audio file: {}", path)); }
            } else if p.is_dir() {
                collect_audio_files(p, &mut files);
            } else {
                return Err(format!("Path not found: {}", path));
            }
            let total = files.len();
            if total == 0 { return Err("No audio files found in that path.".to_string()); }
            let new_tracks: Vec<MediaTrack> = files.iter()
                .filter(|f| !existing.contains(f.to_string_lossy().as_ref()))
                .map(|f| read_file_metadata(f))
                .collect();
            Ok((new_tracks, total))
        }).await;

        match result {
            Ok(Ok((tracks, total))) => {
                let n = tracks.len();
                self.library.extend(tracks);
                self.import_progress = ImportProgress::Done(n, total);
                self.needs_rebuild = true;
                self.needs_save = true;
            }
            Ok(Err(e)) => { self.import_progress = ImportProgress::Error(e); }
            Err(e) => { self.import_progress = ImportProgress::Error(e.to_string()); }
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn selected_track_id(&self) -> Option<String> {
        match self.view {
            View::Library => self.lib_ids.get(self.lib_sel).cloned(),
            View::Search  => self.search_results.get(self.search_sel).cloned(),
            View::Albums  => {
                if self.album_open {
                    self.albums.get(self.album_sel)
                        .and_then(|a| a.track_ids.get(self.album_track_sel))
                        .cloned()
                } else { None }
            }
            View::Artists => {
                if self.artist_open {
                    self.artists.get(self.artist_sel)
                        .and_then(|a| a.track_ids.get(self.artist_track_sel))
                        .cloned()
                } else { None }
            }
            View::Queue => self.queue.get(self.queue_sel).cloned(),
            View::Playlists => {
                if self.pl_open {
                    self.playlists.get(self.pl_sel)
                        .and_then(|p| p.track_ids.get(self.pl_track_sel))
                        .cloned()
                } else { None }
            }
            _ => None,
        }
    }

    fn track_by_id(&self, id: &str) -> Option<&MediaTrack> {
        self.library.iter().find(|t| t.id == id)
    }

    fn audio_info(&self) -> AudioStatus {
        self.audio_status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn total_duration_str(&self) -> String {
        let secs: u32 = self.library.iter().map(|t| t.duration_secs).sum();
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if h > 0 { format!("{}h {}m", h, m) } else { format!("{}m", m) }
    }

    fn vol_bar(&self, width: usize) -> String {
        let filled = (self.volume * width as f32) as usize;
        let empty = width.saturating_sub(filled);
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }

    fn progress_bar(&self, elapsed: f64, total: u32, width: usize) -> String {
        if total == 0 { return "░".repeat(width); }
        let pct = (elapsed / total as f64).clamp(0.0, 1.0);
        let filled = (pct * width as f64) as usize;
        let empty = width.saturating_sub(filled.saturating_add(1));
        format!("{}●{}", "━".repeat(filled), "░".repeat(empty))
    }

    fn visualizer_line(&self, elapsed: f64, is_playing: bool, width: usize) -> String {
        let count = width.min(60);
        let mut out = String::with_capacity(count);
        for i in 0..count {
            let h = if is_playing {
                let t = elapsed + self.viz_tick as f64 * 0.05;
                let v1 = ((t * (2.3 + i as f64 * 0.11)).sin().abs() * 5.0) as usize;
                let v2 = ((t * (1.7 + i as f64 * 0.07)).cos().abs() * 3.0) as usize;
                (v1 + v2).min(8)
            } else {
                0
            };
            out.push(BAR_CHARS[h]);
        }
        out
    }
}

// ── App trait ─────────────────────────────────────────────────────────────────

impl App for MediaApp {
    fn id(&self) -> &str { "media" }
    fn name(&self) -> &str { "NeuraMedia" }
    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        // ── Dialogs take priority ──────────────────────────────────────────
        if self.new_pl_dialog { return self.handle_new_pl_key(key); }
        if self.add_to_pl_dialog { return self.handle_add_to_pl_key(key); }

        // ── Import view ────────────────────────────────────────────────────
        if self.view == View::Import { return self.handle_import_key(key); }

        // ── Global playback controls (work in all views) ───────────────────
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if !self.lib_filtering {
            match key.code {
                KeyCode::Char(' ') => { self.toggle_pause(); return true; }
                KeyCode::Char('n') if !ctrl => { self.play_next(); return true; }
                KeyCode::Char('N') => { self.play_prev(); return true; }
                KeyCode::Char('s') => { self.shuffle = !self.shuffle; return true; }
                KeyCode::Char('r') => { self.repeat = self.repeat.cycle(); return true; }
                KeyCode::Char('m') => { self.toggle_mute(); return true; }
                KeyCode::Char('f') if self.view == View::NowPlaying => {
                    self.toggle_favorite_current(); return true;
                }
                KeyCode::Char('+') | KeyCode::Char('=') => { self.set_volume(self.volume + 0.05); return true; }
                KeyCode::Char('-') => { self.set_volume(self.volume - 0.05); return true; }
                _ => {}
            }
        }

        // ── Tab navigation ─────────────────────────────────────────────────
        if !self.lib_filtering {
            match key.code {
                KeyCode::Char('1') => { self.view = View::Library; return true; }
                KeyCode::Char('2') => { self.view = View::Albums; return true; }
                KeyCode::Char('3') => { self.view = View::Artists; return true; }
                KeyCode::Char('4') => { self.view = View::Playlists; return true; }
                KeyCode::Char('5') => { self.view = View::Queue; return true; }
                KeyCode::Char('6') => { self.view = View::NowPlaying; return true; }
                KeyCode::Char('i') => {
                    self.prev_view = self.view.clone();
                    self.view = View::Import;
                    self.import_path.clear();
                    self.import_cursor = 0;
                    self.import_progress = ImportProgress::Idle;
                    return true;
                }
                KeyCode::Char('/') => {
                    self.prev_view = self.view.clone();
                    self.view = View::Search;
                    self.search_query.clear();
                    self.search_cursor = 0;
                    self.search_results.clear();
                    return true;
                }
                _ => {}
            }
        }

        match self.view.clone() {
            View::Library  => self.handle_library_key(key),
            View::Albums   => self.handle_albums_key(key),
            View::Artists  => self.handle_artists_key(key),
            View::Playlists => self.handle_playlists_key(key),
            View::Queue    => self.handle_queue_key(key),
            View::NowPlaying => self.handle_np_key(key),
            View::Search   => self.handle_search_key(key),
            View::Import   => self.handle_import_key(key),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // header / tab bar
                Constraint::Min(5),      // main content
                Constraint::Length(4),   // now-playing bar
                Constraint::Length(1),   // help / hotkeys
            ])
            .split(area);

        self.render_header(frame, chunks[0]);
        match &self.view {
            View::Library   => self.render_library(frame, chunks[1]),
            View::Albums    => self.render_albums(frame, chunks[1]),
            View::Artists   => self.render_artists(frame, chunks[1]),
            View::Playlists => self.render_playlists(frame, chunks[1]),
            View::Queue     => self.render_queue(frame, chunks[1]),
            View::NowPlaying => self.render_now_playing(frame, chunks[1]),
            View::Import    => self.render_import(frame, chunks[1]),
            View::Search    => self.render_search(frame, chunks[1]),
        }
        self.render_player_bar(frame, chunks[2]);
        self.render_help(frame, chunks[3]);

        // Render dialogs on top
        if self.new_pl_dialog { self.render_new_pl_dialog(frame, area); }
        if self.add_to_pl_dialog { self.render_add_to_pl_dialog(frame, area); }
    }

    fn on_resume(&mut self) { self.needs_rebuild = true; }
    fn on_pause(&mut self) {}
    fn on_close(&mut self) { let _ = self.audio_tx.try_send(AudioCmd::Quit); }

    fn save_state(&self) -> Option<Value> {
        Some(serde_json::json!({ "library": &self.library, "playlists": &self.playlists }))
    }
    fn load_state(&mut self, state: Value) {
        if let Some(lib) = state.get("library") {
            if let Ok(tracks) = serde_json::from_value::<Vec<MediaTrack>>(lib.clone()) {
                self.library = tracks;
            }
        }
        if let Some(pls) = state.get("playlists") {
            if let Ok(playlists) = serde_json::from_value::<Vec<Playlist>>(pls.clone()) {
                self.playlists = playlists;
            }
        }
        self.rebuild_indexes();
    }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── Key handlers ──────────────────────────────────────────────────────────────

impl MediaApp {
    fn handle_library_key(&mut self, key: KeyEvent) -> bool {
        // Filter mode
        if self.lib_filtering {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.lib_filtering = false;
                    self.lib_sel = 0;
                    self.lib_scroll = 0;
                }
                KeyCode::Char(c) => {
                    self.lib_filter.insert(self.lib_filter_cursor, c);
                    self.lib_filter_cursor += 1;
                    self.rebuild_lib_ids();
                }
                KeyCode::Backspace => {
                    if self.lib_filter_cursor > 0 {
                        self.lib_filter_cursor -= 1;
                        self.lib_filter.remove(self.lib_filter_cursor);
                        self.rebuild_lib_ids();
                    }
                }
                KeyCode::Delete => {
                    if self.lib_filter_cursor < self.lib_filter.len() {
                        self.lib_filter.remove(self.lib_filter_cursor);
                        self.rebuild_lib_ids();
                    }
                }
                KeyCode::Left => { if self.lib_filter_cursor > 0 { self.lib_filter_cursor -= 1; } }
                KeyCode::Right => { if self.lib_filter_cursor < self.lib_filter.len() { self.lib_filter_cursor += 1; } }
                _ => {}
            }
            return true;
        }

        match key.code {
            KeyCode::Esc => {
                if !self.lib_filter.is_empty() {
                    self.lib_filter.clear();
                    self.lib_filter_cursor = 0;
                    self.rebuild_lib_ids();
                } else {
                    return false;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.lib_sel > 0 { self.lib_sel -= 1; }
                self.clamp_lib_scroll();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.lib_sel + 1 < self.lib_ids.len() { self.lib_sel += 1; }
                self.clamp_lib_scroll();
            }
            KeyCode::PageUp   => { self.lib_sel = self.lib_sel.saturating_sub(10); self.clamp_lib_scroll(); }
            KeyCode::PageDown => { self.lib_sel = (self.lib_sel + 10).min(self.lib_ids.len().saturating_sub(1)); self.clamp_lib_scroll(); }
            KeyCode::Home => { self.lib_sel = 0; self.lib_scroll = 0; }
            KeyCode::End  => { self.lib_sel = self.lib_ids.len().saturating_sub(1); self.clamp_lib_scroll(); }
            KeyCode::Enter => {
                if let Some(id) = self.lib_ids.get(self.lib_sel).cloned() {
                    self.build_queue_and_play(&id.clone());
                }
            }
            KeyCode::Char('q') => {
                if let Some(id) = self.lib_ids.get(self.lib_sel).cloned() {
                    self.add_to_queue(&id.clone());
                }
            }
            KeyCode::Char('f') => { self.toggle_favorite_selected(); }
            KeyCode::Char('d') => {
                if let Some(id) = self.lib_ids.get(self.lib_sel).cloned() {
                    self.library.retain(|t| t.id != id);
                    self.rebuild_indexes();
                    self.needs_save = true;
                    if self.lib_sel >= self.lib_ids.len() && self.lib_sel > 0 { self.lib_sel -= 1; }
                }
            }
            KeyCode::Char('F') => {
                // Filter mode
                self.lib_filtering = true;
                self.lib_filter.clear();
                self.lib_filter_cursor = 0;
                self.rebuild_lib_ids();
            }
            // Sort keys: Shift+T/A/L/Y/D/P
            KeyCode::Char('T') => { self.toggle_sort(SortBy::Title); }
            KeyCode::Char('A') => { self.toggle_sort(SortBy::Artist); }
            KeyCode::Char('L') => { self.toggle_sort(SortBy::Album); }
            KeyCode::Char('Y') => { self.toggle_sort(SortBy::Year); }
            KeyCode::Char('D') => { self.toggle_sort(SortBy::Duration); }
            KeyCode::Char('P') => { self.toggle_sort(SortBy::PlayCount); }
            KeyCode::Char('a') => {
                // Add to playlist dialog
                if let Some(id) = self.lib_ids.get(self.lib_sel).cloned() {
                    self.add_to_pl_track = Some(id);
                    self.add_to_pl_sel = 0;
                    self.add_to_pl_dialog = true;
                }
            }
            _ => {}
        }
        true
    }

    fn toggle_sort(&mut self, field: SortBy) {
        if self.lib_sort == field {
            self.lib_sort_asc = !self.lib_sort_asc;
        } else {
            self.lib_sort = field;
            self.lib_sort_asc = true;
        }
        self.lib_sel = 0;
        self.lib_scroll = 0;
        self.rebuild_lib_ids();
    }

    fn clamp_lib_scroll(&mut self) {
        // Compute visible height: we don't know it here, use 20 as estimate
        let page = 20usize;
        if self.lib_sel < self.lib_scroll { self.lib_scroll = self.lib_sel; }
        if self.lib_sel >= self.lib_scroll + page { self.lib_scroll = self.lib_sel + 1 - page; }
    }

    fn handle_albums_key(&mut self, key: KeyEvent) -> bool {
        if self.album_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => { self.album_open = false; }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.album_track_sel > 0 { self.album_track_sel -= 1; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = self.albums.get(self.album_sel).map(|a| a.track_ids.len()).unwrap_or(0);
                    if self.album_track_sel + 1 < len { self.album_track_sel += 1; }
                }
                KeyCode::Enter => {
                    if let Some(id) = self.albums.get(self.album_sel)
                        .and_then(|a| a.track_ids.get(self.album_track_sel)).cloned()
                    {
                        self.build_queue_and_play(&id.clone());
                    }
                }
                KeyCode::Char('q') => {
                    if let Some(id) = self.albums.get(self.album_sel)
                        .and_then(|a| a.track_ids.get(self.album_track_sel)).cloned()
                    { self.add_to_queue(&id.clone()); }
                }
                KeyCode::Char('f') => { self.toggle_favorite_selected(); }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Esc => return false,
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.album_sel > 0 { self.album_sel -= 1; }
                    if self.album_sel < self.album_scroll { self.album_scroll = self.album_sel; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.album_sel + 1 < self.albums.len() { self.album_sel += 1; }
                    if self.album_sel >= self.album_scroll + 15 { self.album_scroll = self.album_sel.saturating_sub(14); }
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    self.album_open = true;
                    self.album_track_sel = 0;
                }
                KeyCode::Char('p') => {
                    // Play entire album
                    if let Some(album) = self.albums.get(self.album_sel) {
                        if let Some(_first_id) = album.track_ids.first().cloned() {
                            let ids = album.track_ids.clone();
                            self.queue = ids;
                            self.play_queue_at(0);
                        }
                    }
                }
                _ => {}
            }
        }
        true
    }

    fn handle_artists_key(&mut self, key: KeyEvent) -> bool {
        if self.artist_open {
            match key.code {
                KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => { self.artist_open = false; }
                KeyCode::Up | KeyCode::Char('k') => { if self.artist_track_sel > 0 { self.artist_track_sel -= 1; } }
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = self.artists.get(self.artist_sel).map(|a| a.track_ids.len()).unwrap_or(0);
                    if self.artist_track_sel + 1 < len { self.artist_track_sel += 1; }
                }
                KeyCode::Enter => {
                    if let Some(id) = self.artists.get(self.artist_sel)
                        .and_then(|a| a.track_ids.get(self.artist_track_sel)).cloned()
                    { self.build_queue_and_play(&id.clone()); }
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Esc => return false,
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.artist_sel > 0 { self.artist_sel -= 1; }
                    if self.artist_sel < self.artist_scroll { self.artist_scroll = self.artist_sel; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.artist_sel + 1 < self.artists.len() { self.artist_sel += 1; }
                    if self.artist_sel >= self.artist_scroll + 20 { self.artist_scroll = self.artist_sel.saturating_sub(19); }
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    self.artist_open = true;
                    self.artist_track_sel = 0;
                }
                KeyCode::Char('p') => {
                    if let Some(artist) = self.artists.get(self.artist_sel) {
                        let ids = artist.track_ids.clone();
                        self.queue = ids;
                        self.play_queue_at(0);
                    }
                }
                _ => {}
            }
        }
        true
    }

    fn handle_playlists_key(&mut self, key: KeyEvent) -> bool {
        if self.pl_open {
            match key.code {
                KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => { self.pl_open = false; }
                KeyCode::Up | KeyCode::Char('k') => { if self.pl_track_sel > 0 { self.pl_track_sel -= 1; } }
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = self.playlists.get(self.pl_sel).map(|p| p.track_ids.len()).unwrap_or(0);
                    if self.pl_track_sel + 1 < len { self.pl_track_sel += 1; }
                }
                KeyCode::Enter => {
                    if let Some(_id) = self.playlists.get(self.pl_sel)
                        .and_then(|p| p.track_ids.get(self.pl_track_sel)).cloned()
                    {
                        let ids = self.playlists[self.pl_sel].track_ids.clone();
                        self.queue = ids;
                        self.play_queue_at(self.pl_track_sel);
                    }
                }
                KeyCode::Char('d') => {
                    if self.pl_sel < self.playlists.len() {
                        let pl = &mut self.playlists[self.pl_sel];
                        if self.pl_track_sel < pl.track_ids.len() {
                            pl.track_ids.remove(self.pl_track_sel);
                            if self.pl_track_sel >= pl.track_ids.len() && self.pl_track_sel > 0 {
                                self.pl_track_sel -= 1;
                            }
                            self.needs_save = true;
                        }
                    }
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Esc => return false,
                KeyCode::Up | KeyCode::Char('k') => { if self.pl_sel > 0 { self.pl_sel -= 1; } }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.pl_sel + 1 < self.playlists.len() { self.pl_sel += 1; }
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    if !self.playlists.is_empty() { self.pl_open = true; self.pl_track_sel = 0; }
                }
                KeyCode::Char('n') => {
                    self.new_pl_name.clear();
                    self.new_pl_cursor = 0;
                    self.new_pl_dialog = true;
                }
                KeyCode::Char('d') => {
                    if self.pl_sel < self.playlists.len() {
                        self.playlists.remove(self.pl_sel);
                        if self.pl_sel >= self.playlists.len() && self.pl_sel > 0 { self.pl_sel -= 1; }
                        self.needs_save = true;
                    }
                }
                KeyCode::Char('p') => {
                    if let Some(pl) = self.playlists.get(self.pl_sel) {
                        let ids = pl.track_ids.clone();
                        self.queue = ids;
                        if !self.queue.is_empty() { self.play_queue_at(0); }
                    }
                }
                _ => {}
            }
        }
        true
    }

    fn handle_queue_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => return false,
            KeyCode::Up | KeyCode::Char('k') => { if self.queue_sel > 0 { self.queue_sel -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.queue_sel + 1 < self.queue.len() { self.queue_sel += 1; }
            }
            KeyCode::Enter => { self.play_queue_at(self.queue_sel); }
            KeyCode::Char('d') => {
                if self.queue_sel < self.queue.len() {
                    self.queue.remove(self.queue_sel);
                    if self.queue_sel >= self.queue.len() && self.queue_sel > 0 { self.queue_sel -= 1; }
                }
            }
            KeyCode::Char('c') => { self.queue.clear(); self.queue_sel = 0; }
            _ => {}
        }
        true
    }

    fn handle_np_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => return false,
            KeyCode::Up | KeyCode::Char('k') => { self.np_scroll = self.np_scroll.saturating_sub(1); }
            KeyCode::Down | KeyCode::Char('j') => { self.np_scroll = self.np_scroll.saturating_add(1); }
            _ => {}
        }
        true
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.view = self.prev_view.clone();
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') if !self.search_results.is_empty() => {
                if self.search_sel > 0 { self.search_sel -= 1; }
            }
            KeyCode::Down | KeyCode::Char('j') if !self.search_results.is_empty() => {
                if self.search_sel + 1 < self.search_results.len() { self.search_sel += 1; }
            }
            KeyCode::Enter => {
                if let Some(id) = self.search_results.get(self.search_sel).cloned() {
                    self.build_queue_and_play(&id.clone());
                }
            }
            KeyCode::Char('q') => {
                if let Some(id) = self.search_results.get(self.search_sel).cloned() {
                    self.add_to_queue(&id.clone());
                }
            }
            KeyCode::Char('f') => { self.toggle_favorite_selected(); }
            KeyCode::Char('a') => {
                if let Some(id) = self.search_results.get(self.search_sel).cloned() {
                    self.add_to_pl_track = Some(id);
                    self.add_to_pl_sel = 0;
                    self.add_to_pl_dialog = true;
                }
            }
            KeyCode::Char(c) => {
                self.search_query.insert(self.search_cursor, c);
                self.search_cursor += 1;
                self.run_search();
            }
            KeyCode::Backspace => {
                if self.search_cursor > 0 {
                    self.search_cursor -= 1;
                    self.search_query.remove(self.search_cursor);
                    self.run_search();
                }
            }
            KeyCode::Left  => { if self.search_cursor > 0 { self.search_cursor -= 1; } }
            KeyCode::Right => { if self.search_cursor < self.search_query.len() { self.search_cursor += 1; } }
            _ => {}
        }
        true
    }

    fn handle_import_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.view = self.prev_view.clone();
                return true;
            }
            KeyCode::Enter => {
                let path = self.import_path.trim().to_string();
                if !path.is_empty() {
                    self.import_progress = ImportProgress::Scanning(
                        format!("Scanning \"{}\"…", &path[..path.len().min(40)])
                    );
                    self.pending_import = Some(path);
                }
            }
            KeyCode::Char(c) => {
                self.import_path.insert(self.import_cursor, c);
                self.import_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.import_cursor > 0 {
                    self.import_cursor -= 1;
                    self.import_path.remove(self.import_cursor);
                }
            }
            KeyCode::Delete => {
                if self.import_cursor < self.import_path.len() {
                    self.import_path.remove(self.import_cursor);
                }
            }
            KeyCode::Left  => { if self.import_cursor > 0 { self.import_cursor -= 1; } }
            KeyCode::Right => { if self.import_cursor < self.import_path.len() { self.import_cursor += 1; } }
            KeyCode::Home  => { self.import_cursor = 0; }
            KeyCode::End   => { self.import_cursor = self.import_path.len(); }
            _ => {}
        }
        true
    }

    fn handle_new_pl_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => { self.new_pl_dialog = false; }
            KeyCode::Enter => {
                let name = self.new_pl_name.trim().to_string();
                if !name.is_empty() {
                    self.playlists.push(Playlist {
                        id: gen_id(),
                        name,
                        track_ids: Vec::new(),
                        created_at: Utc::now().to_rfc3339(),
                    });
                    self.needs_save = true;
                }
                self.new_pl_dialog = false;
            }
            KeyCode::Char(c) => {
                self.new_pl_name.insert(self.new_pl_cursor, c);
                self.new_pl_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.new_pl_cursor > 0 {
                    self.new_pl_cursor -= 1;
                    self.new_pl_name.remove(self.new_pl_cursor);
                }
            }
            KeyCode::Left  => { if self.new_pl_cursor > 0 { self.new_pl_cursor -= 1; } }
            KeyCode::Right => { if self.new_pl_cursor < self.new_pl_name.len() { self.new_pl_cursor += 1; } }
            _ => {}
        }
        true
    }

    fn handle_add_to_pl_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => { self.add_to_pl_dialog = false; }
            KeyCode::Up | KeyCode::Char('k') => { if self.add_to_pl_sel > 0 { self.add_to_pl_sel -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.add_to_pl_sel + 1 < self.playlists.len() { self.add_to_pl_sel += 1; }
            }
            KeyCode::Enter => {
                if let Some(ref track_id) = self.add_to_pl_track.clone() {
                    if let Some(pl) = self.playlists.get_mut(self.add_to_pl_sel) {
                        if !pl.track_ids.contains(track_id) {
                            pl.track_ids.push(track_id.clone());
                            self.needs_save = true;
                        }
                    }
                }
                self.add_to_pl_dialog = false;
            }
            KeyCode::Char('n') => {
                self.add_to_pl_dialog = false;
                self.new_pl_name.clear();
                self.new_pl_cursor = 0;
                self.new_pl_dialog = true;
            }
            _ => {}
        }
        true
    }
}

// ── Render methods ─────────────────────────────────────────────────────────────

impl MediaApp {
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let _audio = self.audio_info();
        let tabs = [
            ("[1] Library", View::Library),
            ("[2] Albums",  View::Albums),
            ("[3] Artists", View::Artists),
            ("[4] Playlists", View::Playlists),
            ("[5] Queue",   View::Queue),
            ("[6] Now Playing", View::NowPlaying),
        ];
        let mut spans: Vec<Span> = vec![Span::raw(" ")];
        for (label, v) in &tabs {
            let active = &self.view == v;
            spans.push(Span::styled(
                label.to_string(),
                if active { Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD | Modifier::UNDERLINED) }
                else { Style::default().fg(DIM) },
            ));
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            format!("[i] Import  [/] Search  ·  {} tracks  {}", self.library.len(), self.total_duration_str()),
            Style::default().fg(DIM),
        ));

        let header = Paragraph::new(Line::from(spans))
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .title(" NeuraMedia ")
                .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)));
        frame.render_widget(header, area);
    }

    fn render_player_bar(&self, frame: &mut Frame, area: Rect) {
        let audio = self.audio_info();
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(BORDER));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);

        // Line 1: track info + visualizer
        let (track_info, is_fav) = if let Some(ref id) = self.current_track_id {
            if let Some(t) = self.track_by_id(id) {
                (format!("{} {} — {}", if t.is_favorite { "♥" } else { "♩" }, t.title, t.display_artist()), t.is_favorite)
            } else { ("No track".to_string(), false) }
        } else { ("  No track playing — press [Enter] on a track to start".to_string(), false) };

        let is_playing = audio.is_playing;
        let viz = self.visualizer_line(audio.elapsed_secs, is_playing, 28);
        let status_icon = if audio.is_paused { "▐▐" } else if is_playing { "▶" } else { "■" };
        let shuffle_s = if self.shuffle { format!("🔀") } else { "  ".to_string() };
        let repeat_s = format!("{} {}", self.repeat.icon(), self.repeat.label());
        let vol_pct = (self.volume * 100.0) as u32;
        let mute_s = if self.is_muted { "🔇" } else { "🔊" };

        let line1 = Line::from(vec![
            Span::styled(format!(" {} ", status_icon), Style::default().fg(GREEN)),
            Span::styled(truncate(&track_info, 40), Style::default().fg(TEXT).add_modifier(if is_fav { Modifier::BOLD } else { Modifier::empty() })),
            Span::raw("  "),
            Span::styled(viz, Style::default().fg(CYAN)),
            Span::raw("  "),
            Span::styled(format!("{} {}  {}  {} {}%", shuffle_s, repeat_s, mute_s, self.vol_bar(8), vol_pct),
                Style::default().fg(MUTED)),
        ]);
        frame.render_widget(Paragraph::new(line1), chunks[0]);

        // Line 2: progress bar
        let elapsed = audio.elapsed_secs;
        let dur = self.track_duration_secs;
        let elapsed_str = fmt_time(elapsed as u32);
        let dur_str = fmt_time(dur);
        let bar_w = inner.width.saturating_sub(20) as usize;
        let bar = self.progress_bar(elapsed, dur, bar_w);
        let line2 = Line::from(vec![
            Span::styled(format!(" {} ", elapsed_str), Style::default().fg(MUTED)),
            Span::styled(bar, Style::default().fg(PRIMARY)),
            Span::styled(format!(" {} ", dur_str), Style::default().fg(MUTED)),
        ]);
        frame.render_widget(Paragraph::new(line2), chunks[1]);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let base = "  [Space] pause  [n/N] next/prev  [s] shuffle  [r] repeat  [m] mute  [+/-] vol  [f] fav  [1-6] tabs  [i] import  [/] search";
        let view_hint = match self.view {
            View::Library  => "  [Enter] play  [q] queue  [a] playlist  [d] del  [F] filter  [T/A/L/Y/D/P] sort",
            View::Albums   => "  [Enter] open  [p] play album  [→] open",
            View::Artists  => "  [Enter] open  [p] play artist  [→] open",
            View::Playlists=> "  [n] new  [p] play  [d] del  [Enter] open",
            View::Queue    => "  [Enter] play  [d] remove  [c] clear queue",
            View::NowPlaying => "  [j/k] scroll queue  [f] favorite",
            View::Search   => "  [type] search  [Enter] play  [q] queue  [a] playlist  [f] fav",
            View::Import   => "  [Enter] import  [Esc] cancel",
        };
        frame.render_widget(
            Paragraph::new(format!("{}{}", base, view_hint)).style(Style::default().fg(DIM)),
            area,
        );
    }

    fn render_library(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(format!(
                " Library ({} tracks{}) — Sort: {} {} {} ",
                self.library.len(),
                if self.lib_filter.is_empty() { String::new() } else { format!(", filter: \"{}\"", self.lib_filter) },
                self.lib_sort.label(),
                if self.lib_sort_asc { "↑" } else { "↓" },
                if self.lib_filtering { "[FILTERING]" } else { "" }
            ))
            .title_style(Style::default().fg(PRIMARY));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Filter bar
        let content_area = if self.lib_filtering || !self.lib_filter.is_empty() {
            let f_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(inner);
            let filter_text = format!("  Filter: {}_", self.lib_filter);
            frame.render_widget(
                Paragraph::new(filter_text).style(Style::default().fg(CYAN)),
                f_chunks[0],
            );
            f_chunks[1]
        } else { inner };

        // Column header
        let header = format!("  {:<3} {:<2} {:<36} {:<24} {:<22} {:<6} {:<5}",
            "#", "", "Title", "Artist", "Album", "Time", "Year");
        let h_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(content_area);
        frame.render_widget(
            Paragraph::new(header).style(Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
            h_chunks[0],
        );

        let visible_h = h_chunks[1].height as usize;
        let max_scroll = self.lib_ids.len().saturating_sub(visible_h);
        let scroll = self.lib_scroll.min(max_scroll);

        let items: Vec<ListItem> = self.lib_ids.iter().enumerate()
            .skip(scroll)
            .take(visible_h)
            .map(|(idx, id)| {
                let is_sel = idx == self.lib_sel;
                let is_playing = self.current_track_id.as_deref() == Some(id.as_str());
                let track = self.track_by_id(id);
                let style = if is_sel {
                    Style::default().fg(PRIMARY).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else if is_playing {
                    Style::default().fg(GREEN)
                } else {
                    Style::default().fg(TEXT)
                };
                let (title, artist, album, dur, year, fav, _num) = track.map(|t| (
                    truncate(&t.title, 36),
                    truncate(t.display_artist(), 24),
                    truncate(t.display_album(), 22),
                    t.duration_str(),
                    t.year,
                    t.is_favorite,
                    t.track_number,
                )).unwrap_or_default();
                let note = if is_playing { "♪" } else if fav { "♥" } else { " " };
                let text = format!("  {:<3} {} {:<36} {:<24} {:<22} {:<6} {:<5}",
                    idx + 1, note, title, artist, album, dur, if year > 0 { year.to_string() } else { String::new() });
                ListItem::new(text).style(style)
            })
            .collect();

        if self.lib_ids.is_empty() {
            frame.render_widget(
                Paragraph::new("\n  No tracks found. Press [i] to import music, or [F] to clear the filter.")
                    .style(Style::default().fg(DIM)),
                h_chunks[1],
            );
        } else {
            frame.render_widget(List::new(items), h_chunks[1]);
        }
    }

    fn render_albums(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        // Album list
        let list_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" Albums ({}) ", self.albums.len()))
            .title_style(Style::default().fg(PRIMARY));
        let list_inner = list_block.inner(chunks[0]);
        frame.render_widget(list_block, chunks[0]);

        let vis_h = list_inner.height as usize;
        let scroll = self.album_scroll.min(self.albums.len().saturating_sub(vis_h));
        let items: Vec<ListItem> = self.albums.iter().enumerate()
            .skip(scroll)
            .take(vis_h)
            .map(|(i, album)| {
                let is_sel = i == self.album_sel;
                let color = album_color(&album.name);
                let style = if is_sel {
                    Style::default().fg(color).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else { Style::default().fg(TEXT) };
                let prefix = if is_sel { "▸ " } else { "  " };
                let year_s = if album.year > 0 { format!(" ({})", album.year) } else { String::new() };
                ListItem::new(format!("{}▬  {}{}", prefix, truncate(&album.name, 28), year_s)).style(style)
            })
            .collect();
        frame.render_widget(List::new(items), list_inner);

        // Detail pane
        let detail_title = self.albums.get(self.album_sel)
            .map(|a| format!(" {} — {} ({} tracks) ", a.name, a.artist, a.track_ids.len()))
            .unwrap_or_else(|| " Select an album ".to_string());
        let detail_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.album_open { PRIMARY } else { BORDER }))
            .title(detail_title)
            .title_style(Style::default().fg(PRIMARY));
        let detail_inner = detail_block.inner(chunks[1]);
        frame.render_widget(detail_block, chunks[1]);

        if let Some(album) = self.albums.get(self.album_sel) {
            let items: Vec<ListItem> = album.track_ids.iter().enumerate().map(|(i, id)| {
                let is_sel = self.album_open && i == self.album_track_sel;
                let is_playing = self.current_track_id.as_deref() == Some(id.as_str());
                let style = if is_sel {
                    Style::default().fg(PRIMARY).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else if is_playing { Style::default().fg(GREEN) }
                else { Style::default().fg(TEXT) };
                let track = self.track_by_id(id);
                let (num, title, dur) = track.map(|t| (t.track_number, t.title.clone(), t.duration_str())).unwrap_or_default();
                let note = if is_playing { "♪" } else if is_sel { "▸" } else { " " };
                let text = format!("  {} {:>2}. {:<36} {}", note, if num > 0 { num.to_string() } else { String::new() }, title, dur);
                ListItem::new(text).style(style)
            }).collect();
            frame.render_widget(List::new(items), detail_inner);
        } else {
            frame.render_widget(
                Paragraph::new("\n  No albums yet. Import music with [i].").style(Style::default().fg(DIM)),
                detail_inner,
            );
        }
    }

    fn render_artists(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let list_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" Artists ({}) ", self.artists.len()))
            .title_style(Style::default().fg(PRIMARY));
        let list_inner = list_block.inner(chunks[0]);
        frame.render_widget(list_block, chunks[0]);

        let vis_h = list_inner.height as usize;
        let scroll = self.artist_scroll.min(self.artists.len().saturating_sub(vis_h));
        let items: Vec<ListItem> = self.artists.iter().enumerate()
            .skip(scroll)
            .take(vis_h)
            .map(|(i, artist)| {
                let is_sel = i == self.artist_sel;
                let style = if is_sel {
                    Style::default().fg(MAGENTA).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else { Style::default().fg(TEXT) };
                let prefix = if is_sel { "▸ " } else { "  " };
                ListItem::new(format!("{}♬  {}", prefix, truncate(&artist.name, 28))).style(style)
            })
            .collect();
        frame.render_widget(List::new(items), list_inner);

        let artist_info = self.artists.get(self.artist_sel)
            .map(|a| format!(" {} — {} albums, {} tracks ", a.name, a.album_names.len(), a.track_ids.len()))
            .unwrap_or_else(|| " Select an artist ".to_string());
        let detail_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.artist_open { PRIMARY } else { BORDER }))
            .title(artist_info)
            .title_style(Style::default().fg(MAGENTA));
        let detail_inner = detail_block.inner(chunks[1]);
        frame.render_widget(detail_block, chunks[1]);

        if let Some(artist) = self.artists.get(self.artist_sel) {
            let items: Vec<ListItem> = artist.track_ids.iter().enumerate().map(|(i, id)| {
                let is_sel = self.artist_open && i == self.artist_track_sel;
                let is_playing = self.current_track_id.as_deref() == Some(id.as_str());
                let style = if is_sel {
                    Style::default().fg(MAGENTA).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else if is_playing { Style::default().fg(GREEN) }
                else { Style::default().fg(TEXT) };
                let track = self.track_by_id(id);
                let (title, album, dur) = track.map(|t| (t.title.clone(), t.display_album().to_string(), t.duration_str())).unwrap_or_default();
                let note = if is_playing { "♪" } else if is_sel { "▸" } else { " " };
                ListItem::new(format!("  {} {:<30} {:<24} {}", note, title, album, dur)).style(style)
            }).collect();
            frame.render_widget(List::new(items), detail_inner);
        }
    }

    fn render_playlists(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let pl_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" Playlists ({}) ", self.playlists.len()))
            .title_style(Style::default().fg(PRIMARY));
        let pl_inner = pl_block.inner(chunks[0]);
        frame.render_widget(pl_block, chunks[0]);

        let items: Vec<ListItem> = if self.playlists.is_empty() {
            vec![ListItem::new("  No playlists. Press [n] to create one.").style(Style::default().fg(DIM))]
        } else {
            self.playlists.iter().enumerate().map(|(i, pl)| {
                let is_sel = i == self.pl_sel;
                let style = if is_sel {
                    Style::default().fg(ORANGE).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else { Style::default().fg(TEXT) };
                ListItem::new(format!("{}  ♫  {}", if is_sel { "▸" } else { " " }, truncate(&pl.name, 26))).style(style)
            }).collect()
        };
        frame.render_widget(List::new(items), pl_inner);

        let detail_title = self.playlists.get(self.pl_sel)
            .map(|p| format!(" {} ({} tracks) ", p.name, p.track_ids.len()))
            .unwrap_or_else(|| " Select a playlist ".to_string());
        let detail_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.pl_open { ORANGE } else { BORDER }))
            .title(detail_title)
            .title_style(Style::default().fg(ORANGE));
        let detail_inner = detail_block.inner(chunks[1]);
        frame.render_widget(detail_block, chunks[1]);

        if let Some(pl) = self.playlists.get(self.pl_sel) {
            if pl.track_ids.is_empty() {
                frame.render_widget(
                    Paragraph::new("\n  Playlist is empty.\n  Go to Library or Search, select a track, and press [a].").style(Style::default().fg(DIM)),
                    detail_inner,
                );
            } else {
                let items: Vec<ListItem> = pl.track_ids.iter().enumerate().map(|(i, id)| {
                    let is_sel = self.pl_open && i == self.pl_track_sel;
                    let is_playing = self.current_track_id.as_deref() == Some(id.as_str());
                    let style = if is_sel {
                        Style::default().fg(ORANGE).bg(SEL_BG).add_modifier(Modifier::BOLD)
                    } else if is_playing { Style::default().fg(GREEN) }
                    else { Style::default().fg(TEXT) };
                    let track = self.track_by_id(id);
                    let (title, artist, dur) = track.map(|t| (t.title.clone(), t.display_artist().to_string(), t.duration_str())).unwrap_or_default();
                    let note = if is_playing { "♪" } else if is_sel { "▸" } else { " " };
                    ListItem::new(format!("  {} {:>2}. {:<30} {:<22} {}", note, i + 1, title, artist, dur)).style(style)
                }).collect();
                frame.render_widget(List::new(items), detail_inner);
            }
        }
    }

    fn render_queue(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" Queue ({} tracks) ", self.queue.len()))
            .title_style(Style::default().fg(PRIMARY));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.queue.is_empty() {
            frame.render_widget(
                Paragraph::new("\n  Queue is empty.\n  Press [Enter] on a track to play, or [q] to add to queue.")
                    .style(Style::default().fg(DIM)),
                inner,
            );
            return;
        }

        let items: Vec<ListItem> = self.queue.iter().enumerate().map(|(i, id)| {
            let is_sel = i == self.queue_sel;
            let is_cur = i == self.queue_pos && self.current_track_id.is_some();
            let style = if is_cur {
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
            } else if is_sel {
                Style::default().fg(PRIMARY).bg(SEL_BG).add_modifier(Modifier::BOLD)
            } else { Style::default().fg(TEXT) };
            let track = self.track_by_id(id);
            let (title, artist, dur) = track.map(|t| (t.title.clone(), t.display_artist().to_string(), t.duration_str())).unwrap_or_default();
            let note = if is_cur { "♪ " } else if is_sel { "▸ " } else { "  " };
            ListItem::new(format!("{}{:>3}. {:<32} {:<24} {}", note, i + 1, title, artist, dur)).style(style)
        }).collect();
        frame.render_widget(List::new(items), inner);
    }

    fn render_now_playing(&self, frame: &mut Frame, area: Rect) {
        let audio = self.audio_info();

        if let Some(ref id) = self.current_track_id {
            if let Some(track) = self.track_by_id(id) {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(CYAN))
                    .title(" ♪ Now Playing ")
                    .title_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD));
                let inner = block.inner(area);
                frame.render_widget(block, area);

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),  // spacer
                        Constraint::Length(1),  // title
                        Constraint::Length(1),  // artist · album
                        Constraint::Length(1),  // genre · year · plays
                        Constraint::Length(1),  // spacer
                        Constraint::Length(1),  // visualizer
                        Constraint::Length(1),  // spacer
                        Constraint::Length(1),  // progress bar
                        Constraint::Length(1),  // controls
                        Constraint::Length(1),  // spacer
                        Constraint::Min(1),     // up next
                    ])
                    .split(inner);

                // Title
                let fav_icon = if track.is_favorite { "♥" } else { "♩" };
                frame.render_widget(
                    Paragraph::new(format!("  {} {}", fav_icon, track.title))
                        .style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD))
                        .alignment(Alignment::Left),
                    chunks[1],
                );

                // Artist · Album
                let album_year = if track.year > 0 { format!("{} ({})", track.display_album(), track.year) } else { track.display_album().to_string() };
                frame.render_widget(
                    Paragraph::new(format!("     {} · {}", track.display_artist(), album_year))
                        .style(Style::default().fg(MUTED)),
                    chunks[2],
                );

                // Meta
                let plays = if track.play_count == 1 { "1 play".to_string() } else { format!("{} plays", track.play_count) };
                frame.render_widget(
                    Paragraph::new(format!("     {} · {} · {} · {}", track.genre, track.size_str(), plays,
                        if track.track_number > 0 { format!("Track {}", track.track_number) } else { String::new() }))
                        .style(Style::default().fg(DIM)),
                    chunks[3],
                );

                // Visualizer
                let w = inner.width.saturating_sub(4) as usize;
                let viz = self.visualizer_line(audio.elapsed_secs, audio.is_playing, w);
                frame.render_widget(
                    Paragraph::new(format!("  {}", viz)).style(Style::default().fg(CYAN)),
                    chunks[5],
                );

                // Progress bar
                let bar_w = inner.width.saturating_sub(18) as usize;
                let bar = self.progress_bar(audio.elapsed_secs, self.track_duration_secs, bar_w);
                let elapsed_s = fmt_time(audio.elapsed_secs as u32);
                let total_s = fmt_time(self.track_duration_secs);
                frame.render_widget(
                    Paragraph::new(format!("  {} {} {}", elapsed_s, bar, total_s))
                        .style(Style::default().fg(TEXT)),
                    chunks[7],
                );

                // Controls
                let status_icon = if audio.is_paused { "▐▐ Paused" } else if audio.is_playing { "▶  Playing" } else { "■  Stopped" };
                let vol_pct = (self.volume * 100.0) as u32;
                let controls = format!(
                    "  {}  |  🔀 Shuffle: {}  {}  Repeat: {}  |  {} {}{}%  [Space] ▐▐  [n/N] ⏭⏮",
                    status_icon,
                    if self.shuffle { "ON " } else { "OFF" },
                    self.repeat.icon(), self.repeat.label(),
                    if self.is_muted { "🔇 " } else { "🔊 " },
                    self.vol_bar(10), vol_pct,
                );
                frame.render_widget(
                    Paragraph::new(controls).style(Style::default().fg(MUTED)),
                    chunks[8],
                );

                // Up Next
                let next_area = chunks[10];
                let mut next_lines: Vec<Line> = vec![
                    Line::from(Span::styled("  Up Next:", Style::default().fg(DIM))),
                ];
                for i in 1..=5 {
                    let next_pos = self.queue_pos + i;
                    if next_pos < self.queue.len() {
                        let nid = &self.queue[next_pos];
                        if let Some(nt) = self.track_by_id(nid) {
                            next_lines.push(Line::from(Span::styled(
                                format!("  {}. {} — {}", i, truncate(&nt.title, 32), truncate(nt.display_artist(), 20)),
                                Style::default().fg(if i == 1 { TEXT } else { DIM }),
                            )));
                        }
                    }
                }
                frame.render_widget(Paragraph::new(next_lines), next_area);

                return;
            }
        }

        // Nothing playing
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" Now Playing ")
            .title_style(Style::default().fg(PRIMARY));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("\n\n\n         ♪  No track playing\n\n         Go to Library [1] and press [Enter] to start playing.")
                .style(Style::default().fg(DIM))
                .alignment(Alignment::Center),
            inner,
        );
    }

    fn render_import(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ORANGE))
            .title(" Import Music ")
            .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), Constraint::Length(3), Constraint::Length(2),
                Constraint::Min(3),
            ])
            .split(inner);

        frame.render_widget(
            Paragraph::new("  Import a local audio file or folder of audio files into your library.\n  Supported: MP3, FLAC, WAV, OGG, M4A, AAC, OPUS, WMA")
                .style(Style::default().fg(MUTED)),
            chunks[0],
        );

        // Path input
        let path_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PRIMARY))
            .title(" Path (file or directory) ")
            .title_style(Style::default().fg(PRIMARY));
        let path_inner = path_block.inner(chunks[1]);
        frame.render_widget(path_block, chunks[1]);
        frame.render_widget(
            Paragraph::new(self.import_path.clone()).style(Style::default().fg(TEXT)),
            path_inner,
        );
        // Cursor
        let cx = path_inner.x + (self.import_cursor as u16).min(path_inner.width.saturating_sub(1));
        frame.set_cursor_position((cx, path_inner.y));

        frame.render_widget(
            Paragraph::new("  [Enter] import  [Esc] cancel").style(Style::default().fg(DIM)),
            chunks[2],
        );

        // Progress
        let (prog_text, prog_style) = match &self.import_progress {
            ImportProgress::Idle => (
                "  Ready. Enter a path above and press [Enter].".to_string(),
                Style::default().fg(DIM)
            ),
            ImportProgress::Scanning(msg) => (
                format!("  ⟳  {}…", msg),
                Style::default().fg(ORANGE)
            ),
            ImportProgress::Done(n, total) => (
                format!("  ✓  Imported {} new track(s) ({} file(s) found). Library updated.", n, total),
                Style::default().fg(GREEN)
            ),
            ImportProgress::Error(e) => (
                format!("  ✗  Error: {}", e),
                Style::default().fg(RED)
            ),
        };

        // Tips
        let tips = "\n  Examples:\n    C:\\Users\\Name\\Music\\song.mp3\n    C:\\Users\\Name\\Music\\           (scans entire folder)\n    /home/user/music/song.flac";
        frame.render_widget(
            Paragraph::new(format!("{}\n{}", prog_text, tips))
                .style(prog_style),
            chunks[3],
        );
    }

    fn render_search(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5)])
            .split(area);

        // Search bar
        let sb_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(CYAN))
            .title(format!(" Search ({} results) ", self.search_results.len()))
            .title_style(Style::default().fg(CYAN));
        let sb_inner = sb_block.inner(chunks[0]);
        frame.render_widget(sb_block, chunks[0]);
        frame.render_widget(
            Paragraph::new(self.search_query.clone()).style(Style::default().fg(TEXT)),
            sb_inner,
        );
        let cx = sb_inner.x + (self.search_cursor as u16).min(sb_inner.width.saturating_sub(1));
        frame.set_cursor_position((cx, sb_inner.y));

        // Results
        let res_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" Results ")
            .title_style(Style::default().fg(PRIMARY));
        let res_inner = res_block.inner(chunks[1]);
        frame.render_widget(res_block, chunks[1]);

        if self.search_results.is_empty() {
            let msg = if self.search_query.is_empty() {
                "  Start typing to search…"
            } else { "  No results found." };
            frame.render_widget(
                Paragraph::new(msg).style(Style::default().fg(DIM)),
                res_inner,
            );
        } else {
            let items: Vec<ListItem> = self.search_results.iter().enumerate().map(|(i, id)| {
                let is_sel = i == self.search_sel;
                let is_playing = self.current_track_id.as_deref() == Some(id.as_str());
                let style = if is_sel {
                    Style::default().fg(CYAN).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else if is_playing { Style::default().fg(GREEN) }
                else { Style::default().fg(TEXT) };
                let track = self.track_by_id(id);
                let (title, artist, album, dur) = track.map(|t| (t.title.clone(), t.display_artist().to_string(), t.display_album().to_string(), t.duration_str())).unwrap_or_default();
                let note = if is_playing { "♪" } else if is_sel { "▸" } else { " " };
                ListItem::new(format!("  {} {:<32} {:<22} {:<22} {}", note, title, artist, album, dur)).style(style)
            }).collect();
            frame.render_widget(List::new(items), res_inner);
        }
    }

    fn render_new_pl_dialog(&self, frame: &mut Frame, area: Rect) {
        let popup = center_popup(area, 50, 7);
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ORANGE))
            .title(" New Playlist ")
            .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        frame.render_widget(
            Paragraph::new(format!("\n  Playlist name:\n  > {}_\n\n  [Enter] create  [Esc] cancel", self.new_pl_name))
                .style(Style::default().fg(TEXT)),
            inner,
        );
    }

    fn render_add_to_pl_dialog(&self, frame: &mut Frame, area: Rect) {
        let popup = center_popup(area, 50, 15.min(self.playlists.len() + 6));
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ORANGE))
            .title(" Add to Playlist ")
            .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        if self.playlists.is_empty() {
            frame.render_widget(
                Paragraph::new("\n  No playlists yet.\n  Press [n] to create one first.")
                    .style(Style::default().fg(MUTED)),
                inner,
            );
        } else {
            let items: Vec<ListItem> = self.playlists.iter().enumerate().map(|(i, pl)| {
                let is_sel = i == self.add_to_pl_sel;
                let style = if is_sel {
                    Style::default().fg(ORANGE).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else { Style::default().fg(TEXT) };
                ListItem::new(format!("{} {}", if is_sel { "▸" } else { " " }, pl.name)).style(style)
            }).collect();
            frame.render_widget(List::new(items), inner);
        }
    }
}

// ── Audio thread ──────────────────────────────────────────────────────────────

fn spawn_audio_thread(rx: mpsc::Receiver<AudioCmd>, status: Arc<Mutex<AudioStatus>>) {
    std::thread::spawn(move || {
        run_audio(rx, status);
    });
}

fn run_audio(rx: mpsc::Receiver<AudioCmd>, status: Arc<Mutex<AudioStatus>>) {
    use rodio::{Decoder, OutputStream, Sink};
    use std::io::BufReader;
    use std::time::Instant;

    // Try to open audio output
    let audio_result = OutputStream::try_default();
    let (sink, _stream) = match audio_result {
        Ok((stream, handle)) => match Sink::try_new(&handle) {
            Ok(sink) => (Some(sink), Some(stream)),
            Err(_) => (None, None),
        },
        Err(_) => (None, None),
    };

    let audio_ok = sink.is_some();
    if let Ok(mut s) = status.lock() { s.audio_ok = audio_ok; }

    let mut play_start: Option<Instant> = None;
    let mut accumulated: f64 = 0.0;
    let mut was_playing = false;

    loop {
        // Drain command queue
        let mut quit = false;
        loop {
            match rx.try_recv() {
                Ok(AudioCmd::Quit) => { quit = true; break; }
                Ok(cmd) => {
                    if let Some(ref s) = sink {
                        match cmd {
                            AudioCmd::Play(path) => {
                                s.stop();
                                accumulated = 0.0;
                                play_start = None;
                                if let Ok(file) = std::fs::File::open(&path) {
                                    let buf = BufReader::new(file);
                                    if let Ok(dec) = Decoder::new(buf) {
                                        s.append(dec);
                                        s.play();
                                        play_start = Some(Instant::now());
                                        was_playing = true;
                                    }
                                }
                            }
                            AudioCmd::Pause => {
                                if !s.is_paused() {
                                    if let Some(start) = play_start.take() {
                                        accumulated += start.elapsed().as_secs_f64();
                                    }
                                    s.pause();
                                }
                            }
                            AudioCmd::Resume => {
                                if s.is_paused() {
                                    play_start = Some(Instant::now());
                                    s.play();
                                }
                            }
                            AudioCmd::TogglePause => {
                                if s.is_paused() {
                                    play_start = Some(Instant::now());
                                    s.play();
                                } else {
                                    if let Some(start) = play_start.take() {
                                        accumulated += start.elapsed().as_secs_f64();
                                    }
                                    s.pause();
                                }
                            }
                            AudioCmd::Stop => {
                                s.stop();
                                play_start = None;
                                accumulated = 0.0;
                                was_playing = false;
                            }
                            AudioCmd::SetVolume(v) => { s.set_volume(v); }
                            AudioCmd::Quit => { quit = true; break; }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => { quit = true; break; }
            }
        }
        if quit { break; }

        // Compute status
        let (is_playing, is_paused, elapsed, finished) = if let Some(ref s) = sink {
            let paused = s.is_paused();
            let empty = s.empty();
            let playing = !paused && !empty;
            let fin = was_playing && empty && !paused;
            if fin { was_playing = false; play_start = None; accumulated = 0.0; }
            let el = if paused {
                accumulated
            } else if let Some(start) = play_start {
                accumulated + start.elapsed().as_secs_f64()
            } else { 0.0 };
            (playing, paused, el, fin)
        } else { (false, false, 0.0, false) };

        if let Ok(mut s) = status.lock() {
            s.is_playing = is_playing;
            s.is_paused = is_paused;
            s.elapsed_secs = elapsed;
            if finished { s.track_finished = true; }
            s.audio_ok = audio_ok;
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    if let Some(ref s) = sink { s.stop(); }
}

// ── Metadata reading via lofty ────────────────────────────────────────────────

fn read_file_metadata(path: &std::path::Path) -> MediaTrack {
    use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};

    let path_str = path.to_string_lossy().to_string();
    let filename = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let mut title = String::new();
    let mut artist = String::new();
    let mut album = String::new();
    let album_artist = String::new();
    let mut genre = String::new();
    let mut year = 0u32;
    let mut track_number = 0u32;
    let mut duration_secs = 0u32;

    if let Ok(tagged_file) = lofty::read_from_path(path) {
        duration_secs = tagged_file.properties().duration().as_secs() as u32;
        let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());
        if let Some(t) = tag {
            title        = t.title().as_deref().unwrap_or("").to_string();
            artist       = t.artist().as_deref().unwrap_or("").to_string();
            album        = t.album().as_deref().unwrap_or("").to_string();
            genre        = t.genre().as_deref().unwrap_or("").to_string();
            year         = t.year().unwrap_or(0);
            track_number = t.track().unwrap_or(0);
        }
    }

    // Fallback: parse filename as "Artist - Title" or just use filename
    if title.is_empty() {
        if let Some((a, t)) = filename.split_once(" - ") {
            if artist.is_empty() { artist = a.trim().to_string(); }
            title = t.trim().to_string();
        } else {
            title = filename;
        }
    }

    MediaTrack {
        id: gen_id(),
        title,
        artist,
        album,
        album_artist,
        genre,
        year,
        track_number,
        duration_secs,
        path: path_str,
        file_size_bytes: file_size,
        added_at: Utc::now().to_rfc3339(),
        play_count: 0,
        is_favorite: false,
    }
}

// ── File scanning utilities ───────────────────────────────────────────────────

fn is_audio_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str(),
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" | "wma" | "aiff" | "alac"
    )
}

fn collect_audio_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut sorted: Vec<_> = entries.flatten().collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            let path = entry.path();
            if path.is_dir() {
                collect_audio_files(&path, files);
            } else if is_audio_file(&path) {
                files.push(path);
            }
        }
    }
}

// ── General utilities ─────────────────────────────────────────────────────────

fn gen_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    // Mix with a thread-local counter for uniqueness within the same nanosecond
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let c = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{:016x}{:08x}", t as u64 ^ (c << 32), c)
}

fn fmt_time(secs: u32) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{:02}:{:02}", m, s)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else { format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>()) }
}

/// Generate a consistent color for an album based on its name.
fn album_color(name: &str) -> Color {
    let hash: u32 = name.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let colors = [PRIMARY, CYAN, MAGENTA, GREEN, ORANGE, Color::Rgb(255, 150, 150), Color::Rgb(150, 255, 200)];
    colors[(hash as usize) % colors.len()]
}

fn center_popup(area: Rect, width: u16, height: usize) -> Rect {
    let h = height as u16;
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect { x, y, width: width.min(area.width), height: h.min(area.height) }
}
