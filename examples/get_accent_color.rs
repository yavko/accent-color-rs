use std::pin::pin;

use accent_color::{accent_color_stream, get_accent_color, AccentResult};
use palette::IntoColor;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> AccentResult<()> {
    let color = get_accent_color().await?.into_color();
    println!("first: {color:?}");
    let mut stream = pin!(accent_color_stream().await?);
    while let Some(color) = stream.next().await {
        println!("after: {:?}", color.into_color());
    }
    Ok(())
}
