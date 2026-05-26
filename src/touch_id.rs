#![allow(unexpected_cfgs)]

#[cfg(target_os = "macos")]
mod platform {
    use block::ConcreteBlock;
    use objc::{
        class, msg_send,
        runtime::{BOOL, Object, YES},
        sel, sel_impl,
    };
    use std::{ffi::CString, ptr, sync::mpsc, time::Duration};

    const POLICY_DEVICE_OWNER_AUTHENTICATION_WITH_BIOMETRICS: i64 = 1;
    const TOUCH_ID_TIMEOUT: Duration = Duration::from_secs(45);

    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}

    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}

    pub(crate) fn is_available() -> bool {
        unsafe {
            let context: *mut Object = msg_send![class!(LAContext), new];
            if context.is_null() {
                return false;
            }

            let mut error: *mut Object = ptr::null_mut();
            let available: BOOL = msg_send![
                context,
                canEvaluatePolicy: POLICY_DEVICE_OWNER_AUTHENTICATION_WITH_BIOMETRICS
                error: &mut error
            ];
            let _: () = msg_send![context, release];
            available == YES
        }
    }

    pub(crate) fn authenticate(reason: &str) -> Result<(), String> {
        unsafe {
            let context: *mut Object = msg_send![class!(LAContext), new];
            if context.is_null() {
                return Err("Touch ID is unavailable on this Mac".to_string());
            }

            let mut error: *mut Object = ptr::null_mut();
            let available: BOOL = msg_send![
                context,
                canEvaluatePolicy: POLICY_DEVICE_OWNER_AUTHENTICATION_WITH_BIOMETRICS
                error: &mut error
            ];
            if available != YES {
                let _: () = msg_send![context, release];
                return Err("Touch ID is not available or not enrolled".to_string());
            }

            let reason = CString::new(reason)
                .map_err(|_| "Touch ID reason cannot contain null bytes".to_string())?;
            let reason: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: reason.as_ptr()];
            let (tx, rx) = mpsc::channel::<Result<(), String>>();
            let reply = ConcreteBlock::new(move |success: BOOL, _error: *mut Object| {
                let result = if success == YES {
                    Ok(())
                } else {
                    Err("Touch ID was canceled or failed".to_string())
                };
                let _ = tx.send(result);
            })
            .copy();

            let _: () = msg_send![
                context,
                evaluatePolicy: POLICY_DEVICE_OWNER_AUTHENTICATION_WITH_BIOMETRICS
                localizedReason: reason
                reply: &*reply
            ];

            let result = rx
                .recv_timeout(TOUCH_ID_TIMEOUT)
                .map_err(|_| "Touch ID timed out".to_string())?;
            let _: () = msg_send![context, release];
            result
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub(crate) fn is_available() -> bool {
        false
    }

    pub(crate) fn authenticate(_reason: &str) -> Result<(), String> {
        Err("Touch ID is only available on macOS".to_string())
    }
}

pub(crate) fn is_available() -> bool {
    platform::is_available()
}

pub(crate) fn authenticate(reason: &str) -> Result<(), String> {
    platform::authenticate(reason)
}
