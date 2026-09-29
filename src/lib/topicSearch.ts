// Keyword ranking shared by the curated catalogs (settings topics, how-to
// guides). Catalogs are too big to list in the 4096-token prompt, so each user
// message is matched against them and only the best few are shown to the model.

export interface SearchableTopic {
  id: string;
  title: string;
  keywords: string[];
}

const STOPWORDS = new Set(
  "a an the my me i to is it on of for and or in how do can you please want turn make get this that be with from".split(" ")
);

// Everyday words → the words the catalogs use.
const SYNONYMS: Record<string, string[]> = {
  font: ["text"],
  word: ["text"],
  letter: ["text"],
  tiny: ["small"],
  subtitl: ["caption"],
  internet: ["wifi"],
  wireless: ["wifi", "bluetooth"],
  headphon: ["bluetooth", "sound"],
  airpod: ["bluetooth"],
  loud: ["volume"],
  quiet: ["volume"],
  cursor: ["pointer"],
  arrow: ["pointer"],
  blind: ["voiceover"],
  deaf: ["caption"],
  magnif: ["zoom"],
  popup: ["notification"],
  alert: ["notification"],
  dark: ["dark", "brightness"],
  dim: ["brightness"],
};

function stem(word: string): string {
  for (const suffix of ["ing", "ed", "es", "s"]) {
    if (word.length > suffix.length + 3 && word.endsWith(suffix)) return word.slice(0, -suffix.length);
  }
  return word;
}

function tokens(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, " ")
    .replace(/-/g, "")
    .split(/\s+/)
    .filter((w) => w && !STOPWORDS.has(w))
    .map(stem);
}

function expand(words: string[]): Set<string> {
  const out = new Set(words);
  for (const w of words) for (const s of SYNONYMS[w] ?? []) out.add(s);
  return out;
}

export function rankTopics<T extends SearchableTopic>(topics: T[], text: string, k: number): T[] {
  const queryWords = tokens(text);
  const query = expand(queryWords);
  if (query.size === 0) return [];
  const joined = ` ${queryWords.join(" ")} `;

  const scored = topics.map((t) => {
    let score = 0;
    const keywordWords = new Set(t.keywords.flatMap(tokens));
    const titleWords = new Set([...tokens(t.title), ...tokens(t.id.replace(/_/g, " "))]);
    for (const w of query) {
      if (keywordWords.has(w)) score += 2;
      else if (titleWords.has(w)) score += 1;
    }
    for (const kw of t.keywords) {
      const phrase = tokens(kw);
      if (phrase.length > 1 && joined.includes(` ${phrase.join(" ")} `)) score += 2 * phrase.length;
    }
    return { t, score };
  });

  return scored
    .filter((s) => s.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, k)
    .map((s) => s.t);
}

export function formatTopicList(topics: SearchableTopic[]): string {
  return topics.map((t) => `  ${t.id} — ${t.title}`).join("\n");
}
