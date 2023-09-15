use crate::message::Song;

pub struct BarOutput {
    pub now_playing: String,
    pub playbackstatus: String,
}

impl BarOutput {
    /// Create the output according to defined format
    pub fn new(song: Song, output_format: &str) -> BarOutput {
        let song_artist = song.artist;
        let song_title = song.title;

        let now_playing = output_format
            .replace("{{artist}}", &song_artist)
            .replace("{{title}}", &song_title);

        BarOutput {
            playbackstatus: song.playbackstatus,
            now_playing,
        }
    }

    /// Waybar doesn't like ampersand. So we replace them in the output string.
    pub fn escape_ampersand(&mut self) -> &mut Self {
        self.now_playing = self.now_playing.replace('&', "&amp;");
        self
    }

    /// Print the output to Waybar
    pub fn send(&self) {
        println!(
            r#"{{"text": "{}", "alt": "{}", "class": "{}"}}"#,
            self.now_playing, self.playbackstatus, self.playbackstatus
        );
    }
}
