// Curated System Settings walkthroughs. The model only picks a topic id; the
// page to open and the click steps come from settingsCatalog.json, because a
// small local model can't reliably remember deep links or macOS menu wording.
// The same JSON is compiled into Rust (`open_system_settings`) so both sides
// agree on what each id opens. Validate with scripts/dump-settings-anchors.mjs.
import catalog from "./settingsCatalog.json";
import { formatTopicList, rankTopics } from "./topicSearch";

export interface SettingsTopic {
  id: string;
  title: string;
  category: string;
  pane: string;
  anchor?: string;
  keywords: string[];
  controlId?: string;
  controlLabel?: string;
  steps: string[];
  caution?: string;
}

export const SETTINGS_TOPICS: SettingsTopic[] = catalog;

export function findSettingsTopic(id: string): SettingsTopic | undefined {
  return SETTINGS_TOPICS.find((t) => t.id === id.trim().toLowerCase());
}

export function searchSettings(text: string, k = 6): SettingsTopic[] {
  return rankTopics(SETTINGS_TOPICS, text, k);
}

export { formatTopicList };
