# Lystra

Lystra is a simple and small app that lets [Waybar](https://github.com/Alexays/Waybar) display what you're currently listening to by using DBus signals.
  
Lystra supports any mediaplayer supporting [MPRIS](https://wiki.archlinux.org/title/MPRIS).
  
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

## Example
`lystra --playing "契 " --paused " " --playbackcolor "#9CABCA" --separator ": " --order "title,artist" --mediaplayer "spotify"`

## Todo
- ~~Better and more examples of usage~~.
- ~~Make a release~~.
- ~~Look into supporting more interfaces (media players)~~.
- Feature: autopause/resume when other player begins playing.