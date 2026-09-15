---
name: 完成 PassValet macOS v1 并逐项验收
overview: "按 autonomous-mission 持续补完 README 承诺的功能，在 GitHub Actions macOS runner 上运行真实桌面、CLI、扩展端到端测试，修复失败并提供安装包和验收证据；真实身份验证与 SaaS 账号测试不以模拟结果代替。"
todos:
  - id: mission-contract
    content: 建立 spec.md 和 state 任务记录，把每个用户功能写成可验证的验收项
    status: completed
  - id: macos-harness
    content: 建立 macOS Actions 和真实 Tauri、CLI、Chrome 扩展测试，先跑通最短完整流程
    status: pending
  - id: vault-authorization
    content: 修复并验证保险库、恢复、重新绑定、授权、撤销和本地通信安全
    status: in_progress
  - id: collection-rotation
    content: 完成七个服务的采集与安全轮换、模型调用、暂停取消和断线处理
    status: pending
  - id: desktop-cli-delivery
    content: 完成桌面全部操作、CLI 项目注入、安装注册和发布包使用流程
    status: pending
  - id: verify-delivery
    content: 运行全量 macOS 验收、检查截图与安全边界、修复回归并交付安装包和测试报告
    status: pending
isProject: true
---

# 完成 PassValet macOS v1 并逐项验收

## 现状与范围

- [README.md](../../../README.md) 已声明本机密钥管理、浏览器采集、MCP/CLI 授权分发和轮换。本次以这些承诺和现有可见页面为范围，不增加云同步、团队、多设备、Windows/Linux 或代理服务。
- [apps/desktop/src/App.tsx](../../../apps/desktop/src/App.tsx) 有密钥、采集、会话、访问日志、设置及授权页面；[commands.rs](../../../apps/desktop/src-tauri/src/commands.rs) 和 [ipc_handler.rs](../../../apps/desktop/src-tauri/src/ipc_handler.rs) 已连接主要后端操作。存在实现不等于已证明可用，先测再确定修复内容。
- [unlock.rs](../../../apps/desktop/src-tauri/src/unlock.rs) 第 55–115 行在 debug 模式下可用文件代替钥匙串、跳过用户验证；[ipc_handler.rs](../../../apps/desktop/src-tauri/src/ipc_handler.rs) 第 120–149 行有 `dev.*` 测试入口。这些只用于隔离测试，不证明 Touch ID 或 Passkey 成功。
- [crates/passvalet-core/src/vault.rs](../../../crates/passvalet-core/src/vault.rs) 已有加密、会话与恢复测试；未发现全应用端到端测试。GitHub API 返回 Actions 工作流数量为 0。
- [scripts/build-release.sh](../../../scripts/build-release.sh) 打包 CLI、扩展、app 和 dmg，但扩展 zip 失败被忽略，需要改为失败即停止并验证产物。
- 当前没有根目录 `spec.md`、`state/` 或已有计划。当前工作树干净。本轮只写本计划；批准后才创建任务记录和改代码。

## A. 执行方式与完成定义

1. 批准后建立 `spec.md`，把本计划的范围、验收表和授权限制写入。按 `autonomous-mission` 创建 `state/plan.md`、`contract.md`、`progress.md`、`decisions.md`、`blocked.md`、`discovered-issues.md` 和 `agent-jobs.md`。
2. 本计划 todos 记录六个工作阶段；`state/` 记录各阶段的具体功能、命令和证据，不重复维护两份不同的功能清单。任意时刻本计划只有一个 `in_progress`。
3. 每个功能按“复现或补失败测试 → 最小修复 → 构建与测试 → 单独检查改动 → 更新进度”推进。连续两次修复无效后重新诊断；仍受外部条件阻塞则记录并继续独立任务。
4. 每个验收项记录自动检查、用户操作、结果和证据。`SKIPPED`、`UNSURE`、`FLAKY` 不能算完成；偶发失败即使重试通过也要查清，最终关键流程不能依赖重试过关。
5. 不默认开其他 agent；由当前线程持续实施、检查和验收。中断后从 `state/` 恢复。

## B. 先建立 macOS 真实端到端测试

