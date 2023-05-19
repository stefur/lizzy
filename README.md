# Lystra

Lystra is a simple and small app that lets [Waybar](https://github.com/Alexays/Waybar) display what you're currently listening to by using DBus signals.
  
## Features
- Customizable output format, including colors (see Usage below)
- Support for any [MPRIS](https://wiki.archlinux.org/title/MPRIS) mediaplayer of preference
- Automatic pause/resume when other media content begins/stops playing (ex. YouTube videos)
- Clearing of output when mediaplayer is closed
- No constant polling, Lystra only updates when a signal is received
  
Some examples of its output here:  
<p align="center">
    <img src="assets/examples.png" alt="Examples">
</p>

## Installation from source
1. Make sure you've got Rust installed. Either via your distributions package manager or [`rustup`](https://rustup.rs/).
2. `cargo install --git https://github.com/stefur/lystra lystra`

## Configure Waybar
Add a custom module to your Waybar config:  
```
"custom/spotify": {
    "exec": "cat /tmp/lystra-output",
    "interval": "once",
    "signal": 8
}
```  
Don't forget to add the module to your bar!

Then run `lystra`, and preferably add `lystra` to whatever autostart method you're using.

## Usage
Currently the following options can be used to customize the output of Lystra.

| Flag | Default value | Description |
| --- | --- | --- |
| `--length` | 45 | Max length of the output before truncating (adds … to output). |
| `--signal` | 8 | Set a custom signal number used to update Waybar. |
| `--playing` | "Playing: " | Indicator used when a song is playing. |
| `--paused` | "Paused: " | Indicator used when a song is paused. |
| `--separator` | " - " | Separator between song artist and title. |
| `--order` | "artist,title" | The order of artist and title, comma-separated. |
| `--playbackcolor` | None | Text color for playback status. |
| `--textcolor` | None | Text color for artist and title. |
| `--mediaplayer`| None | Mediaplayer interface that Lystra should listen to. Usually the name of the mediaplayer. Blank means listening to all mediaplayers. |
| `--autotoggle` | False | Include this flag to automatically pause/resume the mediaplayer if other media content playing is detected (for example a YouTube video) |

## Example
`lystra --playing "契 " --paused " " --playbackcolor "#9CABCA" --separator ": " --order "title,artist" --mediaplayer "spotify" --autotoggle`
