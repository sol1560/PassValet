#![cfg(target_os = "macos")]

use std::os::raw::c_void;

use swift_rs::{SRData, SRObject, SRString};
use tokio::sync::oneshot;

use crate::{PasskeyError, PasskeyLoginResult, PasskeyRegistrationResult};

type PasskeyResultCallback = unsafe extern "C" fn(result: *mut c_void, context: u64);
type BoolCallback = unsafe extern "C" fn(ok: bool, context: u64);

extern "C" {
    fn begin_passkey_registration(
        window_ptr: *mut c_void,
        domain: SRString,
        challenge: SRData,
        username: SRString,
        user_id: SRData,
        salt: SRData,
        context: u64,
        callback: PasskeyResultCallback,
    );
    fn begin_passkey_login(
        window_ptr: *mut c_void,
        domain: SRString,
        challenge: SRData,
        salt: SRData,
        context: u64,
        callback: PasskeyResultCallback,
    );
    fn evaluate_local_auth(
        reason: SRString,
        allow_password_fallback: bool,
        context: u64,
        callback: BoolCallback,
    );
    fn local_auth_biometrics_available() -> bool;
    fn passkey_bridge_last_error() -> SRString;
}

#[allow(non_snake_case)]
#[repr(C)]
struct RegistrationResultObject {
    id: SRString,
    rawId: SRString,
    clientDataJSON: SRString,
    attestationObject: SRString,
    prfOutput: SRData,
}

#[allow(non_snake_case)]
#[repr(C)]
struct LoginResultObject {
    id: SRString,
    rawId: SRString,
    clientDataJSON: SRString,
    authenticatorData: SRString,
    signature: SRString,
    userHandle: SRString,
    prfOutput: SRData,
}

extern "C" fn ptr_callback(result: *mut c_void, context: u64) {
    let sender: Box<oneshot::Sender<usize>> = unsafe { Box::from_raw(context as *mut _) };
    let _ = sender.send(result as usize);
}

extern "C" fn bool_callback(ok: bool, context: u64) {
    let sender: Box<oneshot::Sender<bool>> = unsafe { Box::from_raw(context as *mut _) };
    let _ = sender.send(ok);
}

fn last_error() -> String {
    let s = unsafe { passkey_bridge_last_error() };
    s.to_string()
}

fn classify(msg: String) -> PasskeyError {
    let lower = msg.to_lowercase();
    if lower.contains("cancel") || lower.contains("1001") {
        PasskeyError::Cancelled
    } else if msg.is_empty() {
        PasskeyError::Failed("passkey operation failed".into())
    } else {
        PasskeyError::Failed(msg)
    }
}

fn window_ptr<R: tauri::Runtime>(window: &tauri::Window<R>) -> Result<usize, PasskeyError> {
    window
        .ns_window()
        .map(|p| p as usize)
        .map_err(|_| PasskeyError::NoWindow)
}

pub async fn register<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    domain: &str,
    challenge: &[u8],
    username: &str,
    user_id: &[u8],
    salt: &[u8],
) -> Result<PasskeyRegistrationResult, PasskeyError> {
    let wp = window_ptr(window)?;
    let (tx, rx) = oneshot::channel::<usize>();
    let ctx = Box::into_raw(Box::new(tx)) as u64;
    unsafe {
        begin_passkey_registration(
            wp as *mut c_void,
            SRString::from(domain),
            SRData::from(challenge),
            SRString::from(username),
            SRData::from(user_id),
            SRData::from(salt),
            ctx,
            ptr_callback,
        );
    }
    let ptr = rx.await.map_err(|_| PasskeyError::Failed("bridge dropped".into()))? as *mut c_void;
    if ptr.is_null() {
        return Err(classify(last_error()));
    }
    let obj: SRObject<RegistrationResultObject> = unsafe { std::mem::transmute(ptr) };
    Ok(PasskeyRegistrationResult {
        id: obj.id.to_string(),
        raw_id: obj.rawId.to_string(),
        client_data_json: obj.clientDataJSON.to_string(),
        attestation_object: obj.attestationObject.to_string(),
        prf_output: obj.prfOutput.to_vec(),
    })
}

pub async fn login<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    domain: &str,
    challenge: &[u8],
    salt: &[u8],
) -> Result<PasskeyLoginResult, PasskeyError> {
    let wp = window_ptr(window)?;
    let (tx, rx) = oneshot::channel::<usize>();
    let ctx = Box::into_raw(Box::new(tx)) as u64;
    unsafe {
        begin_passkey_login(
            wp as *mut c_void,
            SRString::from(domain),
            SRData::from(challenge),
            SRData::from(salt),
            ctx,
            ptr_callback,
        );
    }
    let ptr = rx.await.map_err(|_| PasskeyError::Failed("bridge dropped".into()))? as *mut c_void;
    if ptr.is_null() {
        return Err(classify(last_error()));
    }
    let obj: SRObject<LoginResultObject> = unsafe { std::mem::transmute(ptr) };
    Ok(PasskeyLoginResult {
        id: obj.id.to_string(),
        raw_id: obj.rawId.to_string(),
        client_data_json: obj.clientDataJSON.to_string(),
        authenticator_data: obj.authenticatorData.to_string(),
        signature: obj.signature.to_string(),
        user_handle: obj.userHandle.to_string(),
        prf_output: obj.prfOutput.to_vec(),
    })
}

pub async fn touch_id(reason: &str, allow_password: bool) -> Result<bool, PasskeyError> {
    let (tx, rx) = oneshot::channel::<bool>();
    let ctx = Box::into_raw(Box::new(tx)) as u64;
    unsafe {
        evaluate_local_auth(SRString::from(reason), allow_password, ctx, bool_callback);
    }
    let ok = rx.await.map_err(|_| PasskeyError::Failed("bridge dropped".into()))?;
    if ok {
        Ok(true)
    } else {
        Err(classify(last_error()))
    }
}

pub fn biometrics_available() -> bool {
    unsafe { local_auth_biometrics_available() }
}
