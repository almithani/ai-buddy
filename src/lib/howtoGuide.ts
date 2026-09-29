// Everyday "how do I…" guides. Like the settings catalog, the model only picks
// a topic id; steps and the app to open come from howtoCatalog.json (generated
// by scripts/build-howto-catalog.py, also compiled into Rust's howto.rs).
import catalog from "./howtoCatalog.json";
import { rankTopics } from "./topicSearch";

export interface HowtoTopic {
  id: string;
  title: string;
  category: string;
  keywords: string[];
  steps: string[];
  shortcut?: string;
  app?: string;
  settingsTopic?: string;
}

export const HOWTO_TOPICS: HowtoTopic[] = catalog;

export function findHowto(id: string): HowtoTopic | undefined {
  return HOWTO_TOPICS.find((t) => t.id === id.trim().toLowerCase());
}

export function searchHowtos(text: string, k = 4): HowtoTopic[] {
  return rankTopics(HOWTO_TOPICS, text, k);
}
