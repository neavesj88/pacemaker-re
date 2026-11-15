// The pure side of the Help view, so plain node can run its tests (helpLogic.test.mjs):
// reading an article file (front matter), a small safe Markdown reader, the search index and the
// search itself. Nothing here touches Svelte, the DOM or the file system.
//
// Safety: the reader never produces HTML. It returns a tree of blocks and inline pieces whose text
// is plain strings, and the view draws them as text. A "<script>" in an article is therefore only
// ever shown as the characters it is. A link is another article (a relative Markdown link such as
// storage-upgrade.md, which works on GitHub too, or help:id) or an http(s) address (shown as
// plain text, because the app has no way to open the system browser); any other target is dropped.

// ---- articles -------------------------------------------------------------------------------

export type Article = {
  /** Stable: other views and articles link to it, so it is never renamed once shipped. */
  id: string;
  title: string;
  topic: string;
  keywords: string[];
  /** One sentence for the list and for the search results. */
  summary: string;
  /** Position inside the topic (lower first); ties by title. */
  order: number;
  /** The Markdown after the front matter. */
  body: string;
};

/** The topics in the order the list shows them; a topic not named here follows, in name order. */
export const TOPIC_ORDER = [
  "Start here",
  "Tracks",
  "Analysis and grids",
  "Cases",
  "Mixes",
  "Library tools",
  "Libraries and sync",
  "Device page",
  "Firmware and hardware",
  "Using the Pacemaker",
  "Problems and answers",
] as const;

/** Splits `---` front matter from the body. Lines are `key: value`; no nesting. */
export function parseFrontMatter(raw: string): { meta: Record<string, string>; body: string } {
  const text = raw.replace(/^﻿/, "").replace(/\r\n?/g, "\n");
  const m = /^---\n([\s\S]*?)\n---[ \t]*(?:\n|$)/.exec(text);
  if (!m) return { meta: {}, body: text };
  const meta: Record<string, string> = {};
  for (const line of m[1].split("\n")) {
    const kv = /^([A-Za-z][A-Za-z0-9_-]*)\s*:\s*(.*)$/.exec(line);
    if (kv) meta[kv[1].toLowerCase()] = kv[2].trim();
  }
  return { meta, body: text.slice(m[0].length) };
}

const REQUIRED = ["id", "title", "topic", "keywords", "summary"] as const;

/** Reads one article file. Throws, naming the file, when a required field is missing or the id is not usable. */
export function parseArticle(raw: string, file = "an article"): Article {
  const { meta, body } = parseFrontMatter(raw);
  for (const k of REQUIRED) if (!meta[k]) throw new Error(`${file}: the front matter has no "${k}"`);
  if (!/^[a-z0-9][a-z0-9-]*$/.test(meta.id)) throw new Error(`${file}: "${meta.id}" is not a usable id (lower case letters, digits and dashes)`);
  const keywords = meta.keywords.split(",").map((k) => k.trim()).filter(Boolean);
  if (keywords.length === 0) throw new Error(`${file}: "keywords" is empty`);
  const order = meta.order !== undefined && meta.order !== "" ? Number(meta.order) : 100;
  if (!Number.isFinite(order)) throw new Error(`${file}: "order" is not a number`);
  return { id: meta.id, title: meta.title, topic: meta.topic, keywords, summary: meta.summary, order, body: dropTitleLine(body.trim(), meta.title) };
}

/** The file starts with "# Title" so GitHub shows a title; the app draws the title from the front matter, so that line goes. */
function dropTitleLine(body: string, title: string): string {
  const m = /^#[ \t]+(.*?)[ \t]*#*[ \t]*(?:\n|$)/.exec(body);
  return m && m[1].trim() === title.trim() ? body.slice(m[0].length).trimStart() : body;
}

/** Articles grouped by topic, in `TOPIC_ORDER`, each group sorted by `order` then title. */
export function groupByTopic(articles: readonly Article[]): { topic: string; articles: Article[] }[] {
  const known: string[] = [...TOPIC_ORDER];
  const extra = [...new Set(articles.map((a) => a.topic))].filter((t) => !known.includes(t)).sort();
  return [...known, ...extra]
    .map((topic) => ({
      topic,
      articles: articles.filter((a) => a.topic === topic).sort((a, b) => a.order - b.order || a.title.localeCompare(b.title)),
    }))
    .filter((g) => g.articles.length > 0);
}