- 新增 `.github/workflows/macos-e2e.yml`，固定 `macos-15`，使用锁文件和项目声明的 pnpm 版本，安装固定版本 Rust/Node、构建依赖和 Chrome for Testing。主验收在 Apple Silicon；Intel 构建和安装包启动另列兼容检查，不把单架构通过当作双架构通过。
- 新增 `tests/e2e/`、`wdio.conf.ts` 和必要脚本。在 [apps/desktop/src-tauri/Cargo.toml](../../../apps/desktop/src-tauri/Cargo.toml) 与 [lib.rs](../../../apps/desktop/src-tauri/src/lib.rs) 接入 `tauri-plugin-wdio-webdriver`，使用 `@wdio/tauri-service` 的 embedded 模式驱动真实 WKWebView。实现前核对实际包版本与接口。插件仅在专用测试 feature 且 debug 条件下编译、注册；正式构建不得包含自动化服务。
- 首个 runner 检查：启动真实 app → 通过界面完成测试保险库设置 → 手工添加测试密钥 → 启动真实 `passvalet mcp` → 界面批准 → `get_key` 得到预期值 → 撤销后拒绝。界面批准不能全部用 `dev.decide` 替代。
- Chrome for Testing 使用临时 profile，加载真实构建的 MV3 扩展，注册真实 native messaging host，通过真实 socket 连到 app。新增本地测试控制台和可控模型响应服务，只替换外部 SaaS/模型，不替换扩展、CLI、加密存储和应用命令。
- 明确区分三类结果：真实桌面和进程测试；外部服务使用测试替身的集成测试；专用账号上的真实站点测试。独立截图检查授权窗口、多窗口、错误状态和取消后状态。
- 每次使用临时 `PASSVALET_HOME`、临时项目目录和临时 Chrome profile；不使用用户现有数据库和浏览器登录资料。日志和附件不得包含密钥、恢复码、会话 token、数据库或浏览器 profile。
- 设置任务超时、取消过期运行、依赖缓存和附件保留期限；失败也保存脱敏日志和截图。工作流默认只读仓库权限，不包含部署或发布步骤。

## C. 保险库、授权与恢复必须经得起失败

- 主要修改位置：[vault.rs](../../../crates/passvalet-core/src/vault.rs)、[manifest.rs](../../../crates/passvalet-core/src/manifest.rs)、[unlock.rs](../../../apps/desktop/src-tauri/src/unlock.rs)、[prompt.rs](../../../apps/desktop/src-tauri/src/prompt.rs)、[state.rs](../../../apps/desktop/src-tauri/src/state.rs)、[ipc_handler.rs](../../../apps/desktop/src-tauri/src/ipc_handler.rs)。复用现有类型、事务和错误结构，不另建一套后端。
- 验收初次设置、拒绝或取消设置、重复设置、锁定、空闲自动锁定、重启后解锁、正确与错误恢复码、重新生成恢复码、旧码失效、重新绑定后数据仍可读。
- 专门检查重新绑定顺序：外部钥匙串写入和 SQLite 重加密不是同一个事务。用故障注入验证任一步失败后旧数据仍能恢复，不留下无法解锁的库；涉及格式变更先提供本地备份和升级兼容测试。
- 授权覆盖批准、拒绝、超时、窗口关闭、多个同时请求、到期、撤销、全部撤销、锁定期间请求和未授权服务/类型。对权限校验与实际读取之间的竞争补测试。
- 审查 Unix socket 文件权限、请求大小、断线处理、扩展握手与事件来源；不能让其他连接冒充扩展结束任务或送入密钥。记录本机同一用户权限下的限制，不声称防住所有同用户恶意进程。
- 用可辨识的假密钥检查 SQLite、日志、模型请求和错误文本；只允许在经过授权的响应和明确指定的测试 env 文件出现明文。
- release 构建设置 `PASSVALET_DEV_SKIP_PRESENCE=1` 仍不能调用 `dev.*`，不得创建 `dev-kek.bin` 或开启测试服务器。通过编译条件检查和运行检查共同验证。

## D. 采集、模型调用和轮换

