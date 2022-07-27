# Lystra

Lystra is a simple and small app that lets [Waybar](https://github.com/Alexays/Waybar) display what is currently playing on Spotify. 

Simple examples of its output here:  
![](assets/preview1.png)  
![](assets/preview2.png)

Lystra listens to DBus signals emitted by Spotify. A signal will trigger Lystra to print the the currently playing artist and song to `/tmp/lystra_output.txt`. Lystra then sends a signal to let Waybar know it's time to update its output. 
The custom module in Waybar uses a simple `cat` command to read the contents of the file created by Lystra.

The main goal of this application is for me to explore and learn a bit of Rust, thus the code will be far from perfect. Feel free to give me feedback on it.

## Installation from source
1. Make sure you've got Rust installed. Either via your distributions package manager or [`rustup`](https://rustup.rs/).
2. `cargo install --git https://github.com/stefur/lystra lystra`

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

3. Run `lystra`, and preferably add `lystra` to whatever autostart method you're using.
4. Listen to music!

## Usage
Currently the following flags can be used:
- `-l`, `--length` is used to set the length of the string before truncating the text (adding …). Default: 40
- `-s`, `--signal` to set a custom signal number used to update Waybar. Default: 8

## Todo
- More user configuration options of output (custom status indicator, separator, order of output)
- Simplify the property fetching by sending messages and awaiting replies
- Clear output on quit/termination
- Better and more examples
- Make a release?