# 执行阶段

详细行为与文件范围见 [批准计划](../.agents/plans/2026-09-15-complete-passvalet.plan.md)。

| 阶段 | 功能 | 依赖 | 主要位置 |
| --- | --- | --- | --- |
| M1 任务记录 | F1.1 建立spec、验收项、进度、环境记录 | 无 | spec.md、state/ |
| M2 macOS真实测试 | F2.1 安装核实测试依赖；F2.2 桌面+MCP最短流程；F2.3 Chrome扩展完整通信；F2.4 Actions实际运行 | M1 | tests/e2e、workflow、桌面测试插件 |
| M3 安全与恢复 | F3.1 锁定恢复与重新绑定；F3.2 授权/撤销/并发；F3.3 socket与release隔离 | M2，独立单元测试可提前 | core、ipc、desktop后端 |
| M4 采集与轮换 | F4.1 工具/模型错误处理与脱敏；F4.2 持久保存与安全撤销；F4.3 七服务真实验收 | M2/M3，真实账号另需授权 | agent、extension、playbooks |
| M5 用户使用与安装 | F5.1 页面全部操作；F5.2 CLI/env/安装配置；F5.3 正式包完整性与启动 | M2/M3/M4 | desktop前端、cli、scripts |
| M6 最终验证 | F6.1 全量检查与反向测试；F6.2 两次干净macOS运行；F6.3 报告与产物 | M2–M5 | state/reports、Actions附件 |

每次功能先测试再修复、构建、单独检查改动；两次修复失败后重新诊断，继续失败记录阻塞。
