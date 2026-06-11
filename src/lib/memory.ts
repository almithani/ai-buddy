export interface MemoryItem {
  id: number;
  kind: "rule" | "setting";
  key: string | null;
  value: string;
  created_at: string;
}

/// Human-readable description of a memory row. Rules are shown verbatim;
/// known settings get friendly wording.
export function describeMemory(item: MemoryItem): string {
  if (item.kind === "rule") return item.value;
  switch (item.key) {
    case "transcript_dir":
      return `Save transcripts to ${item.value}`;
    case "transcript_include_time":
      return item.value === "false"
        ? "Omit the time from transcript filenames"
        : "Include the time in transcript filenames";
    default:
      return `${item.key} = ${item.value}`;
  }
}
