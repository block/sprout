#[cfg(target_os = "macos")]
use std::sync::mpsc;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use block2::{DynBlock, RcBlock};
#[cfg(target_os = "macos")]
use objc2::{class, msg_send, runtime::Bool};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;

#[cfg(target_os = "macos")]
const CAMERA_MEDIA_TYPE_VIDEO: &str = "vide";
#[cfg(target_os = "macos")]
const CAMERA_AUTHORIZED_STATUS: i32 = 3;
#[cfg(target_os = "macos")]
const CAMERA_PERMISSION_TIMEOUT: Duration = Duration::from_secs(60);

#[cfg(target_os = "macos")]
fn check_camera_permission_sync() -> bool {
    unsafe {
        let media_type = NSString::from_str(CAMERA_MEDIA_TYPE_VIDEO);
        let status: i32 = msg_send![
            class!(AVCaptureDevice),
            authorizationStatusForMediaType: &*media_type
        ];

        status == CAMERA_AUTHORIZED_STATUS
    }
}

#[cfg(target_os = "macos")]
fn request_camera_permission_sync() -> Result<bool, String> {
    if check_camera_permission_sync() {
        return Ok(true);
    }

    let (sender, receiver) = mpsc::channel::<bool>();
    let completion = RcBlock::new(move |granted: Bool| {
        let _ = sender.send(granted.as_bool());
    });
    let completion: &DynBlock<dyn Fn(Bool)> = &completion;

    unsafe {
        let media_type = NSString::from_str(CAMERA_MEDIA_TYPE_VIDEO);
        let _: () = msg_send![
            class!(AVCaptureDevice),
            requestAccessForMediaType: &*media_type,
            completionHandler: completion
        ];
    }

    receiver
        .recv_timeout(CAMERA_PERMISSION_TIMEOUT)
        .map_err(|_| "Timed out waiting for camera permission.".to_string())
}

#[tauri::command]
pub async fn check_camera_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        check_camera_permission_sync()
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub async fn request_camera_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(request_camera_permission_sync)
            .await
            .map_err(|error| error.to_string())?
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}
