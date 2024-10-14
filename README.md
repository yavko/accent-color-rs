# accent-color-rs
Library to get and listen for changes to the OS accent color


## Windows Implementation (Only one tested to work)
This implementation uses the registry to get the
accent color, and then listen for changes to the
registry value, I'm not sure if this is the best
way of doing this, if there is any Windows API
expert that can make my code more safe, and implement
it better, then please do

## Linux Support
This only works on modern Desktop Environments that
implement the latest `xdg-desktop-portal` api for
accent colors, the only ones at the moment are KDE
pantheon (the elementaryOS DE), but the next
release of GNOME should have it too

## macOS implementation
I'm having trouble getting the `Stream` implementation
for macOS working, so for now I will leave my broken
implementation

## API
This library provides mainly 2 functions, a Color struct, and also a unified Error type + Result alias
- `async fn get_accent_color() -> Result<Color>` - just gets the current accent color
- `async fn accent_color_stream() -> Result<impl Stream<Item = Color>>` - creates a `Stream` that returns accent colors when a change is detected

## Example Usage
just the example from `examples/get_accent_color` that
gets the initial color and prints new accent colors
```rust, no_run
#[tokio::main]
async fn main() -> AccentResult<()> {
    let Color(r, g, b) = get_accent_color().await?;
    println!("first: rgb({r},{g},{b})");
    let mut stream = pin!(accent_color_stream().await?);
    while let Some(Color(r, g, b)) = stream.next().await {
        println!("after: rgb({r},{g},{b})");
    }
    Ok(())
}
```

## Similar Crates
<https://lib.rs/crates/dark-light> for dark/light mode detection
<https://github.com/bash/mundy> this crate but seems to be better lol

## Credits
> I couldn't have made this without these, because my
> knowledge when it comes to non Linux OS APIs is very limited

- A lot of the windows code comes from https://github.com/WLBF/reg-watcher/
- WIP macOS impl is based off of https://github.com/freethinkel/tauri-plugin-accent-color/
