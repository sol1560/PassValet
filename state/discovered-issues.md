# 待验证问题

- 发布脚本忽略扩展zip失败，可能报告完成却缺产物。
- rebind先写钥匙串再改SQLite，需故障测试确认旧库可恢复。
- dev身份绕过与dev命令仅以debug开关控制，需确保发布版不可用。
- 其他观察先写复现与证据再归类，不凭代码存在宣称通过。

## 已复现并修复

- IPC bind会删除普通文件，Drop会删除路径上的替换文件；新测试先失败，修复后通过。
- 丢弃客户端接收端不能结束空闲peer；缺换行的超大消息无限增长。补1MiB上限及接收端关闭检测，检查上限两侧。
- CLI只更新第一处同名env变量，后面的旧值仍有效；无效UTF-8被当空文件覆盖；写入不是原子操作且权限设置晚于写入；换行没有转义。新增4项回归先失败后通过。

## 仍待处理，不算完成

- 严格clippy：core/db.rs的update_meta_keys参数过多；core/manifest.rs三处manual_is_multiple_of。真实模型测试还有unused import警告。
- Setup收到vault:changed时父组件按initialized切换，可能跳过恢复码确认。代码观察尚未由真实macOS测试确认；已有E2E覆盖该要求。
- Keys的“隐藏”仍调用reveal，锁定后还保留revealed状态；待桌面测试复现和修复。
- CLI已有多行引号值、部分取key失败的退出码，以及并发修改env文件还需补测试。
