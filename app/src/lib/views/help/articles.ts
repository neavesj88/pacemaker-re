// The articles bundled with the app: every docs/kb/*.md of the Git project (the one source, also read on
// GitHub), read once, on first use. README.md there is the index for GitHub, not an article. The files
// are Markdown with a front matter block (id, title, topic, keywords, summary); the rules are in
// helpLogic.ts and the checks in helpLogic.test.mjs.
import { buildIndex, parseArticle, type HelpIndex } from "./helpLogic";

// docs/kb is at the repository root, five folders up from here (help, views, lib, src, app)
const files = import.meta.glob<string>(["../../../../../docs/kb/*.md", "!../../../../../docs/kb/README.md"], { query: "?raw", import: "default", eager: true });

// the pictures the articles show (`![alt](images/name.png)`), bundled with the app like the articles
const imageFiles = import.meta.glob<string>("../../../../../docs/kb/images/*.{png,svg,jpg,jpeg,webp}", { query: "?url", import: "default", eager: true });
const images = new Map(Object.entries(imageFiles).map(([file, url]) => [file.slice(file.lastIndexOf("/") + 1), url]));

/** The bundled address of a picture in docs/kb/images, by file name; null when there is no such file. */
export function helpImage(name: string): string | null {
  return images.get(name) ?? null;
}

let index: HelpIndex | null = null;

/** The search index of every article. Built on the first call, then reused: searching is only comparing strings. */
export function helpIndex(): HelpIndex {
  index ??= buildIndex(
    Object.entries(files)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([file, raw]) => parseArticle(raw, file)),
  );
  return index;
}
