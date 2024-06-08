pub struct Media {
    pub artist: String,
    pub title: String,
    pub playbackstatus: String,
}

impl Media {
    // Construct a new instance of media output
    pub fn new(artist: String, title: String, playbackstatus: String) -> Self {
        Media {
            artist,
            title,
            playbackstatus,
        }
    }

    /// Send the media output to Waybar
    pub fn send(&self, output_format: &str) {
        // Construct the output from user defined format and escape ampersands
        let now_playing = output_format
            .replace("{{artist}}", &self.artist)
            .replace("{{title}}", &self.title)
            .replace('&', "&amp;");

        // Print the output in JSON format to be parsed by Waybar
        println!(
            r#"{{"text": "{}", "alt": "{}", "class": "{}"}}"#,
            now_playing, self.playbackstatus, self.playbackstatus
        );
    }
}