- 主要修改位置：[run.rs](../../../crates/passvalet-agent/src/run.rs)、[playbook.rs](../../../crates/passvalet-agent/src/playbook.rs)、[redact.rs](../../../crates/passvalet-agent/src/redact.rs)、[runs.rs](../../../apps/desktop/src-tauri/src/runs.rs)、[extension.rs](../../../apps/desktop/src-tauri/src/extension.rs)、[executor.ts](../../../apps/extension/src/executor.ts)、[background.ts](../../../apps/extension/entrypoints/background.ts) 和 `playbooks/*.toml`。
- 对 Supabase、Stripe、OpenAI、Anthropic、Vercel、Cloudflare、GitHub 分别列出当前剧本支持的 key 类型和轮换行为。先核对当前站点/API规则再修剧本，服务支持表不能列出实际无法采集的能力。
- 测试页面覆盖直接读取、先显示再读取、复制按钮、一次性显示、登录/二次验证需要用户介入、多个账号/项目、页面变化、非法地址、提示注入、操作超时、扩展断开和重新连接。
- 通过测试服务记录收到的模型请求，断言没有测试 secret。检查模型返回非法工具参数、配额不足、401、429、网络失败、升级模型、次数耗尽、部分成功，确保结果不被误报为全部成功。
- 轮换必须依靠程序约束：确认新 key 已捕获并可靠保存后才能撤销精确匹配的旧 key；无法识别旧 key、保存失败或高影响共享密钥场景停止。不能仅靠提示词约束删除；按现有工具和运行状态扩展最小必要字段及测试。
- 用户取消、轮换失败、app退出、连接中断都不能丢失可用密钥；旧 key 撤销失败时保留新 key 并显示明确结果，不假装全部完成。
- 真实站点测试只使用单独授权的测试账号、项目和可丢弃 key；不把现有生产登录态上传到 GitHub，不自动撤销生产 key。没有账号时继续其他工作，但这些项保持未验收。

## E. 桌面操作、CLI 和交付包

- 桌面覆盖 [Setup.tsx](../../../apps/desktop/src/pages/Setup.tsx)、`Keys`、`Collect`、`Sessions`、`Audit`、`Settings`、`Prompts`、`PromptWindow`：每个按钮、加载/空/错误/锁定状态、表单校验、重复点击、复制、恢复入口、删除确认、任务进行中与结束状态。
- 保持现有外观，只修可用性问题。用键盘和正常鼠标操作验证关键流程；检查窗口最小尺寸、多窗口、长文本、列表刷新和可理解的失败提示。每个外观改动都须查看真实渲染截图。
- [project.rs](../../../crates/passvalet-cli/src/project.rs)、[mcp.rs](../../../crates/passvalet-cli/src/mcp.rs)、[native_host.rs](../../../crates/passvalet-cli/src/native_host.rs)、[install.rs](../../../crates/passvalet-cli/src/install.rs)、[launch.rs](../../../crates/passvalet-cli/src/launch.rs)：验证 env推断、自定义类型、重复变量、引号/换行、保留原文件、原子写入、文件权限、忽略规则、配置错误、app未运行、多个 MCP 客户端会话隔离与 native messaging 分帧/断线。
- 安装脚本不得覆盖用户已有编辑器配置；在临时 HOME 中测试首次安装、重复安装和已有其他 MCP 配置的情况。
- 修复 `scripts/build-release.sh`，任何必需产物缺失都失败。解包 app/dmg/扩展 zip，验证内嵌 CLI 可执行、扩展可加载、资源路径正确，并在干净测试目录启动打包后的 app。debug 测试通过不等于 release 包通过。
- 可交付 unsigned/ad-hoc app/dmg 供测试，并清楚说明 Gatekeeper 限制；普通用户可直接安装的正式包需要 Apple 签名与公证。没有条件不能将测试包描述为正式签名版。

## 验证

执行阶段添加测试命令后，在 macOS runner 保存以下实际输出、退出码和对应提交/run链接：

