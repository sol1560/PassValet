# 决定与环境

- 用户已批准计划并允许专用分支push及macOS Actions，不允许合并/发布/部署/购买额度。
- spec配置：auto_approve_plan=true（已获得批准），use_cross_review=true，use_rescue=true，adversarial_final=true，use_test_agent=true，test_agent_backend=auto，max_parallel_features=1。检查和复核在当前线程直接执行。
- 当前为Linux orb，不是用户Mac；pnpm 10.28.2，Node 26.5.1；初次检查rustc不在PATH。
- macOS才可验证Swift/Keychain/真实桌面；Linux跑可移植单元与前端检查。
- 工作树初始只有本线程计划文件；从local main创建专用分支，远端为github.com/sol1560/PassValet。
- macOS15 Apple Silicon为主要验收；Intel另做构建/启动兼容检查。
- 优先核实WebdriverIO embedded包实际可用版本，不能仅凭网页说明接入。
- 含敏感内容的调试产物不得上传；最终媒体位置遵循orb规则 `.amp/in/artifacts/`。

## 重新绑定的失败保护

- 每次设置/重新绑定使用数据库已有随机kek_salt命名独立钥匙串条目，数据库格式不变。新条目先持久保存，再提交SQLite；失败不会覆盖旧条目。
- 解锁优先读取数据库指定条目；不存在时兼容旧固定kek条目。测试文件采取相同规则并使用0600权限。
- 暂不删除旧条目：避免中断、并发和旧数据库恢复失去解锁能力。因此重新绑定不等于销毁旧数据库副本的解密能力；不能宣称旧备份已不可解密。
- 自动测试只使用隔离目录中的文件代替钥匙串，不作为真实Touch ID或Keychain访问提示的验收证据。

## 用户追加的云端agent研究

- Amp官方MCP文档明确：orb不继承本机设置，stdio服务器在orb内执行；支持保存在ampcode.com的remote MCP及OAuth。当前PassValet只有stdio→本机Unix socket，不能直接供orb调用。
- Amp runner可在Mac运行并由网页发起任务，适合现有v1，不需要将保险库服务器公开。但get_key返回值会进入agent工具结果，不能宣传为密钥绝不离开设备。
- 真云端方案建议使用HTTPS MCP加桌面主动连接的中转。先申请任务、在Mac显示已验证的任务身份与范围，再领取短时授权；Mac离线/锁定时明确等待或拒绝，不复制整个保险库。
- Amp有OIDC工作负载身份，默认10分钟，可校验issuer/audience/签名/expiry及project_id/user_id/thread_id/token_use。MCP调用中用户填写agent_name或thread_id不是可信身份；需要orb侧helper向服务送真实签名token，不能假设远程MCP自动附带。
- Cursor Cloud Agents官方支持HTTP/stdio MCP与OAuth、启动时注入secrets；没有证据表明与Amp使用相同OIDC身份，所以不要硬套Amp方案。通用接入需要OAuth账号加单任务批准。
- 优先用服务商短期且范围有限的凭据；永久key一旦发出，撤销PassValet会话不能收回副本。真正即时阻止使用需服务商撤销key或受控代调用，后者超出v1范围。
- 不能同时承诺“Mac离线仍可新取key”“不在云端持有key”“每次Mac本地授权”。必须明确取舍。本轮只研究，不部署中转、不配远程访问。
- 来源：https://ampcode.com/docs/customize/mcp 、https://ampcode.com/docs/orbs/handling-secrets 、https://ampcode.com/docs/cli/runners 、https://cursor.com/docs/cloud-agent 、https://cursor.com/docs/cloud-agent/capabilities 。

## 服务入口和令牌格式核对

- Vercel于2026-02-09公布新前缀vcp/vci/vca/vcr/vck；当前个人令牌文档明确vcp_，未给出固定总长度。因此个人令牌接受vcp_加非空字母数字/下划线/短横线，并兼容旧24位字母数字。此检查只确认外形，不证明令牌有效或有权限。五类已知新前缀在扩展和应用两处隐藏。
- Vercel当前创建入口为https://vercel.com/account/tokens；官方支持账户、团队、项目三种范围，不能把文档核对当成真实账号验证。来源：https://vercel.com/changelog/new-token-formats-and-secret-scanning 、https://vercel.com/docs/accounts/access-tokens 。
- Anthropic当前控制台为https://platform.claude.com/，API key页面为https://platform.claude.com/settings/keys；同步登记页与采集入口，避免旧主机跳转触发网站限制。来源：https://platform.claude.com/docs/en/get-api-key 。
- 采集页面原有“永远看不到key”承诺过强，改为说明按已知格式隐藏仍可能遗漏；新文案需随下一轮真实macOS截图检查。

## 安全轮换尚未完成

- 先在Runner入口拒绝Rotate；不能只凭网页提示词允许删除，也不能把此保护当成轮换完成。恢复自动执行前必须有受程序控制的创建、持久保存、精确撤销路径。
- OpenAI官方已有服务账号密钥创建端点 `POST /organization/projects/{project_id}/service_accounts/{service_account_id}/api_keys`，返回完整value和唯一id，支持scopes和expires_in_seconds；示例使用OPENAI_ADMIN_KEY。它是后续候选方式，不是已实现或已获管理权限。不能把ZenMux模型调用凭据用于服务管理。
- 来源：https://developers.openai.com/api/reference/resources/admin/subresources/organization/subresources/projects/subresources/service_accounts/subresources/api_keys/methods/create 。当前没有调用此接口，没有创建或撤销真实资源。
- 进一步核对本地实现：`VaultSink::store`只向metadata写入run_id，没有第三方密钥/项目/账号ID；`Vault::put_secret`覆盖同一(service,key_type)行的旧密文与metadata，没有旧版本记录。可靠轮换还需要保留旧版本、保存新版本和精确的第三方撤销入口，不能只开放现有通用click。
- GitHub组织PAT管理端点撤销的是对该组织的访问，不等于删除用户PAT；官方说明仅GitHub Apps可调用。因此不能把它当成现有PAT的通用轮换接口。来源：https://docs.github.com/en/rest/orgs/personal-access-tokens 、https://docs.github.com/en/organizations/managing-programmatic-access-to-your-organization/reviewing-and-revoking-personal-access-tokens-in-your-organization 。没有调用管理端点。
