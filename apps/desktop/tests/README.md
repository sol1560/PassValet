# 使用概况与界面验证

统计使用本机时区的最近 7 个自然日（含今天），只计算最近 500 条审计记录中有 `session_id` 的 `key_read`。手动显示、复制、拒绝及单纯批准授权不计入。达到 500 条时明确提示不完整。当前会话和密钥总数来自 `vault_info`；密钥总数包含采集模型密钥。

## 自动检查

需要支持 TypeScript 类型擦除的 Node.js 22.6+。

```sh
pnpm typecheck
pnpm --filter @passvalet/desktop build
TZ=Asia/Jakarta pnpm --filter @passvalet/desktop test
TZ=America/New_York pnpm --filter @passvalet/desktop test
cargo test --workspace
```

测试覆盖自然日零点前后、跨月、夏令时、未来时间、事件类型、手动读取排除、500 条上限、最近五条排序及返回字段中的隐私信息排除。

## 独立 Mac 沙盒

不要使用真实保险库。`scripts/dev.sh --sandbox` 使用固定的 `/tmp/passvalet-dev`；重复验证建议新建目录：

```sh
export PASSVALET_HOME="$(mktemp -d /tmp/passvalet-ui.XXXXXX)"
export PASSVALET_DEV_SKIP_PRESENCE=1
# 先按 scripts/dev.sh 的构建步骤准备 sidecar，再启动桌面开发程序。
pnpm --filter @passvalet/desktop tauri dev
# 在另一个终端使用上面输出的同一目录：
python3 scripts/test-ui-sandbox.py /tmp/passvalet-ui.XXXXXX
```

沙盒脚本拒绝非指定 `/tmp` 前缀和已有保险库，只写入 `FAKE-UI-TEST` 假值。它通过实际桌面进程的 Unix socket 验证初始化、多密钥、批准/拒绝、读取权限、扩展连接状态和审计事件。批准操作使用已有 debug 方法；扩展使用连接模拟，不等于真实 Chrome 扩展或 Touch ID 验证。

本轮桌面进程在 macOS 上运行成功，但系统会话处于锁屏状态，无法进行窗口鼠标检查或原生截图/录像。浏览器界面测试另用模拟接口，不能替代原生材质、系统鉴权、剪贴板或真实控制台采集验证。原生侧栏使用 Tauri 已有的 `sidebar` 系统材质；主内容不透明，低透明度偏好使用实底。这不是新增的 `NSGlassEffectView` 实现。
