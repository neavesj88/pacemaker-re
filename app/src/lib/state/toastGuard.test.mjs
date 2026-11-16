// Run: node src/lib/state/toastGuard.test.mjs   (from app/)
//
// Keeps the toasts short (rule in toastText.ts). Reads every source file under src/, finds each
// `toast.show(...)` / `toast.error(...)` call and checks the words it can see: every string or template
// literal in the message argument must be at most TOAST_HARD words (a `${...}` counts as one word).
// Text built at run time (a Rust summary, a variable) cannot be seen here; toast.show condenses that
// itself (condense in toastText.ts) and warns in dev builds.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const SRC = join(fileURLToPath(new URL(".", import.meta.url)), "..", "..");
const HARD = 15;
/** A failure head ("Couldn't save the mix.") shares its card with the reason, so it gets less. */
const HEAD_HARD = 9;
const BS = String.fromCharCode(92);

const files = [];
(function walk(d) {
  for (const e of readdirSync(d, { withFileTypes: true })) {
    const p = join(d, e.name);
    if (e.isDirectory()) walk(p);
    else if (/[.](ts|svelte)$/.test(e.name)) files.push(p);
  }
})(SRC);

/** The text of the first argument of the call whose "(" is just before index `i`, and where it ends. */
function firstArg(s, i) {
  let depth = 1;
  let q = null;
  const stack = [];
  for (let j = i; j < s.length; j++) {
    const c = s[j];
    if (q) {
      if (c === BS) { j++; continue; }
      if (q === "`" && c === "$" && s[j + 1] === "{") { stack.push(depth); q = null; j++; depth++; continue; }
      if (c === q) q = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") { q = c; continue; }
    if ("([{".includes(c)) depth++;
    else if (")]}".includes(c)) {
      depth--;
      if (c === "}" && stack.length && stack[stack.length - 1] === depth) { stack.pop(); q = "`"; continue; }
      if (depth === 0) return s.slice(i, j);
    } else if (c === "," && depth === 1) return s.slice(i, j);
  }
  return s.slice(i);
}

/** Every string / template literal in `expr`, with `${...}` replaced by one placeholder word. */
function literals(expr) {
  const out = [];
  let j = 0;
  while (j < expr.length) {
    const c = expr[j];
    if (c === '"' || c === "'") {
      let k = j + 1;
      let t = "";
      while (k < expr.length && expr[k] !== c) { if (expr[k] === BS) k++; t += expr[k] ?? ""; k++; }
      out.push(t);
      j = k + 1;
    } else if (c === "`") {
      let k = j + 1;
      let t = "";
      let depth = 0;
      while (k < expr.length) {
        const d = expr[k];
        if (depth === 0 && d === BS) { t += expr[k + 1] ?? ""; k += 2; continue; }
        if (depth === 0 && d === "`") break;
        if (depth === 0 && d === "$" && expr[k + 1] === "{") { depth = 1; t += "X"; k += 2; continue; }
        if (depth > 0) {
          if (d === "{") depth++;
          else if (d === "}") depth--;
          k++;
          continue;
        }
        t += d;
        k++;
      }
      out.push(t);
      j = k + 1;
    } else j++;
  }
  return out;
}

const words = (t) => t.split(/\s+/).filter(Boolean).length;

const seen = { calls: 0, literals: 0, over12: 0 };
const bad = [];
for (const f of files) {
  const s = readFileSync(f, "utf8");
  const re = /toast[.](show|error)[(]/g;
  let m;
  while ((m = re.exec(s))) {
    const arg = firstArg(s, m.index + m[0].length);
    const line = s.slice(0, m.index).split("\n").length;
    seen.calls++;
    const limit = m[1] === "error" ? HEAD_HARD : HARD;
    for (const lit of literals(arg)) {
      const n = words(lit);
      if (n === 0) continue;
      seen.literals++;
      if (n > 12) seen.over12++;
      if (n > limit) bad.push(`${relative(SRC, f)}:${line} (${n} words) ${lit.replace(/\s+/g, " ").slice(0, 120)}`);
    }
  }
}

assert.ok(seen.calls > 150, `found only ${seen.calls} toast calls: the scan is broken`);
assert.deepEqual(bad, [], "toast text over the word limit (keep it to one short sentence; put the rest in Details):\n  " + bad.join("\n  "));
console.log(`ok  ${seen.calls} toast calls, ${seen.literals} literals, none over ${HARD} words (${seen.over12} over 12)`);
