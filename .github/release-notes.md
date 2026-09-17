Pomona：以古罗马果园女神命名的 git 瘦身工具。按分支 tip 保留压缩包、清除历史旧版本，跨全部分支一次完成；命令为 `git pomona`，仓库及下载包均公开。

- `git pomona analyze`：只读分析本地克隆，选定要清理的包路径，产出 TOML plan。
- `git pomona clean`：建镜像并重写历史，每个分支 tip 当前的包保留，历史旧版本删除；重写后自动校验 tip 树不变、待删 blob 不可达。
- `git pomona push`：校验后强推回远程，默认 dry-run；支持 `pre-push` / `post-push` hooks。
- 原生重写引擎基于 `git fast-export` / `git fast-import`，目标机器只需要 git，不依赖 Python 与 git-filter-repo；`--engine filter-repo` 可回退。
- 识别规则走 profile 配置文件，默认扩展名表只含归档、包、磁盘镜像格式。
- GitHub Actions 在 x86_64 / ARM64 原生 runner 上测试、构建并发布静态 musl 二进制。

下载 `git-pomona-linux-x86_64` 或 `git-pomona-linux-aarch64`，核对 `SHA256SUMS` 后放入 PATH，即可使用 `git pomona`。
