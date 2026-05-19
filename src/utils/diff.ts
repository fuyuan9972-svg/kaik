import DiffMatchPatch, { DIFF_DELETE, DIFF_EQUAL, DIFF_INSERT } from "diff-match-patch";

export interface DiffPart {
  type: "equal" | "insert" | "delete";
  text: string;
}

const dmp = new DiffMatchPatch();

export function diffText(original: string, rewritten: string): DiffPart[] {
  const diffs = dmp.diff_main(original, rewritten);
  dmp.diff_cleanupSemantic(diffs);
  return diffs.map(([operation, text]) => ({
    type:
      operation === DIFF_INSERT
        ? "insert"
        : operation === DIFF_DELETE
          ? "delete"
          : operation === DIFF_EQUAL
            ? "equal"
            : "equal",
    text,
  }));
}
