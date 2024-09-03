pub struct Media {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub playbackstatus: Option<String>,
}

impl Media {
    /// Construct a new instance of media output
    pub fn new(
        artist: Option<String>,
        title: Option<String>,
        playbackstatus: Option<String>,
    ) -> Self {
        Media {
            artist,
            title,
            playbackstatus,
        }
    }

    /// Send the media output to Waybar
    pub fn send(&self, output_format: &str) {
        // All fields must be some
        if let Self {
            artist: Some(artist),
            title: Some(title),
            playbackstatus: Some(playbackstatus),
        } = self
        {
            // Construct the output from user defined format and escape ampersands
            let now_playing = output_format
                .replace("{{artist}}", artist)
                .replace("{{title}}", title)
                .replace('&', "&amp;");

            // Print the output in JSON format to be parsed by Waybar
            println!(
                r#"{{"text": "{}", "alt": "{}", "class": "{}"}}"#,
                now_playing, playbackstatus, playbackstatus
            );
        }
    }
}
