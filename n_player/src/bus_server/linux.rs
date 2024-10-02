use crate::bus_server::{BusServer, Property};
use crate::runner::{Runner, RunnerMessage};
use crate::{get_image, runner};
use flume::Sender;
use mpris_server::zbus::fdo;
use mpris_server::zbus::zvariant::ObjectPath;
use mpris_server::{
    zbus, LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, RootInterface,
    Server, Time, TrackId, Volume,
};
use n_audio::music_track::MusicTrack;
use n_audio::remove_ext;
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;

impl<T: PlayerInterface + 'static> BusServer for Server<T> {
    async fn properties_changed<P: IntoIterator<Item = Property>>(
        &self,
        properties: P,
    ) -> Result<(), String> {
        Server::properties_changed(
            self,
            properties.into_iter().map(|p| match p {
                Property::Playing(playing) => mpris_server::Property::PlaybackStatus(if playing {
                    PlaybackStatus::Playing
                } else {
                    PlaybackStatus::Paused
                }),
                Property::Metadata(metadata) => {
                    let mut meta = Metadata::new();

                    meta.set_title(metadata.title);
                    meta.set_artist(metadata.artists);
                    meta.set_length(Some(Time::from_secs(metadata.length as i64)));
                    meta.set_art_url(metadata.image_path);
                    meta.set_trackid(Some(ObjectPath::from_string_unchecked(metadata.id)));

                    mpris_server::Property::Metadata(meta)
                }
                Property::Volume(volume) => mpris_server::Property::Volume(volume),
            }),
        )
        .await
        .map_err(|e| e.to_string())
    }
}

pub struct MPRISBridge {
    runner: Arc<RwLock<Runner>>,
    tx: Sender<RunnerMessage>,
}

impl MPRISBridge {
    pub fn new(runner: Arc<RwLock<Runner>>, tx: Sender<RunnerMessage>) -> Self {
        Self { runner, tx }
    }
}

impl RootInterface for MPRISBridge {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn set_fullscreen(&self, _fullscreen: bool) -> zbus::Result<()> {
        Ok(())
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok(String::from("N Music"))
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Err(fdo::Error::NotSupported(String::from("no entry found")))
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
}

impl PlayerInterface for MPRISBridge {
    async fn next(&self) -> fdo::Result<()> {
        self.tx.send_async(RunnerMessage::PlayNext).await.unwrap();
        Ok(())
    }

    async fn previous(&self) -> fdo::Result<()> {
        self.tx
            .send_async(RunnerMessage::PlayPrevious)
            .await
            .unwrap();
        Ok(())
    }

    async fn pause(&self) -> fdo::Result<()> {
        self.tx.send_async(RunnerMessage::Pause).await.unwrap();
        Ok(())
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        self.tx
            .send_async(RunnerMessage::TogglePause)
            .await
            .unwrap();
        Ok(())
    }

    async fn stop(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn play(&self) -> fdo::Result<()> {
        self.tx.send_async(RunnerMessage::Play).await.unwrap();
        Ok(())
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        self.tx
            .send_async(RunnerMessage::Seek(runner::Seek::Relative(
                offset.as_secs() as f64 + (offset.as_millis() as f64 / 1000.0),
            )))
            .await
            .unwrap();
        Ok(())
    }

    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        self.tx
            .send_async(RunnerMessage::Seek(runner::Seek::Absolute(
                position.as_millis() as f64 / 1000.0,
            )))
            .await
            .unwrap();
        Ok(())
    }

    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        let playback = self.runner.read().await.playback();
        if playback {
            Ok(PlaybackStatus::Playing)
        } else {
            Ok(PlaybackStatus::Paused)
        }
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::Playlist)
    }

    async fn set_loop_status(&self, _loop_status: LoopStatus) -> zbus::Result<()> {
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn set_rate(&self, _rate: PlaybackRate) -> zbus::Result<()> {
        Ok(())
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn set_shuffle(&self, _shuffle: bool) -> zbus::Result<()> {
        Ok(())
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let mut index = self.runner.read().await.index();
        let path = self.runner.read().await.path();
        let queue = self.runner.read().await.queue();
        if index > queue.len() {
            index = 0;
        }
        let track_name = &queue[index];
        let mut path_buf = PathBuf::new();
        path_buf.push(&path);
        path_buf.push(track_name);
        let track = MusicTrack::new(path_buf.to_str().unwrap())
            .expect("can't get track for currently playing song");
        let meta = track.get_meta();
        let image = get_image(path_buf);
        let mut tmp = NamedTempFile::new().expect("can't create tmp file for mpris bridge");
        let image_path = if image.is_empty() {
            None
        } else {
            tmp.rewind().expect("can't rewind tmp file");
            tmp.write_all(&image)
                .expect("can't write image data to tmp file");
            Some(tmp.path().to_str().unwrap().to_string())
        };

        let mut metadata = Metadata::new();
        metadata.set_title(Some(if !meta.title.is_empty() {
            meta.title
        } else {
            remove_ext(track_name)
        }));
        metadata.set_artist(if meta.artist.is_empty() {
            None
        } else {
            Some(vec![meta.artist])
        });
        metadata.set_length(Some(Time::from_secs(meta.time.len_secs as i64)));
        metadata.set_trackid(Some(ObjectPath::from_static_str_unchecked("/n_music")));
        metadata.set_art_url(image_path);

        Ok(metadata)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        let volume = self.runner.read().await.volume();
        Ok(volume)
    }

    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        self.tx
            .send_async(RunnerMessage::SetVolume(volume))
            .await
            .unwrap();
        Ok(())
    }

    async fn position(&self) -> fdo::Result<Time> {
        let track_time = self.runner.read().await.time();
        Ok(Time::from_secs(track_time.pos_secs as i64)
            + Time::from_millis((track_time.pos_frac * 1000.0) as i64))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}