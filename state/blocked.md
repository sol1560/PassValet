# 阻塞条件

## 已确认阻塞

- macOS Actions [运行34938681897](https://github.com/sol1560/PassValet/actions/runs/34938681897) 因GitHub账号付款失败或支出限制，未启动runner，jobs.steps为空。工作流推送和读取权限可用；不是应用测试失败。需要账号所有者处理Billing，不购买额度、不重复触发相同阻塞。

## 尚未确认的验收条件

- 真实Touch ID/Passkey所需Mac、Apple签名、域名关联和账号。
- 七个SaaS的专用测试账号及明确可创建/撤销的资源。
- 签名公证所需Apple Developer条件。

这些条件不阻止本地实现和隔离测试，但未经验证不计入全功能完成。
