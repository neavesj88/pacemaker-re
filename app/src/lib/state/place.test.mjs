// Run: node --experimental-strip-types src/lib/state/place.test.mjs
// The same table of paths as the Rust tests of library_kind (commands/library/kind.rs): the
// browser preview and the app must agree on what is a Pacemaker and what is this computer.
import assert from "node:assert/strict";
import { missingComputerPath, libraryKind, libraryLabel, pickerOptions, placeWords, sameRoot, startupRoot, welcomeOffer } from "./place.ts";

const kinds = (paths, want) => {
  for (const p of paths) assert.equal(libraryKind(p), want, JSON.stringify(p));
};

kinds(["G:\\", "G:", "g:/", "G:\\\\", "  G:\\", "\\\\?\\G:\\", "//?/g:/"], "pacemaker");
kinds(["C:\\Users\\me\\Music\\Pacemaker Revival", "G:\\Music", "G:\\Music\\", "g:/Music/x/", "G:foo", "\\\\?\\C:\\Users\\me"], "computer");
kinds(["\\\\nas\\music", "\\\\nas\\music\\", "//nas/music", "\\\\?\\UNC\\nas\\music", "\\\\?\\UNC\\nas\\music\\lib", "\\\\.\\G:\\"], "computer");
kinds(["/Volumes/PACEMAKER", "/Volumes/PACEMAKER/", "/media/pm", "/media/jo/PACEMAKER", "/run/media/jo/PACEMAKER/", "/run/media/pm", "/mnt/usb", "/mnt/usb//"], "pacemaker");
kinds(
  ["/Volumes", "/media", "/mnt", "/run/media", "/Volumes/PACEMAKER/Music/x", "/media/jo/PM/Music/x", "/mnt/a/b/c", "/home/jo/Music/Pacemaker Revival", "/", "/mnt/usb/../x", "/mnt/./usb/x/..", "/volumes/pm", "/mnt2/x"],
  "computer",
);
kinds(["", " ", "Music", "Music\\Pacemaker Revival", "./G:", "mnt/usb", "..\\G:\\", "G"], "computer");

// the words that name the place
assert.equal(placeWords("pacemaker").on, "on the Pacemaker");
assert.equal(placeWords("computer").on, "in this library");
assert.equal(placeWords("computer").the, "this library");
assert.equal(placeWords(undefined).the, "the Pacemaker", "no library open reads as a Pacemaker");
assert.deepEqual([placeWords("computer").card, placeWords("computer").drive], ["the library", "the library folder"]);
assert.deepEqual([placeWords("pacemaker").card, placeWords("pacemaker").drive], ["the card", "the drive"]);
assert.deepEqual([placeWords("pacemaker").device, placeWords("pacemaker").thisCard], ["the device", "this card"]);
assert.deepEqual([placeWords("computer").device, placeWords("computer").thisCard], ["the library", "this library"]);

// how a library is named
assert.equal(libraryLabel("computer", "C:\\Users\\me\\Music\\Pacemaker Revival"), "This computer");
assert.equal(libraryLabel("pacemaker", "G:\\"), "Pacemaker (G:)");
assert.equal(libraryLabel("pacemaker", "g:"), "Pacemaker (g:)");
assert.equal(libraryLabel("pacemaker", "/media/jo/PACEMAKER/"), "Pacemaker (PACEMAKER)");

// two spellings of one root
assert.ok(sameRoot("E:", "e:/"));
assert.ok(sameRoot("C:\\Music\\Lib\\", "c:/music/lib"));
assert.ok(!sameRoot("E:\\", "F:\\"));

