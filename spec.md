---
auto_approve_plan: true
use_cross_review: true
use_rescue: true
adversarial_final: true
use_test_agent: true
test_agent_backend: auto
max_parallel_features: 1
---

# PassValet macOS v1

用户已通过 `/plan go` 批准 [完整计划](.agents/plans/2026-09-15-complete-passvalet.plan.md)。

## 目标

完成 README 承诺的 macOS v1：本地加密保险库、恢复与重新绑定、MCP/CLI授权取密钥、浏览器扩展采集、七个内置服务的安全轮换、桌面管理与安装包。
现有页面和公开命令必须可用；失败、取消、断线、权限拒绝不得泄漏密钥或丢失数据。
不增加云同步、团队、多设备、Windows/Linux或代理功能，不做无关重设计。

## 完成标准

- 按 `state/contract.md` 逐项记录命令检查和用户操作检查，不能以模拟服务或跳过身份验证声称真实站点、Touch ID、Passkey通过。
- GitHub Actions macOS运行真实应用、CLI、Chrome扩展；检查正式包与测试版区别，正式版无测试控制入口。
- 关键流程在两个干净运行中通过，失败修复后重测，偶发通过不算完成。
- 交付app/dmg、CLI、扩展zip、校验值、Actions运行链接、检查过的截图和完整报告。
- 缺少测试账号、真实硬件、签名条件或Actions权限，记录阻塞并先完成独立工作；未验收项存在时不标全任务完成。

## 已授权与限制

- 已允许向 `sol1560/PassValet` 的 `amp/complete-passvalet` 专用分支推送代码和测试工作流、运行和必要时重跑macOS Actions、存储脱敏测试附件。
- 未允许合并main、部署、发布Release、改仓库权限、购买额度、使用生产账号创建或撤销key。
- 测试用隔离目录、临时浏览器profile和假密钥；不得上传用户登录资料、真实数据库、密钥、恢复码或会话token。
- 任务进度只写 `state/`，阶段状态同步至批准计划；当前线程直接实施，未另行授权多agent。
