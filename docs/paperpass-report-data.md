# PaperPass 报告转数据脚本

用于把“改写前文章 + 改写后文章 + PaperPass AIGC 报告”转成标准 JSON 数据，后续可用于校准混合检测和反推改写策略。

## 用法

```bash
npm run paperpass:data -- \
  --before "/path/to/改写前.docx" \
  --after "/path/to/改写后.docx" \
  --report "/path/to/PaperPass-免费版-检测报告/AIGC检测报告.html" \
  --out "/tmp/paperpass-data.json"
```

如果报告总分需要手动覆盖：

```bash
npm run paperpass:data -- \
  --before "/path/to/改写前.docx" \
  --after "/path/to/改写后.docx" \
  --report "/path/to/AIGC检测报告.html" \
  --pp 20.39 \
  --out "/tmp/paperpass-data.json"
```

只有 PaperPass 报告、暂时没有原稿/改写稿文件时，也可以先抽取外部报告证据：

```bash
npm run paperpass:data -- \
  --report "/path/to/AIGC检测报告.html" \
  --pp 15.33 \
  --out "/tmp/paperpass-report-only.json"
```

## 输出内容

- `measuredAigc`：PaperPass 总疑似 AIGC。
- `originalMetrics` / `rewrittenMetrics`：改写前后文本指标。
- `metricsDelta`：改写前后差异。
- `externalReport`：可放入 App 校准反馈的报告证据。
- `externalReport.segments`：PaperPass 命中的疑似片段，按疑似度从高到低排序。
- `externalReport.rewriteGuidance`：根据报告片段反推出的改写建议。

报告单独解析时不会生成 `originalMetrics`、`rewrittenMetrics` 和 `metricsDelta`，只作为 PP/维普外部报告证据进入校准库。

## 目前支持

- `.docx`
- `.doc`，通过 macOS `textutil` 读取
- `.txt`
- `.html`
- PaperPass 免费版报告目录或 `AIGC检测报告.html`
