# Lystra

Lystra is a simple and small app that lets [Waybar](https://github.com/Alexays/Waybar) display what is currently playing on Spotify. 

Simple examples of its output here:  
![](assets/preview1.png)  
![](assets/preview2.png)

This is done by listening to DBus signals emitted by Spotify. The update signals trigger Lystra to print the the currently playing artist and song to `/tmp/lystra_output.txt`. Lystra then sends a signal (SIGRTMIN+8) to let Waybar know it's time to update its output. 
The custom module in Waybar uses a simple `cat` command to read the contents of the file created by Lystra.

The main goal of this application is for me to explore and learn a bit of Rust, thus the code will be far from perfect. Feel free to give me feedback on it.

## Installation from source
1. Make sure you've got Rust installed. Either via your distributions package manager or [`rustup`](https://rustup.rs/).
2. Clone this repository.
3. `cd` into `lystra` and run `cargo build --release` to build.
4. The resulting binary can be found in `lystra/target/release/`.

## Configure Waybar
1. Add the following custom module to your Waybar config:
    ```
    "custom/spotify": {
        "interval": "once",
        "exec": "cat /tmp/lystra_output.txt"
        "signal": 8
    }
    ``` 
    Don't forget to add the module to your bar!

3. Add `lystra` to whatever autostart method you're using.
4. Listen to music!

## Configuration
Currently the following flag can be used:
- `-l`, `--length` is used to set the length of the string before truncating the text (adding …). Default: 40

## Todo
- More user configuration options of output (custom status indicator, separator, order of output, signal to use)
- Send signals via std to update Waybar
- Simplify the property fetching by sending messages and awaiting replies
- Clear output on quit/termination
- Better and more examples
- Make a release?