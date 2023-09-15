use crate::message::Media;

pub struct BarOutput {
    pub now_playing: String,
    pub playbackstatus: String,
}

impl BarOutput {
    /// Create the output according to defined format
    pub fn new(media: Media, output_format: &str) -> BarOutput {
        let media_artist = media.artist;
        let media_title = media.title;

        let now_playing = output_format
            .replace("{{artist}}", &media_artist)
            .replace("{{title}}", &media_title);

        BarOutput {
            playbackstatus: media.playbackstatus,
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
