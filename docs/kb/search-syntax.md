---
id: search-syntax
title: Search syntax
topic: Tracks
order: 2
keywords: search, query, bpm:, key:, range, comparison, or, not, exclude, minus, tag:, colour:, energy:, added:, played:, is:fav, rating, year, field, phrase, quotes, camelot, compatible
summary: Everything the search box understands, with an example for each: words, phrases, fields, ranges, comparisons, keys, tags, dates, flags, not and or.
---

# Search syntax

The search box at the top of the window (and the one in the Tracks view and the mix track table) takes plain words and a small query language. Nothing you type can break it: a term it cannot read is ignored and flagged under the filters, never an empty list.

## The basics

| You type | It finds | Example |
|---|---|---|
| words | Every word must appear in the title, artist, album, genre, key, comments, label, remixer, composer, year or a tag. Spaces mean "and" | `deep house` |
| "a phrase" | The words exactly as written, spaces included | `"summer night"` |
| field:text | Tracks whose title, artist, album, genre, label, remixer, composer, comments or format contain the text | `label:example` |
| field:"" | Tracks where that field is empty | `genre:""` |

## Ranges and comparisons

Numbers can be given as a range (both ends included) or compared. `bpm:120-128` and `bpm:120..128` mean the same. These fields take numbers: `bpm`, `year`, `rating`, `energy`, `plays`, `bitrate`, `size`, `time`.

| You type | Meaning |
|---|---|
| `bpm:120-128` | From 120 to 128 |
| `bpm>=128` | 128 or more. The comparisons are `>`, `>=`, `<`, `<=` and `=` |
| `bpm:128` | Just 128, which means 127.5 to 128.5 |
| `rating>=4` | Four stars or more |
| `plays>10` | Played more than ten times |
| `time>3:30` | Longer than 3 minutes 30 (seconds or m:ss; `time:3:00-5:00` is a range) |
| `size>10` | Bigger than 10 MB (500k, 10m and 1g also work) |
| `bitrate>=256` | 256 kbit/s or more |

A track that still needs analysis has no BPM to compare, so BPM searches skip it.

## Keys

- `key:8A` finds that exact key. Musical names work too: `key:Am`.
- `key:~8A` also finds the keys that mix with it: one step round the Camelot wheel, and the relative major or minor. See [BPM, keys and waveforms](bpm-keys-waveforms.md).

## Tags, colours and energy

- `tag:peak` finds tracks with that tag (the whole tag, any case). `*` is a wildcard (`tag:sum*`). Quote a tag with spaces: `tag:"warm up"`.
- `colour:red` finds a colour label: red, orange, yellow, green, teal, blue, purple or pink. `colour:none` finds tracks with no colour.
- `energy:7-9` uses the energy under the **Energy source** setting (your rating, the estimate from the audio, or yours with the estimate as a fallback). `myenergy:` is only your rating and `autoenergy:` is only the estimate (`autoenergy:7` means 6.5 to 7.5).

Tags, colours and energy are kept by this app, not on the Pacemaker.

## Dates

- `added:7d` means added in the last 7 days. Units are `d`, `w`, `m` (30 days) and `y` (365 days). `<` is newer and `>` is older.
- A date also works: `added:2026`, `added:2026-03`, `added:2026-03-14`, `added:>=2026-01-01` or `added:2026-01-01..2026-03-01`.
- `played:` works the same way for the last played time. `played:never` finds tracks that were never played.

## Flags

`is:fav` (favourites), `is:needs` (needs analysis), `is:analysed`, `is:unrated`, `is:unplayed` and `is:untagged`.

## Not and or

- A minus sign in front of any term means "not": `-genre:techno`, `-tag:skip`, `-is:unplayed`, `-"live version"`.
- `a OR b` finds either. "And" binds tighter than OR, so `x y OR z` means `(x y) OR z`. Example: `genre:house OR genre:techno`.

## Putting it together

`genre:house bpm:122-128 key:~8A -tag:skip rating>=3 added:90d`

finds house tracks between 122 and 128 BPM that mix with 8A, are not tagged skip, have three stars or more and were added in the last 90 days.

## When a word looks like a field but is not

Something such as `re:mix` is searched as ordinary text and flagged under the filters. Put it in quotes to search for it on purpose.

The ? button in the Tracks toolbar lists the same rows and has a Try button that puts an example into the search box.
