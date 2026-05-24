#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { execFileSync } from "node:child_process";

const AI_TERMS = [
  "促进",
  "推动",
  "完善",
  "深度融合",
  "具有重要意义",
  "赋能",
  "实践参考",
  "调研发现",
  "旨在",
  "有效路径",
  "显著提升",
  "不断完善",
  "提供参考",
  "优化路径",
  "现状及对策",
  "高质量发展",
  "创新形式",
  "核心载体",
  "重要资源",
  "理论参考",
];

const BUFFER_TERMS = [
  "比较",
  "一些",
  "还",
  "可以",
  "会",
  "方面",
  "情况",
  "过程中",
  "一定",
  "里面",
  "这样",
  "来看",
  "不够",
  "并不",
  "有的",
  "了",
  "的",
];

const PLAIN_TERMS = [
  "有一点",
  "更充分一些",
  "不太",
  "不够",
  "比较",
  "一些",
  "还",
  "可以",
  "会",
  "里面",
  "这样",
];

const CONNECTORS = [
  "首先",
  "其次",
  "最后",
  "此外",
  "综上",
  "因此",
  "同时",
  "不仅",
  "而且",
  "从而",
  "进而",
  "通过",
  "基于",
  "为了",
];

const RISK_TYPES = [
  ["问卷访谈提纲命中", ["请简要介绍", "您此次", "使用频率如何", "哪些体验较差", "有哪些具体建议", "访谈前", "访谈中", "追问细节", "这份问卷", "填写结果"]],
  ["参考文献命中", ["[J]", "[D]", "[N]", "旅游纵览", "智能城市", "西部旅游", "燕山大学", "吉林大学"]],
  ["声明授权命中", ["本人声明", "学位论文", "原创性声明", "独创性声明", "法律结果由本人承担", "版权使用授权书"]],
  ["文献综述包装段", ["国内外研究", "研究动态", "文献", "已有研究"]],
  ["理论定义包装段", ["理论", "概念", "模型", "维度", "体系"]],
  ["数据解释包装段", ["数据", "比例", "得分", "评分", "投诉量", "表"]],
  ["条目解释包装段", ["第三", "第四", "首先", "其次", "趣味性", "基本权利"]],
  ["术语例句解释段", ["例句", "相当于", "意思大致", "表目的", "表结果", "用于连接"]],
  ["语体功能解释段", ["语体功能", "程式化", "庄重", "严谨", "公文格式", "语气"]],
  ["案例完整包装段", ["案例", "Hello Kitty", "小小飞行家", "主题", "仪式感", "参与"]],
  ["致谢作文腔", ["致谢", "感谢", "导师", "家人", "室友", "论文也算"]],
  ["策略清单包装段", ["建议", "对策", "策略", "一是", "二是", "第一阶段", "第二阶段"]],
  ["意义闭环包装段", ["提供参考", "推动", "促进", "完善", "形成", "意义"]],
  ["服务质量包装段", ["服务质量", "顾客体验", "满意度", "低成本航空"]],
];

function usage() {
  console.error(
    [
      "Usage:",
      "  npm run paperpass:data -- --before <改写前.docx|txt> --after <改写后.docx|txt> --report <AIGC检测报告.html|报告目录> [--pp 20.39] [--out result.json]",
      "  npm run paperpass:data -- --report <AIGC检测报告.html|报告目录> [--pp 15.33] [--out result.json]",
      "",
      "Notes:",
      "  - .txt/.html can be read directly.",
      "  - .docx is extracted from word/document.xml with a lightweight parser.",
      "  - .doc uses macOS textutil when available.",
      "  - The output JSON can be used as externalReport / calibration evidence.",
    ].join("\n"),
  );
}

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) continue;
    const key = arg.slice(2);
    const value = argv[i + 1] && !argv[i + 1].startsWith("--") ? argv[++i] : "true";
    args[key] = value;
  }
  return args;
}

function readTextFile(filePath) {
  return fs.readFileSync(filePath, "utf8");
}

