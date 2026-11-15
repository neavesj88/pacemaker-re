// Run: node --experimental-strip-types --test src/lib/views/help/helpLogic.test.mjs
// The rules of the Help view (front matter, the safe Markdown reader, the search) and the checks on the
// articles themselves: docs/kb/*.md at the repository root is the one source, so these tests read the
// real files.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import * as L from "./helpLogic.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const KB = path.resolve(here, "../../../../../docs/kb");
const files = fs.readdirSync(KB).filter((f) => f.endsWith(".md") && f !== "README.md").sort();
const raws = new Map(files.map((f) => [f, fs.readFileSync(path.join(KB, f), "utf8")]));
const articles = files.map((f) => L.parseArticle(raws.get(f), f));
const index = L.buildIndex(articles);
const readme = fs.readFileSync(path.join(KB, "README.md"), "utf8");

const top = (q, n = 1) => L.search(index, q).slice(0, n).map((h) => h.article.id);

describe("front matter", () => {
  it("reads key: value lines, with CRLF and a byte order mark", () => {
    const r = L.parseFrontMatter("\uFEFF---\r\nid: a\r\ntitle: A: b\r\n---\r\nbody\r\n");
    assert.equal(r.meta.id, "a");
    assert.equal(r.meta.title, "A: b");
    assert.equal(r.body, "body\n");
  });
  it("a file without front matter is all body", () => {
    assert.deepEqual(L.parseFrontMatter("# hi"), { meta: {}, body: "# hi" });
  });
  it("names the file and the field when one is missing", () => {
    assert.throws(() => L.parseArticle("---\nid: a\ntitle: A\ntopic: T\nsummary: s\n---\nx", "f.md"), /f\.md.*"keywords"/);
  });
  it("refuses an id that is not lower case letters, digits and dashes", () => {
    assert.throws(() => L.parseArticle("---\nid: Not Ok\ntitle: A\ntopic: T\nkeywords: k\nsummary: s\n---\nx", "f.md"), /not a usable id/);
  });
  it("splits keywords on commas and drops the title line that GitHub needs", () => {
    const a = L.parseArticle("---\nid: a\ntitle: The A\ntopic: T\nkeywords: one, two words , three\nsummary: s\n---\n\n# The A\n\nText.", "f.md");
    assert.deepEqual(a.keywords, ["one", "two words", "three"]);
    assert.equal(a.body, "Text.");
    const other = L.parseArticle("---\nid: a\ntitle: The A\ntopic: T\nkeywords: k\nsummary: s\n---\n# Different\n\nText.", "f.md");
    assert.ok(other.body.startsWith("# Different"));
  });
});

describe("the reader never makes HTML", () => {
  const text = (blocks) => L.blocksText(blocks);
  it("angle brackets stay characters", () => {
    const b = L.parseMarkdown('A <script>alert(1)</script> and <img src=x onerror=alert(1)> end');
    assert.equal(b.length, 1);
    assert.equal(b[0].t, "p");
    assert.ok(text(b).includes("<script>alert(1)</script>"));
    assert.ok(b[0].inl.every((n) => n.t === "text"));
  });
  it("a link with a script or file target is dropped to its label", () => {
    for (const bad of ["javascript:alert(1)", "data:text/html,x", "file:///c:/x", "//evil.example/x", "../notes/x.md", "http://a b", "mailto:a@b.c"]) {
      const b = L.parseMarkdown(`[click](${bad}) now`);
      assert.ok(b[0].inl.every((n) => n.t === "text"), bad);
      assert.equal(text(b), "click now", bad);
    }
  });
  it("an http(s) link is an external node (shown as text; the app has no way to open it)", () => {
    const b = L.parseMarkdown("See [the page](https://example.com/a?b=1) now");
    const ext = b[0].inl.find((n) => n.t === "ext");
    assert.deepEqual(ext, { t: "ext", url: "https://example.com/a?b=1", text: "the page" });
  });
  it("relative .md links, ./ links and help: links become article links", () => {
    const b = L.parseMarkdown("[a](storage-upgrade.md) [b](./storage-upgrade.md#what-works) [c](help:install-firmware#steps)");
    assert.deepEqual(L.helpLinks(b), [
      { id: "storage-upgrade", anchor: null },
      { id: "storage-upgrade", anchor: "what-works" },
      { id: "install-firmware", anchor: "steps" },
    ]);
  });
  it("bold, code, escapes", () => {
    const b = L.parseMarkdown("**bold** `a<b` \\*not\\* and \\[x\\]");
    assert.deepEqual(b[0].inl.map((n) => n.t), ["strong", "text", "code", "text"]);
    assert.equal(text(b), "bold a<b *not* and [x]");
  });
  it("headings, bullets, numbered steps, a note, code and a table", () => {
    const b = L.parseMarkdown(["# One", "## Two", "- a", "- b", "", "1. x", "2. y", "", "> note", "", "```", "code <b>", "```", "", "| A | B |", "|---|---|", "| 1 | `2` |"].join("\n"));
    assert.deepEqual(b.map((x) => x.t), ["h", "h", "ul", "ol", "note", "code", "table"]);
    assert.equal(b[0].level, 2, "# is drawn as a section heading because the title comes from the front matter");
    assert.equal(b[1].slug, "two");
    assert.deepEqual(b[2].items.length, 2);
    assert.equal(b[5].text, "code <b>");
    assert.equal(b[6].rows[0][1][0].t, "code");
  });
  it("never throws on rubbish", () => {
    for (const s of ["", "[", "](", "[a](", "**", "`", "|", "| a |\n|", "```\nopen", "> ", "- ", "1.", "\\"]) assert.doesNotThrow(() => L.parseMarkdown(s));
  });
});

