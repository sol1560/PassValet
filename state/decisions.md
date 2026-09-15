# 决定与环境

- 用户已批准计划并允许专用分支push及macOS Actions，不允许合并/发布/部署/购买额度。
- spec配置：auto_approve_plan=true（已获得批准），use_cross_review=true，use_rescue=true，adversarial_final=true，use_test_agent=true，test_agent_backend=auto，max_parallel_features=1。检查和复核在当前线程直接执行。
- 当前为Linux orb，不是用户Mac；pnpm 10.28.2，Node 26.5.1；初次检查rustc不在PATH。
- macOS才可验证Swift/Keychain/真实桌面；Linux跑可移植单元与前端检查。
- 工作树初始只有本线程计划文件；从local main创建专用分支，远端为github.com/sol1560/PassValet。
- macOS15 Apple Silicon为主要验收；Intel另做构建/启动兼容检查。
- 优先核实WebdriverIO embedded包实际可用版本，不能仅凭网页说明接入。
- 含敏感内容的调试产物不得上传；最终媒体位置遵循orb规则 `.amp/in/artifacts/`。

## 用户追加的云端agent研究

- Amp官方MCP文档明确：orb不继承本机设置，stdio服务器在orb内执行；支持保存在ampcode.com的remote MCP及OAuth。当前PassValet只有stdio→本机Unix socket，不能直接供orb调用。
- Amp runner可在Mac运行并由网页发起任务，适合现有v1，不需要将保险库服务器公开。但get_key返回值会进入agent工具结果，不能宣传为密钥绝不离开设备。
- 真云端方案建议使用HTTPS MCP加桌面主动连接的中转。先申请任务、在Mac显示已验证的任务身份与范围，再领取短时授权；Mac离线/锁定时明确等待或拒绝，不复制整个保险库。
- Amp有OIDC工作负载身份，默认10分钟，可校验issuer/audience/签名/expiry及project_id/user_id/thread_id/token_use。MCP调用中用户填写agent_name或thread_id不是可信身份；需要orb侧helper向服务送真实签名token，不能假设远程MCP自动附带。
- Cursor Cloud Agents官方支持HTTP/stdio MCP与OAuth、启动时注入secrets；没有证据表明与Amp使用相同OIDC身份，所以不要硬套Amp方案。通用接入需要OAuth账号加单任务批准。
- 优先用服务商短期且范围有限的凭据；永久key一旦发出，撤销PassValet会话不能收回副本。真正即时阻止使用需服务商撤销key或受控代调用，后者超出v1范围。
- 不能同时承诺“Mac离线仍可新取key”“不在云端持有key”“每次Mac本地授权”。必须明确取舍。本轮只研究，不部署中转、不配远程访问。
- 来源：https://ampcode.com/docs/customize/mcp 、https://ampcode.com/docs/orbs/handling-secrets 、https://ampcode.com/docs/cli/runners 、https://cursor.com/docs/cloud-agent 、https://cursor.com/docs/cloud-agent/capabilities 。
