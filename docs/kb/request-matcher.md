---
id: request-matcher
title: Request list matcher
topic: Library tools
order: 6
keywords: request list, matcher, track matcher, wish list, missing tracks, owned, spotify, beatport, youtube, playlist text, csv, artist title, to check
summary: Paste or open a list of wanted tracks and see which you already own, which to check and which are missing.
---

# Request list matcher

**Track matcher** (under Tools) compares a list of wanted tracks with the library. Nothing leaves your computer: the list is compared here.

## Steps

1. Paste lines like `Artist - Title` into the box, or press **Open file...** for a text, CSV, TSV or M3U file. A Spotify, Beatport or YouTube playlist saved as text or CSV works the same way. Lines with only a title also work.
2. Press **Match**. A progress figure shows while it works.
3. Look through the lines. Each is **owned**, **to check** or **missing**, and you can filter the list by those. Lines that could not be read are listed under "lines skipped".
4. Fix any line by hand: **Use best** takes the best suggestion, **Find...** or **Change...** picks a track yourself, **Not owned** marks it missing and **Undo** goes back to the automatic answer.
5. Take the missing list away: **Copy list**, **Save .txt** or **Save .csv**. It holds exactly the lines the Missing filter shows (and the lines to check, if you include them), as `Artist - Title`.

## Settings

Settings, Import and export, **Request list matcher**: how plain lines are read, the scores that count as owned, to check and missing, how mix names such as "(Extended Mix)" count, and how many suggestions a line gets.