// ---- the Markdown reader ----------------------------------------------------------------------

export type Inline =
  | { t: "text"; text: string }
  | { t: "strong"; text: string }
  | { t: "code"; text: string }
  /** A link to another article, optionally to one of its headings. */
  | { t: "help"; id: string; anchor: string | null; text: string }
  /** An http(s) address. */
  | { t: "ext"; url: string; text: string };

export type Block =
  | { t: "h"; level: 2 | 3 | 4; text: string; slug: string }
  | { t: "p"; inl: Inline[] }
  | { t: "ul"; items: Inline[][] }
  | { t: "ol"; items: Inline[][] }
  | { t: "table"; head: Inline[][]; rows: Inline[][][] }
  | { t: "code"; text: string }
  | { t: "note"; inl: Inline[] };

/** "Boot with USB unplugged" becomes "boot-with-usb-unplugged": the anchor of a heading. */
export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/**
 * A link target the reader accepts: another article as `storage-upgrade.md` or `./storage-upgrade.md#what-works`
 * (the file name is the article id, which the tests enforce), `help:id[#heading]`, or an http(s) address.
 * Null for anything else.
 */
export function classify(target: string): { kind: "help"; id: string; anchor: string | null } | { kind: "ext"; url: string } | null {
  const t = target.trim();
  const h = /^help:([a-z0-9][a-z0-9-]*)(?:#([a-z0-9-]+))?$/.exec(t);
  if (h) return { kind: "help", id: h[1], anchor: h[2] ?? null };
  const f = /^(?:\.\/)?([a-z0-9][a-z0-9-]*)\.md(?:#([a-z0-9-]+))?$/.exec(t);
  if (f) return { kind: "help", id: f[1], anchor: f[2] ?? null };
  if (/^https?:\/\/[^\s<>"']+$/i.test(t)) return { kind: "ext", url: t };
  return null;
}

/** Bold, code and links inside one run of text. Anything else is text, including angle brackets. */
export function parseInline(src: string): Inline[] {
  const out: Inline[] = [];
  let buf = "";
  const flush = () => {
    if (buf) out.push({ t: "text", text: buf });
    buf = "";
  };
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\" && i + 1 < src.length && /[\\`*[\]()|{}]/.test(src[i + 1])) {
      buf += src[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      const end = src.indexOf("`", i + 1);
      if (end > i + 1) {
        flush();
        out.push({ t: "code", text: src.slice(i + 1, end) });
        i = end + 1;
        continue;
      }
    }
    if (c === "*" && src[i + 1] === "*") {
      const end = src.indexOf("**", i + 2);
      if (end > i + 2) {
        flush();
        out.push({ t: "strong", text: stripMarks(src.slice(i + 2, end)) });
        i = end + 2;
        continue;
      }
    }
    if (c === "[") {
      const close = src.indexOf("](", i + 1);
      const end = close < 0 ? -1 : closingParen(src, close + 2);
      if (close > i && end > close) {
        const label = stripMarks(src.slice(i + 1, close));
        const target = classify(src.slice(close + 2, end));
        flush();
        if (!target) out.push({ t: "text", text: label });
        else if (target.kind === "help") out.push({ t: "help", id: target.id, anchor: target.anchor, text: label });
        else out.push({ t: "ext", url: target.url, text: label });
        i = end + 1;
        continue;
      }
    }
    buf += c;
    i++;
  }
  flush();
  return out;
}

/** The ")" that closes a link target starting at `from`; brackets inside the target are matched, as in CommonMark. -1 if none. */
function closingParen(src: string, from: number): number {
  let depth = 0;
  for (let i = from; i < src.length; i++) {
    if (src[i] === "(") depth++;
    else if (src[i] === ")") {
      if (depth === 0) return i;
      depth--;
    }
  }
  return -1;
}

/** Text with the inline marks taken off (used for link labels, bold text and the index). */
function stripMarks(s: string): string {
  return s.replace(/\\([\\`*[\]()|{}])/g, "$1").replace(/`([^`]*)`/g, "$1").replace(/\*\*([^*]*)\*\*/g, "$1");
}

const isTableRow = (l: string) => /^\s*\|.*\|\s*$/.test(l);
const isTableRule = (l: string) => /^\s*\|(\s*:?-{2,}:?\s*\|)+\s*$/.test(l);
const BULLET = /^\s{0,3}[-*]\s+(.*)$/;
const NUMBERED = /^\s{0,3}\d+[.)]\s+(.*)$/;

function splitRow(line: string): string[] {
  // a "\|" inside a cell is a bar, not a cell edge
  const cells: string[] = [];
  let cur = "";
  const t = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  for (let i = 0; i < t.length; i++) {
    if (t[i] === "\\" && t[i + 1] === "|") {
      cur += "\\|";
      i++;
    } else if (t[i] === "|") {
      cells.push(cur.trim());
      cur = "";
    } else cur += t[i];
  }
  cells.push(cur.trim());
  return cells;
}

/** Reads the article body. Never throws; text it cannot read stays text. */
export function parseMarkdown(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i++;
      continue;
    }
    // fenced code
    if (/^\s*```/.test(line)) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) body.push(lines[i++]);
      i++;
      blocks.push({ t: "code", text: body.join("\n") });
      continue;
    }
    // heading (the article's own title comes from the front matter, so # is drawn as ##)
    const h = /^(#{1,4})\s+(.*?)\s*#*\s*$/.exec(line);
    if (h) {
      const text = stripMarks(h[2]);
      blocks.push({ t: "h", level: Math.min(4, Math.max(2, h[1].length)) as 2 | 3 | 4, text, slug: slugify(text) });
      i++;
      continue;
    }
    // note (a block quote)
    if (/^\s*>/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ""));
      blocks.push({ t: "note", inl: parseInline(body.join(" ").trim()) });
      continue;
    }
    // table
    if (isTableRow(line) && i + 1 < lines.length && isTableRule(lines[i + 1])) {
      const head = splitRow(line).map(parseInline);
      i += 2;
      const rows: Inline[][][] = [];
      while (i < lines.length && isTableRow(lines[i])) rows.push(splitRow(lines[i++]).map(parseInline));
      blocks.push({ t: "table", head, rows });
      continue;
    }
    // lists (one level; an indented line continues the item above it)
    const bullet = BULLET.exec(line);
    const numbered = bullet ? null : NUMBERED.exec(line);
    if (bullet || numbered) {
      const re = bullet ? BULLET : NUMBERED;
      const items: string[] = [];
      while (i < lines.length) {
        const m = re.exec(lines[i]);
        if (m) {
          items.push(m[1]);
          i++;
        } else if (/^\s{2,}\S/.test(lines[i]) && items.length > 0 && !BULLET.test(lines[i]) && !NUMBERED.test(lines[i])) {
          items[items.length - 1] += " " + lines[i].trim();
          i++;
        } else break;
      }
      blocks.push({ t: bullet ? "ul" : "ol", items: items.map(parseInline) });
      continue;
    }
    // paragraph
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^\s*```/.test(lines[i]) &&
      !/^(#{1,4})\s+/.test(lines[i]) &&
      !/^\s*>/.test(lines[i]) &&
      !BULLET.test(lines[i]) &&
      !NUMBERED.test(lines[i]) &&
      !(isTableRow(lines[i]) && i + 1 < lines.length && isTableRule(lines[i + 1]))
    )
      para.push(lines[i++].trim());
    blocks.push({ t: "p", inl: parseInline(para.join(" ")) });
  }
  return blocks;
}

const inlineText = (inl: Inline[]): string => inl.map((n) => n.text).join("");

/** The plain text of the blocks, for searching and for snippets. */
export function blocksText(blocks: Block[]): string {
  const parts: string[] = [];
  for (const b of blocks) {
    if (b.t === "h") parts.push(b.text);
    else if (b.t === "p" || b.t === "note") parts.push(inlineText(b.inl));
    else if (b.t === "ul" || b.t === "ol") for (const it of b.items) parts.push(inlineText(it));
    else if (b.t === "table") {
      parts.push(b.head.map(inlineText).join(" "));
      for (const r of b.rows) parts.push(r.map(inlineText).join(" "));
    } else if (b.t === "code") parts.push(b.text.replace(/\s+/g, " "));
  }
  return parts.join(" ").replace(/\s+/g, " ").trim();
}

/** Every link to another article in the blocks. */
export function helpLinks(blocks: Block[]): { id: string; anchor: string | null }[] {
  const out: { id: string; anchor: string | null }[] = [];
  const scan = (inl: Inline[]) => {
    for (const n of inl) if (n.t === "help") out.push({ id: n.id, anchor: n.anchor });
  };
  for (const b of blocks) {
    if (b.t === "p" || b.t === "note") scan(b.inl);
    else if (b.t === "ul" || b.t === "ol") b.items.forEach(scan);
    else if (b.t === "table") {
      b.head.forEach(scan);
      b.rows.forEach((r) => r.forEach(scan));
    }
  }
  return out;
}

// ---- the search index and the search -----------------------------------------------------------

export type Entry = {
  article: Article;
  blocks: Block[];
  titleL: string;
  titleWords: string[];
  keywordsL: string[];
  headings: string[];
  headingsL: string[];
  summaryL: string;
  /** The whole body as one line of text. */
  text: string;
  textL: string;
};

export type HelpIndex = { entries: Entry[]; byId: Map<string, Entry> };

const wordsOf = (s: string): string[] => s.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);

/** Reads every article once. Search then only compares strings. */
export function buildIndex(articles: readonly Article[]): HelpIndex {
  const entries = articles.map((article): Entry => {
    const blocks = parseMarkdown(article.body);
    const text = blocksText(blocks);
    const headings = blocks.filter((b): b is Extract<Block, { t: "h" }> => b.t === "h").map((b) => b.text);
    return {
      article,
      blocks,
      titleL: article.title.toLowerCase(),
      titleWords: wordsOf(article.title),
      keywordsL: article.keywords.map((k) => k.toLowerCase()),
      headings,
      headingsL: headings.map((h) => h.toLowerCase()),
      summaryL: article.summary.toLowerCase(),
      text,
      textL: text.toLowerCase(),
    };
  });
  return { entries, byId: new Map(entries.map((e) => [e.article.id, e])) };
}

/** The words of a search: lower case, punctuation (including the colon of "bpm:") taken off. */
export function queryWords(query: string): string[] {
  const ws = wordsOf(query);
  // one-letter words match half the text; they count only when they are all there is
  const long = ws.filter((w) => w.length > 1);
  return long.length > 0 ? long : ws;
}

export type Seg = { text: string; hit: boolean };

/** `text` cut into pieces, the ones that contain a search word marked. */
export function highlight(text: string, words: readonly string[]): Seg[] {
  const ws = [...new Set(words)].filter(Boolean).sort((a, b) => b.length - a.length);
  if (ws.length === 0 || text === "") return [{ text, hit: false }];
  const re = new RegExp(ws.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|"), "gi");
  const out: Seg[] = [];
  let last = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m[0] === "") {
      re.lastIndex++;
      continue;
    }
    if (m.index > last) out.push({ text: text.slice(last, m.index), hit: false });
    out.push({ text: m[0], hit: true });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last), hit: false });
  return out.length > 0 ? out : [{ text, hit: false }];
}

export type Hit = {
  article: Article;
  score: number;
  /** The title with the search words marked. */
  title: Seg[];
  /** A short piece of the text around the match, the match marked; null when only the title or keywords matched. */
  snippet: Seg[] | null;
  /** Where the best match was. */
  where: "title" | "keyword" | "heading" | "summary" | "body";
};

/** What one word is worth in one entry; 0 when it is nowhere. Title and keyword hits outweigh everything else. */
function wordScore(e: Entry, w: string): { total: number; title: number; keyword: number; heading: number; summary: number; body: number } {
  let title = 0;
  for (const tw of e.titleWords) {
    if (tw === w) title = Math.max(title, 100);
    else if (tw.startsWith(w)) title = Math.max(title, 70);
    else if (tw.includes(w)) title = Math.max(title, 40);
  }
  let keyword = 0;
  for (const k of e.keywordsL) {
    if (k === w) keyword = Math.max(keyword, 90);
    else if (wordsOf(k).includes(w)) keyword = Math.max(keyword, 75);
    else if (wordsOf(k).some((kw) => kw.startsWith(w))) keyword = Math.max(keyword, 55);
    else if (k.includes(w)) keyword = Math.max(keyword, 30);
  }
  let heading = 0;
  for (const h of e.headingsL) {
    const hw = wordsOf(h);
    if (hw.includes(w)) heading = Math.max(heading, 30);
    else if (hw.some((x) => x.startsWith(w))) heading = Math.max(heading, 22);
    else if (h.includes(w)) heading = Math.max(heading, 12);
  }
  const summary = e.summaryL.includes(w) ? 15 : 0;
  let body = 0;
  if (e.textL.includes(w)) {
    let n = 0;
    for (let at = e.textL.indexOf(w); at >= 0 && n < 6; at = e.textL.indexOf(w, at + w.length)) n++;
    body = 8 + n;
  }
  return { total: title + keyword + heading + summary + body, title, keyword, heading, summary, body };
}

/** A window of the article text around the first place a search word occurs, ending at word edges. */
export function snippetAround(text: string, words: readonly string[], width = 150): string | null {
  const lower = text.toLowerCase();
  let at = -1;
  for (const w of words) {
    const p = lower.indexOf(w);
    if (p >= 0 && (at < 0 || p < at)) at = p;
  }
  if (at < 0) return null;
  let start = Math.max(0, at - 45);
  let end = Math.min(text.length, start + width);
  if (start > 0) {
    const sp = text.indexOf(" ", start);
    if (sp >= 0 && sp < at) start = sp + 1;
  }
  if (end < text.length) {
    const sp = text.lastIndexOf(" ", end);
    if (sp > at) end = sp;
  }
  return `${start > 0 ? "… " : ""}${text.slice(start, end).trim()}${end < text.length ? " …" : ""}`;
}

/**
 * Articles matching every word of the query, best first. Title and keyword hits rank above heading,
 * summary and body hits, so "battery" puts the battery article ahead of the ones that only mention
 * it. An empty query has no hits (the list view shows everything then).
 */
export function search(index: HelpIndex, query: string, limit = 50): Hit[] {
  const words = queryWords(query);
  if (words.length === 0) return [];
  const phrase = words.join(" ");
  // a search written with punctuation ("bpm:", "key:~8A", "-genre") means that exact text: an article that writes it
  // that way (the search syntax) outranks the ones that only mention the word
  const literal = /[^a-z0-9\s]/i.test(query.trim()) ? query.trim().toLowerCase() : "";
  const hits: Hit[] = [];
  for (const e of index.entries) {
    let score = 0;
    let best: Hit["where"] = "body";
    let bestVal = -1;
    let all = true;
    for (const w of words) {
      const s = wordScore(e, w);
      if (s.total === 0) {
        all = false;
        break;
      }
      score += s.total;
      const kinds: [Hit["where"], number][] = [["title", s.title], ["keyword", s.keyword], ["heading", s.heading], ["summary", s.summary], ["body", s.body]];
      for (const [k, v] of kinds) {
        if (v > bestVal) {
          bestVal = v;
          best = k;
        }
      }
    }
    if (!all) continue;
    // the whole query as one phrase in the title or a keyword is the best sign of all
    if (literal) {
      if (e.keywordsL.includes(literal)) score += 160;
      if (e.titleL.includes(literal)) score += 40;
      if (e.textL.includes(literal)) score += 30;
    }
    if (words.length > 1) {
      if (e.titleL.includes(phrase)) score += 60;
      if (e.keywordsL.some((k) => k.includes(phrase))) score += 50;
      if (e.keywordsL.includes(phrase)) score += 100; // the question is itself one of the article's keywords
      if (e.textL.includes(phrase)) score += 10;
    }
    const snippet = snippetFor(e, words);
    hits.push({ article: e.article, score, title: highlight(e.article.title, words), snippet: snippet ? highlight(snippet, words) : null, where: best });
  }
  hits.sort((a, b) => b.score - a.score || a.article.order - b.article.order || a.article.title.localeCompare(b.article.title));
  return hits.slice(0, limit);
}

function snippetFor(e: Entry, words: readonly string[]): string | null {
  // the body first (it has the detail); a word found only in the summary or a heading shows that
  const fromBody = snippetAround(e.text, words);
  if (fromBody) return fromBody;
  const h = e.headings.find((x) => words.some((w) => x.toLowerCase().includes(w)));
  if (h) return h;
  return words.some((w) => e.summaryL.includes(w)) ? e.article.summary : null;
}
