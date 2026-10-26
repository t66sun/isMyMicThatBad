# 问题跟踪：GitHub

[English](issue-tracker.md) | 中文

问题与规格存放在 `t66sun/isMyMicThatBad` 的 GitHub Issues 中。
在本仓库使用 `gh` CLI；它会从 `origin` 解析仓库。

## 约定

- 创建：`gh issue create --title "..." --body-file <file>`。
- 读取：`gh issue view <number> --comments`。
- 列表：`gh issue list --state open --json number,title,body,labels,comments`。
- 评论：`gh issue comment <number> --body-file <file>`。
- 添加或移除标签：`gh issue edit <number> --add-label "..."` 或 `--remove-label "..."`。
- 关闭：`gh issue close <number> --comment "..."`。

多行正文使用 UTF-8 文件。
当技能要求“发布到问题跟踪器”时，创建 GitHub issue。
当技能要求“获取相关工单”时，读取该 issue 及其评论。

## 作为分诊表面的拉取请求

**PR 作为请求表面：否。**

## Wayfinding 操作

- 地图：一个标记为 `wayfinder:map` 的 issue，包含 Notes、Decisions-so-far 和 Fog。
- 子工单：将其链接为 GitHub 子 issue；若不可用，在子 issue 中添加 `Part of #<map>`，并在地图中添加任务列表条目。
- 工单类型：使用 `wayfinder:research`、`wayfinder:prototype`、`wayfinder:grilling` 或 `wayfinder:task`。
- 阻塞：使用原生 GitHub issue 依赖；若不可用，在子 issue 正文中记录 `Blocked by: #<number>`。
- 前沿：选择地图顺序中第一个无未关闭阻塞、无负责人的开放子 issue。
- 认领：`gh issue edit <number> --add-assignee "@me"`。
- 解决：用答案评论、关闭子 issue，并在地图的 Decisions-so-far 中追加摘要和链接。