function decodeXml(text) {
  return text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'");
}

function readDocxText(filePath) {
  const bytes = fs.readFileSync(filePath);
  const name = "word/document.xml";
  const eocd = findEndOfCentralDirectory(bytes);
  const centralSize = bytes.readUInt32LE(eocd + 12);
  const centralOffset = bytes.readUInt32LE(eocd + 16);
  let localHeader = -1;
  let centralCompressedSize = 0;
  let cursor = centralOffset;
  while (cursor < centralOffset + centralSize) {
    if (bytes.readUInt32LE(cursor) !== 0x02014b50) {
      throw new Error(`Invalid docx central directory near ${cursor}`);
    }
    const fileNameLength = bytes.readUInt16LE(cursor + 28);
    const extraLength = bytes.readUInt16LE(cursor + 30);
    const commentLength = bytes.readUInt16LE(cursor + 32);
    const fileName = bytes.subarray(cursor + 46, cursor + 46 + fileNameLength).toString("utf8");
    if (fileName === name) {
      localHeader = bytes.readUInt32LE(cursor + 42);
      centralCompressedSize = bytes.readUInt32LE(cursor + 20);
      break;
    }
    cursor += 46 + fileNameLength + extraLength + commentLength;
  }
  if (localHeader === -1) {
    throw new Error(`Cannot find ${name} in ${filePath}`);
  }
  if (bytes.readUInt32LE(localHeader) !== 0x04034b50) {
    throw new Error(`Invalid local zip header for ${name}`);
  }
  const compression = bytes.readUInt16LE(localHeader + 8);
  const localCompressedSize = bytes.readUInt32LE(localHeader + 18);
  const compressedSize = localCompressedSize || centralCompressedSize;
  const fileNameLength = bytes.readUInt16LE(localHeader + 26);
  const extraLength = bytes.readUInt16LE(localHeader + 28);
  const dataStart = localHeader + 30 + fileNameLength + extraLength;
  const compressed = bytes.subarray(dataStart, dataStart + compressedSize);
  let xml;
  if (compression === 0) {
    xml = compressed.toString("utf8");
  } else if (compression === 8) {
    xml = zlib.inflateRawSync(compressed).toString("utf8");
  } else {
    throw new Error(`Unsupported docx zip compression ${compression}`);
  }
  const paragraphs = [...xml.matchAll(/<w:p[\s\S]*?<\/w:p>/g)]
    .map((match) =>
      [...match[0].matchAll(/<w:t[^>]*>([\s\S]*?)<\/w:t>/g)]
        .map((part) => decodeXml(part[1]))
        .join(""),
    )
    .map((item) => item.trim())
    .filter(Boolean);
  return paragraphs.join("\n");
}

function findEndOfCentralDirectory(bytes) {
  for (let i = bytes.length - 22; i >= 0; i -= 1) {
    if (bytes.readUInt32LE(i) === 0x06054b50) {
      return i;
    }
  }
  throw new Error("Invalid docx zip: end of central directory not found");
}

function readDocumentText(filePath) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`Input file does not exist: ${filePath}`);
  }
  const ext = path.extname(filePath).toLowerCase();
  if (ext === ".docx") return readDocxText(filePath);
  if (ext === ".doc") return readDocText(filePath);
  return readTextFile(filePath);
}

