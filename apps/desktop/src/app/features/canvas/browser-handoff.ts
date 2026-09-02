import { normalizeBrowserUrl } from "./canvas-state";

/** Produces reviewable terminal input and deliberately never submits it. */
export function browserUrlForTerminal(value: string): string | null {
  const url = normalizeBrowserUrl(value);
  return url || null;
}

/** Appends an explicit browser reference without treating remote text as markup. */
export function appendBrowserUrlToNote(note: string, value: string): string {
  const url = normalizeBrowserUrl(value);
  if (!url) {
    return note;
  }
  if (!note) {
    return url;
  }
  const separator = note.endsWith("\n\n")
    ? ""
    : note.endsWith("\n")
      ? "\n"
      : "\n\n";
  return `${note}${separator}${url}`;
}
