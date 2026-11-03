# Rust 搜索引擎调研

## 结论

Kite 的即时文件名搜索采用 `ignore` crate。

`ignore` 是 ripgrep 使用的并行目录遍历库。
`ignore` 不建立索引。
`ignore` 适合首次搜索、目录变化频繁和桌面文件管理器。
Kite 当前搜索已使用 `ignore::WalkBuilder::build_parallel`。

未来增加文件内容全文检索时，采用 Tantivy。
Tantivy 是可嵌入 Rust 应用的全文索引库。
Tantivy 适合重复查询和内容检索。
Tantivy 需要索引存储、增量更新和索引生命周期管理。

Meilisearch 和 Quickwit 不作为 Kite 默认引擎。
Meilisearch 是独立搜索服务。
Quickwit 是面向云存储和可观测性数据的分布式搜索服务。
两个项目都增加桌面应用部署和进程管理成本。

## 候选对比

| 引擎 | 类型 | 最强场景 | 索引成本 | Kite 适用性 |
| --- | --- | --- | --- | --- |
| `ignore` / ripgrep | 并行文件遍历工具库 | 首次递归文件名搜索、代码搜索、内容扫描 | 无索引 | 当前首选 |
| Tantivy | 嵌入式全文索引库 | 多次全文查询、排序、BM25、分面检索 | 需要建立和维护索引 | 内容检索阶段首选 |
| Meilisearch | 独立搜索服务 | 拼写容错、应用搜索、混合检索 | 服务端索引 | 桌面默认方案过重 |
| Quickwit | 分布式搜索服务 | 云存储日志、链路和可观测性数据 | 分布式索引 | 不适合本地文件管理器 |

## 依据

### ripgrep 与 ignore

ripgrep 官方 Benchmark 显示同一 Linux 内核树查询耗时 0.082 秒。
同页对比的 git grep 为 0.273 秒。
测试使用 Intel i9-12900K，结果只代表该测试环境。

ripgrep 的快来自 Rust `regex` 引擎、SIMD、字面量优化和并行目录遍历。
Kite 只搜索文件名时，不需要文本正则和全文索引。
因此 Kite 使用 `ignore` 的并行遍历层。

来源：
- https://github.com/BurntSushi/ripgrep
- https://docs.rs/ignore/latest/ignore/

### Tantivy

Tantivy 官方文档将 Tantivy 定位为 Rust 全文搜索引擎库。
Tantivy 支持增量索引、BM25、分面检索和 mmap 索引目录。
Tantivy 官方文档报告启动时间低于 10 ms。
Tantivy 官方 Benchmark 报告其平均查询延迟约为 Lucene 的一半。
这些数据来自项目方 Benchmark，需要在实际文件集上复测。

来源：
- https://github.com/quickwit-oss/tantivy
- https://tantivy-search.github.io/bench/

### Meilisearch

Meilisearch 提供完整 HTTP API、拼写容错和混合搜索。
Meilisearch 的优势是应用搜索体验和独立服务运维。
Kite 若内置 Meilisearch，需要管理额外进程、端口、索引目录和升级。

来源：
- https://github.com/meilisearch/meilisearch

### Quickwit

Quickwit 构建在 Tantivy 上。
Quickwit 面向云对象存储、日志、链路和分布式查询。
Quickwit 的架构目标不是本地桌面文件检索。

来源：
- https://github.com/quickwit-oss/quickwit

## 实施决策

1. 文件名搜索使用 `ignore` 并行遍历。
2. 搜索结果最多保留 1000 项。
3. 搜索任务支持取消，旧结果不会覆盖新结果。
4. 搜索完成后统一排序，保证跨线程结果顺序稳定。
5. 文件内容全文检索需要单独立项。立项时评估 Tantivy 的索引位置、增量策略和隐私边界。

## 限制

“性能最强”没有单一答案。
首次查询、重复查询、内容检索和分布式检索使用不同性能指标。
最终选择必须用 Kite 的目标文件集、磁盘类型和查询模式复测。