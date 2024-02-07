use std::fs;
use std::fs::DirEntry;
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{
    Button, Label, Response, ScrollArea, Slider, SliderOrientation, ViewportCommand, Visuals,
    Widget,
};
use eframe::epaint::FontFamily;
use eframe::glow::Context;
use eframe::{egui, Storage};
use n_audio::queue::QueuePlayer;
use n_audio::{from_path_to_name_without_ext, TrackTime};

use crate::{add_all_tracks_to_player, vec_contains, FileTrack, FileTracks};

pub struct App {
    path: Option<String>,
    player: QueuePlayer<String>,
    volume: f32,
    time: f64,
    cached_track_time: Option<TrackTime>,
    files: FileTracks,
    title: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::configure_fonts(&cc.egui_ctx);

        let mut player: QueuePlayer<String> = QueuePlayer::new();

        let mut files = FileTracks { tracks: vec![] };
        let mut saved_files = FileTracks { tracks: vec![] };
        let mut volume = 1.0;

        let mut maybe_path = None;

        if let Some(storage) = cc.storage {
            if let Some(data) = storage.get_string("durations") {
                if let Ok(read_data) = toml::from_str(&data) {
                    saved_files = read_data;
                }
            }
            if let Some(data_v) = storage.get_string("volume") {
                volume = data_v.parse().unwrap_or(1.0);
            }

            if let Some(path) = storage.get_string("path") {
                add_all_tracks_to_player(&mut player, path.clone());
                maybe_path = Some(path);
            }
        }

        player.set_volume(volume).unwrap();

        if let Some(path) = &maybe_path {
            Self::init(
                PathBuf::new().join(path),
                &mut player,
                &mut files,
                &saved_files,
            );
        }

