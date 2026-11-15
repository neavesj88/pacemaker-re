// Rust error text that reaches the screen as it is written in the code, turned into a sentence. Pure,
// so it runs under plain node (plainError.test.mjs).

/** `Decode(Unsupported("id3v2: unsupported ID3v2 version"))` and its kin become plain English. Anything
 *  that is not a decode error is returned as it came. */
export function plainError(raw: string): string {
  const m = /^Decode\((\w+)\((?:"(.*)")?\)\)$/s.exec(raw.trim());
  if (!m) return raw;
  const why = (m[2] ?? "").replace(/\\"/g, '"');
  switch (m[1]) {
    case "Unsupported":
      return /id3v2/i.test(why) ? "Could not read this file: its tags are in a format this app does not support." : "Could not read this file: its format is not supported.";
    case "DecodeError":
    case "Malformed":
      return "Could not read this file: it looks damaged.";
    default:
      return "Could not read this file as audio.";
  }
}
