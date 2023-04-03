# Lystra

Lystra is a simple and small app that lets [Waybar](https://github.com/Alexays/Waybar) display what is currently playing on Spotify. Lystra listens to DBus signals emitted by Spotify.

Simple examples of its output here:  
![](assets/preview1.png)  
![](assets/preview2.png)

## Installation from source
1. Make sure you've got Rust installed. Either via your distributions package manager or [`rustup`](https://rustup.rs/).
2. `cargo install --git https://github.com/stefur/lystra lystra`

## Configure Waybar
Add a custom module to your Waybar config:  
```
"custom/spotify": {
    "exec": "lystra <OPTIONS HERE>"
}
```  
Don't forget to add the module to your bar!

## Usage
Currently the following options can be used to customize the output of Lystra.

| Flag | Default value | Description |
| --- | --- | --- |
| `--length` | 45 | Max length of the output before truncating (adds … to output). |
| `--playing` | "Playing: " | Indicator used when a song is playing. |
| `--paused` | "Paused: " | Indicator used when a song is paused. |
| `--separator` | " - " | Separator between song artist and title. |
| `--order` | "artist,title" | The order of artist and title, comma-separated. |
| `--playbackcolor` | None | Text color for playback status.. |
| `--textcolor` | None | Text color for artist and title. |

## Todo
- Better and more examples of usage.
- Make a release.
- Look into more interfaces (media players)
- Consider combining playing/paused args