        Self {
            path: maybe_path,
            player,
            volume,
            time: 0.0,
            cached_track_time: None,
            files,
            title: String::from("N Music"),
        }
    }

    fn slider_seek(&mut self, slider: Response, track_time: Option<TrackTime>) {
        if let Some(track_time) = track_time {
            if slider.changed() {
                self.player.pause().unwrap();
                let total_time = track_time.dur_secs as f64 + track_time.dur_frac;
                let seek_time = total_time * self.time;
                self.player
                    .seek_to(seek_time.floor() as u64, seek_time.fract())
                    .unwrap();
                self.player.unpause().unwrap();
            }
        }
    }

    pub fn configure_fonts(ctx: &egui::Context) -> Option<()> {
        let font_file = Self::find_cjk_font()?;
        let font_name = font_file.split('/').last()?.split('.').next()?.to_string();
        let font_file_bytes = fs::read(font_file).ok()?;

        let font_data = egui::FontData::from_owned(font_file_bytes);
        let mut font_def = eframe::egui::FontDefinitions::default();
        font_def.font_data.insert(font_name.to_string(), font_data);

        font_def
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push(font_name);

        ctx.set_fonts(font_def);
        Some(())
    }

    fn find_cjk_font() -> Option<String> {
        #[cfg(unix)]
        {
            use std::process::Command;
            // linux/macOS command: fc-list
            let output = Command::new("sh").arg("-c").arg("fc-list").output().ok()?;
            let stdout = std::str::from_utf8(&output.stdout).ok()?;
            #[cfg(target_os = "macos")]
            let font_line = stdout
                .lines()
                .find(|line| line.contains("Regular") && line.contains("Hiragino Sans GB"))
                .unwrap_or("/System/Library/Fonts/Hiragino Sans GB.ttc");
            #[cfg(target_os = "linux")]
            let font_line = stdout
                .lines()
                .find(|line| line.contains("Regular") && line.contains("CJK"))
                .unwrap_or("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc");

            let font_path = font_line.split(':').next()?.trim();
            Some(font_path.to_string())
        }
        #[cfg(windows)]
        {
            let font_file = {
                // c:/Windows/Fonts/msyh.ttc
                let mut font_path = PathBuf::from(std::env::var("SystemRoot").ok()?);
                font_path.push("Fonts");
                font_path.push("msyh.ttc");
                font_path.to_str()?.to_string().replace("\\", "/")
            };
            Some(font_file)
        }
    }

    fn finish_init(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Add music folder");

            if ui.button("Open folder…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    let saved = FileTracks { tracks: vec![] };
                    ui.label("Loading...");
                    Self::init(path.clone(), &mut self.player, &mut self.files, &saved);
                    self.path = Some(path.to_str().unwrap().to_string());
                }
            }
        });
    }

    fn init(
        path: PathBuf,
        player: &mut QueuePlayer<String>,
        files: &mut FileTracks,
        saved_files: &FileTracks,
    ) {
        let paths = fs::read_dir(&path).expect("Can't read files in the chosen directory");
        let entries: Vec<DirEntry> = paths.filter_map(|item| item.ok()).collect();

        add_all_tracks_to_player(player, path.to_str().unwrap().to_string());

        for entry in &entries {
            if entry.metadata().unwrap().is_file()
                && infer::get_from_path(entry.path())
                    .unwrap()
                    .unwrap()
                    .mime_type()
                    .contains("audio")
            {
                let name = from_path_to_name_without_ext(&entry.path());
                let contains = vec_contains(saved_files, &name);
                let duration = if contains.0 {
                    saved_files[contains.1].duration
                } else {
                    player
                        .get_duration_for_track(player.get_index_from_track_name(&name).unwrap())
                        .dur_secs
                };
                files.push(FileTrack { name, duration });
            }
        }

        files.sort();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::dark());

        if self.path.is_none() {
            self.finish_init(ctx);
            return;
        }

        if self.player.has_ended() {
            self.player.play_next();

            self.title = format!("N Music - {}", self.player.current_track_name());
            ctx.send_viewport_cmd(ViewportCommand::Title(self.title.clone()));
        }

        egui::TopBottomPanel::bottom("control_panel").show(ctx, |ui| {
            ui.set_min_height(40.0);

            let track_time = self.player.get_time();

            ui.horizontal(|ui| {
                let slider = Slider::new(&mut self.time, 0.0..=1.0)
                    .orientation(SliderOrientation::Horizontal)
                    .show_value(false)
                    .ui(ui);
                ui.add_space(10.0);

                let volume_slider = Slider::new(&mut self.volume, 0.0..=1.0)
                    .show_value(false)
                    .ui(ui);

                self.slider_seek(slider, track_time.clone());

                if volume_slider.changed() {
                    self.player.set_volume(self.volume).unwrap();
                }
            });

            self.time = if let Some(track_time) = &track_time {
                let value = (track_time.ts_secs as f64 + track_time.ts_frac)
                    / (track_time.dur_secs as f64 + track_time.dur_frac);
                self.cached_track_time = Some(track_time.clone());
                value
            } else if let Some(track_time) = &self.cached_track_time {
                (track_time.ts_secs as f64 + track_time.ts_frac)
                    / (track_time.dur_secs as f64 + track_time.dur_frac)
            } else {
                0.0
            };

            ui.horizontal(|ui| {
                ScrollArea::horizontal().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;

                    ui.label(self.player.current_track_name());
                    ui.add_space(10.0);

                    let text_toggle = if !self.player.is_playing() || self.player.is_paused() {
                        "▶"
                    } else {
                        "⏸"
                    };

                    let previous = Button::new("⏮").frame(false).ui(ui);
                    let toggle = Button::new(text_toggle).frame(false).ui(ui);
                    let next = Button::new("⏭").frame(false).ui(ui);

                    if previous.clicked() {
                        if let Some(cached_track_time) = &self.cached_track_time {
                            if cached_track_time.ts_secs < 2 {
                                self.player.seek_to(0, 0.0).unwrap();
                            } else {
                                self.player.end_current().unwrap();
                                self.player.play_previous();

                                self.title =
                                    format!("N Music - {}", self.player.current_track_name());
                                ctx.send_viewport_cmd(ViewportCommand::Title(self.title.clone()));
                            }
                        }
                    }
                    if toggle.clicked() {
                        if self.player.is_paused() {
                            self.player.unpause().unwrap();
                        } else {
                            self.player.pause().unwrap();
                        }
                        if !self.player.is_playing() {
                            self.player.play_next();

                            self.title = format!("N Music - {}", self.player.current_track_name());
                            ctx.send_viewport_cmd(ViewportCommand::Title(self.title.clone()));
                        }
                    }
                    if next.clicked() {
                        self.player.end_current().unwrap();
                        self.player.play_next();

                        self.title = format!("N Music - {}", self.player.current_track_name());
                        ctx.send_viewport_cmd(ViewportCommand::Title(self.title.clone()));
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::warn_if_debug_build(ui);
            ScrollArea::vertical().show(ui, |ui| {
                // TODO: implement culling (maybe using ui.is_rect_visible()); total height is 20
                for track in self.files.iter() {
                    let name = &track.name;
                    let duration = &track.duration;
                    ui.horizontal(|ui| {
                        let mut frame = false;
                        if self.player.is_playing() && &self.player.current_track_name() == name {
                            ui.add_space(10.0);
                            frame = true;
                        }
                        let button = Button::new(name).frame(frame).ui(ui);
                        ui.add(Label::new(format!(
                            "{:02}:{:02}",
                            duration / 60,
                            duration % 60
                        )));

                        if button.clicked() {
                            let index = self.player.get_index_from_track_name(name).unwrap();
                            self.player.end_current().unwrap();
                            self.player.play(index);

                            self.title = format!("N Music - {}", self.player.current_track_name());
                            ctx.send_viewport_cmd(ViewportCommand::Title(self.title.clone()));
                        }
                    });
                }
                ui.allocate_space(ui.available_size());
            });
        });

        if !self.player.is_paused() {
            ctx.request_repaint_after(Duration::from_millis(750));
        }
    }

    fn save(&mut self, storage: &mut dyn Storage) {
        storage.set_string("durations", toml::to_string(&self.files).unwrap());
        storage.set_string("volume", self.volume.to_string());
        if let Some(path) = &self.path {
            storage.set_string("path", path.to_string());
        }
    }

    fn on_exit(&mut self, _gl: Option<&Context>) {
        self.player.end_current().unwrap();
    }
}
