# Pet 性能测量

测量工具不改变产品代码。正常 `cargo test` 不执行性能基准，默认串行测试配置继续有效。
第 5 步分两阶段：先 Review 工具和 Debug 验证，再授权提交并在干净工作区运行 Release，补充正式基线。临时 Plan 不提交；Release 构建前将其移到仓库外保存，测量结果也写到仓库外，不绕过 build.rs 的检查。

## 角色加载

在干净工作区先构建，再运行已编译的测试程序并将日志保存到临时目录：

```powershell
cargo test --release --no-run
$testExe = Get-ChildItem target/release/deps/stand_awhile-*.exe | Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $testExe.FullName selected_character_load_benchmark --ignored --nocapture | Tee-Object -FilePath "$env:TEMP/stand-awhile-load.txt"
```

每个角色重复 30 次，直接调用生产 `load_character`，不加载旧 catalog、不克隆帧。
计时包含清单解析、嵌入 PNG 解码和像素预乘；排除 GDI+ 初始化、释放、Pet 窗口创建与缩放绘制。第 5 步的原始基线还包含当时的 hitbox 扫描，第 6 步删除该无用计算后仍用相同基准对比。
输出每次毫秒数、解码像素字节数，以及首次、全部 30 次的中位数、P95（nearest rank）、最小/最大值。
同进程中的首次调用不等于冷启动：dog 在 cat 之后运行，共享已初始化的 GDI+ 和进程缓存。
解码字节数仅是帧像素缓冲区，不等于角色或进程的全部内存。测量时保持机器空闲，至少执行三组，记录机器/OS、提交号及构建类型。

## 真实程序资源循环

退出已运行的 Stand Awhile，在交互式 Windows 桌面执行：

```powershell
cargo build --release
powershell -NoProfile -File scripts/measure-pet-resources.ps1 -Output "$env:TEMP/stand-awhile-cat-96.csv"
```

脚本拒绝已有主实例和已存在的输出文件，启动指定 EXE；默认运行 30 轮，结束时通过正常托盘 Exit 命令关闭自己启动的进程。
它使用当前用户配置（默认 cat / 1200 秒），不改配置；应用本身仍执行正常启动初始化和状态保存。测量期间不要操作该实例、切换配置或显示器。
分别用 Settings 选择 cat/dog，退出后重新测量；不同 DPI 在相应显示器上分别运行。CSV 保存实际窗口 DPI、角色、路径和采样参数；另外记录提交号、Release/Debug、机器与显示器布局。

每轮调用真实托盘 Start 处理器，用 `WM_TIMER` 加速倒计时，触发生产路径的同步加载、Pet 创建和提醒。
动画定时器仍使用真实时间。默认显示 15 秒，以覆盖 Jump、Walk 与气泡；随后再次 Start 触发真实退出动画，等待 Pet 窗口销毁及主窗口释放，再静置 2 秒采样。
窗口查找核对进程 ID，发送消息和等待销毁均有限时。若超时/加载失败，保留已有 CSV，报告失败，不能当成成功的 30 轮。
`trigger_ms` 是最后一条倒计时消息从发送到返回的耗时，包含加载、窗口创建/首次绘制、计时 UI 更新和跨进程消息开销，不能当作纯解码耗时。
该流程验证提醒和退出资源路径；自然倒计时、鼠标点击及跨显示器拖动仍按桌面验收文档手动验证。

CSV 指标：

| 字段 | 含义 |
| --- | --- |
| startup | 主窗口就绪后静置 SettleMs；尚未主动触发提醒 |
| visible | Pet 显示 VisibleMs 后 |
| released | 确认 Pet 窗口不存在，再静置 SettleMs 后；进程仍存活 |
| working_set_bytes | 操作系统当前计入进程的驻留工作集，含共享页 |
| private_bytes | 私有提交内存，不要求全部驻留物理内存 |
| gdi_objects / user_objects | GetGuiResources 获取当前 GDI / USER 对象数 |
| handles | 进程句柄总数 |

工具在另一个 PowerShell 进程中采样，其内存不计入目标进程；消息发送与采样仍有少量调度开销。
默认每次运行输出 61 行（启动 1 行，加 30 组 visible/released）。对比 startup、第一轮 released 和第 2–30 轮 released 的趋势与首末差值，记录 visible 峰值。
释放后的 GDI/USER/句柄应趋于稳定；私有内存可能因分配器缓存保持平台值，工作集也受 Windows 回收策略影响。工作集不立即下降不能单独证明泄漏；若资源随每轮持续增长，再定位具体持有者。

## 本次工具验证（Debug，非正式性能基线）

正式 Release 测量已完成。基线报告 `performance-baseline-*.md` 和原始数据 `performance-data/` 仅在本地保留，由 .gitignore 排除。以下保留先前 Debug 工具验证记录。
Debug 加载基准已完整运行：cat 30 次，像素 28,773,696 字节，中位数 253.640 ms；dog 30 次，像素 29,467,984 字节，中位数 259.310 ms。
这些数值仅说明工具能输出重复采样，不用于判断 Release 卡顿或决定优化。

真实 Debug EXE 已完成 30 轮（dog、DPI 168、period 5 秒，VisibleMs=1000、SettleMs=500），CSV 61 行，30 个 released 均没有 Pet 窗口。
startup GDI/USER 为 23/24；第 1–30 轮 released 为 24/25，没有逐轮增加；句柄数范围 203–204。
released 私有提交范围 2,736,128–3,395,584 字节，第 30 轮为 2,850,816 字节。
一次性资源初始化可能使第一轮释放后的数量高于 startup，需以随后多轮趋势评估。
上述缩短的停留时间只验证采样流程；正式基线使用默认 15 秒显示和 2 秒静置，并覆盖两个角色。
脚本最终清理调整后另跑 1 轮，确认正常启动、采样及 Exit。
完整 `cargo test`：148 通过、2 性能基准忽略；加载基准单独通过。`cargo build`、`cargo fmt -- --check` 和 `git diff --check` 通过。
