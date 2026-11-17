---
id: request-matcher
title: Request list matcher
topic: Library tools
order: 6
keywords: request list, matcher, track matcher, wish list, missing tracks, owned, spotify, beatport, youtube, playlist text, csv, artist title, to check
summary: Paste or open a list of wanted tracks and see which you already own, which to check and which are missing.
---

# Request list matcher

**Track matcher** (under Tools) compares a list of wanted tracks with the library.

## Match a list

1. Paste lines such as `Artist - Title`, or press **Open file...** for a text, CSV, TSV or M3U file. Spotify, Beatport and YouTube playlists saved as text or CSV work too.
2. Press **Match**.
3. Each line is **owned**, **to check** or **missing**. Filter by these. Unreadable lines are listed under "lines skipped".

## Correct a line

| Button | Action |
|---|---|
| **Use best** | Takes the best suggestion |
| **Find...** / **Change...** | Pick a track yourself |
| **Not owned** | Marks the line missing |
| **Undo** | Returns to the automatic answer |

## Export the missing list

**Copy list**, **Save .txt** or **Save .csv** output the lines the Missing filter shows (plus lines to check, if included) as `Artist - Title`.

## Options

Settings, Import and export, **Request list matcher**: how plain lines are read, the scores for owned, to check and missing, how mix names such as "(Extended Mix)" count, and how many suggestions a line gets.