| 检查 | 命令或动作 | 通过标准 |
| --- | --- | --- |
| 类型与前端 | `pnpm typecheck`、`pnpm --filter @passvalet/desktop build`、`pnpm build:extension` | 全部成功，无忽略错误 |
| Rust | `cargo test --workspace --locked`、`cargo fmt --all -- --check`、适用的 `cargo clippy --workspace --all-targets --locked -- -D warnings` | 测试通过；现有问题逐项处理，不关闭检查掩盖失败 |
| 真实桌面 | 新增 `pnpm test:e2e:desktop` | 驱动真实 WKWebView，检查界面状态及实际存储/授权结果 |
| CLI与扩展全流程 | 新增 `pnpm test:e2e:integration` | 真实app/CLI/Chrome/扩展通信，只有外部站点与模型可用替身 |
| 真实模型 | `cargo test -p passvalet-agent --test live_providers -- --ignored`，仅注入专用测试凭据 | 实际调用成功；没有凭据明确未测 |
| 打包 | `scripts/build-release.sh` + 打包产物启动测试 | app/dmg/CLI/扩展zip完整，release无调试后门 |
| 实际站点与身份验证 | 专用账号逐服务采集/轮换；真实Mac验证Touch ID、取消、Passkey注册/恢复 | 每项记录实际证据；替身通过不能替代 |

- 关键流程在两个独立、干净的 macOS 运行中通过；新增安全回归应同时覆盖正常路径和能区分错误实现的失败路径。
- 下载并逐一检查代表截图，不只上传附件；交付截图不含真实敏感值。用户查看的最终媒体存放 `.amp/in/artifacts/`，临时诊断截图不放该目录。
- 最后专门检查越权、恢复失败、轮换误删、明文泄漏、并发撤销和发布版测试入口。失败回到修复步骤，不将报告当完成证据。
- 交付安装包、校验值、对应GitHub Actions运行链接、代表截图和 `state/reports/mission-report.md`。报告须逐项列出通过、失败、未测和所需外部条件。

## 假设、授权与不可替代的条件

- 将用户所说的 automassion 理解为已安装的 `autonomous-mission`。本次 `/plan` 优先：只写此计划，批准前不创建 `spec.md`、`state/`、工作流或修改源码。
- GitHub仓库是私有仓库；读取 Actions workflows 成功且数量为0，读取权限设置返回403。这不证明无法运行 Actions。执行时检查实际推送/工作流权限，失败则使用工具提供的授权方式，不反复尝试无权限写入。
- 本计划执行请求包含：推送专用 `amp/complete-passvalet` 分支及该分支上的测试工作流，运行和必要时重跑 GitHub macOS Actions、保存本仓库的测试附件。用户批准本计划即允许这些必要远端测试步骤；不包含合并 main、部署、发布 Release、修改仓库权限、购买额度或开通收费 runner。
- 手动触发工作流如需默认分支已有配置，不擅自修改 main；先使用专用分支 push 触发。Actions付费额度不足时记录阻塞，不购买额度。
- GitHub托管Mac不能完成真实用户的指纹操作；Passkey还依赖Apple账号、签名、域名关联和支持PRF的钥匙串。runner只验证可自动化部分，真实身份验证仍需有这些条件的Mac。不能以debug跳过验证来宣称全功能通过。
- 真实 SaaS 采集/轮换需要专用账号和对具体资源的授权；签名/公证需要有效Apple Developer条件。执行先完成不依赖它们的工作，缺少条件时最终状态保持阻塞而不是 `COMPLETED`。不要求用户在聊天里发送任何密钥。
- “好用”按本计划中的首次使用、可理解提示、取消/恢复、键盘操作和最小窗口检查验收，不做无关重设计。不承诺未来第三方页面变化后仍永远通过。
- 参考的测试事实：[Tauri macOS WebDriver](https://v2.tauri.app/develop/tests/webdriver)、[WebdriverIO Tauri embedded方式](https://webdriver.io/docs/desktop-testing/tauri)、[GitHub macOS runner条件](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。执行时用实际安装版本与运行结果确认可用性。
- 执行调整1：首次Actions运行因账号付款/支出上限被GitHub拒绝启动（run 34938681897，无测试步骤）。M2保持未完成，先推进可本地验证的M3–M5，恢复计费后补齐macOS验收。用户另加的云端agent研究只提出方案，不启用远程密钥访问。
