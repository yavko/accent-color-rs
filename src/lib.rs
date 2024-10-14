use palette::{IntoColor, Srgb};
use thiserror::Error;
use tokio_stream::{Stream, StreamExt};

#[derive(Error, Debug)]
pub enum AccentError {
    #[cfg(target_os = "linux")]
    #[error("an error occured with XdgDesktopPortals: `{0}`")]
    AshpdError(#[from] ashpd::Error),
    #[cfg(target_os = "windows")]
    #[error("an error occured getting the windows registry key: `{0}`")]
    WindowsError(#[from] windows_result::Error),
    #[cfg(target_os = "windows")]
    #[error("an error occured while creating the thread to await an accent change")]
    JoinError(#[from] tokio::task::JoinError),
    #[cfg(target_os = "macos")]
    #[error("an error occured while awaiting getting the accent color")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("An error occured: {0}")]
    StringError(String),
}
#[cfg(target_os = "windows")]
macro_rules! str_err {
    ($str:literal) => {
        $crate::AccentError::StringError($str.to_string())
    };
    ($str:literal, $($exp:expr),*) => {
        $crate::AccentError::StringError(format!($str, $($exp),*))
    };
}

pub type AccentResult<T> = Result<T, AccentError>;

#[cfg(target_os = "linux")]
pub async fn get_accent_color() -> AccentResult<impl IntoColor<Srgb<f64>>> {
    use ashpd::desktop::settings::Settings;
    let color = Settings::new().await?.accent_color().await?;
    Ok(convert_color(color))
}

#[cfg(target_os = "linux")]
fn convert_color(color: ashpd::desktop::Color) -> Srgb<f64> {
    Srgb::<f64>::new(color.red(), color.green(), color.blue())
}

#[cfg(target_os = "linux")]
pub async fn accent_color_stream(
) -> AccentResult<impl Stream<Item = impl IntoColor<Srgb<f64>>> + Send> {
    use ashpd::desktop::settings::Settings;
    Ok(Settings::new()
        .await?
        .receive_accent_color_changed()
        .await?
        .map(|x| convert_color(x)))
}

#[cfg(target_os = "windows")]
pub async fn get_accent_color() -> AccentResult<impl IntoColor<Srgb<f64>>> {
    use windows_registry::CURRENT_USER;
    let key = CURRENT_USER.open("Software\\Microsoft\\Windows\\DWM")?;
    let color = key.get_u32("AccentColor")?;
    Ok(windows_stuff::convert_color(color))
}

#[cfg(target_os = "windows")]
mod windows_stuff {
    use super::*;
    use windows::{
        core::{HSTRING, PCWSTR},
        Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::Threading::CreateEventW,
        },
    };
    pub(crate) fn convert_color(color: u32) -> Srgb<u8> {
        // possibly use palette's PackedBgra type
        let (b, g, r) = (
            (color & 0xFF0000) >> 16,
            (color & 0xFF00) >> 8,
            color & 0xFF,
        );

        Srgb::<u8>::new(r as u8, g as u8, b as u8)
    }
    // Borrowed from https://github.com/WLBF/reg-watcher/blob/master/src/lib.rs#L152
    pub(crate) struct WaitEvent {
        handle: HANDLE,
    }
    impl WaitEvent {
        pub fn new(name: String) -> AccentResult<Self> {
            let s = PCWSTR(HSTRING::from(name).as_ptr());
            let handle = unsafe { CreateEventW(None, false, true, s)? };
            Ok(WaitEvent { handle })
        }
        pub fn handle(&self) -> HANDLE {
            self.handle
        }
    }

    impl Drop for WaitEvent {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle).expect("failed to close event handler");
            }
        }
    }
    pub(crate) enum WatchResponse {
        Timeout,
        Notify,
    }
}

#[cfg(target_os = "windows")]
pub async fn accent_color_stream(
) -> AccentResult<impl Stream<Item = impl IntoColor<Srgb<f64>>> + Send> {
    use windows::Win32::Foundation::{
        GetLastError, ERROR_SUCCESS, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::System::Registry::{
        RegNotifyChangeKeyValue, HKEY, REG_NOTIFY_CHANGE_LAST_SET,
    };
    use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows_registry::CURRENT_USER;

    use crate::windows_stuff::{WaitEvent, WatchResponse};
    use async_stream::stream;
    use std::time::Duration;
    use tokio::task::spawn_blocking;
    use tokio::time::sleep;
    Ok(stream! {
        loop {
            let res = spawn_blocking(move || {
                let hkey = CURRENT_USER.open("Software\\Microsoft\\Windows\\DWM")?;
                let random_name = ulid::Ulid::new().to_string() + "-accent_color_listener";
                let event = WaitEvent::new(random_name)?;

                let err = unsafe {
                    RegNotifyChangeKeyValue(
                        HKEY(hkey.as_raw()),
                        false,
                        REG_NOTIFY_CHANGE_LAST_SET,
                        event.handle(),
                        true,
                    )
                };
                if err != ERROR_SUCCESS {
                    Err(str_err!("RegNotifyChangeKeyValue return code: {:?}", err))?;
                }
                let val = unsafe {
                    match WaitForSingleObject(event.handle(), INFINITE) {
                        WAIT_ABANDONED => Err(str_err!("WaitForSingleObject return WAIT_ABANDONED")),
                        WAIT_OBJECT_0 => Ok(WatchResponse::Notify),
                        WAIT_TIMEOUT => Ok(WatchResponse::Timeout),
                        WAIT_FAILED => Err(str_err!("WaitForSingleObject return code: {:?}",GetLastError())),
                        _ => unreachable!(),
                    }
                };
                val
            }).await;
            if let Ok(Ok(WatchResponse::Notify)) = res {
                // required cuz the registry doesn't update instantly :P
                sleep(Duration::from_millis(25)).await;
                yield match get_accent_color().await {
                    Ok(val) => val,
                    Err(err) => {
                        eprintln!("an error occured in the accent color listener: {err}");
                        continue;
                    }
                };
            } else {
                break;
            }
        }
    })
}

#[cfg(all(
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "macos")
))]
compile_error!("only Linux, Windows and macOS are supported");