// the picker: This computer first, then the Pacemakers, plus a folder opened by hand
const lib = "C:\\Users\\me\\Music\\Pacemaker Revival";
assert.deepEqual(pickerOptions(lib, true, ["G:\\", "H:\\"]), [
  { value: lib, label: "This computer", title: lib },
  { value: "G:\\", label: "Pacemaker (G:)", title: "G:\\" },
  { value: "H:\\", label: "Pacemaker (H:)", title: "H:\\" },
]);
assert.equal(pickerOptions(lib, false, [])[0].label, "This computer (new)", "a library that is not made yet says so");
assert.deepEqual(pickerOptions(null, false, ["G:\\"]), [{ value: "G:\\", label: "Pacemaker (G:)", title: "G:\\" }], "no computer path, no entry");
assert.equal(pickerOptions(lib, true, ["G:\\"], { root: "g:", kind: "pacemaker" }).length, 2, "an open Pacemaker is not listed twice");
const hand = pickerOptions(lib, true, [], { root: "D:\\Other", kind: "computer" });
assert.deepEqual(hand[1], { value: "D:\\Other", label: "Folder (Other)", title: "D:\\Other" }, "a folder opened by hand is added under its own name, not as a second This computer");
assert.equal(libraryLabel("computer", "D:\\Other\\", lib), "Folder (Other)");
assert.equal(libraryLabel("computer", lib.toLowerCase(), lib), "This computer", "the computer library by another spelling");
assert.equal(libraryLabel("computer", "D:\\Other", null), "This computer", "computer library path not loaded yet: the real library is never mislabelled as a Folder");

// start-up: only the library on this computer, only when it exists and the setting is on
const info = (exists, open_at_start) => ({ exists, path: lib, settings: { open_at_start } });
assert.equal(startupRoot(info(true, true)), lib);
assert.equal(startupRoot(info(true, false)), null);
assert.equal(startupRoot(info(false, true)), null, "nothing to open yet");
assert.equal(startupRoot(null), null, "not read yet");

// the Welcome screen
const offer = (o) => ({ unreadable: false, missingPath: null, ...o });
assert.deepEqual(welcomeOffer(info(false, true), []), offer({ start: true, open: false, copyFrom: null }));
assert.deepEqual(welcomeOffer(info(false, true), ["G:\\", "H:\\"]), offer({ start: true, open: false, copyFrom: "G:\\" }), "copy is offered when a Pacemaker is there");
assert.deepEqual(welcomeOffer(info(true, false), ["G:\\"]), offer({ start: false, open: true, copyFrom: null }), "no copy offer once a library exists");
assert.deepEqual(welcomeOffer(null, ["G:\\"]), offer({ start: false, open: false, copyFrom: null }));

// a damaged computer-library.json: NOTHING opens by itself, not even a library that exists at the default
// path, and the Welcome screen offers neither "start" nor "open" until the user chooses
const damaged = { exists: true, path: lib, unreadable: true, settings: { open_at_start: true } };
assert.equal(startupRoot(damaged), null, "the default library is not opened when the settings file is unreadable");
assert.deepEqual(welcomeOffer(damaged, ["G:\\"]), { start: false, open: false, copyFrom: null, unreadable: true, missingPath: null });
assert.equal(startupRoot({ ...damaged, unreadable: false }), lib, "a readable file still opens it");

// a path in the file that does not exist: not opened, not made; the screen says where it looked
const gone = { exists: false, path: "E:/Music/Lib", settings: { path: "E:/Music/Lib", open_at_start: true } };
assert.equal(startupRoot(gone), null);
assert.equal(welcomeOffer(gone, []).missingPath, "E:/Music/Lib");
assert.equal(welcomeOffer(info(false, true), []).missingPath, null, "the default location is just a library not made yet, not a missing one");

assert.equal(missingComputerPath(gone), "E:/Music/Lib");
assert.equal(missingComputerPath(info(false, true)), null, "the default location is not a missing path");
assert.equal(missingComputerPath({ ...gone, exists: true }), null, "it is there");
assert.equal(missingComputerPath({ ...gone, unreadable: true }), null, "unreadable settings are another case");

console.log("place.test.mjs ok");
