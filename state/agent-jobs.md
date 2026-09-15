# 运行中的工作

| id | kind | target | status | notes |
| --- | --- | --- | --- | --- |
| 34938681897 | GitHub Actions | M2 | done | 第二次运行找到创建参数错误；已读失败截图 |
| 34941386442 | GitHub Actions | M2 | done | 创建/恢复确认通过；添加成功但测试选择器错误；已读截图 |
| 34942291681 | GitHub Actions | M2 | done | 创建、隐藏/锁定通过；添加测试超时；Chrome已连接但误读隐藏的ID |
| 34943849197 | GitHub Actions | M2 | done | 定位每条驱动命令固定延迟，后由前端companion初始化修复 |
| 34945942248 | GitHub Actions | M2 | done | attempt 1/2各5项真实端到端测试通过；第一轮截图已检查 |
| 34947040427 | GitHub Actions | M3故障复现 | done | 5通过1失败：注入rebind数据库错误后无法解锁；失败截图已检查 |
| 34947480645 | GitHub Actions | M3修复验证 | done | 7项通过：重新绑定失败、成功和旧版兼容 |
| 34947892659 | GitHub Actions | M3恢复码 | done | 8项通过：旧恢复码失效，新码能恢复数据 |
| 34948250195 | GitHub Actions | M3授权拒绝 | done | 9项通过：UI拒绝后MCP不能读取；32项Rust测试通过 |
| 34948658354 | GitHub Actions | M3扩展事件 | done | 修复前普通连接未被拒绝，回归失败 |
| 34949091189 | GitHub Actions | M3扩展修复 | done | 9项通过；新增当前连接单元测试通过 |
| 34949434775 | GitHub Actions | M3并发 | done | 10项通过：两个客户端不能串用批准，全撤销生效 |
| 34949906969 | GitHub Actions | M3超时 | done | 11项通过：超时请求无法继续批准，不生成会话 |
| 34950601179 | GitHub Actions | M3/M5正式包 | done | 11项通过；release包完整性、CLI和dev隔离通过；已检查截图但白屏，继续诊断 |
| 34952456878 | GitHub Actions | M5启动截图 | running | 等待5秒后截图，需下载检查；包含图片隐私修复，不包含本地暂停通知修复 |
| 34952976122 | GitHub Actions | M3/M4/M5回归 | running | desktop已通过；包含暂停通知与格式拒绝/保留原值；release正在构建，完成后下载截图；watch pid 80482 |
| 8283 | test | baseline | done | 原有20项通过，3项ignored |
| 13062 | test | IPC回归 | done | 修复前4项失败，原有1项通过 |
| 14838 | test | IPC回归 | done | 修复后含边界检查6项通过 |
| 15492 | test | CLI回归 | done | 修复前4项失败，原有2项通过 |
| 16912 | test | CLI回归 | done | 修复后6项通过 |
| 17405 | test | 合并检查 | done | Rust29项通过、3项ignored；严格clippy失败 |
| 17418 | build | 前端 | done | 类型检查、桌面前端和扩展构建通过 |

没有子agent或开发服务。
