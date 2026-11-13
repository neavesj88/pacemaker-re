import type { SettingsEntry } from "../settings/types";
import MatcherSettingsFields from "./MatcherSettingsFields.svelte";

const entries: SettingsEntry[] = [
  {
    id: "request-matcher",
    area: "Import and export",
    title: "Request list matcher",
    help: "How plain lines are read, the scores that count as owned, to check and missing, how mix names count and how many suggestions a line gets.",
    keywords: "match request list owned missing score artist title version suggestions",
    how: "inline",
    component: MatcherSettingsFields,
  },
];

export default entries;