#[cfg(target_os = "macos")]
pub async fn get_accent_color() -> AccentResult<impl IntoColor<Srgb<f64>> + Send> {
    use objc2_app_kit::NSColor;
    tokio::task::spawn_blocking(|| {
        let color = unsafe { NSColor::controlAccentColor() };
        Ok(macos_stuff::convert_color(color))
    })
    .await?
}

#[cfg(target_os = "macos")]
mod macos_stuff {
    use super::*;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSColor, NSColorSpace};
    use objc2_foundation::NSDistributedNotificationCenter;
    use tokio::sync::mpsc::UnboundedReceiver;

    pub(crate) fn convert_color(color: Retained<NSColor>) -> Srgb<f64> {
        let color = unsafe { color.colorUsingColorSpace(&*NSColorSpace::sRGBColorSpace()) }
            .expect("failed to convert colorspace");
        let (mut r, mut g, mut b, mut a) = (0.0, 0.0, 0.0, 0.0);
        unsafe { color.getRed_green_blue_alpha(&mut r, &mut g, &mut b, &mut a) };
        Srgb::<f64>::new(r, g, b)
    }
    pub(crate) struct AccentStream {
        pub observer: Retained<objc2_foundation::NSObject>,
        pub receiver: UnboundedReceiver<()>,
    }
    impl Stream for AccentStream {
        type Item = ();
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            println!("[polling stream]");
            let res = self.receiver.poll_recv(cx);
            println!("[polled stream: {res:#?}]");
            res
        }
    }

    impl Drop for AccentStream {
        fn drop(&mut self) {
            println!("[dropping the stream]");
            let ns_center = unsafe { NSDistributedNotificationCenter::defaultCenter() };
            let Self { observer, receiver } = self;
            receiver.close();
            unsafe { ns_center.removeObserver(&*observer) };
        }
    }
}

#[cfg(target_os = "macos")]
pub async fn accent_color_stream(
) -> AccentResult<impl Stream<Item = impl IntoColor<Srgb<f64>>> + Send> {
    use async_stream::stream;
    let mut last = get_accent_color().await?.into_color();
    let stream = stream! {
        while let Ok(current) = get_accent_color().await {
            let current_conv = current.into_color();
            if last != current_conv {
                last = current_conv;
                yield current_conv;
            }
        }
    };
    Ok(stream)
}

#[cfg(target_os = "macos")]
pub async fn _accent_color_stream() -> AccentResult<impl Stream<Item = impl IntoColor<Srgb<f64>>>> {
    use async_stream::stream;
    use block2::RcBlock;
    use macos_stuff::AccentStream;
    use objc2_foundation::{ns_string, NSDate, NSDistributedNotificationCenter};
    use std::ptr::NonNull;
    use tokio::sync::mpsc::unbounded_channel;
    println!("[creating channel]");
    let (tx, rx) = unbounded_channel::<()>();

    println!("[creating notification center]");
    let notification_name = ns_string!("AppleColorPreferencesChangedNotification");
    let ns_center = unsafe { NSDistributedNotificationCenter::defaultCenter() };
    println!("[creating block]");
    use objc2_foundation::{NSNotification, NSRunLoop};
    let block = move |_: NonNull<NSNotification>| {
        println!("[sending message]");
        tx.send(()).unwrap();
        println!("[sent message]");
    };
    let block = RcBlock::new(block);

    println!("[creating observer]");
    let observer = unsafe {
        ns_center.addObserverForName_object_queue_usingBlock(
            Some(notification_name),
            None,
            None,
            &*block,
        )
    };
    println!("[creating stream wrapper w/ dropping]");
    let mut stream = AccentStream {
        observer,
        receiver: rx,
    };
    Ok(stream! {
        println!("[entering stream loop]");
        tokio::task::spawn_blocking(|| {
            let rl = unsafe { NSRunLoop::currentRunLoop() };
            let mut date = unsafe { NSDate::now() };
            loop {
                date = unsafe { date.dateByAddingTimeInterval(1.0) };
                unsafe { rl.runUntilDate(&date); }
            }
        });
        while let Some(_) = stream.next().await {
            println!("[getting color]");
            yield get_accent_color().await.expect("how did u get here...");
        }
        println!("[exiting stream loop]");
    })
}
