# 验收表

所有项初始为 PENDING。具体运行记录放 `state/reports/`；已运行和未运行必须区分。

## M1 任务记录
### Programmatic
- 计划校验脚本通过；spec和所有任务记录存在；每个阶段有验收标准。
### UX
- 不适用：文档。

## M2 macOS真实测试
### Programmatic
- pnpm typecheck；桌面与扩展构建；cargo test --workspace --locked。
- 实际macOS Actions退出0；测试使用真实app二进制、MCP子进程、native host和Chrome扩展。
- 模型/站点替身只替换外部服务；测试数据与浏览器profile隔离；输出无敏感值。
### UX
- 创建保险库、添加测试key、由MCP请求、真实授权窗口批准、读取正确key、撤销后拒绝。
- 检查真实WKWebView截图；多个窗口可定位；扩展状态真实变更。

## M3 安全与恢复
### Programmatic
- 正确/错误/旧恢复码、rekey故障、锁定、重启和存储加密检查。
- manifest范围、过期、拒绝、超时、撤销、并发撤销立即有效。
- IPC来源/大小/权限与断线处理；明文不进日志、SQLite或模型请求。
- release即使设置测试环境变量也无dev入口、测试服务器、dev-kek文件。
### UX
- 初次设置取消可重试，锁定与恢复操作清晰，授权窗口关闭/拒绝不批准请求。
- Touch ID/Passkey真实操作需适用Mac和签名/域名条件，缺失保持BLOCKED。

## M4 采集与轮换
### Programmatic
- 测试控制台覆盖读取/显示/复制/一次性key、多项目、非法URL、提示注入、断线。
- 模型401/429/超时/无工具/非法参数/预算耗尽均有正确结果，不假报成功。
- 新key可靠保存后才允许精确撤销旧key；失败/取消不丢有效密钥。
- 七服务支持表逐类型核对；真实测试账号逐个验证，不用替身结果冒充。
### UX
- 登录/二次验证有明确等待与继续，取消有效，失败说明可理解，部分成功可辨识。

## M5 用户操作与安装
### Programmatic
- CLI env引号/换行/重复值/权限/原子写入/保留内容，已有MCP配置不覆盖。
- app未启动自动打开，MCP多客户端隔离，native framing和重连。
- build-release必须生成所有包；app内CLI和资源可用，解包启动成功。
### UX
- Setup/Keys/Collect/Sessions/Audit/Settings/Prompts/PromptWindow的每个可见操作与空/加载/失败/锁定状态。
- 键盘操作、最小窗口、长文本、复制、删除确认、列表刷新；截图人工检查。
- 干净目录安装包启动、注册扩展与编辑器配置；签名缺失必须明确。

## M6 最终验证
### Programmatic
- 类型检查、构建、Rust测试、格式与clippy、桌面/集成测试全部通过。
- 两个独立干净macOS运行，正式包检查，实际模型与真实站点验证各自记录。
- 最后检查越权、明文泄漏、误删、恢复失败、并发与调试后门。
### UX
- 再走首次使用→添加/采集→授权→读取→撤销→恢复→轮换全过程，检查代表截图。
- 报告列出PASS/FAIL/FLAKY/SKIPPED/UNSURE/BLOCKED，未测不能算完成。
