# 阻塞条件

## 已解除阻塞

- 用户已手动公开仓库，GitHub查询确认PUBLIC；许可证已在main和实现分支。此前修改可见性403不再阻塞。
- macOS Actions [运行34938681897](https://github.com/sol1560/PassValet/actions/runs/34938681897) 第二次运行已真正启动runner，类型检查与前端构建通过；此前计费拒绝启动已解除。

## 尚未确认的验收条件

- 真实Touch ID/Passkey所需Mac、Apple签名、域名关联和账号。
- 七个SaaS的专用测试账号及明确可创建/撤销的资源。
- 签名公证所需Apple Developer条件。

这些条件不阻止本地实现和隔离测试，但未经验证不计入全功能完成。
