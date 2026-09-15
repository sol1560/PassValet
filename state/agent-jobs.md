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
| 34952456878 | GitHub Actions | M5启动截图 | superseded | API仍显示queued，但取消接口称已完成；已用后续运行验证相同代码，不再反复取消 |
| 34952976122 | GitHub Actions | M3/M4/M5回归 | done | 11项桌面和37项Rust通过，3项ignored；release完整性/隔离通过，5秒截图不再白屏但卡片溢出；日志/tmp/passvalet-release-second.log |
| 34954128893 | GitHub Actions | M5选项布局 | done | 11项桌面与正式包通过；setup-option-1/2与正式包截图均已检查，无横向溢出 |
| 34955006643 | GitHub Actions | M5 CLI故障复现 | done | 10通过1失败：完整注入通过，部分缺失却退出0，/tmp/passvalet-cli-failed.log |
| 34955477283 | GitHub Actions | M5 CLI修复 | done | desktop和release通过，完整/部分/全部缺失三种注入已验证；未包含目录锁修复 |
| 34956448788 | GitHub Actions | M4/M5弹窗与目录锁 | done | Rust、12项真实桌面、正式包检查通过；包含macOS目录锁和弹窗拒绝回归 |
| 34957618846 | GitHub Actions | M4完整采集测试 | done | 原有12项通过；第二次启动Chrome失败，改为独立目录后重测，尚未验证完整采集 |
| 34958176558 | GitHub Actions | M4真实扩展采集 | done | 13项桌面与正式包通过；网页取值→保险库读回、三次模型请求不含完整测试密钥；成功截图已检查 |
| 34959191822 | GitHub Actions | M4中止/M5安装 | done | 13项桌面、安装脚本、正式包通过；中止模型等待与保留旧密钥已验证，取消截图已检查 |
| 34960142166 | GitHub Actions | M4服务格式与文案 | done | 桌面及正式包通过，新隐私文案截图已检查 |
| 34961338797 | GitHub Actions | M4一次性复制与模型 | done | 13项桌面与正式包通过，一次性复制截图已检查 |
| 8283 | test | baseline | done | 原有20项通过，3项ignored |
| 13062 | test | IPC回归 | done | 修复前4项失败，原有1项通过 |
| 14838 | test | IPC回归 | done | 修复后含边界检查6项通过 |
| 15492 | test | CLI回归 | done | 修复前4项失败，原有2项通过 |
| 16912 | test | CLI回归 | done | 修复后6项通过 |
| 17405 | test | 合并检查 | done | Rust29项通过、3项ignored；严格clippy失败 |
| 17418 | build | 前端 | done | 类型检查、桌面前端和扩展构建通过 |

没有子agent或开发服务。
