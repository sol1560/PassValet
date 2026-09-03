# 原生 Passkey（PRF）配置

PassValet 的正式解锁方式是 macOS 原生 passkey + WebAuthn PRF 扩展：保险库的 KEK 由 `HKDF(prf_output, kek_salt)` 派生，
不落盘。它通过 `crates/tauri-plugin-macos-passkey`（vendored 自 yminghua/tauri-plugin-macos-passkey，Swift 桥接
`ASAuthorizationPlatformPublicKeyCredentialProvider`）实现。

在没有下面这些条件时，请用「Touch ID + 钥匙串」方式（随机 KEK 存入登录钥匙串，每次授权由 LocalAuthentication 校验），
二者可在设置里互相切换（`vault_rebind` 会重新包裹所有 DEK 并生成新的恢复密钥）。

## 硬性条件

macOS 原生 passkey 对 `localhost` 没有豁免，app 必须与 relying-party 域名建立关联：

1. Apple Developer 账号（需要 Team ID，例如 `ABCDE12345`）。
2. 一个你控制的 HTTPS 域名，例如 `passvalet.app`。
3. 在该域名托管 `https://passvalet.app/.well-known/apple-app-site-association`（无扩展名，`Content-Type: application/json`）：

```json
{
  "webcredentials": {
    "apps": ["ABCDE12345.ai.passvalet.desktop"]
  }
}
```

4. `apps/desktop/src-tauri/entitlements.plist` 里的 `com.apple.developer.associated-domains` 指向同一域名（已有
   `webcredentials:passvalet.app` 与 `?mode=developer` 两条，把域名换成你的）。
5. 用带 Associated Domains capability 的 provisioning profile 签名：

```bash
# 一次性：在 developer.apple.com 为 ai.passvalet.desktop 创建 App ID（勾选 Associated Domains），下载 profile
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (ABCDE12345)"
export APPLE_PROVISIONING_PROFILE=~/path/to/PassValet.provisionprofile
pnpm --filter @passvalet/desktop tauri build
```

Tauri 会把 entitlements 与 profile 嵌入 `.app`。首次启动后 macOS 从 Apple CDN 拉取 AASA 并缓存；开发期可在
「系统设置 → 隐私与安全性 → 开发者 → Associated Domains Development」打开开发者模式，让 `?mode=developer` 的条目
直连你的服务器（仍需 HTTPS、公网可达，`localhost` 不行，可用 ngrok/Cloudflare Tunnel）。

6. 在 PassValet「设置 → 安全」把「Passkey relying party 域名」填成同一域名，然后点「切换到原生 Passkey」。

## 运行时要求

- macOS 15 或更新（PRF 扩展是 15 才有的）。
- passkey 存在 iCloud 钥匙串（Apple Passwords）。第三方 passkey 管理器（1Password 等）需自身支持 PRF。
- 注册时若返回空 PRF，PassValet 会拒绝并提示更换 provider。

## 常见错误

- `Application is not associated with domain`：AASA 未生效 / Team ID 或 bundle id 不匹配 / 未用带 entitlement 的
  profile 签名。检查 `curl -sI https://<domain>/.well-known/apple-app-site-association`。
- `The operation couldn't be completed. (com.apple.AuthenticationServices.AuthorizationError error 1001.)`：用户取消。
- PRF 为空：passkey 提供方不支持 PRF，或系统低于 macOS 15。

## 恢复

注册（和每次重新绑定）都会生成一次性恢复密钥（8 组 base32 + 校验码）。它用 HKDF 派生的密钥包裹一份 KEK 副本存在
vault 元数据里。passkey 丢失时：解锁页 →「使用恢复密钥」→ 解锁后到设置里重新绑定 Touch ID / Passkey，
旧恢复密钥即失效。恢复密钥只能打开这台设备上的 `vault.sqlite`；v1 不做云同步，跨设备请连库文件一起迁移。