describe("search", () => {
  it("ranks the title and keywords above mentions in the text", () => {
    assert.equal(top("battery")[0], "battery-replacement");
    assert.equal(top("sd card")[0], "storage-upgrade");
    assert.equal(top("crossfader")[0], "pm-mixing");
    assert.equal(top("effects")[0], "pm-effects");
    assert.equal(top("charge")[0], "pm-power-charging");
    assert.equal(top("smart case")[0], "cases");
    assert.equal(top("dfu", 2).sort().join(), "firmware-methods,pm-button-combos-recovery");
  });
  it("firmware finds the firmware articles first", () => {
    const ids = top("firmware", 3);
    assert.ok(ids.includes("install-firmware") && ids.includes("firmware-methods"), ids.join());
  });
  it("bpm: finds the search syntax and the BPM article", () => {
    assert.equal(top("bpm:")[0], "search-syntax");
    const ids = top("bpm:", 4);
    assert.ok(ids.includes("bpm-keys-waveforms") && ids.includes("search-syntax"), ids.join());
  });
  it("half tempo finds the BPM range and the mix tempo articles", () => {
    assert.equal(top("half tempo")[0], "bpm-keys-waveforms", "an article that lists the question as a keyword comes first");
    const ids = top("half tempo", 3);
    assert.ok(ids.includes("bpm-keys-waveforms") && ids.includes("mixes-crossfades-tempo"), ids.join());
  });
  it("every word must be found", () => {
    assert.deepEqual(L.search(index, "battery zzzqqq", 50, { partial: false }), []);
    assert.deepEqual(L.search(index, ""), []);
    assert.deepEqual(L.search(index, "   "), []);
    assert.ok(L.search(index, "battery").length > L.search(index, "battery 1100").length);
  });
  it("the match is marked in the title and in a short snippet", () => {
    const h = L.search(index, "battery")[0];
    assert.ok(h.title.some((s) => s.hit));
    assert.ok(h.snippet && h.snippet.some((s) => s.hit));
    const plain = h.snippet.map((s) => s.text).join("");
    assert.ok(plain.length <= 190, `snippet is ${plain.length} characters`);
    assert.ok(/battery/i.test(plain));
  });
  it("highlight marks every word, case-insensitively, and survives regex characters", () => {
    assert.deepEqual(L.highlight("Half Tempo, half", ["half"]).filter((s) => s.hit).length, 2);
    assert.deepEqual(L.highlight("a.b (c)", ["(c)", "."]).map((s) => s.hit), [false, true, false, true]);
    assert.deepEqual(L.highlight("", ["x"]), [{ text: "", hit: false }]);
  });
  it("is instant: indexed once, then only string comparisons", () => {
    const t0 = performance.now();
    for (let i = 0; i < 50; i++) for (const q of ["battery", "sd card", "bpm:", "half tempo", "crossfader"]) L.search(index, q);
    const per = (performance.now() - t0) / 250;
    assert.ok(per < 10, `${per.toFixed(2)} ms per search`);
  });
  it("groups by topic in the stated order", () => {
    const g = L.groupByTopic(articles);
    const names = g.map((x) => x.topic);
    assert.deepEqual(names, L.TOPIC_ORDER.filter((t) => names.includes(t)));
    assert.equal(g.flatMap((x) => x.articles).length, articles.length);
  });
});

