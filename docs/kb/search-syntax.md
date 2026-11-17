---
id: search-syntax
title: Search syntax
topic: Tracks
order: 2
keywords: search, query, bpm:, key:, range, comparison, or, not, exclude, minus, tag:, colour:, energy:, added:, played:, is:fav, rating, year, field, phrase, quotes, camelot, compatible
summary: Reference for the search box query language: words, phrases, fields, ranges, comparisons, keys, tags, dates, flags, not and or.
---

# Search syntax

The search box in the Tracks view and the mix track table accepts the queries below. Unreadable terms are flagged under the filters. The ? button in the Tracks toolbar lists these rows, and **Try** puts an example into the search box.

## Words, phrases and fields

| Type | Finds | Example |
|---|---|---|
| words | Every word in title, artist, album, genre, key, comments, label, remixer, composer, year or tags. Space means AND | `deep house` |
| "a phrase" | The exact words | `"summer night"` |
| field:text | Text in title, artist, album, genre, label, remixer, composer, comments or format | `label:example` |
| field:"" | Tracks where the field is empty | `genre:""` |

Put a word like `re:mix` in quotes to search it as text.

## Ranges and comparisons

Numeric fields: `bpm`, `year`, `rating`, `energy`, `plays`, `bitrate`, `size`, `time`. Operators: `>`, `>=`, `<`, `<=`, `=`. Ranges include both ends.

| Query | Meaning |
|---|---|
| `bpm:120-128` or `bpm:120..128` | 120 to 128 |
| `bpm>=128` | 128 or more |
| `bpm:128` | 127.5 to 128.5 |
| `rating>=4` | Four stars or more |
| `plays>10` | Played more than ten times |
| `time>3:30` | Longer than 3:30 (seconds or m:ss; `time:3:00-5:00` is a range) |
| `size>10` | Over 10 MB (500k, 10m, 1g also work) |
| `bitrate>=256` | 256 kbit/s or more |

BPM searches skip tracks that need analysis.

## Keys

- `key:8A` or a musical name such as `key:Am`.
- `key:~8A` adds keys that mix with it. See [BPM, keys and waveforms](bpm-keys-waveforms.md).

## Tags, colours and energy

- `tag:peak` matches the whole tag, any case. `*` is a wildcard: `tag:sum*`. Quote spaces: `tag:"warm up"`.
- `colour:red` matches a colour label. `colour:none` finds tracks without one.
- `energy:7-9` uses the **Energy source** setting.
- `myenergy:` uses your rating. `autoenergy:` uses the audio estimate.

## Dates

- `added:7d` is the last 7 days. Units: `d`, `w`, `m`, `y`. `<` is newer, `>` is older.
- `added:2026`, `added:2026-03`, `added:2026-03-14`, `added:>=2026-01-01`, `added:2026-01-01..2026-03-01`.
- `played:` takes the same forms. `played:never` finds unplayed tracks.

## Flags

`is:fav`, `is:needs` (needs analysis), `is:analysed`, `is:unrated`, `is:unplayed`, `is:untagged`.

## Not and or

- A minus negates: `-genre:techno`, `-tag:skip`, `-is:unplayed`, `-"live version"`.
- `genre:house OR genre:techno` finds either. AND binds tighter than OR.

## Combined example

`genre:house bpm:122-128 key:~8A -tag:skip rating>=3 added:90d`
