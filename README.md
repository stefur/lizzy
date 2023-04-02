# Lystra

Lystra is a simple and small app that lets [Waybar](https://github.com/Alexays/Waybar) display what is currently playing on Spotify. 

Simple examples of its output here:  
![](assets/preview1.png)  
![](assets/preview2.png)

Lystra listens to DBus signals emitted by Spotify.

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
| `--length` | 40 | Set the length of the string before truncating the text (and adds …). |
| `--playing` | "Playing: " | Set your own indicator for when a song is playing. |
| `--paused` | "Paused: " | Set your own indicator for when a song is paused. |
| `--separator` | " - " | Separator for *artist* and *title* in the output. |
| `--order` | "artist,title" | Comma-separated setting with the keywords *artist* and *title* to set a desired order of the output. |
| `--playbackcolor` | None | Optional color setting for the playback status indicator. |
| `--textcolor` | None | Optional color setting for the artist and title text. |

## Todo
- Better and more examples of usage.
- Make a release
- Consider combining playing/paused args
- Consider json format for configuration