describe("the articles in docs/kb", () => {
  it("there is a good number of them", () => assert.ok(articles.length >= 35, `${articles.length}`));
  it("each has an id equal to its file name, a title, a known topic, keywords and a summary", () => {
    const ids = new Set();
    for (const [i, a] of articles.entries()) {
      assert.equal(a.id + ".md", files[i], "the id is the file name, so a relative link and a help link mean the same");
      assert.ok(!ids.has(a.id), `${a.id} twice`);
      ids.add(a.id);
      assert.ok(L.TOPIC_ORDER.includes(a.topic), `${a.id}: topic "${a.topic}" is not in TOPIC_ORDER`);
      assert.ok(a.title.length > 3 && a.summary.length > 20 && a.keywords.length >= 3, a.id);
      assert.ok(a.body.length > 300, `${a.id} is too short`);
    }
  });
  it("every topic has an article and the ones the app opens by id exist", () => {
    const have = new Set(articles.map((a) => a.topic));
    for (const t of L.TOPIC_ORDER) assert.ok(have.has(t), `no article in "${t}"`);
    const ids = new Set(articles.map((a) => a.id));
    // ids the app links to: the "?" links and the start page
    for (const id of ["getting-started", "tracks-view", "search-syntax", "mixes-building", "device-page", "install-firmware", "storage-upgrade", "settings-page", "pm-controls-tour", "troubleshooting"])
      assert.ok(ids.has(id), id);
  });
  it("every link between articles resolves, with its heading", () => {
    const slugs = new Map(index.entries.map((e) => [e.article.id, new Set(e.blocks.filter((b) => b.t === "h").map((b) => b.slug))]));
    let n = 0;
    for (const e of index.entries) {
      for (const l of L.helpLinks(e.blocks)) {
        n++;
        assert.ok(slugs.has(l.id), `${e.article.id} links to ${l.id}, which does not exist`);
        if (l.anchor) assert.ok(slugs.get(l.id).has(l.anchor), `${e.article.id} links to ${l.id}#${l.anchor}, which has no such heading`);
      }
    }
    assert.ok(n > 80, `only ${n} links`);
  });
  it("every relative .md link in the files points at a file in docs/kb (so it works on GitHub)", () => {
    for (const [f, raw] of [...raws, ["README.md", readme]]) {
      for (const m of raw.matchAll(/\]\(([^)\s]+)\)/g)) {
        const target = m[1];
        if (/^https?:\/\//.test(target)) continue;
        const m2 = /^([a-z0-9-]+\.md)(#[a-z0-9-]+)?$/.exec(target);
        assert.ok(m2, `${f}: "${target}" is neither an https link nor a plain relative link to an article`);
        assert.ok(fs.existsSync(path.join(KB, m2[1])), `${f}: ${target} does not exist`);
      }
    }
  });
  it("no raw HTML, no help: links (the files must read well on GitHub), no unfinished text", () => {
    for (const [f, raw] of raws) {
      assert.ok(!/<\/?[a-z][^>]*>/i.test(raw.replace(/`[^`]*`/g, "")), `${f} has an HTML tag`);
      assert.ok(!/\]\(help:/.test(raw), `${f} uses a help: link; use a relative .md link`);
      assert.ok(!/\b(TODO|FIXME|TBD|lorem)\b/i.test(raw), f);
    }
  });
  it("the README index lists every article, in a section for its topic", () => {
    for (const a of articles) {
      const line = readme.split("\n").find((l) => l.includes(`](${a.id}.md)`));
      assert.ok(line, `README.md does not list ${a.id}`);
      assert.ok(line.includes(`[${a.title}]`), `README.md names ${a.id} differently from its title`);
    }
    const sections = [...readme.matchAll(/^## (.+)$/gm)].map((m) => m[1]);
    for (const t of new Set(articles.map((a) => a.topic))) assert.ok(sections.includes(t), `README.md has no "${t}" section`);
    assert.equal([...readme.matchAll(/^- \[[^\]]+\]\([a-z0-9-]+\.md\):/gm)].length, articles.length, "README lists something that is not an article, or twice");
  });
  it("the project README links the index", () => {
    const root = fs.readFileSync(path.resolve(KB, "../../README.md"), "utf8");
    assert.ok(root.includes("](docs/kb/README.md)"));
  });
  it("the hardware articles say what is verified", () => {
    for (const id of ["storage-upgrade", "storage-alternatives", "battery-replacement", "pm-controls-tour"]) {
      const a = articles.find((x) => x.id === id);
      assert.ok(/verified/i.test(a.body), id);
    }
    const bat = articles.find((x) => x.id === "battery-replacement").body;
    assert.ok(/not known/i.test(bat) && /polarity/i.test(bat) && /multimeter/i.test(bat));
    const up = articles.find((x) => x.id === "storage-upgrade").body;
    assert.ok(/What we used/.test(up) && /What works/.test(up) && /What to avoid/.test(up) && /Sintech/.test(up) && /U3/.test(up));
  });
  it("this is a public repository: no e-mail address, serial number or personal path", () => {
    for (const [f, raw] of [...raws, ["README.md", readme]]) {
      assert.ok(!/[\w.+-]+@[\w-]+\.[a-z]{2,}/i.test(raw), `${f}: an e-mail address`);
      assert.ok(!/serial\s*(no\.?|number)?\s*[:=]?\s*[A-Z0-9]{8,}/i.test(raw), `${f}: a serial number`);
      assert.ok(!/C:\\Users|\/Users\/|binja|neaves|jared/i.test(raw), `${f}: a personal path or name`);
    }
  });
  it("plain wording: no 'device' or 'card' for a library on this computer", () => {
    for (const id of ["getting-started", "tracks-view", "cases", "mixes-building", "set-builder", "search-syntax", "track-inspector-labels"]) {
      const body = articles.find((x) => x.id === id).body;
      assert.ok(!/library on this (device|card)|this computer'?s? (device|card)/i.test(body), id);
    }
  });
});

const real = (id) => index.byId.get(id).blocks;
const listSizes = (blocks) => blocks.filter((b) => b.t === "ol" || b.t === "ul").map((b) => `${b.t}${b.items.length}`);

describe("nested lists", () => {
  it("a sub-list under an item stays under it and the numbers go on", () => {
    const b = L.parseMarkdown("1. one\n2. two\n   - sub a\n   - sub b\n3. three\n4. four");
    assert.equal(b.length, 1);
    assert.equal(b[0].t, "ol");
    assert.equal(b[0].items.length, 4);
    assert.equal(b[0].subs[1].t, "ul");
    assert.equal(b[0].subs[1].items.length, 2);
    assert.equal(b[0].subs[0], null);
  });
  it("numbered sub-steps nest instead of joining the main list", () => {
    const b = L.parseMarkdown("1. a\n2. b\n   1. b1\n   2. b2\n3. c");
    assert.equal(b.length, 1);
    assert.equal(b[0].items.length, 3);
    assert.equal(b[0].subs[1].items.length, 2);
    assert.equal(b[0].subs[1].t, "ol");
  });
  it("a numbered list keeps counting across a blank line, and a list may start at another number", () => {
    const b = L.parseMarkdown("1. a\n\n2. b\n\n3. c");
    assert.equal(b.length, 1);
    assert.equal(b[0].items.length, 3);
    assert.equal(L.parseMarkdown("4. a\n5. b")[0].start, 4);
  });
  it("a plain paragraph or another kind of list still ends a list", () => {
    assert.deepEqual(L.parseMarkdown("- a\n- b\n\ntext\n\n- c").map((x) => x.t), ["ul", "p", "ul"]);
    assert.deepEqual(L.parseMarkdown("- a\n1. b").map((x) => x.t), ["ul", "ol"]);
  });
  it("text and links inside a nested list are found by the text and link scans", () => {
    const b = L.parseMarkdown("1. a\n   - see [it](storage-upgrade.md#what-works) deep");
    assert.ok(L.blocksText(b).includes("deep"));
    assert.deepEqual(L.helpLinks(b), [{ id: "storage-upgrade", anchor: "what-works" }]);
  });
  it("the real articles: install-firmware has one list of 8 steps, the sub-steps under step 7", () => {
    const ols = real("install-firmware").filter((b) => b.t === "ol");
    const main = ols.find((b) => b.items.length === 8);
    assert.ok(main, listSizes(real("install-firmware")).join());
    assert.equal(main.subs[6].items.length, 4);
    assert.ok(main.items[7].map((n) => n.text).join("").startsWith("Open the Device page again"));
  });
  it("the real articles: storage-upgrade steps run 1 to 8 in one list", () => {
    const steps = real("storage-upgrade").filter((b) => b.t === "ol");
    assert.equal(steps.length, 1, listSizes(real("storage-upgrade")).join());
    assert.equal(steps[0].items.length, 8);
    assert.equal(steps[0].subs[3].items.length, 2);
  });
  it("the real articles: firmware-methods DFU steps and free-space steps are not split in two", () => {
    const dfu = real("firmware-methods").filter((b) => b.t === "ol");
    assert.ok(dfu.some((b) => b.items.length === 6), listSizes(real("firmware-methods")).join());
    const fs = real("free-space").filter((b) => b.t === "ol");
    assert.equal(fs.length, 1, listSizes(real("free-space")).join());
  });
});

describe("search finds what people type", () => {
  const ids = (q, n = 3) => top(q, n);
  it("a misspelt word finds the article it was meant for", () => {
    assert.equal(ids("batery")[0], "battery-replacement");
    assert.equal(ids("batterry")[0], "battery-replacement");
    assert.ok(ids("fimware", 4).includes("install-firmware"), ids("fimware", 4).join());
    assert.equal(L.editDistance("fimware", "firmware", 1), 1);
    assert.equal(L.editDistance("batery", "battery", 1), 1);
    assert.equal(L.editDistance("abcd", "abdc", 1), 1, "a swap is one edit");
    assert.ok(L.editDistance("abcdef", "uvwxyz", 2) > 2);
  });
  it("a short word is never guessed at", () => {
    assert.deepEqual(L.fuzzyWords(index, "bat"), []);
    assert.deepEqual(L.search(index, "zzqx"), []);
  });
  it("memory words find the storage upgrade", () => {
    assert.equal(ids("memory upgrade")[0], "storage-upgrade");
    assert.equal(ids("memory card")[0], "storage-upgrade");
    assert.equal(ids("sdcard")[0], "storage-upgrade");
    assert.equal(ids("bigger drive")[0], "storage-upgrade");
  });
  it("it will not turn on / wont charge find the power article", () => {
    for (const q of ["wont turn on", "won't turn on", "will not turn on", "wont charge", "won't charge", "dead"]) assert.ok(ids(q, 2).includes("pm-power-charging"), `${q}: ${ids(q, 2).join()}`);
  });
  it("US and UK spellings find each other", () => {
    assert.ok(L.search(index, "colors").length > 0);
    assert.equal(L.search(index, "colors").length, L.search(index, "colours").length);
    assert.ok(L.search(index, "analyze").length > 0);
    assert.equal(L.search(index, "analyze").length, L.search(index, "analyse").length);
    assert.deepEqual(L.spellingVariants("colors"), ["colours"]);
    assert.deepEqual(L.spellingVariants("analyze"), ["analyse"]);
    assert.deepEqual(L.spellingVariants("organise"), ["organize"]);
  });
  it("when no article has every word, the ones with some are listed and marked partial", () => {
    const h = L.search(index, "battery zzzqqq");
    assert.ok(h.length > 0 && h.every((x) => x.partial));
    assert.equal(h[0].article.id, "battery-replacement");
    assert.ok(L.search(index, "battery").every((x) => !x.partial));
  });
  it("small words alone do not make a partial match of everything", () => {
    assert.deepEqual(L.search(index, "the zzzqqq"), []);
  });
});
