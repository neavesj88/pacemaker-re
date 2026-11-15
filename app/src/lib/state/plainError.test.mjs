// Run: node --experimental-strip-types src/lib/state/plainError.test.mjs
import assert from "node:assert/strict";
import { plainError } from "./plainError.ts";

assert.match(plainError('Decode(Unsupported("id3v2: unsupported ID3v2 version"))'), /tags are in a format/);
assert.match(plainError('Decode(Unsupported("no suitable format reader found"))'), /format is not supported/);
assert.match(plainError('Decode(DecodeError("invalid data"))'), /damaged/);
assert.equal(plainError("Decode(IoError)"), "Decode(IoError)", "only the shapes it knows are rewritten");
assert.equal(plainError("disk full"), "disk full", "other text is left alone");
console.log("plainError.test.mjs ok");
