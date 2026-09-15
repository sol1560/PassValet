# 阻塞条件

## 已确认阻塞

- macOS Actions [运行34938681897](https://github.com/sol1560/PassValet/actions/runs/34938681897) 因GitHub账号付款失败或支出限制，未启动runner，jobs.steps为空。工作流推送和读取权限可用；不是应用测试失败。需要账号所有者处理Billing，不购买额度、不重复触发相同阻塞。
- 用户已授权改为公开并采用严格付费许可证。许可证已推到main及专用实现分支，但修改可见性被GitHub返回403拒绝；再次查询仍为PRIVATE。需要所有者在仓库Settings → General → Danger Zone中改为Public；完成后再实际重跑macOS Actions，不能预先认定计费阻塞已解除。

## 尚未确认的验收条件

- 真实Touch ID/Passkey所需Mac、Apple签名、域名关联和账号。
- 七个SaaS的专用测试账号及明确可创建/撤销的资源。
- 签名公证所需Apple Developer条件。

这些条件不阻止本地实现和隔离测试，但未经验证不计入全功能完成。