function readDocText(filePath) {
  try {
    return execFileSync("textutil", ["-convert", "txt", "-stdout", filePath], {
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
  } catch (error) {
    throw new Error(`Cannot read .doc file with macOS textutil: ${error.message}`);
  }
}

function cjkCount(text) {
  return [...text].filter((ch) => ch >= "\u4e00" && ch <= "\u9fff").length;
}

function sentenceCount(text) {
  const matches = text.match(/[。！？!?；;]/g);
  return Math.max(1, matches ? matches.length : 1);
}

function countTerms(text, terms) {
  let total = 0;
  for (const term of terms) {
    let index = text.indexOf(term);
    while (index !== -1) {
      total += 1;
      index = text.indexOf(term, index + term.length);
    }
  }
  return total;
}

function metricsFromText(text) {
  const paragraphs = text
    .split(/\n+/)
    .map((item) => item.trim())
    .filter(Boolean);
  const body = paragraphs.filter((item) => cjkCount(item) >= 12 && item.length >= 24);
  const joined = body.join("\n");
  const cjk = Math.max(1, cjkCount(joined));
  const punctuation = (joined.match(/[。！？!?；;，,、：:]/g) ?? []).length;
  const avgParagraphLen = body.length ? body.reduce((sum, item) => sum + cjkCount(item), 0) / body.length : 0;
  return {
    totalParagraphs: paragraphs.length,
    bodyParagraphs: body.length,
    cjkChars: cjkCount(joined),
    avgParagraphLen: round(avgParagraphLen, 1),
    avgSentenceLen: round(cjk / sentenceCount(joined), 1),
    punctuationPer100: round((punctuation / cjk) * 100, 2),
    aiTermsPer10k: round((countTerms(joined, AI_TERMS) / cjk) * 10000, 2),
    bufferTermsPer10k: round((countTerms(joined, BUFFER_TERMS) / cjk) * 10000, 2),
    plainTermsPer10k: round((countTerms(joined, PLAIN_TERMS) / cjk) * 10000, 2),
    connectorsPer10k: round((countTerms(joined, CONNECTORS) / cjk) * 10000, 2),
  };
}

function delta(before, after) {
  return {
    cjkCharsDelta: after.cjkChars - before.cjkChars,
    avgParagraphLenDelta: round(after.avgParagraphLen - before.avgParagraphLen, 1),
    avgSentenceLenDelta: round(after.avgSentenceLen - before.avgSentenceLen, 1),
    punctuationPer100Delta: round(after.punctuationPer100 - before.punctuationPer100, 2),
    aiTermsPer10kDelta: round(after.aiTermsPer10k - before.aiTermsPer10k, 2),
    bufferTermsPer10kDelta: round(after.bufferTermsPer10k - before.bufferTermsPer10k, 2),
    plainTermsPer10kDelta: round(after.plainTermsPer10k - before.plainTermsPer10k, 2),
    connectorsPer10kDelta: round(after.connectorsPer10k - before.connectorsPer10k, 2),
  };
}

function extractJsValue(source, marker) {
  const markerIndex = source.indexOf(marker);
  if (markerIndex === -1) return null;
  let start = markerIndex + marker.length;
  while (/\s/.test(source[start] ?? "")) start += 1;
  if (source[start] === "=") start += 1;
  while (/\s/.test(source[start] ?? "")) start += 1;
  const opener = source[start];
  const closer = opener === "{" ? "}" : opener === "[" ? "]" : null;
  if (!closer) return null;
  let depth = 0;
  let quote = "";
  let escaped = false;
  for (let i = start; i < source.length; i += 1) {
    const ch = source[i];
    if (quote) {
      if (escaped) escaped = false;
      else if (ch === "\\") escaped = true;
      else if (ch === quote) quote = "";
      continue;
    }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === opener) depth += 1;
    else if (ch === closer) {
      depth -= 1;
      if (depth === 0) return source.slice(start, i + 1);
    }
  }
  return null;
}

function findReportRoot(reportPath) {
  const stat = fs.statSync(reportPath);
  if (stat.isDirectory()) return reportPath;
  let dir = path.dirname(reportPath);
  if (fs.existsSync(path.join(dir, "htmls"))) return dir;
  if (path.basename(dir) === "htmls") return path.dirname(dir);
  return dir;
}

function readMaybe(filePath) {
  return fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : "";
}

function parsePaperPassReport(reportPath) {
  const root = findReportRoot(reportPath);
  const reduceSource = readMaybe(path.join(root, "htmls/js/reduceaigcpagedata.js"));
  const simpleSource = readMaybe(path.join(root, "htmls/js/simplesentenceresult_ai.js"));
  const detailSource = readMaybe(path.join(root, "htmls/js/detaildata.js"));
  const reduceRaw = extractJsValue(reduceSource, "var reduceAiDataInfo");
  const simpleRaw = extractJsValue(simpleSource, "var data");
  const reduce = reduceRaw ? JSON.parse(reduceRaw) : {};
  const simple = simpleRaw ? JSON.parse(simpleRaw) : {};
  const title = matchVar(detailSource, "title") ?? path.basename(reportPath);
  const segments = Object.entries(simple)
    .map(([key, value]) => {
      const text = (value.sectionContentList ?? []).join("");
      const ratio = Number(value.overall ?? 0);
      return {
        no: Number(key),
        text,
        suspectedChars: cjkCount(text),
        suspectedRatio: round(ratio, 2),
        segmentKind: classifySegmentKind(text),
      };
    })
    .filter((item) => item.text)
    .sort((a, b) => b.suspectedRatio - a.suspectedRatio);
  const riskTypes = inferRiskTypes(segments);
  return {
    provider: "paperpass",
    reportFileName: path.basename(reportPath),
    reportFilePath: path.resolve(reportPath),
    title,
    reportScore: nullableNumber(reduce.score),
    totalSuspectedRatio: nullableNumber(reduce.totalSuspectedTextRatio),
    highAndMiddleSuspectedRatio: nullableNumber(reduce.highAndMiddleSuspectedTextRatio),
    highSuspectedRatio: nullableNumber(reduce.highSuspectedTextRatio),
    middleSuspectedRatio: nullableNumber(reduce.middleSuspectedTextRatio),
    lowSuspectedRatio: nullableNumber(reduce.lowSuspectedTextRatio),
    noAiSuspectedRatio: nullableNumber(reduce.noAISuspectedTextRatio),
    humanWrittenRate:
      reduce.totalSuspectedTextRatio == null ? null : round(100 - Number(reduce.totalSuspectedTextRatio), 2),
    suspiciousSegmentCount: segments.length,
    bodySuspiciousSegmentCount: segments.filter((item) => item.segmentKind === "body").length,
    appendixLikeSegmentCount: segments.filter((item) => item.segmentKind === "appendix").length,
    referenceSegmentCount: segments.filter((item) => item.segmentKind === "reference").length,
    markedSpanCount: 0,
    markedChars: segments.reduce((sum, item) => sum + item.suspectedChars, 0),
    severeSegmentCount: segments.filter((item) => item.suspectedRatio >= 70).length,
    moderateSegmentCount: segments.filter((item) => item.suspectedRatio >= 60 && item.suspectedRatio < 70).length,
    mildSegmentCount: segments.filter((item) => item.suspectedRatio >= 50 && item.suspectedRatio < 60).length,
    riskTypes,
    segments,
    analysisSummary: buildAnalysisSummary(reduce, segments, riskTypes),
    rewriteGuidance: buildRewriteGuidance(reduce, segments, riskTypes),
  };
}

function matchVar(source, name) {
  const pattern = new RegExp(`var\\s+${name}\\s*=\\s*(['"])(.*?)\\1;`);
  return source.match(pattern)?.[2] ?? null;
}

function nullableNumber(value) {
  return value == null || Number.isNaN(Number(value)) ? null : round(Number(value), 2);
}

function inferRiskTypes(segments) {
  const found = new Set();
  const full = segments.map((segment) => segment.text).join("\n");
  for (const [type, terms] of RISK_TYPES) {
    if (terms.some((term) => full.includes(term))) {
      found.add(type);
    }
  }
  return [...found];
}

function classifySegmentKind(text) {
  const compact = text.replace(/\s+/g, "");
  if (looksReferenceSegment(compact)) return "reference";
  if (looksAppendixSegment(compact)) return "appendix";
  return "body";
}

function looksReferenceSegment(text) {
  const referenceMarkers = (text.match(/\[[JDMN]\]/g) ?? []).length;
  return (
    referenceMarkers >= 1 ||
    /[，,.]\d{4}[，,.(（]/.test(text) ||
    text.includes("旅游纵览") ||
    text.includes("智能城市") ||
    text.includes("西部旅游") ||
    text.includes("燕山大学") ||
    text.includes("吉林大学")
  );
}

function looksAppendixSegment(text) {
  const questionSignals = [
    "请简要介绍",
    "是否使用",
    "使用频率如何",
    "哪些体验较差",
    "有哪些具体建议",
    "通常咨询哪些",
    "根据您的观察",
    "访谈前",
    "访谈中",
    "结束后及时整理",
    "这份问卷",
    "填写结果",
    "感谢您参加这次访谈",
    "访谈时间大概",
    "再次感谢您的参与",
    "本人声明",
    "原创性声明",
    "独创性声明",
    "法律结果由本人承担",
    "版权使用授权书",
  ];
  const numberedQuestions = (text.match(/\d+[.．、]/g) ?? []).length;
  return (
    questionSignals.some((term) => text.includes(term)) ||
    (numberedQuestions >= 3 && (text.includes("您") || text.includes("访谈") || text.includes("问卷")))
  );
}

function buildAnalysisSummary(reduce, segments, riskTypes) {
  const total = nullableNumber(reduce.totalSuspectedTextRatio);
  const high = nullableNumber(reduce.highSuspectedTextRatio) ?? 0;
  const middle = nullableNumber(reduce.middleSuspectedTextRatio) ?? 0;
  const low = nullableNumber(reduce.lowSuspectedTextRatio) ?? 0;
  return `PaperPass报告总疑似${total ?? "未知"}%，高疑似${high}%，中疑似${middle}%，低疑似${low}%；共命中${segments.length}个片段，主要集中在${riskTypes.join("、") || "摘要/理论/数据/策略包装段"}。`;
}

function buildRewriteGuidance(reduce, segments, riskTypes) {
  const totalRatio = nullableNumber(reduce.totalSuspectedTextRatio) ?? 100;
  const highRatio = nullableNumber(reduce.highSuspectedTextRatio) ?? 100;
  const middleRatio = nullableNumber(reduce.middleSuspectedTextRatio) ?? 100;
  const hasHigh = highRatio > 0;
  const bodySegments = segments.filter((segment) => segment.segmentKind === "body");
  const appendixLikeCount = segments.length - bodySegments.length;
  const top = segments.slice(0, 3).map((segment) => compact(segment.text, 70)).join(" / ");
  const severity =
    totalRatio < 20
      ? "PP低于20%，按当前目标已经过线；已改写稿不建议继续改写"
      : hasHigh
        ? "仍有高疑似片段，优先做命中正文段定向改写"
        : "高疑似为0，优先处理中低风险正文片段，不要继续全篇大扩写";
  const appendixGuidance =
    appendixLikeCount >= 5 && totalRatio <= 12
      ? `本报告有${appendixLikeCount}个问卷/访谈/参考文献类命中，正文有效命中约${bodySegments.length}个；这类附录型命中不应按正文失败处理。`
      : "";
  const broadGuidance =
    totalRatio > 30 && bodySegments.length > 40
      ? "本报告正文命中范围过大，不适合继续做PP定向；这种情况会退化成带报告提示的全文重跑，优先回到17 2.0测试20段后叠加全文，并加强对长解释段、文献综述和策略清单的拆散。"
      : "";
  const ratioGuidance =
    totalRatio <= 10 &&
    highRatio <= 0.1 &&
    middleRatio <= 4.5 &&
    segments.length <= 12
      ? "这类10%内强成功报告说明，17 2.0经过测试20段后叠加全文可以进入极低PP区间，少量致谢、问卷说明和定义解释命中不代表失败。"
      : totalRatio <= 10 &&
    highRatio <= 2 &&
    middleRatio <= 5 &&
    bodySegments.length <= 6
      ? "这类10%内报告只能说明命中段处理后也能过线；若同时存在20段叠加全文的更低样本，应优先归因给17 2.0测试20段后叠加全文，PP定向只作为原稿高PP或叠加后仍高于20时的补救。"
      : totalRatio <= 18 &&
          highRatio <= 3 &&
          middleRatio <= 8.5 &&
          segments.length <= 14
        ? "这类15%左右报告只能说明当前稿已达到过线目标；如果该样本来自PP定向，不应反推为优于20段叠加全文，仍以17 2.0测试20段后叠加全文作为默认主链路。"
        : totalRatio <= 18 &&
            highRatio <= 0.1 &&
            middleRatio <= 15 &&
            bodySegments.length <= 12
          ? "这类零高疑似、16%左右报告属于低占比成功边界：正文中疑似仍集中在文献综述、案例观察、理论定义和数据说明；已改写稿可直接停止，未改写原稿才按这些命中段定向处理。"
        : "";
  return `${severity}。${broadGuidance}${ratioGuidance}${appendixGuidance}当前成功链路应按“测试20段后叠加全文”理解，不按普通整篇直跑或PP定向归因；PP定向只用于原稿先测PP并高于20，且正文命中范围较窄的情况。若是未改写原稿先跑PP或PP仍高于20，再重点拆散${riskTypes.join("、") || "完整包装段"}，把表格/数据解释、文献综述、理论定义、条目解释和案例完整包装改得更分散、更具体。典型命中：${top}`;
}

function compact(text, limit) {
  const value = text.replace(/\s+/g, "");
  return value.length > limit ? `${value.slice(0, limit)}…` : value;
}

function round(value, digits) {
  const factor = 10 ** digits;
  return Math.round(Number(value) * factor) / factor;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.report || Boolean(args.before) !== Boolean(args.after)) {
    usage();
    process.exit(1);
  }
  const report = parsePaperPassReport(args.report);
  const measuredAigc = args.pp != null ? nullableNumber(args.pp) : report.totalSuspectedRatio;
  const output = args.before
    ? buildPairedOutput(args, report, measuredAigc)
    : buildReportOnlyOutput(args, report, measuredAigc);
  const json = `${JSON.stringify(output, null, 2)}\n`;
  if (args.out) {
    fs.writeFileSync(args.out, json);
  } else {
    process.stdout.write(json);
  }
}

function buildPairedOutput(args, report, measuredAigc) {
  const beforeText = readDocumentText(args.before);
  const afterText = readDocumentText(args.after);
  const beforeMetrics = metricsFromText(beforeText);
  const afterMetrics = metricsFromText(afterText);
  return {
    generatedAt: new Date().toISOString(),
    originalFile: path.resolve(args.before),
    rewrittenFile: path.resolve(args.after),
    reportFile: path.resolve(args.report),
    measuredAigc,
    originalMetrics: beforeMetrics,
    rewrittenMetrics: afterMetrics,
    metricsDelta: delta(beforeMetrics, afterMetrics),
    externalReport: report,
    calibrationHint: {
      provider: "paperpass",
      measuredAigc,
      appShouldLowerRiskWhen:
        "highSuspectedRatio is 0, aiTerms/connectors drop, plain terms rise, and remaining segments are mostly middle/low package fragments",
      rewriteShouldFocusOn:
        "remaining report-hit middle/low fragments: literature review, theory definition, data explanation, strategy list, meaning closure",
    },
  };
}

function buildReportOnlyOutput(args, report, measuredAigc) {
  return {
    generatedAt: new Date().toISOString(),
    reportFile: path.resolve(args.report),
    measuredAigc,
    externalReport: report,
    calibrationHint: {
      provider: "paperpass",
      measuredAigc,
      appShouldUseAs:
        "external report evidence only; no original/rewritten metrics delta because before/after files were not provided",
      rewriteShouldFocusOn:
        "remaining report-hit middle/low fragments: terminology examples, register-function explanation, literature review, acknowledgements, meaning closure",
      workflowHint:
        "treat 17 2.0 successful reports as test-20-paragraphs followed by stacked full rewrite, not ordinary one-pass full rewrite",
    },
  };
}

main();
