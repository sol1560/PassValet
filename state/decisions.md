# 决定与环境

- 用户已批准计划并允许专用分支push及macOS Actions，不允许合并/发布/部署/购买额度。
- spec配置：auto_approve_plan=true（已获得批准），use_cross_review=true，use_rescue=true，adversarial_final=true，use_test_agent=true，test_agent_backend=auto，max_parallel_features=1。检查和复核在当前线程直接执行。
- 当前为Linux orb，不是用户Mac；pnpm 10.28.2，Node 26.5.1；初次检查rustc不在PATH。
- macOS才可验证Swift/Keychain/真实桌面；Linux跑可移植单元与前端检查。
- 工作树初始只有本线程计划文件；从local main创建专用分支，远端为github.com/sol1560/PassValet。
- macOS15 Apple Silicon为主要验收；Intel另做构建/启动兼容检查。
- 优先核实WebdriverIO embedded包实际可用版本，不能仅凭网页说明接入。
- 含敏感内容的调试产物不得上传；最终媒体位置遵循orb规则 `.amp/in/artifacts/`。
