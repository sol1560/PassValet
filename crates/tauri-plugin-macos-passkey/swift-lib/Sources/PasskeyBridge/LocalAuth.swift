import Foundation
import LocalAuthentication
import SwiftRs

// Last error message from the passkey / local-auth bridge, readable from Rust.
nonisolated(unsafe) private var lastErrorMessage: String = ""
private let lastErrorLock = NSLock()

func setLastError(_ message: String) {
    lastErrorLock.lock()
    lastErrorMessage = message
    lastErrorLock.unlock()
}

@_cdecl("passkey_bridge_last_error")
public func passkey_bridge_last_error() -> SRString {
    lastErrorLock.lock()
    defer { lastErrorLock.unlock() }
    return SRString(lastErrorMessage)
}

/// Prompt Touch ID (or the account password as fallback). Calls back with 1 on success, 0 on failure.
@_cdecl("evaluate_local_auth")
public func evaluate_local_auth(
    reason: SRString,
    allowPasswordFallback: Bool,
    context: UInt64,
    callback: @Sendable @convention(c) (Bool, UInt64) -> Void
) {
    let reasonString = reason.toString()
    let ctx = LAContext()
    ctx.localizedCancelTitle = "取消"
    let policy: LAPolicy = allowPasswordFallback
        ? .deviceOwnerAuthentication
        : .deviceOwnerAuthenticationWithBiometrics
    var evalError: NSError?
    guard ctx.canEvaluatePolicy(policy, error: &evalError) else {
        setLastError(evalError?.localizedDescription ?? "LocalAuthentication unavailable")
        callback(false, context)
        return
    }
    ctx.evaluatePolicy(policy, localizedReason: reasonString) { success, error in
        if let error = error {
            setLastError(error.localizedDescription)
        }
        callback(success, context)
    }
}

/// Whether biometrics (Touch ID) are enrolled and usable.
@_cdecl("local_auth_biometrics_available")
public func local_auth_biometrics_available() -> Bool {
    let ctx = LAContext()
    var err: NSError?
    return ctx.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &err)
}
